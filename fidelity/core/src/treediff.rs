//! Ordered tree differencing for semantic trees.
//!
//! `diff` is the production algorithm: subtrees with equal content hashes are matched
//! outright, the rest of each sibling sequence is aligned by dynamic programming with a
//! node cost that knows what a renamed control looks like, matched pairs recurse, and
//! deleted and inserted subtrees with equal hashes become moves. It is O(n·m) per
//! sibling sequence and yields an edit script with paths into both trees, which is
//! what a person or a fix needs.
//!
//! `zhang_shasha` is the exact ordered tree edit distance with unit costs. It is
//! O(n²m²) at worst and gives no mapping; the tests use it to check that `diff` never
//! reports a distance below the true one, and matches it on the cases that matter.
use crate::normalize::{node_hash, HashFields};
use crate::schema::{NodePath, SemNode, SemTree, Tolerances};
use crate::text::similarity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    /// Present in the reference, absent in the simulation.
    Delete {
        path: NodePath,
        role: String,
        name: String,
        size: usize,
    },
    /// Absent in the reference, present in the simulation.
    Insert {
        path: NodePath,
        role: String,
        name: String,
        size: usize,
    },
    /// Same control, one field differs.
    Update {
        path_a: NodePath,
        path_b: NodePath,
        field: String,
        from: String,
        to: String,
    },
    /// Same subtree, different place.
    Move {
        from: NodePath,
        to: NodePath,
        role: String,
        name: String,
    },
}
impl EditOp {
    pub fn cost(&self) -> f32 {
        match self {
            EditOp::Delete { size, .. } | EditOp::Insert { size, .. } => *size as f32,
            EditOp::Update { field, .. } => field_cost(field),
            EditOp::Move { .. } => 1.0,
        }
    }
}
/// How much one differing field weighs against a whole missing node (1.0).
pub fn field_cost(field: &str) -> f32 {
    match field {
        "role" => 1.0,
        "name" => 0.6,
        "value" => 0.4,
        "bounds" => 0.0,
        _ => 0.2,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TreeDiff {
    pub ops: Vec<EditOp>,
    /// Pairs of matched nodes (path in a, path in b), the root pair included.
    pub matched: Vec<(NodePath, NodePath)>,
    /// Sum of op costs, bounds excluded.
    pub distance: f32,
    pub size_a: usize,
    pub size_b: usize,
}
impl TreeDiff {
    /// Distance relative to the larger tree; 0 when identical.
    pub fn relative(&self) -> f32 {
        let n = self.size_a.max(self.size_b).max(1);
        self.distance / n as f32
    }
    pub fn identical(&self) -> bool {
        self.ops.is_empty()
    }
}

/// Whether two nodes may be the same control, and how far apart they are (0 = equal,
/// 1 = unrelated). Role must agree; names must be similar, or both empty with
/// overlapping positions.
pub fn node_distance(a: &SemNode, b: &SemNode, tol: &Tolerances) -> Option<f32> {
    if a.role != b.role {
        return None;
    }
    let mut d = 0.0;
    // Windows are the same window whatever their titles say: titles carry document
    // names, modified marks and version numbers, and the children decide the rest.
    if crate::schema::is_window_role(&a.role) && a.name != b.name {
        let s = similarity(&a.name, &b.name);
        let same_app = match (a.attrs.get("app"), b.attrs.get("app")) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        };
        if s >= tol.name_similarity_min || same_app {
            return Some(d + 0.6 * (1.0 - s));
        }
        return None;
    }
    if a.name != b.name {
        let s = similarity(&a.name, &b.name);
        if a.name.is_empty() || b.name.is_empty() {
            // One side lost its label: the same control only if it sits in the same place.
            if a.bounds.iou(&b.bounds) < 0.5 {
                return None;
            }
            d += 0.6;
        } else if s < tol.name_similarity_min {
            return None;
        } else {
            d += 0.6 * (1.0 - s);
        }
    }
    if a.value != b.value {
        d += 0.4;
    }
    if a.states.checked != b.states.checked
        || a.states.selected != b.states.selected
        || a.states.expanded != b.states.expanded
    {
        d += 0.2;
    }
    if a.states.enabled != b.states.enabled {
        d += 0.2;
    }
    Some(d)
}

struct Hashes {
    a: HashMap<*const SemNode, u64>,
    b: HashMap<*const SemNode, u64>,
}
fn fill(n: &SemNode, into: &mut HashMap<*const SemNode, u64>) -> u64 {
    let h = node_hash(n, HashFields::Content);
    into.insert(n as *const _, h);
    for c in &n.children {
        fill(c, into);
    }
    h
}

pub fn diff(a: &SemTree, b: &SemTree, tol: &Tolerances) -> TreeDiff {
    diff_nodes(&a.root, &b.root, tol)
}

pub fn diff_nodes(a: &SemNode, b: &SemNode, tol: &Tolerances) -> TreeDiff {
    let mut hashes = Hashes {
        a: HashMap::new(),
        b: HashMap::new(),
    };
    fill(a, &mut hashes.a);
    fill(b, &mut hashes.b);
    let mut out = TreeDiff {
        size_a: a.size(),
        size_b: b.size(),
        ..Default::default()
    };
    let mut deleted: Vec<(NodePath, &SemNode)> = Vec::new();
    let mut inserted: Vec<(NodePath, &SemNode)> = Vec::new();
    align(
        a,
        b,
        &mut Vec::new(),
        &mut Vec::new(),
        &hashes,
        tol,
        &mut out,
        &mut deleted,
        &mut inserted,
    );
    // Equal subtrees deleted here and inserted there moved.
    let mut used = vec![false; inserted.len()];
    let mut moves = Vec::new();
    let mut still_deleted = Vec::new();
    for (pa, na) in &deleted {
        let ha = hashes.a[&(*na as *const _)];
        let found = inserted
            .iter()
            .enumerate()
            .find(|(i, (_, nb))| !used[*i] && hashes.b[&(*nb as *const _)] == ha && na.size() > 1);
        if let Some((i, (pb, _))) = found {
            used[i] = true;
            moves.push((pa.clone(), pb.clone(), na));
        } else {
            still_deleted.push((pa.clone(), *na));
        }
    }
    let mut ops = Vec::new();
    for (from, to, n) in moves {
        out.matched.push((from.clone(), to.clone()));
        ops.push(EditOp::Move {
            from,
            to,
            role: n.role.clone(),
            name: n.name.clone(),
        });
    }
    for (path, n) in still_deleted {
        ops.push(EditOp::Delete {
            path,
            role: n.role.clone(),
            name: n.name.clone(),
            size: n.size(),
        });
    }
    for (i, (path, n)) in inserted.into_iter().enumerate() {
        if !used[i] {
            ops.push(EditOp::Insert {
                path,
                role: n.role.clone(),
                name: n.name.clone(),
                size: n.size(),
            });
        }
    }
    out.ops.extend(ops);
    out.distance = out.ops.iter().map(EditOp::cost).sum();
    out
}

#[allow(clippy::too_many_arguments)]
fn align<'a>(
    a: &'a SemNode,
    b: &'a SemNode,
    pa: &mut NodePath,
    pb: &mut NodePath,
    hashes: &Hashes,
    tol: &Tolerances,
    out: &mut TreeDiff,
    deleted: &mut Vec<(NodePath, &'a SemNode)>,
    inserted: &mut Vec<(NodePath, &'a SemNode)>,
) {
    out.matched.push((pa.clone(), pb.clone()));
    updates(a, b, pa, pb, out);
    let ca = &a.children;
    let cb = &b.children;
    if ca.is_empty() && cb.is_empty() {
        return;
    }
    // Pass 1: identical subtrees, matched in order.
    let mut matched_a = vec![None::<usize>; ca.len()];
    let mut matched_b = vec![false; cb.len()];
    {
        let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
        for (j, c) in cb.iter().enumerate() {
            by_hash
                .entry(hashes.b[&(c as *const _)])
                .or_default()
                .push(j);
        }
        for (i, c) in ca.iter().enumerate() {
            let h = hashes.a[&(c as *const _)];
            if let Some(list) = by_hash.get_mut(&h) {
                // Prefer the same index, else the first unused.
                let pick = list
                    .iter()
                    .position(|&j| j == i && !matched_b[j])
                    .or_else(|| list.iter().position(|&j| !matched_b[j]));
                if let Some(p) = pick {
                    let j = list[p];
                    matched_b[j] = true;
                    matched_a[i] = Some(j);
                    list.remove(p);
                }
            }
        }
    }
    // Pass 2: align the residue by DP over the unmatched subsequences, in order.
    let ra: Vec<usize> = (0..ca.len()).filter(|i| matched_a[*i].is_none()).collect();
    let rb: Vec<usize> = (0..cb.len()).filter(|j| !matched_b[*j]).collect();
    let n = ra.len();
    let m = rb.len();
    // dp[i][j] = min cost aligning ra[..i] with rb[..j]
    let mut dp = vec![vec![0f32; m + 1]; n + 1];
    let mut back = vec![vec![0u8; m + 1]; n + 1]; // 1 = delete a, 2 = insert b, 3 = match
    for i in 1..=n {
        dp[i][0] = dp[i - 1][0] + ca[ra[i - 1]].size() as f32;
        back[i][0] = 1;
    }
    for j in 1..=m {
        dp[0][j] = dp[0][j - 1] + cb[rb[j - 1]].size() as f32;
        back[0][j] = 2;
    }
    for i in 1..=n {
        for j in 1..=m {
            let del = dp[i - 1][j] + ca[ra[i - 1]].size() as f32;
            let ins = dp[i][j - 1] + cb[rb[j - 1]].size() as f32;
            let mut best = del;
            let mut arg = 1;
            if ins < best {
                best = ins;
                arg = 2;
            }
            if let Some(d) = node_distance(&ca[ra[i - 1]], &cb[rb[j - 1]], tol) {
                // Matching also commits the children; estimate their cost by the
                // size gap, which the recursion then refines.
                let sub = (ca[ra[i - 1]].size() as i64 - cb[rb[j - 1]].size() as i64).unsigned_abs()
                    as f32;
                let sub_cost = dp[i - 1][j - 1] + d + sub * 0.5;
                if sub_cost < best {
                    best = sub_cost;
                    arg = 3;
                }
            }
            dp[i][j] = best;
            back[i][j] = arg;
        }
    }
    let (mut i, mut j) = (n, m);
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    while i > 0 || j > 0 {
        match back[i][j] {
            3 => {
                pairs.push((ra[i - 1], rb[j - 1]));
                i -= 1;
                j -= 1;
            }
            1 => {
                i -= 1;
            }
            _ => {
                j -= 1;
            }
        }
    }
    for (ia, jb) in pairs {
        matched_a[ia] = Some(jb);
        matched_b[jb] = true;
    }
    // Pairs matched out of order are moves: the ones outside the longest increasing
    // subsequence of b-indices, so the fewest siblings are charged.
    let in_order = longest_increasing(&matched_a);
    // Recurse into matched pairs in a's order; record the rest.
    for (i, c) in ca.iter().enumerate() {
        pa.push(i as u32);
        match matched_a[i] {
            Some(j) => {
                pb.push(j as u32);
                if !in_order[i] {
                    out.ops.push(EditOp::Move {
                        from: pa.clone(),
                        to: pb.clone(),
                        role: c.role.clone(),
                        name: c.name.clone(),
                    });
                }
                align(c, &cb[j], pa, pb, hashes, tol, out, deleted, inserted);
                pb.pop();
            }
            None => deleted.push((pa.clone(), c)),
        }
        pa.pop();
    }
    for (j, c) in cb.iter().enumerate() {
        if !matched_b[j] {
            pb.push(j as u32);
            inserted.push((pb.clone(), c));
            pb.pop();
        }
    }
}

/// For each matched index of `a`, whether it belongs to a longest increasing
/// subsequence of the matched `b` indices (unmatched entries are `true`).
fn longest_increasing(matched: &[Option<usize>]) -> Vec<bool> {
    let idx: Vec<usize> = (0..matched.len())
        .filter(|i| matched[*i].is_some())
        .collect();
    let vals: Vec<usize> = idx.iter().map(|&i| matched[i].unwrap()).collect();
    let n = vals.len();
    let mut len = vec![1usize; n];
    let mut prev = vec![usize::MAX; n];
    for i in 0..n {
        for j in 0..i {
            if vals[j] < vals[i] && len[j] + 1 > len[i] {
                len[i] = len[j] + 1;
                prev[i] = j;
            }
        }
    }
    let mut out = vec![true; matched.len()];
    if n == 0 {
        return out;
    }
    for &i in &idx {
        out[i] = false;
    }
    let mut k = (0..n)
        .max_by_key(|&i| (len[i], std::cmp::Reverse(i)))
        .unwrap();
    loop {
        out[idx[k]] = true;
        if prev[k] == usize::MAX {
            break;
        }
        k = prev[k];
    }
    out
}

fn updates(a: &SemNode, b: &SemNode, pa: &NodePath, pb: &NodePath, out: &mut TreeDiff) {
    let mut push = |field: &str, from: String, to: String| {
        out.ops.push(EditOp::Update {
            path_a: pa.clone(),
            path_b: pb.clone(),
            field: field.to_owned(),
            from,
            to,
        });
    };
    if a.role != b.role {
        push("role", a.role.clone(), b.role.clone());
    }
    if a.name != b.name {
        push("name", a.name.clone(), b.name.clone());
    }
    if a.value != b.value {
        push(
            "value",
            a.value.clone().unwrap_or_default(),
            b.value.clone().unwrap_or_default(),
        );
    }
    let (sa, sb) = (&a.states, &b.states);
    if sa.enabled != sb.enabled {
        push("enabled", sa.enabled.to_string(), sb.enabled.to_string());
    }
    if sa.checked != sb.checked {
        push("checked", fmt_opt(sa.checked), fmt_opt(sb.checked));
    }
    if sa.selected != sb.selected {
        push("selected", fmt_opt(sa.selected), fmt_opt(sb.selected));
    }
    if sa.expanded != sb.expanded {
        push("expanded", fmt_opt(sa.expanded), fmt_opt(sb.expanded));
    }
    if sa.focused != sb.focused {
        push("focused", sa.focused.to_string(), sb.focused.to_string());
    }
    if a.bounds != b.bounds {
        push(
            "bounds",
            format!(
                "{},{},{},{}",
                a.bounds.x, a.bounds.y, a.bounds.w, a.bounds.h
            ),
            format!(
                "{},{},{},{}",
                b.bounds.x, b.bounds.y, b.bounds.w, b.bounds.h
            ),
        );
    }
}
fn fmt_opt(v: Option<bool>) -> String {
    v.map_or_else(|| "-".to_owned(), |b| b.to_string())
}

/// Exact ordered tree edit distance (Zhang & Shasha 1989) with unit insert/delete and
/// a rename cost of 1 when role or name differ, 0 otherwise.
pub fn zhang_shasha(a: &SemNode, b: &SemNode) -> f32 {
    struct Post<'a> {
        nodes: Vec<&'a SemNode>,
        lmd: Vec<usize>,
        keyroots: Vec<usize>,
    }
    fn post<'a>(root: &'a SemNode) -> Post<'a> {
        let mut nodes = Vec::new();
        let mut lmd = Vec::new();
        fn go<'a>(n: &'a SemNode, nodes: &mut Vec<&'a SemNode>, lmd: &mut Vec<usize>) -> usize {
            let mut first = None;
            for c in &n.children {
                let l = go(c, nodes, lmd);
                if first.is_none() {
                    first = Some(l);
                }
            }
            nodes.push(n);
            let idx = nodes.len() - 1;
            let l = first.unwrap_or(idx);
            lmd.push(l);
            l
        }
        go(root, &mut nodes, &mut lmd);
        let n = nodes.len();
        let mut keyroots = Vec::new();
        for i in 0..n {
            if !(i + 1..n).any(|j| lmd[j] == lmd[i]) {
                keyroots.push(i);
            }
        }
        Post {
            nodes,
            lmd,
            keyroots,
        }
    }
    let ta = post(a);
    let tb = post(b);
    let (n, m) = (ta.nodes.len(), tb.nodes.len());
    let mut td = vec![vec![0f32; m]; n];
    let rename = |x: &SemNode, y: &SemNode| {
        if x.role == y.role && x.name == y.name {
            0.0
        } else {
            1.0
        }
    };
    for &i in &ta.keyroots {
        for &j in &tb.keyroots {
            let (li, lj) = (ta.lmd[i], tb.lmd[j]);
            let rows = i - li + 2;
            let cols = j - lj + 2;
            let mut fd = vec![vec![0f32; cols]; rows];
            for x in 1..rows {
                fd[x][0] = fd[x - 1][0] + 1.0;
            }
            for y in 1..cols {
                fd[0][y] = fd[0][y - 1] + 1.0;
            }
            for x in 1..rows {
                for y in 1..cols {
                    let (ai, bj) = (li + x - 1, lj + y - 1);
                    if ta.lmd[ai] == li && tb.lmd[bj] == lj {
                        let r = fd[x - 1][y - 1] + rename(ta.nodes[ai], tb.nodes[bj]);
                        fd[x][y] = (fd[x - 1][y] + 1.0).min(fd[x][y - 1] + 1.0).min(r);
                        td[ai][bj] = fd[x][y];
                    } else {
                        let px = ta.lmd[ai] - li;
                        let py = tb.lmd[bj] - lj;
                        fd[x][y] = (fd[x - 1][y] + 1.0)
                            .min(fd[x][y - 1] + 1.0)
                            .min(fd[px][py] + td[ai][bj]);
                    }
                }
            }
        }
    }
    td[n - 1][m - 1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Rect;

    fn leaf(role: &str, name: &str, x: i32) -> SemNode {
        SemNode::new(role, name, Rect::new(x, 0, 10, 10))
    }
    fn window(name: &str, children: Vec<SemNode>) -> SemNode {
        let mut w = SemNode::new("window", name, Rect::new(0, 0, 100, 100));
        w.children = children;
        w
    }
    fn tree(children: Vec<SemNode>) -> SemTree {
        let mut t = SemTree::desktop(100, 100);
        t.root.children = children;
        t
    }

    #[test]
    fn identical_trees_have_no_ops() {
        let t = tree(vec![window(
            "w",
            vec![leaf("button", "Save", 0), leaf("button", "Open", 20)],
        )]);
        let d = diff(&t, &t, &Tolerances::default());
        assert!(d.identical());
        assert_eq!(d.distance, 0.0);
        assert_eq!(zhang_shasha(&t.root, &t.root), 0.0);
    }

    #[test]
    fn missing_control_is_a_delete() {
        let a = tree(vec![window(
            "w",
            vec![
                leaf("button", "Save", 0),
                leaf("button", "Open", 20),
                leaf("button", "Close", 40),
            ],
        )]);
        let b = tree(vec![window(
            "w",
            vec![leaf("button", "Save", 0), leaf("button", "Close", 40)],
        )]);
        let d = diff(&a, &b, &Tolerances::default());
        assert_eq!(d.ops.len(), 1);
        match &d.ops[0] {
            EditOp::Delete { path, name, .. } => {
                assert_eq!(path, &vec![0, 1]);
                assert_eq!(name, "Open");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(d.distance, 1.0);
        assert_eq!(zhang_shasha(&a.root, &b.root), 1.0);
    }

    #[test]
    fn rename_is_an_update_when_similar() {
        let a = tree(vec![window("w", vec![leaf("button", "Save as", 0)])]);
        let b = tree(vec![window("w", vec![leaf("button", "Save As", 0)])]);
        let d = diff(&a, &b, &Tolerances::default());
        assert_eq!(d.ops.len(), 1);
        assert!(matches!(&d.ops[0], EditOp::Update { field, .. } if field == "name"));
        assert!(d.distance < 1.0);
    }

    #[test]
    fn unrelated_names_are_delete_and_insert() {
        let a = tree(vec![window("w", vec![leaf("button", "Save", 0)])]);
        let b = tree(vec![window("w", vec![leaf("button", "Quit", 0)])]);
        let d = diff(&a, &b, &Tolerances::default());
        assert_eq!(d.ops.len(), 2);
        assert_eq!(d.distance, 2.0);
        // Exact distance renames in one step; ours is an upper bound by design.
        assert!(d.distance >= zhang_shasha(&a.root, &b.root));
    }

    #[test]
    fn moved_subtree_is_a_move() {
        let mut menu = SemNode::new("menu", "File", Rect::new(0, 0, 50, 50));
        menu.children = vec![leaf("menuitem", "New", 0), leaf("menuitem", "Open", 0)];
        let a = tree(vec![window(
            "w",
            vec![menu.clone(), leaf("button", "x", 60)],
        )]);
        let b = tree(vec![window(
            "w",
            vec![leaf("button", "x", 60), menu.clone()],
        )]);
        let tol = Tolerances {
            name_similarity_min: 0.99,
            ..Default::default()
        };
        let d = diff(&a, &b, &tol);
        // Reordering of hash-equal siblings is matched outright: no ops at all, since
        // the first pass pairs by hash regardless of position.
        assert!(
            d.ops
                .iter()
                .all(|o| !matches!(o, EditOp::Delete { .. } | EditOp::Insert { .. })),
            "{:?}",
            d.ops
        );
    }

    #[test]
    fn move_across_windows_reported() {
        let mut menu = SemNode::new("menu", "File", Rect::new(0, 0, 50, 50));
        menu.children = vec![leaf("menuitem", "New", 0), leaf("menuitem", "Open", 0)];
        let a = tree(vec![window("w1", vec![menu.clone()]), window("w2", vec![])]);
        let b = tree(vec![window("w1", vec![]), window("w2", vec![menu.clone()])]);
        let d = diff(&a, &b, &Tolerances::default());
        assert!(
            d.ops.iter().any(|o| matches!(o, EditOp::Move { .. })),
            "{:?}",
            d.ops
        );
        assert_eq!(d.distance, 1.0);
    }

    #[test]
    fn windows_match_across_titles() {
        // Titles carry the document and the version; the same application's window
        // on each side is the same window.
        let mut wa = window("design — Schematic Editor", vec![leaf("button", "Save", 0)]);
        wa.attrs.insert("app".into(), "kicad".into());
        let mut wb = window("KiCad", vec![leaf("button", "Save", 0)]);
        wb.attrs.insert("app".into(), "kicad".into());
        let a = tree(vec![wa]);
        let b = tree(vec![wb]);
        let d = diff(&a, &b, &Tolerances::default());
        assert!(
            d.ops.iter().all(|o| matches!(o, EditOp::Update { .. })),
            "{:?}",
            d.ops
        );
        assert!(d.distance < 1.0);
    }

    #[test]
    fn bounds_change_costs_nothing_but_is_reported() {
        let a = tree(vec![window("w", vec![leaf("button", "Save", 0)])]);
        let b = tree(vec![window("w", vec![leaf("button", "Save", 3)])]);
        let d = diff(&a, &b, &Tolerances::default());
        assert_eq!(d.distance, 0.0);
        assert!(d
            .ops
            .iter()
            .any(|o| matches!(o, EditOp::Update { field, .. } if field == "bounds")));
    }

    #[test]
    fn never_below_exact_distance() {
        // A handful of shaped cases; the DP's estimate may exceed the exact distance
        // but must never undercut it.
        let cases = vec![
            (
                tree(vec![window(
                    "w",
                    vec![
                        leaf("button", "a", 0),
                        leaf("button", "b", 1),
                        leaf("button", "c", 2),
                    ],
                )]),
                tree(vec![window(
                    "w",
                    vec![
                        leaf("button", "c", 0),
                        leaf("button", "b", 1),
                        leaf("button", "a", 2),
                    ],
                )]),
            ),
            (
                tree(vec![
                    window("w", vec![leaf("textbox", "Name", 0)]),
                    window("v", vec![]),
                ]),
                tree(vec![window("v", vec![leaf("textbox", "Name", 0)])]),
            ),
            (
                tree(vec![]),
                tree(vec![window("w", vec![leaf("button", "a", 0)])]),
            ),
        ];
        for (a, b) in cases {
            let d = diff(&a, &b, &Tolerances::default());
            let exact = zhang_shasha(&a.root, &b.root);
            assert!(
                d.distance + 1e-6 >= exact,
                "{} < {exact}: {:?}",
                d.distance,
                d.ops
            );
        }
    }

    #[test]
    fn zhang_shasha_known_values() {
        // Classic example: f(d(a c(b)) e) vs f(c(d(a b)) e) has distance 2.
        fn n(name: &str, children: Vec<SemNode>) -> SemNode {
            let mut x = SemNode::new("x", name, Rect::default());
            x.children = children;
            x
        }
        let a = n(
            "f",
            vec![
                n("d", vec![n("a", vec![]), n("c", vec![n("b", vec![])])]),
                n("e", vec![]),
            ],
        );
        let b = n(
            "f",
            vec![
                n("c", vec![n("d", vec![n("a", vec![]), n("b", vec![])])]),
                n("e", vec![]),
            ],
        );
        assert_eq!(zhang_shasha(&a, &b), 2.0);
    }
}
