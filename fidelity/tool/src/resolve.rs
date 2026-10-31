//! Finding a procedure target in a reference semantic tree, the way `Run::gesture`
//! finds one in a scene: every field given must match; the topmost (active window
//! first), then the most specific (smallest) visible node wins.
use cw_fidelity::normalize::{canonical_role, is_interactive};
use cw_fidelity::text::normalize_name;
use cw_fidelity::{is_window_role, NodePath, Rect, Resolution, SemNode, SemTree, Target};

fn visible_in(n: &SemNode, desktop: &Rect) -> bool {
    n.states.visible && !n.bounds.is_empty() && n.bounds.intersection(desktop).is_some()
}

/// Point inside `r` at thousandths `at`, or its centre.
pub fn point_in(r: &Rect, at: Option<[u32; 2]>) -> (i32, i32) {
    match at {
        Some([ax, ay]) => (
            r.x + ((r.w as i64 * ax as i64) / 1000) as i32,
            r.y + ((r.h as i64 * ay as i64) / 1000) as i32,
        ),
        None => r.center(),
    }
}

pub fn resolve(tree: &SemTree, target: &Target) -> Option<Resolution> {
    let desktop = tree.root.bounds;
    let want_role = target.role.as_deref().map(canonical_role);
    let want_label = target.label.as_deref().map(normalize_name);
    let want_text = target.text.as_deref().map(normalize_name);
    let active = tree.active_window.as_ref().and_then(|p| p.first().copied());
    let mut best: Option<(i32, i64, NodePath, &SemNode)> = None; // (rank, area, path, node)
    for (path, n) in tree.root.walk() {
        if path.is_empty() || !visible_in(n, &desktop) {
            continue;
        }
        if let Some(r) = want_role {
            if canonical_role(&n.role) != r {
                continue;
            }
        }
        let name = normalize_name(&n.name);
        if let Some(l) = &want_label {
            if &name != l && !name.eq_ignore_ascii_case(l) {
                continue;
            }
        }
        if let Some(t) = &want_text {
            let value = n.value.as_deref().map(normalize_name).unwrap_or_default();
            let hay = format!("{name}\n{value}").to_lowercase();
            let needle = t.to_lowercase();
            // A short text (`R`, `C1`) must be a whole word, or every label containing
            // the letter would match.
            let hit = if needle.chars().count() <= 2 {
                hay.split(|c: char| !c.is_alphanumeric())
                    .any(|w| w == needle)
            } else {
                hay.contains(&needle)
            };
            if !hit {
                continue;
            }
        }
        if let Some(app) = &target.app {
            let (_, w) = tree.root.window_of(&path)?;
            let kind = w.attrs.get("app").cloned().unwrap_or_default();
            if !kind.contains(&app.to_lowercase())
                && !w.name.to_lowercase().contains(&app.to_lowercase())
            {
                continue;
            }
        }
        if let Some(wg) = &target.window {
            let Some((_, w)) = tree.root.window_of(&path) else {
                continue;
            };
            if !cw_fidelity::text::glob_matches(wg, &w.name) {
                continue;
            }
        }
        if target.interaction.is_some() {
            // Interaction ids are the simulator's; a reference has none.
            continue;
        }
        if want_role.is_none()
            && want_label.is_none()
            && want_text.is_none()
            && target.window.is_none()
        {
            continue;
        }
        // Rank: the active window first, then interactive nodes, then windows last.
        let mut rank = 0;
        if Some(path[0]) == active {
            rank -= 100;
        } else {
            rank -= path[0] as i32; // later windows are higher in the stack
        }
        if is_interactive(&n.role) {
            rank -= 10;
        }
        if is_window_role(&n.role) {
            rank += 50;
        }
        // `smallest` is the most specific node; `largest` a canvas or work area; `first`
        // keeps tree order.
        let area = match target.pick.as_deref() {
            Some("largest") => -n.bounds.area(),
            Some("first") => 0,
            _ => n.bounds.area(),
        };
        let better = match &best {
            None => true,
            Some((r, a, _, _)) => rank < *r || (rank == *r && area < *a),
        };
        if better {
            best = Some((rank, area, path.clone(), n));
        }
    }
    let (_, _, path, n) = best?;
    let (x, y) = point_in(&n.bounds, target.at);
    Some(Resolution {
        path,
        role: n.role.clone(),
        name: n.name.clone(),
        bounds: n.bounds,
        x,
        y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picks_active_window_and_smallest() {
        let mut t = SemTree::desktop(800, 600);
        let mut w1 = SemNode::new("window", "Back", Rect::new(0, 0, 400, 300));
        w1.children
            .push(SemNode::new("button", "_Save", Rect::new(10, 10, 80, 30)));
        let mut w2 = SemNode::new("window", "Front", Rect::new(100, 100, 400, 300));
        w2.children
            .push(SemNode::new("button", "Save", Rect::new(110, 110, 80, 30)));
        let mut big = SemNode::new("button", "Save", Rect::new(110, 200, 200, 60));
        big.children
            .push(SemNode::new("text", "Save", Rect::new(120, 210, 40, 12)));
        w2.children.push(big);
        t.root.children.push(w1);
        t.root.children.push(w2);
        t.active_window = Some(vec![1]);
        let r = resolve(
            &t,
            &Target {
                role: Some("button".into()),
                label: Some("Save".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.path, vec![1, 0]);
        assert_eq!((r.x, r.y), (150, 125));
        let r = resolve(
            &t,
            &Target {
                text: Some("save".into()),
                at: Some([0, 0]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.path, vec![1, 0]);
        assert_eq!((r.x, r.y), (110, 110));
        assert!(resolve(
            &t,
            &Target {
                label: Some("Quit".into()),
                ..Default::default()
            }
        )
        .is_none());
    }
}
