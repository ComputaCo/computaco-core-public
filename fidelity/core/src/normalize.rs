//! Turning what each side emits into one comparable form: a canonical role vocabulary,
//! pruning of what no person sees, collapsing of toolkit wrapper nodes, name cleanup
//! and reading-order siblings.
use crate::hash::Fnv;
use crate::schema::{is_window_role, NodePath, Rect, SemNode, SemTree};
use crate::text::normalize_name;
use serde::{Deserialize, Serialize};

/// The canonical vocabulary. Both sides' roles map into this list; anything else
/// becomes `unknown` and is still compared by name and bounds.
pub const ROLES: &[&str] = &[
    "desktop",
    "application",
    "window",
    "dialog",
    "frame",
    "alert",
    "tooltip",
    "menubar",
    "menu",
    "menuitem",
    "toolbar",
    "button",
    "switch",
    "checkbox",
    "radio",
    "textbox",
    "searchbox",
    "combobox",
    "spinbutton",
    "slider",
    "scrollbar",
    "progressbar",
    "tablist",
    "tab",
    "list",
    "option",
    "tree",
    "treeitem",
    "grid",
    "table",
    "row",
    "cell",
    "columnheader",
    "rowheader",
    "link",
    "heading",
    "text",
    "label",
    "img",
    "canvas",
    "panel",
    "group",
    "document",
    "statusbar",
    "separator",
    "unknown",
];

/// Map an AT-SPI, ARIA, UIA or ComputerWorld role name to the canonical vocabulary.
pub fn canonical_role(role: &str) -> &'static str {
    let r = role.trim().to_lowercase().replace(['_', '-'], " ");
    let out: &'static str = match r.as_str() {
        "desktop" | "desktop frame" => "desktop",
        "application" => "application",
        "window" | "frame" | "toplevel" => "window",
        "dialog" | "file chooser" | "color chooser" | "font chooser" | "alertdialog" => "dialog",
        "alert" => "alert",
        "tool tip" | "tooltip" => "tooltip",
        "menu bar" | "menubar" => "menubar",
        "menu" | "popup menu" | "context menu" => "menu",
        "menu item" | "menuitem" | "check menu item" | "radio menu item" | "tearoff menu item"
        | "menuitemcheckbox" | "menuitemradio" => "menuitem",
        "tool bar" | "toolbar" => "toolbar",
        "push button" | "button" | "toggle button" | "menubutton" | "pushbutton" => "button",
        "switch" => "switch",
        "check box" | "checkbox" => "checkbox",
        "radio button" | "radio" => "radio",
        "entry" | "password text" | "textbox" | "edit" | "editable text" => "textbox",
        "search box" | "searchbox" => "searchbox",
        "combo box" | "combobox" => "combobox",
        "spin button" | "spinbutton" => "spinbutton",
        "slider" | "dial" => "slider",
        "scroll bar" | "scrollbar" => "scrollbar",
        "progress bar" | "progressbar" | "level bar" => "progressbar",
        "page tab list" | "tablist" | "tab list" => "tablist",
        "page tab" | "tab" => "tab",
        "list" | "list box" | "listbox" => "list",
        "list item" | "listitem" | "option" => "option",
        "tree" | "tree table" | "treegrid" => "tree",
        "tree item" | "treeitem" => "treeitem",
        "grid" | "table" | "data table" => "table",
        "table row" | "row" => "row",
        "table cell" | "cell" | "gridcell" => "cell",
        "table column header" | "column header" | "columnheader" => "columnheader",
        "table row header" | "row header" | "rowheader" => "rowheader",
        "link" | "hyperlink" => "link",
        "heading" | "header" => "heading",
        "static" | "statictext" | "paragraph" | "label" | "caption" => "label",
        "image" | "img" | "icon" | "picture" => "img",
        "canvas" | "drawing area" => "canvas",
        "panel" | "filler" | "viewport" | "scroll pane" | "split pane" | "layered pane"
        | "section" | "pane" | "box" | "generic" | "region" | "form" | "html container"
        | "glass pane" | "root pane" | "internal frame" | "embedded" | "autocomplete" => "panel",
        "group" | "grouping" | "radio group" | "radiogroup" => "group",
        "document frame"
        | "document"
        | "document text"
        | "document web"
        | "document spreadsheet"
        | "document presentation"
        | "document email"
        | "text area"
        | "textarea" => "document",
        "status bar" | "statusbar" | "status" => "statusbar",
        "separator" | "splitter" => "separator",
        "text" | "text leaf" | "textleaf" | "static text" => "text",
        _ => "unknown",
    };
    // The simulator already speaks the vocabulary; keep an exact member as is.
    if out == "unknown" {
        if let Some(exact) = ROLES.iter().find(|x| **x == r) {
            return exact;
        }
    }
    out
}

