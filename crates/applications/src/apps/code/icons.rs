//! The workbench's icons: VS Code's own codicons (render/assets/icons/vscode) through
//! [`glyph`], and vector paths on a 16 px grid for the few it draws itself.
use crate::desktop_scene::Painter;
use cw_scene::Color;

fn s(v: i32, size: u32) -> i32 {
    v * size as i32 / 16
}
/// Stroke a polyline given on the 16 px grid at (x, y).
fn stroke(p: &mut Painter, x: i32, y: i32, size: u32, pts: &[(i32, i32)], c: Color) {
    let points = pts
        .iter()
        .map(|(a, b)| (x + s(*a, size), y + s(*b, size)))
        .collect();
    p.line(points, c, 1);
}
fn fill(p: &mut Painter, x: i32, y: i32, size: u32, pts: &[(i32, i32)], c: Color) {
    let points = pts
        .iter()
        .map(|(a, b)| (x + s(*a, size), y + s(*b, size)))
        .collect();
    p.path(points, c);
}

/// Explorer: the `files` codicon.
pub fn files(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "files", x, y, size, c);
}
/// Source Control: the `source-control` codicon.
pub fn scm(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "source-control", x, y, size, c);
}
/// Run and Debug: the `debug-alt` codicon.
pub fn debug(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "debug-alt", x, y, size, c);
}
pub fn play(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    fill(p, x, y, size, &[(4, 2), (13, 8), (4, 14)], c);
}
/// Step Over: an arc that hops over the line's own call, with the dot it lands on.
pub fn step_over(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(
        p,
        x,
        y,
        size,
        &[(2, 9), (3, 5), (6, 3), (10, 3), (13, 5), (14, 9)],
        c,
    );
    fill(p, x, y, size, &[(11, 8), (14, 12), (17, 8)], c);
    fill(p, x, y, size, &[(6, 10), (10, 10), (10, 14), (6, 14)], c);
}
/// Step Into: an arrow going down into the dot.
pub fn step_into(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(p, x, y, size, &[(8, 1), (8, 8)], c);
    fill(p, x, y, size, &[(5, 7), (8, 11), (11, 7)], c);
    fill(p, x, y, size, &[(6, 12), (10, 12), (10, 16), (6, 16)], c);
}
/// Step Out: an arrow leaving the dot.
pub fn step_out(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(p, x, y, size, &[(8, 11), (8, 4)], c);
    fill(p, x, y, size, &[(5, 5), (8, 1), (11, 5)], c);
    fill(p, x, y, size, &[(6, 12), (10, 12), (10, 16), (6, 16)], c);
}
/// Stop: a filled square, as every debugger draws it.
pub fn stop(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    fill(p, x, y, size, &[(3, 3), (13, 3), (13, 13), (3, 13)], c);
}
pub fn new_file(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "new-file", x, y, size, c);
}
pub fn new_folder(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "new-folder", x, y, size, c);
}
pub fn collapse(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "collapse-all", x, y, size, c);
}
pub fn error(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "error", x, y, size, c);
}
pub fn warning(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "warning", x, y, size, c);
}
pub fn arrow(p: &mut Painter, x: i32, y: i32, size: u32, up: bool, c: Color) {
    glyph(p, if up { "arrow-up" } else { "arrow-down" }, x, y, size, c);
}
pub fn replace_all(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "replace-all", x, y, size, c);
}
pub fn replace_one(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "replace", x, y, size, c);
}
/// Layout toggles: the `layout-sidebar-left` and `layout-panel` codicons, `-off` while
/// the pane is hidden.
pub fn layout(p: &mut Painter, x: i32, y: i32, size: u32, panel: bool, on: bool, c: Color) {
    // 1.143's codicons: the pane it toggles filled while shown.
    let name = match (panel, on) {
        (true, true) => "layout-panel",
        (true, false) => "layout-panel-off",
        (false, true) => "layout-sidebar-left",
        (false, false) => "layout-sidebar-left-off",
    };
    glyph(p, name, x, y, size, c);
}
/// The secondary side bar's toggle: `layout-sidebar-right-off`.
pub fn layout_secondary(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "layout-sidebar-right-off", x, y, size, c);
}
/// Customize Layout: the `layout` codicon.
pub fn customize_layout(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "layout", x, y, size, c);
}
/// The Extensions view: the `extensions` codicon.
pub fn extensions(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    glyph(p, "extensions", x, y, size, c);
}
/// The Visual Studio Code mark: a ribbon folded into a chevron, in three blues.
pub fn logo(p: &mut Painter, x: i32, y: i32, size: u32, mono: Option<Color>) {
    let pt = |a: i32, b: i32| (x + a * size as i32 / 100, y + b * size as i32 / 100);
    let (back, mid, front) = match mono {
        Some(c) => (c, c, c),
        None => (
            Color::rgb(0, 101, 169),
            Color::rgb(0, 122, 204),
            Color::rgb(31, 156, 240),
        ),
    };
    p.path(vec![pt(4, 36), pt(12, 29), pt(74, 77), pt(74, 94)], back);
    p.path(vec![pt(4, 64), pt(12, 71), pt(74, 23), pt(74, 6)], mid);
    p.path(vec![pt(70, 3), pt(96, 16), pt(96, 84), pt(70, 97)], front);
    if mono.is_none() {
        p.path(
            vec![pt(70, 3), pt(76, 6), pt(76, 94), pt(70, 97)],
            Color(0, 0, 0, 40),
        );
    }
}

