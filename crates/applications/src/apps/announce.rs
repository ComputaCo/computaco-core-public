//! Controls announced as a toolkit's AT-SPI announces them: a role, a name, a value and
//! the bounds it reports, which need not be where the control is drawn (LibreOffice's
//! toolbars report their buttons 27 px above them). The Ubuntu applications the window
//! manager decorates (LibreOffice, DB Browser for SQLite, Kdenlive) are laid out with these
//! from the real ones' trees (fidelity/tours).
use crate::desktop_scene::shared::Align;
use crate::desktop_scene::Painter;
use cw_scene::{Color, Rect, Semantic};

/// A control at `r`: live when `action` is `Ok` (its target), insensitive when it is `Err`
/// (an action the application refuses), announced as `role` with `name` and `value`.
pub fn control(
    p: &mut Painter,
    r: Rect,
    role: &str,
    name: &str,
    value: Option<&str>,
    action: Result<&str, &str>,
) {
    match action {
        Ok(target) => p.region(r, target, name),
        Err(off) => p.insensitive(r, off, name),
    }
    p.announce(role, value);
}
/// A node that takes a click and does nothing with it (`action` is one the application
/// accepts and ignores): a menu bar, tool bar or tab list, which holds the controls inside
/// its bounds as AT-SPI nests them, or a canvas, a column header, a document. The simulator
/// lists only nodes with an action, labels and images.
pub fn bar(p: &mut Painter, r: Rect, action: &str, role: &str, name: &str) {
    p.region(r, action, name);
    p.announce(role, None);
}
/// A label: a node with no action of its own.
pub fn node(p: &mut Painter, r: Rect, role: &str, name: &str) {
    p.node(r, cw_scene::Primitive::Region, None);
    if let Some(n) = p.scene.nodes.last_mut() {
        n.semantic = Some(Semantic {
            role: role.into(),
            label: name.into(),
            value: None,
            disabled: false,
            focusable: false,
        });
    }
}
/// The last node announced as disabled, though it routes nothing either way.
pub fn disable_last(p: &mut Painter) {
    if let Some(s) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
        s.disabled = true;
        s.focusable = false;
    }
}
/// The last node checked (a check box or toggle button that is on) or not.
pub fn check_last(p: &mut Painter, on: bool) {
    if let Some(n) = p.scene.nodes.last_mut() {
        n.state.get_or_insert_with(Default::default).checked = Some(on);
    }
}
/// The last node holds keyboard focus (the current tab of a tab bar, as Qt marks it).
pub fn focus_last(p: &mut Painter) {
    if let Some(n) = p.scene.nodes.last_mut() {
        n.state.get_or_insert_with(Default::default).focused = true;
    }
}
/// Text drawn but not announced: the toolkit names the control it lies on, or does not
/// expose it at all (a status bar's fields).
#[allow(clippy::too_many_arguments)]
pub fn quiet(
    p: &mut Painter,
    x: i32,
    y: i32,
    width: u32,
    text: &str,
    size: u16,
    color: Color,
    bold: bool,
    align: Align,
) -> u32 {
    let w = p.label(x, y, width, text, size, color, bold, align);
    p.presentation();
    w
}
/// A glyph drawn but not announced.
pub fn glyph(p: &mut Painter, name: &str, x: i32, y: i32, size: u32, color: Color) {
    p.symbol(name, x, y, size, color);
    p.presentation();
}