/// Roles that a person interacts with; the ones a target can name.
pub fn is_interactive(role: &str) -> bool {
    matches!(
        role,
        "menuitem"
            | "button"
            | "switch"
            | "checkbox"
            | "radio"
            | "textbox"
            | "searchbox"
            | "combobox"
            | "spinbutton"
            | "slider"
            | "scrollbar"
            | "tab"
            | "option"
            | "treeitem"
            | "cell"
            | "columnheader"
            | "rowheader"
            | "link"
            | "menu"
            | "canvas"
            | "document"
    )
}

/// Roles that are structure, not content: collapsed when unnamed.
pub fn is_wrapper(role: &str) -> bool {
    matches!(
        role,
        "panel" | "group" | "application" | "unknown" | "separator"
    )
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NormalizeOptions {
    /// Drop nodes that are invisible, empty, or wholly outside the desktop.
    pub prune_invisible: bool,
    /// Splice unnamed wrapper nodes' children into their parent.
    pub collapse_wrappers: bool,
    /// Drop a `text`/`label` node whose name is empty.
    pub drop_empty_text: bool,
    /// Keep only windows and, under each, the interactive controls and text as one
    /// flat level: the shape both sides can reach however deep their toolkits nest.
    pub flatten: bool,
    /// Order siblings top-to-bottom, left-to-right in bands this many pixels tall.
    pub reading_order_band: Option<i32>,
    /// Express bounds relative to the owning window's origin.
    pub window_relative: bool,
    /// Clean names (mnemonics, ellipses, whitespace).
    pub clean_names: bool,
    /// Compare names case-insensitively.
    pub fold_case: bool,
    /// Roles removed before comparison.
    #[serde(default)]
    pub drop_roles: Vec<String>,
    /// Names removed before comparison; `*` matches any run of characters.
    #[serde(default)]
    pub drop_names: Vec<String>,
    /// Scrollbars are a platform's, not an application's: drop them.
    pub drop_scrollbars: bool,
    /// Window-manager controls (resize handles, move grips, the title bar's buttons) and
    /// a control that is the whole window: the compositor's, not the application's.
    pub drop_frame_controls: bool,
    /// The desktop shell's own window (task bar, dock, panel): the OS's, compared on its
    /// own, not as part of an application.
    pub drop_shell: bool,
}
impl Default for NormalizeOptions {
    fn default() -> Self {
        Self {
            prune_invisible: true,
            collapse_wrappers: true,
            drop_empty_text: true,
            flatten: true,
            reading_order_band: Some(6),
            window_relative: true,
            clean_names: true,
            fold_case: false,
            drop_roles: vec!["tooltip".into(), "statusbar".into()],
            drop_names: Vec::new(),
            drop_scrollbars: true,
            drop_frame_controls: true,
            drop_shell: true,
        }
    }
}
impl NormalizeOptions {
    /// Keep every node and only canonicalize roles and names: for inspection.
    pub fn faithful() -> Self {
        Self {
            prune_invisible: false,
            collapse_wrappers: false,
            drop_empty_text: false,
            flatten: false,
            reading_order_band: None,
            window_relative: false,
            clean_names: true,
            fold_case: false,
            drop_roles: Vec::new(),
            drop_names: Vec::new(),
            drop_scrollbars: false,
            drop_frame_controls: false,
            drop_shell: false,
        }
    }
}

/// Produce the comparable form of a tree. Paths in the result refer to the result.
pub fn normalize(tree: &SemTree, opts: &NormalizeOptions) -> SemTree {
    let desktop = tree.root.bounds;
    let mut root = tree.root.clone();
    canonicalize(&mut root, opts);
    if opts.prune_invisible {
        prune(&mut root, &desktop, opts, true);
    } else {
        prune(&mut root, &desktop, opts, false);
    }
    if opts.collapse_wrappers {
        collapse(&mut root);
    }
    if opts.flatten {
        root = flatten(root);
    }
    if opts.drop_frame_controls {
        drop_frame(&mut root);
    }
    if opts.drop_shell {
        root.children
            .retain(|w| w.attrs.get("app").is_none_or(|a| a != "shell"));
    }
    dedup(&mut root);
    if opts.window_relative {
        relativize(&mut root, 0, 0);
    }
    if let Some(band) = opts.reading_order_band {
        order(&mut root, band);
    }
    SemTree {
        root,
        focus: None,
        active_window: None,
    }
}

fn canonicalize(n: &mut SemNode, opts: &NormalizeOptions) {
    n.role = canonical_role(&n.role).to_owned();
    if opts.clean_names {
        n.name = normalize_name(&n.name);
        if let Some(v) = &n.value {
            let v2 = v.trim().to_owned();
            n.value = if v2.is_empty() { None } else { Some(v2) };
        }
    }
    if opts.fold_case {
        n.name = n.name.to_lowercase();
    }
    for c in &mut n.children {
        canonicalize(c, opts);
    }
}

fn dropped(n: &SemNode, desktop: &Rect, opts: &NormalizeOptions, visibility: bool) -> bool {
    if opts.drop_roles.iter().any(|r| r == &n.role) {
        return true;
    }
    if opts
        .drop_names
        .iter()
        .any(|p| crate::text::glob_matches(p, &n.name))
    {
        return true;
    }
    if opts.drop_scrollbars && n.role == "scrollbar" {
        return true;
    }
    if opts.drop_empty_text
        && matches!(n.role.as_str(), "text" | "label")
        && n.name.is_empty()
        && n.value.is_none()
    {
        return true;
    }
    if visibility {
        if !n.states.visible {
            return true;
        }
        if n.role != "desktop" && n.role != "application" {
            if n.bounds.is_empty() {
                return true;
            }
            if n.bounds.intersection(desktop).is_none() {
                return true;
            }
        }
    }
    false
}

fn prune(n: &mut SemNode, desktop: &Rect, opts: &NormalizeOptions, visibility: bool) {
    n.children
        .retain(|c| !dropped(c, desktop, opts, visibility));
    for c in &mut n.children {
        prune(c, desktop, opts, visibility);
    }
}

fn collapse(n: &mut SemNode) {
    let mut out: Vec<SemNode> = Vec::with_capacity(n.children.len());
    for mut c in std::mem::take(&mut n.children) {
        collapse(&mut c);
        if is_wrapper(&c.role) && c.name.is_empty() && c.value.is_none() {
            out.extend(c.children);
        } else {
            out.push(c);
        }
    }
    n.children = out;
}

/// Windows under the desktop; under each window, its interactive controls and texts
/// as leaves. A menu keeps its items as children, so an open menu stays one unit.
fn flatten(root: SemNode) -> SemNode {
    fn leaves(n: SemNode, out: &mut Vec<SemNode>) {
        let keep = is_interactive(&n.role)
            || matches!(
                n.role.as_str(),
                "text" | "label" | "heading" | "img" | "menubar" | "toolbar" | "tablist"
            );
        if keep {
            let mut node = n;
            let children = std::mem::take(&mut node.children);
            if matches!(
                node.role.as_str(),
                "menu" | "menubar" | "toolbar" | "tablist" | "combobox"
            ) {
                let mut sub = Vec::new();
                for c in children {
                    leaves(c, &mut sub);
                }
                node.children = sub;
            } else {
                // Text inside a control names it when the control itself has no name.
                if node.name.is_empty() {
                    if let Some(t) = children
                        .iter()
                        .find(|c| matches!(c.role.as_str(), "text" | "label") && !c.name.is_empty())
                    {
                        node.name = t.name.clone();
                    }
                }
                let mut sub = Vec::new();
                for c in children {
                    leaves(c, &mut sub);
                }
                // Controls nested in a control (a cell's editor) stay beside it.
                out.push(node);
                out.extend(sub);
                return;
            }
            out.push(node);
        } else {
            for c in n.children {
                leaves(c, out);
            }
        }
    }
    fn windows(n: SemNode, out: &mut Vec<SemNode>) {
        if is_window_role(&n.role) {
            let mut w = n;
            let children = std::mem::take(&mut w.children);
            let mut flat = Vec::new();
            let mut nested = Vec::new();
            for c in children {
                if is_window_role(&c.role) {
                    windows(c, &mut nested);
                } else {
                    leaves(c, &mut flat);
                }
            }
            w.children = flat;
            out.push(w);
            out.extend(nested);
        } else {
            for c in n.children {
                windows(c, out);
            }
        }
    }
    let mut desktop = root;
    let children = std::mem::take(&mut desktop.children);
    let mut ws = Vec::new();
    let mut loose = Vec::new();
    for c in children {
        if is_window_role(&c.role)
            || c.role == "application"
            || c.role == "panel"
            || c.role == "unknown"
        {
            let mut found = Vec::new();
            windows(c.clone(), &mut found);
            if found.is_empty() {
                leaves(c, &mut loose);
            } else {
                ws.extend(found);
            }
        } else {
            leaves(c, &mut loose);
        }
    }
    // Controls that belong to no window (a shell's own taskbar) form a `shell` window.
    if !loose.is_empty() {
        let mut shell = SemNode::new("window", "", desktop.bounds);
        shell.attrs.insert("app".into(), "shell".into());
        shell.children = loose;
        ws.insert(0, shell);
    }
    desktop.children = ws;
    desktop
}

/// Names a window manager gives its own controls, on any platform.
pub fn is_frame_control(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("resize ")
        || n.starts_with("move ")
        || n == "minimize"
        || n == "minimise"
        || n == "maximize"
        || n == "maximise"
        || n == "restore"
        || n == "close"
        || n.starts_with("minimize window")
        || n.starts_with("maximize or restore")
        || n.starts_with("close window")
        || n == "window menu"
}

/// A node from a source without layout (an rrweb recording): its bounds are a stand-in.
pub fn no_geometry(n: &SemNode) -> bool {
    n.attrs.get("geometry").is_some_and(|g| g == "none")
}

fn drop_frame(n: &mut SemNode) {
    for w in &mut n.children {
        if !is_window_role(&w.role) {
            continue;
        }
        let wb = w.bounds;
        w.children.retain(|c| {
            if is_window_role(&c.role) || !c.children.is_empty() {
                return true;
            }
            if is_interactive(&c.role) && is_frame_control(&c.name) {
                return false;
            }
            if no_geometry(c) {
                return true;
            }
            // A control the size of its window is the window's own focus surface.
            let covers = !wb.is_empty()
                && c.bounds.area() as f32 >= 0.95 * wb.area() as f32
                && c.bounds.iou(&wb) > 0.9;
            !(is_interactive(&c.role) && covers)
        });
    }
}

/// A label painted several times for a shadow or an outline is one label: drop a sibling
/// with the same role and name whose bounds all but coincide with an earlier one's. A
/// text or label that sits inside a control and repeats its name is the control's own
/// caption: drop it too.
fn dedup(n: &mut SemNode) {
    let controls: Vec<(String, Rect)> = n
        .children
        .iter()
        .filter(|c| is_interactive(&c.role) && !c.name.is_empty())
        .map(|c| (c.name.clone(), c.bounds))
        .collect();
    let mut kept: Vec<SemNode> = Vec::with_capacity(n.children.len());
    for c in std::mem::take(&mut n.children) {
        let dup = c.children.is_empty()
            && !no_geometry(&c)
            && kept.iter().any(|k| {
                k.children.is_empty()
                    && k.role == c.role
                    && k.name == c.name
                    && k.value == c.value
                    && k.bounds.iou(&c.bounds) > 0.8
            });
        let caption = c.children.is_empty()
            && !no_geometry(&c)
            && matches!(c.role.as_str(), "text" | "label")
            && controls.iter().any(|(name, b)| {
                *name == c.name
                    && b.intersection(&c.bounds)
                        .is_some_and(|i| i.area() * 10 >= c.bounds.area() * 9)
            });
        if !dup && !caption {
            kept.push(c);
        }
    }
    n.children = kept;
    for c in &mut n.children {
        dedup(c);
    }
}

fn relativize(n: &mut SemNode, ox: i32, oy: i32) {
    let (nx, ny) = if is_window_role(&n.role) {
        (n.bounds.x, n.bounds.y)
    } else {
        (ox, oy)
    };
    for c in &mut n.children {
        relativize(c, nx, ny);
    }
    if !is_window_role(&n.role) && n.role != "desktop" {
        n.bounds = n.bounds.translate(-ox, -oy);
    }
}

fn order(n: &mut SemNode, band: i32) {
    for c in &mut n.children {
        order(c, band);
    }
    // Windows keep stacking order; menus keep their item order.
    if n.role == "desktop" || n.role == "menu" || n.role == "menubar" {
        return;
    }
    let b = band.max(1);
    n.children
        .sort_by_key(|c| (c.bounds.y.div_euclid(b), c.bounds.x, c.bounds.y));
}

/// What a node's hash covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashFields {
    /// Role and children only: the shape of the tree.
    Structure,
    /// Role, name, value and states, and children.
    Content,
    /// Content plus bounds: byte-for-byte sameness of the normalized tree.
    Full,
}