/// VS Code's codicons the renderer bundles (render/assets/icons/vscode), by their own
/// names; the activity bar's also come at 24 px.
const CODICONS: &[&str] = &[
    "account",
    "add",
    "arrow-down",
    "arrow-left",
    "arrow-right",
    "arrow-up",
    "bell",
    "case-sensitive",
    "check",
    "chevron-down",
    "chevron-right",
    "chevron-up",
    "chrome-close",
    "chrome-maximize",
    "chrome-minimize",
    "chrome-restore",
    "circle-filled",
    "close",
    "close-small",
    "cloud",
    "cloud-upload",
    "collapse-all",
    "debug-alt",
    "debug-pause",
    "discard",
    "ellipsis",
    "error",
    "extensions",
    "eye",
    "file",
    "files",
    "folder",
    "folder-opened",
    "git-branch",
    "git-branch-changes",
    "git-commit",
    "git-compare",
    "git-fetch",
    "go-to-file",
    "info",
    "layout",
    "layout-panel",
    "layout-panel-off",
    "layout-sidebar-left",
    "layout-sidebar-left-off",
    "layout-sidebar-right-off",
    "light-bulb",
    "list-selection",
    "new-file",
    "new-folder",
    "open-preview",
    "preserve-case",
    "preview",
    "refresh",
    "regex",
    "remote",
    "remove",
    "replace",
    "replace-all",
    "repo-pull",
    "repo-push",
    "screen-full",
    "search",
    "search-large",
    "settings-gear",
    "source-control",
    "split-horizontal",
    "star-full",
    "target",
    "terminal-bash",
    "trash",
    "warning",
    "whole-word",
];
const CODICONS_24: &[&str] = &[
    "account",
    "debug-alt",
    "extensions",
    "files",
    "search-large",
    "settings-gear",
    "source-control",
];

/// The codicon VS Code draws where the simulator's own symbol set names `symbol`.
fn codicon_for(symbol: &str) -> Option<&'static str> {
    let name = match symbol {
        "person" => "account",
        "gear" => "settings-gear",
        "reload" => "refresh",
        "minus" => "remove",
        "undo" => "discard",
        "plus" => "add",
        "branch" => "git-branch",
        "split" => "split-horizontal",
        "star" => "star-full",
        "select-rect" => "list-selection",
        "terminal" => "terminal-bash",
        "eye-off" => "eye",
        "pause" => "debug-pause",
        "more" => "ellipsis",
        "document" => "go-to-file",
        other => other,
    };
    CODICONS.iter().copied().find(|c| *c == name)
}

/// Draw `symbol` as VS Code does, with its codicon at the size the page sets it in (24 px
/// in the activity bar, 16 px elsewhere), centred in the `size` box a caller lays out; a
/// symbol with no codicon keeps the simulator's own.
pub fn glyph(p: &mut Painter, symbol: &str, x: i32, y: i32, size: u32, c: Color) {
    let Some(name) = codicon_for(symbol) else {
        p.symbol(symbol, x, y, size, c);
        return;
    };
    let px: u32 = if size >= 20 && CODICONS_24.contains(&name) {
        24
    } else {
        16
    };
    let (dx, dy) = ((size as i32 - px as i32) / 2, (size as i32 - px as i32) / 2);
    p.symbol(&format!("codicon-{name}-{px}"), x + dx, y + dy, px, c);
}
