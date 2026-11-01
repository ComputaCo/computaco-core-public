//! Visual Studio Code's custom title bar, which replaces the platform's own on every
//! desktop: traffic lights on macOS, the menu bar and window buttons on Windows and
//! Linux, and the command center in the middle. Every control here forwards to the
//! window's content, so it reaches the same command the palette and shortcuts do.
use super::icons;
use crate::desktop_scene::{shared::Align, DesktopTheme, Painter, ShellContext, WindowView};
use cw_scene::{Color, Rect};

/// Height of the title bar; `window_content_rect_for_kind` starts the client below it.
pub const TITLE_H: u32 = 35;

pub struct TitleColors {
    pub bg: Color,
    pub fg: Color,
    pub border: Color,
    pub hover: Color,
}
pub fn title_colors(dark: bool, focused: bool) -> TitleColors {
    match (dark, focused) {
        (true, true) => TitleColors {
            bg: Color::rgb(0x18, 0x18, 0x18),
            fg: Color::rgb(0xCC, 0xCC, 0xCC),
            border: Color::rgb(0x2B, 0x2B, 0x2B),
            hover: Color(255, 255, 255, 26),
        },
        (true, false) => TitleColors {
            bg: Color::rgb(0x1F, 0x1F, 0x1F),
            fg: Color::rgb(0x9D, 0x9D, 0x9D),
            border: Color::rgb(0x2B, 0x2B, 0x2B),
            hover: Color(255, 255, 255, 26),
        },
        (false, focused) => TitleColors {
            bg: Color::rgb(0xF8, 0xF8, 0xF8),
            fg: if focused {
                Color::rgb(0x1E, 0x1E, 0x1E)
            } else {
                Color::rgb(0x8B, 0x94, 0x9E)
            },
            border: Color::rgb(0xE5, 0xE5, 0xE5),
            hover: Color(0, 0, 0, 20),
        },
    }
}

/// Width of the command center for a window this wide.
pub fn command_center_width(width: u32) -> u32 {
    // 38 % rounded up: 369 px in a 969 px window, as 1.143 lays it out.
    (width * 38).div_ceil(100).clamp(140, 600)
}
/// Where the command center's group starts: Go Back and Go Forward, then the search box,
/// centred in the title bar as VS Code centres them.
pub fn command_center_x(width: u32) -> i32 {
    (width.saturating_sub(command_center_width(width) + NAV_W) / 2) as i32
}
/// The width Go Back and Go Forward take before the command center.
const NAV_W: u32 = 66;
/// The menu bar's left edge, past the logo.
const MENU_X: i32 = 35;
/// Width of the menu bar's "More" entry, which holds the menus that do not fit.
const MORE_W: u32 = 39;
/// Menu bar titles that fit before the command center: (name, label, x from the window's
/// left edge, width). VS Code keeps the menu bar to the room the command center leaves
/// and folds the menus that do not fit into a "More" entry ([`menu_more`]). Shared with
/// the content, which drops each menu down beneath its title.
pub fn menu_bar(p: &Painter, width: u32) -> Vec<(&'static str, &'static str, i32, u32)> {
    let room = (command_center_x(width) - MENU_X - 9).max(0) as u32;
    let menus: Vec<(&str, &str, u32)> = super::commands::MENUS
        .iter()
        .filter(|(name, _, _)| *name != "manage")
        .map(|(name, label, _)| (*name, *label, menu_width(p, label)))
        .collect();
    let all: u32 = menus.iter().map(|m| m.2).sum();
    let mut x = MENU_X;
    let mut used = 0;
    let mut out = vec![];
    for (name, label, w) in menus {
        let reserve = if all <= room { 0 } else { MORE_W };
        if used + w + reserve > room {
            break;
        }
        out.push((name, label, x, w));
        // Each title overlaps the one before it by a pixel, as 1.143 lays them out.
        x += w as i32 - 1;
        used += w;
    }
    out
}
/// A menu title's width in VS Code 1.143's title bar (measured on Linux); another title,
/// by its text.
fn menu_width(p: &Painter, label: &str) -> u32 {
    match label {
        "File" => 38,
        "Edit" => 42,
        "Selection" => 77,
        "View" => 48,
        "Go" => 35,
        "Run" => 41,
        "Terminal" => 67,
        "Help" => 46,
        _ => p.measure(label, 13, false) + 16,
    }
}
/// Where the "More" entry sits when some menus did not fit: (x, width).
pub fn menu_more(p: &Painter, width: u32) -> Option<(i32, u32)> {
    let shown = menu_bar(p, width);
    let total = super::commands::MENUS
        .iter()
        .filter(|(name, _, _)| *name != "manage")
        .count();
    (shown.len() < total).then(|| {
        let x = shown
            .last()
            .map_or(MENU_X, |(_, _, x, w)| x + *w as i32 - 1);
        (x, MORE_W)
    })
}
/// Give the region just painted the role Chromium exposes for it, and say whether it is
/// enabled.
fn role(p: &mut Painter, role: &str, enabled: bool) {
    if let Some(s) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
        s.role = role.into();
        s.disabled = !enabled;
    }
}