pub fn node_hash(n: &SemNode, fields: HashFields) -> u64 {
    let mut h = Fnv::default();
    h.str(&n.role);
    if fields != HashFields::Structure {
        h.str(&n.name);
        h.str(n.value.as_deref().unwrap_or(""));
        let s = &n.states;
        h.bytes(&[
            s.enabled as u8,
            s.focusable as u8,
            s.visible as u8,
            s.checked.map_or(2, u8::from),
            s.selected.map_or(2, u8::from),
            s.expanded.map_or(2, u8::from),
        ]);
    }
    if fields == HashFields::Full {
        h.i32(n.bounds.x)
            .i32(n.bounds.y)
            .i32(n.bounds.w)
            .i32(n.bounds.h);
    }
    for c in &n.children {
        h.u64(node_hash(c, fields));
    }
    h.finish()
}

pub fn tree_hash_hex(t: &SemTree, fields: HashFields) -> String {
    format!("{:016x}", node_hash(&t.root, fields))
}

/// Every window of a normalized tree, with its path.
pub fn windows(t: &SemTree) -> Vec<(NodePath, &SemNode)> {
    t.root
        .walk()
        .into_iter()
        .filter(|(_, n)| is_window_role(&n.role))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::States;

    fn leaf(role: &str, name: &str, r: Rect) -> SemNode {
        SemNode::new(role, name, r)
    }

    #[test]
    fn roles_map() {
        assert_eq!(canonical_role("push button"), "button");
        assert_eq!(
            canonical_role("ROLE_CHECK_MENU_ITEM".trim_start_matches("ROLE_")),
            "menuitem"
        );
        assert_eq!(canonical_role("textbox"), "textbox");
        assert_eq!(canonical_role("page tab"), "tab");
        assert_eq!(canonical_role("whatever"), "unknown");
    }

    #[test]
    fn flattening_keeps_controls_under_windows() {
        let mut t = SemTree::desktop(800, 600);
        let mut app = SemNode::new("application", "KiCad", Rect::default());
        let mut win = SemNode::new("frame", "Schematic Editor", Rect::new(10, 10, 600, 400));
        let mut panel = SemNode::new("filler", "", Rect::new(10, 10, 600, 400));
        let mut toolbar = SemNode::new("tool bar", "", Rect::new(10, 40, 600, 30));
        toolbar
            .children
            .push(leaf("push button", "Save", Rect::new(12, 42, 24, 24)));
        toolbar
            .children
            .push(leaf("push button", "Open...", Rect::new(40, 42, 24, 24)));
        let mut hidden = leaf("push button", "Hidden", Rect::new(70, 42, 24, 24));
        hidden.states.visible = false;
        toolbar.children.push(hidden);
        panel.children.push(toolbar);
        panel
            .children
            .push(leaf("scroll bar", "", Rect::new(600, 40, 10, 300)));
        panel
            .children
            .push(leaf("text", "Hello", Rect::new(20, 100, 50, 12)));
        win.children.push(panel);
        app.children.push(win);
        t.root.children.push(app);
        let n = normalize(&t, &NormalizeOptions::default());
        assert_eq!(n.root.children.len(), 1);
        let w = &n.root.children[0];
        assert_eq!(w.role, "window");
        assert_eq!(w.name, "Schematic Editor");
        let roles: Vec<&str> = w.children.iter().map(|c| c.role.as_str()).collect();
        assert_eq!(roles, vec!["toolbar", "text"]);
        let names: Vec<&str> = w.children[0]
            .children
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, vec!["Save", "Open"]);
        // Window-relative bounds.
        assert_eq!(w.children[0].children[0].bounds, Rect::new(2, 32, 24, 24));
    }

    #[test]
    fn hashes_distinguish_levels() {
        let a = leaf("button", "Save", Rect::new(0, 0, 10, 10));
        let mut b = a.clone();
        b.bounds = Rect::new(1, 0, 10, 10);
        assert_eq!(
            node_hash(&a, HashFields::Content),
            node_hash(&b, HashFields::Content)
        );
        assert_ne!(
            node_hash(&a, HashFields::Full),
            node_hash(&b, HashFields::Full)
        );
        let mut c = a.clone();
        c.name = "Open".into();
        assert_eq!(
            node_hash(&a, HashFields::Structure),
            node_hash(&c, HashFields::Structure)
        );
        assert_ne!(
            node_hash(&a, HashFields::Content),
            node_hash(&c, HashFields::Content)
        );
        let mut d = a.clone();
        d.states = States {
            checked: Some(true),
            ..d.states
        };
        assert_ne!(
            node_hash(&a, HashFields::Content),
            node_hash(&d, HashFields::Content)
        );
    }

    #[test]
    fn shadow_copies_of_a_label_are_one() {
        let mut t = SemTree::desktop(100, 100);
        let mut w = SemNode::new("window", "w", Rect::new(0, 0, 100, 100));
        w.children
            .push(leaf("text", "Home", Rect::new(10, 10, 38, 21)));
        w.children
            .push(leaf("text", "Home", Rect::new(10, 11, 38, 21)));
        w.children
            .push(leaf("text", "Home", Rect::new(10, 12, 38, 21)));
        w.children
            .push(leaf("text", "Home", Rect::new(60, 10, 38, 21)));
        t.root.children.push(w);
        let n = normalize(&t, &NormalizeOptions::default());
        assert_eq!(n.root.children[0].children.len(), 2);
    }

    #[test]
    fn frame_controls_and_captions_drop() {
        let mut t = SemTree::desktop(300, 300);
        let mut w = SemNode::new("window", "Calc", Rect::new(10, 10, 200, 200));
        w.children
            .push(leaf("button", "Resize Calc nw", Rect::new(6, 6, 12, 12)));
        w.children
            .push(leaf("button", "Move Calc", Rect::new(11, 11, 198, 30)));
        w.children
            .push(leaf("button", "Calc", Rect::new(10, 10, 200, 200)));
        w.children
            .push(leaf("button", "Close window", Rect::new(180, 12, 24, 24)));
        w.children
            .push(leaf("button", "7", Rect::new(20, 60, 50, 40)));
        w.children
            .push(leaf("text", "7", Rect::new(40, 70, 10, 20)));
        w.children
            .push(leaf("text", "7", Rect::new(120, 70, 10, 20)));
        t.root.children.push(w);
        let n = normalize(&t, &NormalizeOptions::default());
        let names: Vec<(String, String)> = n.root.children[0]
            .children
            .iter()
            .map(|c| (c.role.clone(), c.name.clone()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("button".to_owned(), "7".to_owned()),
                ("text".to_owned(), "7".to_owned())
            ]
        );
    }

    #[test]
    fn reading_order_sorts_by_band_then_x() {
        let mut w = SemNode::new("window", "w", Rect::new(0, 0, 100, 100));
        w.children
            .push(leaf("button", "c", Rect::new(50, 20, 10, 10)));
        w.children
            .push(leaf("button", "a", Rect::new(0, 0, 10, 10)));
        w.children
            .push(leaf("button", "b", Rect::new(50, 3, 10, 10)));
        order(&mut w, 6);
        let names: Vec<&str> = w.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }
}
