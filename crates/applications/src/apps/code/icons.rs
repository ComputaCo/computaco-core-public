//! Codicon-style line icons for the workbench, drawn as vector paths where the bundled
//! symbol set has no equivalent. All sizes are in pixels on a 16 px design grid.
use crate::desktop_scene::Painter;
use cw_scene::{Color, Rect};

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
fn ring(p: &mut Painter, cx: i32, cy: i32, r: u32, c: Color) {
    p.ring(cx, cy, r, 1, c);
}

/// Explorer: two overlapping documents.
pub fn files(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(
        p,
        x,
        y,
        size,
        &[(5, 1), (11, 1), (15, 5), (15, 13), (5, 13), (5, 1)],
        c,
    );
    stroke(p, x, y, size, &[(11, 1), (11, 5), (15, 5)], c);
    stroke(
        p,
        x,
        y,
        size,
        &[(3, 4), (1, 4), (1, 16), (10, 16), (10, 14)],
        c,
    );
}
/// Source Control: a branch with three commits.
pub fn scm(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    let r = (size / 8).max(2);
    ring(p, x + s(4, size), y + s(3, size), r, c);
    ring(p, x + s(4, size), y + s(13, size), r, c);
    ring(p, x + s(12, size), y + s(5, size), r, c);
    stroke(p, x, y, size, &[(4, 5), (4, 11)], c);
    stroke(p, x, y, size, &[(12, 7), (12, 8), (9, 10), (5, 11)], c);
}
/// Run and Debug: a play triangle with a small bug.
pub fn debug(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(p, x, y, size, &[(3, 1), (13, 7), (3, 13), (3, 1)], c);
    ring(p, x + s(12, size), y + s(12, size), (size / 6).max(2), c);
    stroke(p, x, y, size, &[(9, 10), (8, 9)], c);
    stroke(p, x, y, size, &[(15, 10), (16, 9)], c);
    stroke(p, x, y, size, &[(12, 14), (12, 16)], c);
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
    stroke(
        p,
        x,
        y,
        size,
        &[(8, 14), (3, 14), (3, 1), (9, 1), (13, 5), (13, 8)],
        c,
    );
    stroke(p, x, y, size, &[(9, 1), (9, 5), (13, 5)], c);
    stroke(p, x, y, size, &[(12, 10), (12, 16)], c);
    stroke(p, x, y, size, &[(9, 13), (15, 13)], c);
}
pub fn new_folder(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(
        p,
        x,
        y,
        size,
        &[(8, 13), (1, 13), (1, 2), (6, 2), (8, 4), (15, 4), (15, 8)],
        c,
    );
    stroke(p, x, y, size, &[(12, 9), (12, 15)], c);
    stroke(p, x, y, size, &[(9, 12), (15, 12)], c);
}
pub fn collapse(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(
        p,
        x,
        y,
        size,
        &[(4, 4), (14, 4), (14, 14), (4, 14), (4, 4)],
        c,
    );
    stroke(p, x, y, size, &[(2, 12), (2, 2), (12, 2)], c);
    stroke(p, x, y, size, &[(7, 9), (11, 9)], c);
}
pub fn error(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    ring(p, x + s(8, size), y + s(8, size), size * 7 / 16, c);
    stroke(p, x, y, size, &[(5, 5), (11, 11)], c);
    stroke(p, x, y, size, &[(11, 5), (5, 11)], c);
}
pub fn warning(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(p, x, y, size, &[(8, 1), (15, 14), (1, 14), (8, 1)], c);
    stroke(p, x, y, size, &[(8, 6), (8, 10)], c);
    stroke(p, x, y, size, &[(8, 12), (8, 12)], c);
    p.box_(Rect::new(x + s(8, size) - 1, y + s(12, size), 2, 2), c, 1);
}
pub fn arrow(p: &mut Painter, x: i32, y: i32, size: u32, up: bool, c: Color) {
    if up {
        stroke(p, x, y, size, &[(8, 14), (8, 2)], c);
        stroke(p, x, y, size, &[(3, 7), (8, 2), (13, 7)], c);
    } else {
        stroke(p, x, y, size, &[(8, 2), (8, 14)], c);
        stroke(p, x, y, size, &[(3, 9), (8, 14), (13, 9)], c);
    }
}
pub fn replace_all(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(p, x, y, size, &[(2, 2), (8, 2), (8, 7), (2, 7), (2, 2)], c);
    stroke(
        p,
        x,
        y,
        size,
        &[(8, 9), (14, 9), (14, 14), (8, 14), (8, 9)],
        c,
    );
    stroke(p, x, y, size, &[(11, 3), (13, 3), (13, 6)], c);
}
pub fn replace_one(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    stroke(
        p,
        x,
        y,
        size,
        &[(8, 9), (14, 9), (14, 14), (8, 14), (8, 9)],
        c,
    );
    stroke(p, x, y, size, &[(3, 3), (3, 11), (6, 11)], c);
}
/// Layout toggles: a window outline with the side bar or the panel filled when shown.
pub fn layout(p: &mut Painter, x: i32, y: i32, size: u32, panel: bool, on: bool, c: Color) {
    // 1.143's codicons: a rounded frame, the pane it toggles filled while shown.
    let u = |v: i32| s(v, size);
    p.border(
        Rect::new(x + u(1), y + u(1), u(14) as u32, u(14) as u32),
        Color::TRANSPARENT,
        2,
        c,
    );
    let r = if panel {
        Rect::new(x + u(1), y + u(9), u(14) as u32, u(6) as u32)
    } else {
        Rect::new(x + u(1), y + u(1), u(6) as u32, u(14) as u32)
    };
    if on {
        p.box_(r, c, 0);
    } else if panel {
        p.box_(Rect::new(x + u(1), y + u(9), u(14) as u32, 1), c, 0);
    } else {
        p.box_(Rect::new(x + u(6), y + u(1), 1, u(14) as u32), c, 0);
    }
}
/// The secondary side bar's toggle: the frame, split near its right edge.
pub fn layout_secondary(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    let u = |v: i32| s(v, size);
    p.border(
        Rect::new(x + u(1), y + u(1), u(14) as u32, u(14) as u32),
        Color::TRANSPARENT,
        2,
        c,
    );
    p.box_(Rect::new(x + u(10), y + u(1), 1, u(14) as u32), c, 0);
}
/// Customize Layout: a tall pane beside two short ones.
pub fn customize_layout(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    let u = |v: i32| s(v, size);
    for (a, b, w, h) in [(1, 1, 6, 14), (9, 1, 6, 6), (9, 9, 6, 6)] {
        p.border(
            Rect::new(x + u(a), y + u(b), u(w) as u32, u(h) as u32),
            Color::TRANSPARENT,
            1,
            c,
        );
    }
}
/// The Extensions view's codicon: three squares and a fourth turned on its corner.
pub fn extensions(p: &mut Painter, x: i32, y: i32, size: u32, c: Color) {
    let u = |v: i32| s(v, size);
    for (a, b) in [(3, 6), (3, 14), (11, 14)] {
        p.border(
            Rect::new(x + u(a), y + u(b), u(8) as u32 + 1, u(8) as u32 + 1),
            Color::TRANSPARENT,
            1,
            c,
        );
    }
    let (cx, cy, d) = (x + u(17), y + u(7), u(5));
    p.line(
        vec![
            (cx, cy - d),
            (cx + d, cy),
            (cx, cy + d),
            (cx - d, cy),
            (cx, cy - d),
        ],
        c,
        2,
    );
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
/// A file's type, the way the Seti icon theme marks it: a short coloured glyph.
#[cfg(feature = "code")]
pub fn file_glyph(ext_lang: super::syntax::Language, name: &str) -> (&'static str, Color) {
    use super::syntax::Language as L;
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("readme") {
        return ("i", Color::rgb(81, 154, 186));
    }
    if lower == ".gitignore" {
        return ("◆", Color::rgb(65, 83, 91));
    }
    match ext_lang {
        L::Python => ("py", Color::rgb(81, 154, 186)),
        L::JavaScript => ("JS", Color::rgb(203, 203, 65)),
        L::TypeScript => ("TS", Color::rgb(81, 154, 186)),
        L::Rust => ("rs", Color::rgb(227, 121, 51)),
        L::Json => ("{}", Color::rgb(203, 203, 65)),
        L::Markdown => ("M↓", Color::rgb(81, 154, 186)),
        L::Html => ("<>", Color::rgb(227, 121, 51)),
        L::Css => ("#", Color::rgb(81, 154, 186)),
        L::Shell => (">_", Color::rgb(141, 193, 73)),
        L::PlainText => ("≡", Color::rgb(109, 128, 134)),
    }
}