pub fn title_bar(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView) {
    // The title bar is the page's, set in the page's font.
    let linux = ctx.theme == DesktopTheme::Ubuntu;
    in_page_font(p, linux, |p| title_bar_drawn(p, ctx, w));
}
fn title_bar_drawn(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView) {
    let r = w.rect;
    let colors = title_colors(w.dark_chrome, w.focused);
    let radius = crate::desktop_scene::corner_radius(ctx.theme, w.maximized).saturating_sub(1);
    let bar = Rect::new(r.x + 1, r.y + 1, r.width.saturating_sub(2), TITLE_H - 1);
    p.box_(bar, colors.bg, radius);
    p.box_(
        Rect::new(bar.x, r.y + 18, bar.width, TITLE_H - 18),
        colors.bg,
        0,
    );
    p.hline(bar.x, r.y + TITLE_H as i32 - 1, bar.width, colors.border);
    let mac = ctx.theme == DesktopTheme::Macos;
    // VS Code's own window buttons on Windows and Linux: three, 32 px each.
    let controls = if mac { 0 } else { 96 };
    p.region(
        Rect::new(r.x, r.y, r.width.saturating_sub(controls), TITLE_H),
        &w.action("drag"),
        &format!("Move {}", w.title),
    );
    let action = |name: &str| w.action(&format!("content:code:{name}"));
    #[cfg(feature = "os-macos")]
    let drawn = mac && {
        crate::desktop_scene::macos::traffic_lights(p, ctx, w, 4);
        true
    };
    #[cfg(not(feature = "os-macos"))]
    let drawn = false;
    if !drawn {
        icons::logo(p, r.x + 10, r.y + 9, 16, None);
        let open = w.chrome("menu").unwrap_or("");
        let menus = menu_bar(p, r.width);
        let more = menu_more(p, r.width);
        // The menu bar, as a menubar of menu items (the page's own, not a toolkit's), across
        // the room it has before Go Back.
        let end = more
            .map_or_else(
                || menus.last().map_or(MENU_X, |(_, _, x, w)| x + *w as i32),
                |(x, w)| x + w as i32,
            )
            .max(command_center_x(r.width) - 9);
        p.region(
            Rect::new(r.x + MENU_X, r.y, (end - MENU_X).max(1) as u32, TITLE_H),
            &action("ax:menubar"),
            "",
        );
        role(p, "menubar", true);
        for (name, label, x, width) in menus {
            let hit = Rect::new(r.x + x, r.y, width, TITLE_H);
            let pill = Rect::new(hit.x, r.y + 5, width, 24);
            if open == name || ctx.hovered(pill) {
                p.box_(pill, colors.hover, 5);
            }
            p.label(
                hit.x,
                pill.y + 4,
                width,
                label,
                13,
                colors.fg,
                false,
                Align::Center,
            );
            p.region(hit, &action(&format!("menu:{name}")), label);
            role(p, "menuitem", true);
        }
        if let Some((x, width)) = more {
            let hit = Rect::new(r.x + x, r.y, width, TITLE_H);
            // Open while it, or a menu it holds, is open.
            let shown: Vec<&str> = menu_bar(p, r.width).iter().map(|m| m.0).collect();
            let held = !open.is_empty() && open != "manage" && !shown.contains(&open);
            if held || ctx.hovered(Rect::new(hit.x, r.y + 5, width, 24)) {
                p.box_(Rect::new(hit.x, r.y + 5, width, 24), colors.hover, 5);
            }
            for i in 0..3 {
                p.circle(hit.x + 14 + i * 5, r.y + 17, 1, colors.fg);
            }
            p.region(hit, &action("menu:more"), "More");
            role(p, "menuitem", true);
        }
    }
    // The command center: what the window is working on, and the way into Quick Open,
    // after Go Back and Go Forward, the three as one tool bar.
    let cw = command_center_width(r.width);
    let gx = r.x + command_center_x(r.width);
    // Quick input opens over the command center, which the page then hides.
    let quick = w.chrome("quick") == Some("1");
    let center = p.scene.nodes.len();
    p.region(
        Rect::new(gx, r.y + 5, cw + NAV_W, 25),
        &action("ax:command-center"),
        "",
    );
    role(p, "toolbar", true);
    for (i, (key, name, glyph, enabled)) in [
        (
            "back",
            "Go Back (Ctrl+Alt+-)",
            "arrow-left",
            w.chrome("back") == Some("1"),
        ),
        ("forward", "Go Forward (Ctrl+Shift+-)", "arrow-right", false),
    ]
    .into_iter()
    .enumerate()
    {
        let b = Rect::new(gx + i as i32 * 26, r.y + 6, 23, 23);
        let c = if enabled {
            colors.fg
        } else {
            Color(colors.fg.0, colors.fg.1, colors.fg.2, 90)
        };
        p.symbol(glyph, b.x + 4, b.y + 4, 15, c);
        p.region(b, &action(&format!("ax:{key}")), name);
        role(p, "button", enabled);
    }
    let cc = Rect::new(gx + NAV_W as i32 - 7, r.y + 6, cw, 23);
    p.region(cc, &action("ax:search-box"), "");
    role(p, "toolbar", true);
    let (fill, edge) = if w.dark_chrome {
        (Color(255, 255, 255, 13), Color(255, 255, 255, 30))
    } else {
        (Color(0, 0, 0, 5), Color(0, 0, 0, 34))
    };
    p.border(
        cc,
        if ctx.hovered(cc) { colors.hover } else { fill },
        6,
        edge,
    );
    let text = if w.caption.is_empty() {
        "Search".to_owned()
    } else {
        w.caption.clone()
    };
    // 1.143 sets the caption at the left in 13 px, with no search glyph.
    let tw = p.measure(&text, 13, false).min(cw.saturating_sub(18));
    let tx = cc.x + 9;
    p.label(
        tx,
        cc.y + 2,
        tw + 2,
        &text,
        13,
        colors.fg,
        false,
        Align::Left,
    );
    // The text is the button's own caption, not a label of the page's.
    if let Some(n) = p.scene.nodes.last_mut() {
        n.semantic = Some(cw_scene::Semantic {
            role: "presentation".into(),
            label: String::new(),
            value: None,
            disabled: false,
            focusable: false,
        });
    }
    let target = if w.caption.is_empty() {
        "cmd:workbench.action.showCommands"
    } else {
        "cmd:workbench.action.quickOpen"
    };
    p.region(cc, &action(target), &text);
    if quick {
        p.scene.nodes.truncate(center);
    }
    // Title actions: the layout toggles, which light the pane they show.
    let right = r.x + r.width as i32 - controls as i32 - 4;
    p.region(
        Rect::new(right - 100, r.y, 100, TITLE_H),
        &action("ax:title-actions"),
        "Title actions",
    );
    role(p, "toolbar", true);
    for (i, (pane, id, label)) in [
        (None, "ax:customize-layout", "Customize Layout"),
        (
            Some(false),
            "cmd:workbench.action.toggleSidebarVisibility",
            "Toggle Primary Side Bar (Ctrl+B)",
        ),
        (
            Some(true),
            "cmd:workbench.action.togglePanel",
            "Toggle Panel (Ctrl+J)",
        ),
        (
            None,
            "ax:toggle-secondary",
            "Toggle Secondary Side Bar (Ctrl+Alt+B)",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let hit = Rect::new(right - 100 + i as i32 * 26, r.y + 6, 22, 23);
        if ctx.hovered(hit) {
            p.box_(hit, colors.hover, 5);
        }
        match pane {
            Some(panel) => {
                let on = w.chrome(if panel { "panel" } else { "sidebar" }) == Some("1");
                icons::layout(p, hit.x + 3, hit.y + 3, 16, panel, on, colors.fg);
            }
            None if i == 0 => icons::customize_layout(p, hit.x + 3, hit.y + 3, 16, colors.fg),
            None => icons::layout_secondary(p, hit.x + 3, hit.y + 3, 16, colors.fg),
        }
        p.region(hit, &action(id), label);
    }
    if mac {
        return;
    }
    // VS Code draws its own window buttons on Windows and Linux.
    for (i, (verb, label)) in [
        ("minimize", "Minimize"),
        ("maximize", if w.maximized { "Restore" } else { "Maximize" }),
        ("close", "Close"),
    ]
    .into_iter()
    .enumerate()
    {
        let x = r.x + r.width as i32 - 1 - 96 + i as i32 * 32;
        let hit = Rect::new(x, r.y + 1, 32, TITLE_H - 2);
        let hovered = ctx.hovered(hit);
        let glyph = if hovered && verb == "close" {
            p.box_(hit, Color::rgb(0xE8, 0x11, 0x23), 0);
            Color::WHITE
        } else {
            if hovered {
                p.box_(hit, colors.hover, 0);
            }
            colors.fg
        };
        let (gx, gy) = (x + 11, r.y + 12);
        match verb {
            "minimize" => p.line(vec![(gx, gy + 5), (gx + 10, gy + 5)], glyph, 1),
            "maximize" if w.maximized => {
                p.border(
                    Rect::new(gx + 2, gy - 1, 8, 8),
                    Color::TRANSPARENT,
                    0,
                    glyph,
                );
                p.box_(Rect::new(gx, gy + 1, 8, 8), colors.bg, 0);
                p.border(Rect::new(gx, gy + 1, 8, 8), Color::TRANSPARENT, 0, glyph);
            }
            "maximize" => p.border(Rect::new(gx, gy, 10, 10), Color::TRANSPARENT, 0, glyph),
            _ => {
                p.line(vec![(gx, gy), (gx + 10, gy + 10)], glyph, 1);
                p.line(vec![(gx + 10, gy), (gx, gy + 10)], glyph, 1);
            }
        }
        p.region(hit, &w.action(verb), label);
    }
}

/// VS Code on Linux sets its UI in `system-ui`, which fontconfig resolves to DejaVu Sans on
/// the reference's Ubuntu (the reference machine: Chromium's text measures as DejaVu's to the pixel, about 10 %
/// wider than Ubuntu's): what `f` paints on Linux is measured and drawn so, whatever the
/// shell's own typeface. Shared by the page and its title bar. It lives here, not in render.rs, because the lazy engine
/// builds this file without the rest of the app (apps/code_chrome.rs).
pub fn in_page_font(p: &mut Painter, linux: bool, f: impl FnOnce(&mut Painter)) {
    if !linux {
        return f(p);
    }
    let shell_face = p.scene.typeface;
    let first = p.scene.nodes.len();
    p.scene.typeface = cw_scene::Typeface::DejaVu;
    f(p);
    for n in &mut p.scene.nodes[first..] {
        if let cw_scene::Primitive::UiText { typeface, .. }
        | cw_scene::Primitive::UiTextBold { typeface, .. } = &mut n.primitive
        {
            typeface.get_or_insert(cw_scene::Typeface::DejaVu);
        }
    }
    p.scene.typeface = shell_face;
}
