//! Ubuntu 24.04 / GNOME 46 presentation: 32 px top bar, 68 px Ubuntu Dock, 46 px
//! libadwaita header bars with circular window buttons, and Yaru's orange accent.
//! All controls that accept input dispatch Rust simulator actions.
use super::shared::{Align, Painter, ShellContext, WindowView};
use cw_scene::{Color, Rect};

const PANEL: Color = Color::rgb(19, 19, 19);
const INK: Color = Color::rgb(61, 61, 61);
const DIM: Color = Color::rgb(146, 146, 146);
const ORANGE: Color = Color::rgb(233, 84, 32);
const HEADER: Color = Color::rgb(235, 235, 235);
const HEADER_BACKDROP: Color = Color::rgb(242, 242, 242);
const DARK_HEADER: Color = Color::rgb(48, 48, 48);
const EOG_HEADER: Color = Color::rgb(36, 36, 36);
const EOG_CANVAS: Color = Color::rgb(44, 44, 44);
const LIGHT: Color = Color::rgb(246, 246, 246);
/// Yaru's light shell menus (Ubuntu 24.04's default style): the calendar and the system menu.
const MENU_BG: Color = Color::rgb(250, 250, 250);
const MENU_EDGE: Color = Color::rgb(222, 222, 222);
const MENU_BUTTON: Color = Color::rgb(227, 227, 227);
const MENU_BUTTON_HOVER: Color = Color::rgb(214, 214, 214);
const MENU_DIM: Color = Color::rgb(125, 125, 125);
const MENU_FAINT: Color = Color::rgb(165, 165, 165);
/// Firefox 159's tab strip and navigation toolbar (light theme), as the reference machine's Firefox draws them.
const FF_TABS_H: i32 = 44;
const FF_NAV_H: i32 = 40;
const FF_TABS_BG: Color = Color::rgb(240, 240, 244);
const FF_NAV_BG: Color = Color::rgb(249, 249, 251);
const FF_FIELD: Color = Color::rgb(240, 240, 244);
/// Width of the Files sidebar; shared with the Files client area.
pub(crate) use super::app_content::FILES_SIDEBAR;
/// Every application this shell can present, in Activities grid order.
/// This platform's applications in the order its shell shows them, from the catalog
/// (packages/catalog.json) this build compiled in.
const APPS: &[(&str, &str)] = crate::apps::catalog::SHELF2_UBUNTU;
/// The favourites the Ubuntu dock keeps, in 24.04's default order (Firefox,
/// Thunderbird, Files, Rhythmbox, LibreOffice Writer), then Terminal and Text Editor;
/// Activities carries the whole grid.
const DOCK: [&str; 8] = [
    "browser", "mail", "files", "music", "docs", "terminal", "editor", "code",
];

pub(super) fn app_name(kind: &str) -> Option<&'static str> {
    APPS.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n)
}

fn basename(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("Computer")
}

pub fn background(p: &mut Painter, ctx: &ShellContext<'_>) {
    p.asset(Rect::new(0, 0, ctx.width, ctx.height), "wallpaper/ubuntu");
    if ctx.width > 300 && ctx.height > 250 && ctx.installed("files") {
        // Desktop Icons NG lays icons out from the bottom-right corner, Home first: a
        // 68 px tile with a 64 px icon and its name under it (GNOME Shell 46, 1280x800).
        let x = ctx.width as i32 - 102;
        let top = ctx.height as i32 - 124;
        let slot = Rect::new(x, top, 68, 92);
        if ctx.selected("files") {
            p.box_(slot, Color(255, 255, 255, 64), 8);
        } else if ctx.hovered(slot) {
            p.box_(slot, Color(255, 255, 255, 40), 8);
        }
        p.platform_icon(
            Rect::new(x + 6, top + 6, 56, 56),
            "ubuntu",
            "files",
            "shell:open:files",
            "Open home folder",
        );
        // Yaru's home folder carries a house on it.
        p.symbol("home", x + 24, top + 26, 20, Color(255, 255, 255, 200));
        for (dy, alpha) in [(1, 170), (2, 70)] {
            p.label(
                x - 12,
                top + 70 + dy,
                92,
                "Home",
                14,
                Color(0, 0, 0, alpha),
                false,
                Align::Center,
            );
        }
        p.label(
            x - 12,
            top + 70,
            92,
            "Home",
            14,
            Color::WHITE,
            false,
            Align::Center,
        );
    }
}

pub fn chrome(p: &mut Painter, ctx: &ShellContext<'_>) {
    // A locked or powered-off display covers the session entirely, chrome included.
    if !ctx.awake() {
        shield(p, ctx);
        return;
    }
    // Super and Activities open the window overview; Show Applications (or Super again)
    // the app grid. Typing in either searches.
    if ctx.launcher_open || matches!(ctx.panel, Some("search" | "overview")) {
        activities(p, ctx);
    }
    top_bar(p, ctx);
    dock(p, ctx);
    if let Some(panel) = ctx.panel {
        if !matches!(panel, "search" | "overview") {
            panel_surface(p, ctx, panel);
        }
    }
}

/// GNOME's shield and the powered-off display. The only way back is a real wake.
fn shield(p: &mut Painter, ctx: &ShellContext<'_>) {
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    let (width, height) = (ctx.width as i32, ctx.height as i32);
    if ctx.screen == crate::ScreenState::Off {
        p.box_(full, Color::BLACK, 0);
        p.region(full, "shell:power:wake", "Turn the screen on");
        p.center(
            0,
            height / 2 - 8,
            ctx.width,
            "Screen off",
            13,
            Color::rgb(58, 58, 58),
        );
        return;
    }
    p.asset(full, "wallpaper/ubuntu");
    p.box_(full, Color(0, 0, 0, 178), 0);
    // Clicking anywhere raises the shield; GDM has no password here, there is no auth model.
    p.region(full, "shell:power:wake", "Unlock the session");
    let date = ctx.date();
    p.label(
        0,
        height / 4,
        ctx.width,
        &ctx.time(),
        64,
        Color::WHITE,
        true,
        Align::Center,
    );
    p.center(
        0,
        height / 4 + 96,
        ctx.width,
        &format!("{} {} {}", date.weekday_name(), date.day, date.month_name()),
        17,
        Color(255, 255, 255, 200),
    );
    p.symbol(
        "lock",
        width / 2 - 10,
        height / 2 + 40,
        20,
        Color(255, 255, 255, 190),
    );
    let button = Rect::new(width / 2 - 90, height / 2 + 80, 180, 40);
    p.button(
        button,
        Color(255, 255, 255, if ctx.hovered(button) { 60 } else { 34 }),
        20,
        "shell:power:wake",
        "Unlock the session",
    );
    p.strong_center(button.x, button.y + 11, 180, "Unlock", 14, Color::WHITE);
}

fn top_bar(p: &mut Painter, ctx: &ShellContext<'_>) {
    let width = ctx.width as i32;
    p.box_(Rect::new(0, 0, ctx.width, 32), PANEL, 0);
    let plate = Color(255, 255, 255, 68);
    // GNOME 46's Activities control is a workspace indicator, not a text menu; it opens
    // the window overview, as Super does.
    let activities = Rect::new(0, 0, 72, 32);
    let lit = ctx.hovered(activities) || ctx.launcher_open || ctx.panel == Some("overview");
    if lit {
        p.box_(Rect::new(4, 3, 64, 26), plate, 13);
    }
    p.region(activities, "shell:overview", "Activities");
    p.box_(Rect::new(16, 11, 27, 9), Color::rgb(247, 247, 247), 5);
    p.box_(Rect::new(49, 13, 5, 5), Color::rgb(154, 154, 154), 3);
    let date = ctx.date();
    let clock = format!("{} {}  {}", &date.month_name()[..3], date.day, ctx.time());
    let clock_width = p.measure(&clock, 14, false);
    // The date menu shares the centre box with a hidden 67 px indicator on its left, so
    // it sits right of the screen's centre.
    let centre = width / 2 + 34;
    let hit = Rect::new(
        centre - clock_width as i32 / 2 - 15,
        3,
        clock_width + 30,
        26,
    );
    if ctx.hovered(hit) || ctx.panel == Some("calendar") {
        p.box_(hit, plate, 13);
    }
    p.label(
        hit.x,
        7,
        hit.width,
        &clock,
        14,
        Color::rgb(242, 242, 242),
        false,
        Align::Center,
    );
    p.region(
        Rect::new(hit.x - 12, 0, hit.width + 24, 32),
        "shell:panel:calendar",
        "Open calendar and notifications",
    );
    if width > 380 {
        let tray = Rect::new(width - 100, 3, 94, 26);
        if ctx.hovered(tray) || ctx.panel == Some("quick") {
            p.box_(tray, plate, 13);
        }
        // Network, volume, and the battery on a laptop; with no battery to report,
        // the power glyph GNOME shows on a desktop computer.
        let last = if ctx.battery { "battery" } else { "power" };
        for (i, symbol) in ["wifi", "volume", last].iter().enumerate() {
            p.symbol(
                symbol,
                tray.x + 12 + i as i32 * 28,
                8,
                16,
                Color::rgb(242, 242, 242),
            );
        }
        p.region(
            Rect::new(width - 104, 0, 104, 32),
            "shell:panel:quick",
            "System",
        );
    }
}

/// The Ubuntu Dock's width (dash-to-dock, fixed, full height, on the left).
pub(crate) const DOCK_WIDTH: i32 = 66;

fn dock(p: &mut Painter, ctx: &ShellContext<'_>) {
    let height = ctx.height as i32;
    let w = DOCK_WIDTH as u32;
    // Ubuntu's fixed full-height dock: Yaru's dark dash, a little translucent.
    p.glass(
        Rect::new(0, 32, w, ctx.height.saturating_sub(32)),
        0,
        30,
        Color(29, 29, 29, 225),
        None,
    );
    // Each favourite is a 66x60 button every 64 px from the top; the 48 px icon sits in it.
    let mut next = 38;
    // Then the running applications that are not favourites, in the order they opened,
    // as the Ubuntu Dock lists them before its separator.
    let mut shown: Vec<(&str, &str)> = DOCK
        .iter()
        .filter_map(|kind| APPS.iter().find(|(k, _)| k == kind))
        .filter(|(kind, _)| ctx.installed(kind))
        .map(|(kind, name)| (*kind, *name))
        .collect();
    let mut opened: Vec<&WindowView> = ctx.windows.iter().collect();
    opened.sort_by_key(|w| w.id);
    for w in opened {
        if !DOCK.contains(&w.kind.as_str()) && !shown.iter().any(|(k, _)| *k == w.kind) {
            if let Some((kind, name)) = APPS.iter().find(|(k, _)| *k == w.kind) {
                shown.push((kind, name));
            }
        }
    }
    for (kind, name) in shown {
        let y = next;
        if y + 60 > height - 66 - 64 {
            break;
        }
        next += 64;
        let running: Vec<_> = ctx.windows.iter().filter(|w| w.kind == *kind).collect();
        let slot = Rect::new(0, y, w, 60);
        let hovered = ctx.hovered(slot);
        let focused = running.iter().any(|w| w.focused && !w.minimized);
        if hovered || focused {
            p.box_(
                Rect::new(5, y + 1, 56, 58),
                Color(255, 255, 255, if hovered { 46 } else { 30 }),
                12,
            );
        }
        let action = running
            .last()
            .map_or_else(|| format!("shell:launch:{kind}"), |w| w.action("focus"));
        p.platform_icon(Rect::new(9, y + 6, 48, 48), "ubuntu", kind, &action, name);
        p.region(slot, &action, name);
        if hovered && !ctx.launcher_open {
            let width = p.measure(name, 13, false) + 24;
            let tip = Rect::new(76, y + 15, width, 30);
            p.drop_shadow(tip, 15, 10, 80, 3);
            p.border(tip, Color::rgb(36, 36, 36), 15, Color(255, 255, 255, 30));
            p.center(tip.x, tip.y + 7, width, name, 13, Color::WHITE);
        }
        let dots = running.len().min(4) as i32;
        for n in 0..dots {
            p.circle(3, y + 30 - (dots - 1) * 4 + n * 8, 2, ORANGE);
        }
    }
    // The Trash follows the favourites under a separator, as dash-to-dock shows it.
    if height > 320 {
        p.hline(9, next + 3, 48, Color::rgb(70, 70, 70));
        let y = next + 7;
        let slot = Rect::new(0, y, w, 60);
        if ctx.hovered(slot) {
            p.box_(Rect::new(5, y + 1, 56, 58), Color(255, 255, 255, 46), 12);
        }
        p.asset(Rect::new(9, y + 6, 48, 48), "icon/ubuntu/trash");
        // Deleted files really land in ~/.local/share/Trash/files; this opens that folder.
        p.region(slot, "shell:trash", "Trash");
    }
    if height > 190 {
        let y = height - 66;
        let slot = Rect::new(0, y, w, 66);
        p.button(
            slot,
            Color::TRANSPARENT,
            12,
            "shell:launcher",
            "Show applications",
        );
        if ctx.launcher_open || ctx.hovered(slot) {
            p.box_(Rect::new(5, y + 4, 56, 58), Color(255, 255, 255, 40), 12);
        }
        // Ubuntu 24.04's Show Applications button is the Ubuntu logo.
        p.symbol("ubuntu", 14, y + 14, 38, Color::rgb(241, 241, 241));
    }
}

/// The overview's own background: GNOME Shell 46 under Yaru draws it opaque.
const OVERVIEW_BG: Color = Color::rgb(34, 34, 34);

/// The Activities overview: the search entry over the window overview (Super, the
/// Activities button), the app grid (Show Applications, Super twice) or search results.
fn activities(p: &mut Painter, ctx: &ShellContext<'_>) {
    let width = ctx.width as i32;
    let area = Rect::new(
        DOCK_WIDTH,
        32,
        ctx.width.saturating_sub(DOCK_WIDTH as u32),
        ctx.height.saturating_sub(32),
    );
    p.box_(area, OVERVIEW_BG, 0);
    p.region(area, "shell:dismiss", "Close Activities");
    let centre = (width + DOCK_WIDTH) / 2;
    let search_width = (width - DOCK_WIDTH - 110).clamp(140, 372) as u32;
    let field = Rect::new(centre - search_width as i32 / 2, 56, search_width, 42);
    let searching = !ctx.search.is_empty();
    if searching {
        p.border(field, Color::rgb(59, 59, 59), 21, ORANGE);
    } else {
        p.box_(field, Color::rgb(59, 59, 59), 21);
    }
    p.region(field, "shell:search", "Search applications");
    p.symbol(
        "search",
        field.x + 16,
        field.y + 13,
        16,
        Color::rgb(200, 200, 200),
    );
    if searching {
        let end = p.left(
            field.x + 42,
            field.y + 11,
            search_width - 56,
            ctx.search,
            15,
            Color::WHITE,
        );
        p.box_(
            Rect::new(field.x + 43 + end as i32, field.y + 11, 1, 20),
            Color::WHITE,
            0,
        );
        search_results(p, ctx, centre);
        return;
    }
    p.left(
        field.x + 42,
        field.y + 11,
        search_width - 56,
        "Type to search",
        15,
        Color::rgb(190, 190, 190),
    );
    if ctx.launcher_open || ctx.panel == Some("search") {
        app_grid(p, ctx, centre);
    } else {
        window_overview(p, ctx, centre);
    }
}

/// The applications that match what was typed, one centred row of large tiles, the
/// first (what Enter opens) highlighted.
fn search_results(p: &mut Painter, ctx: &ShellContext<'_>, centre: i32) {
    let query = ctx.search.to_lowercase();
    let fit = ((ctx.width as i32 - DOCK_WIDTH - 60) / 174).max(1) as usize;
    let shown: Vec<&(&str, &str)> = APPS
        .iter()
        .filter(|(kind, name)| {
            ctx.installed(kind) && (name.to_lowercase().contains(&query) || kind.contains(&query))
        })
        .take(fit)
        .collect();
    let left = centre - (shown.len() as i32 * 174 - 30) / 2;
    for (i, (kind, name)) in shown.iter().enumerate() {
        let tile = Rect::new(left + i as i32 * 174, 110, 144, 144);
        if i == 0 || ctx.hovered(tile) {
            p.box_(tile, Color(255, 255, 255, if i == 0 { 30 } else { 20 }), 18);
        }
        let action = format!("shell:launch:{kind}");
        p.region(tile, &action, name);
        p.platform_icon(
            Rect::new(tile.x + 24, tile.y + 12, 96, 96),
            "ubuntu",
            kind,
            &action,
            name,
        );
        p.label(
            tile.x + 12,
            tile.y + 114,
            120,
            name,
            14,
            Color::WHITE,
            false,
            Align::Center,
        );
    }
}

/// The app grid: every installed application the dock does not already carry, in
/// alphabetical order, six columns of 112 px tiles, paged when it overflows, under the
/// workspace minimap.
fn app_grid(p: &mut Painter, ctx: &ShellContext<'_>, centre: i32) {
    let height = ctx.height as i32;
    workspace_minimap(p, ctx, centre, 110);
    let columns = ((ctx.width as i32 - DOCK_WIDTH - 140) / 148).clamp(1, 6);
    let start = centre - (columns * 148 - 36) / 2;
    let top = 261;
    let mut shown: Vec<&(&str, &str)> = APPS
        .iter()
        .filter(|(kind, _)| ctx.installed(kind) && !DOCK.contains(kind))
        .collect();
    shown.sort_by_key(|(_, name)| name.to_lowercase());
    // GNOME pages the app grid: as many rows as fit, and dots under the grid for the
    // pages beyond. The page shown is clamped to the pages there are.
    let rows = ((height - top - 50) / 130).max(1);
    let per_page = (columns * rows) as usize;
    let pages = shown.len().div_ceil(per_page).max(1);
    let page = (ctx.home_page as usize).min(pages - 1);
    for (i, (kind, name)) in shown
        .iter()
        .skip(page * per_page)
        .take(per_page)
        .enumerate()
    {
        let x = start + (i as i32 % columns) * 148;
        let y = top + (i as i32 / columns) * 130;
        let tile = Rect::new(x, y, 112, 112);
        if ctx.hovered(tile) {
            p.box_(tile, Color(255, 255, 255, 24), 16);
        }
        let action = format!("shell:launch:{kind}");
        p.region(tile, &action, name);
        p.platform_icon(
            Rect::new(x + 24, y + 12, 64, 64),
            "ubuntu",
            kind,
            &action,
            name,
        );
        p.label(
            x + 12,
            y + 82,
            88,
            name,
            14,
            Color::WHITE,
            false,
            Align::Center,
        );
    }
    if pages > 1 {
        let dots_y = top + rows * 130;
        let left = centre - pages as i32 * 10;
        for i in 0..pages {
            let dot = Rect::new(left + i as i32 * 20, dots_y, 20, 20);
            p.button(
                dot,
                Color::TRANSPARENT,
                10,
                &format!("shell:home-page:{i}"),
                &format!("Page {} of {pages}", i + 1),
            );
            p.circle(
                dot.x + 10,
                dot.y + 10,
                if i == page { 5 } else { 4 },
                if i == page {
                    Color::WHITE
                } else {
                    Color(255, 255, 255, 110)
                },
            );
        }
    }
}

/// The window overview: the desktop on screen as a large rounded preview with its
/// windows in it, the next desktop peeking in at the right.
fn window_overview(p: &mut Painter, ctx: &ShellContext<'_>, centre: i32) {
    let (width, height) = (ctx.width as i32, ctx.height as i32);
    // 77 % of the screen, under the search entry.
    let pw = width * 771 / 1000;
    let ph = height * 771 / 1000;
    let preview = Rect::new(centre - pw / 2, 122, pw as u32, ph as u32);
    let mark = p.scene.nodes.len();
    p.asset(preview, "wallpaper/ubuntu");
    p.round_clip_since(mark, preview, 18);
    let scale = |v: i32| v * 771 / 1000;
    for w in ctx.windows.iter().filter(|w| !w.minimized) {
        let r = Rect::new(
            preview.x + scale(w.rect.x),
            preview.y + scale(w.rect.y),
            scale(w.rect.width as i32).max(8) as u32,
            scale(w.rect.height as i32).max(8) as u32,
        );
        p.drop_shadow(r, 8, 12, 90, 4);
        p.box_(r, HEADER, 8);
        if let Some(content) = &w.content {
            let head = scale(46).min(r.height as i32 / 3);
            let body = Rect::new(
                r.x,
                r.y + head,
                r.width,
                (r.height as i32 - head).max(1) as u32,
            );
            p.thumbnail(content, body, 0);
        }
        p.region(r, &w.action("focus"), &window_title(w));
    }
    // The next desktop (GNOME keeps an empty one past the last) peeks in at the right.
    let next = Rect::new(preview.x + pw + 92, preview.y, pw as u32, ph as u32);
    if next.x < width {
        let mark = p.scene.nodes.len();
        p.asset(next, "wallpaper/ubuntu");
        p.round_clip_since(mark, next, 18);
        p.box_(next, Color(0, 0, 0, 60), 18);
    }
    if height > 400 {
        // The minimap is GNOME's only when several desktops are in use.
        if ctx.workspaces > 1 {
            workspace_minimap(p, ctx, centre, preview.y + preview.height as i32 + 8);
        }
    }
}

/// The workspace minimap: one thumbnail per desktop the machine really has, the one on
/// screen larger and outlined, then the empty desktop GNOME keeps past the last; each
/// switches to its desktop, and the one on screen offers closing it.
fn workspace_minimap(p: &mut Painter, ctx: &ShellContext<'_>, centre: i32, y: i32) {
    let count = ctx.workspaces.max(1);
    let full = count >= crate::WORKSPACE_LIMIT;
    let tiles = count as i32 + i32::from(!full);
    let span = tiles * 211 - 29 + 10;
    let mut x = centre - span / 2;
    // A window view carries no workspace, so only the desktop on screen can show what
    // is on it; the others are named and reachable but cannot be previewed yet.
    for index in 0..count {
        let here = index == ctx.workspace;
        let r = if here {
            Rect::new(x, y, 182, 115)
        } else {
            Rect::new(x + 5, y + 3, 172, 109)
        };
        let mark = p.scene.nodes.len();
        p.asset(r, "wallpaper/ubuntu");
        p.round_clip_since(mark, r, 6);
        if here {
            for w in ctx.windows.iter().filter(|w| !w.minimized).take(4) {
                let s = |v: i32| v * 142 / 1000;
                p.box_(
                    Rect::new(
                        r.x + s(w.rect.x),
                        r.y + s(w.rect.y),
                        s(w.rect.width as i32).max(4) as u32,
                        s(w.rect.height as i32).max(4) as u32,
                    ),
                    Color(250, 250, 250, 220),
                    2,
                );
            }
        }
        p.button(
            r,
            Color::TRANSPARENT,
            6,
            &format!("shell:workspace:{index}"),
            &format!("Desktop {}", index + 1),
        );
        if here {
            p.border(r, Color::TRANSPARENT, 6, Color(255, 255, 255, 120));
            // The handler closes the desktop you are on, so only that tile offers it.
            if count > 1 {
                let close = Rect::new(r.x + r.width as i32 - 14, r.y - 6, 20, 20);
                p.button(
                    close,
                    Color::rgb(46, 46, 46),
                    10,
                    "shell:workspace:close",
                    &format!("Close desktop {}", index + 1),
                );
                p.symbol("close", close.x + 5, close.y + 5, 10, Color::WHITE);
            }
        }
        x += 211;
    }
    let add = Rect::new(x + 5, y + 3, 172, 109);
    if full {
        p.border(add, Color(255, 255, 255, 12), 6, Color(255, 255, 255, 40));
        // Eight desktops is the machine's limit; a ninth would be refused.
        p.symbol("plus", add.x + 78, add.y + 46, 16, Color(255, 255, 255, 90));
        p.disabled("New desktop");
    } else {
        let mark = p.scene.nodes.len();
        p.asset(add, "wallpaper/ubuntu");
        p.round_clip_since(mark, add, 6);
        p.button(
            add,
            Color(0, 0, 0, if ctx.hovered(add) { 20 } else { 0 }),
            6,
            "shell:workspace:new",
            "New desktop",
        );
    }
}

pub(super) fn window_title(w: &WindowView) -> String {
    match w.kind.as_str() {
        "files" if !w.caption.is_empty() => w.caption.clone(),
        "files" if !w.home.is_empty() && w.tilde(&w.document) == "~" => "Home".into(),
        "files" => basename(&w.document).to_owned(),
        // GNOME Terminal titles itself from the prompt: `user@host: ~/dir`.
        "terminal" => w
            .shell_identity()
            .map(|(user, host, dir)| format!("{user}@{host}: {dir}"))
            .unwrap_or_else(|| "Terminal".into()),
        // GNOME Text Editor titles its window `name (folder) - Text Editor`.
        "editor" => format!("{} ({}) - Text Editor", editor_name(w), editor_folder(w)),
        // LibreOffice titles its window `document — LibreOffice Writer`, `Untitled 1`
        // before a document is open.
        "docs" if w.document.is_empty() => "Untitled 1 — LibreOffice Writer".into(),
        "docs" => format!("{} — LibreOffice Writer", w.document),
        "browser" if w.caption.is_empty() => "New Tab".into(),
        "browser" => w.caption.clone(),
        // KiCad titles each of its frames itself (`project — KiCad 8.0`,
        // `*project [project] — Schematic Editor`), and the window manager shows that title.
        "kicad" => w.title.clone(),
        // So does FreeCAD (`* Unnamed - FreeCAD 1.0.2`).
        "freecad" => w.title.clone(),
        // And VS Code (`● clock.rs - atlas - Visual Studio Code`).
        "code" => w.title.clone(),
        // And Kdenlive (`Untitled / 320x180 24 fps`).
        "kdenlive" => w.title.clone(),
        // Anything else names itself with the name its launcher icon carries.
        kind => app_name(kind).map_or_else(|| w.title.clone(), str::to_owned),
    }
}
/// The document's name in Text Editor: an unsaved draft's first line, or New Document
/// while it is empty.
fn editor_name(w: &WindowView) -> String {
    if !w.document.is_empty() {
        basename(&w.document).to_owned()
    } else if !w.caption.is_empty() {
        w.caption.clone()
    } else {
        "New Document".into()
    }
}
/// The folder Text Editor shows under the name (home written as `~`), or Draft.
fn editor_folder(w: &WindowView) -> String {
    if w.document.is_empty() {
        return "Draft".to_owned();
    }
    w.tilde(w.document.rsplit_once('/').map_or(
        "/",
        |(dir, _)| {
            if dir.is_empty() {
                "/"
            } else {
                dir
            }
        },
    ))
}
/// GNOME Clocks' header bar: Add Location on the world view, the view switcher's four
/// 120 px tabs from 186 px in, and the primary menu (measured on Clocks 46 at 870 px).
/// The model keeps no world clocks or alarms, so those are announced insensitive.
fn clocks_header(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    let face = w.chrome("face").unwrap_or("world");
    if face == "world" {
        let add = Rect::new(r.x + 11, r.y + 11, 34, 34);
        header_button(p, ctx, add, "plus", ink);
        p.insensitive(add, &w.action("content:clock:add-location"), "Add Location");
    }
    // GTK places the switcher's accessible bounds 5 px up and left of where it paints
    // it. The list itself is a control of no action of its own: a click between its
    // tabs does nothing.
    p.region(
        Rect::new(r.x + 186, r.y + 6, 489, 34),
        &w.action("content:shell:noop"),
        "",
    );
    p.announce("tablist", None);
    for (i, (id, label, symbol)) in [
        ("world", "World", "globe"),
        ("alarms", "Alarms", "bell"),
        ("stopwatch", "Stopwatch", "clock"),
        ("timer", "Timer", "dial"),
    ]
    .into_iter()
    .enumerate()
    {
        let tab = Rect::new(r.x + 186 + i as i32 * 123, r.y + 6, 120, 34);
        let painted = Rect::new(tab.x + 5, tab.y + 5, 120, 34);
        let on = face == id;
        if on {
            p.box_(painted, Color::rgb(224, 224, 224), 8);
        } else if ctx.hovered(painted) && id != "alarms" {
            p.box_(painted, Color(0, 0, 0, 18), 8);
        }
        let text = p.measure(label, 14, true) as i32;
        let x = painted.x + (120 - text - 22) / 2;
        p.symbol(symbol, x, painted.y + 9, 16, ink);
        p.label(
            x + 22,
            painted.y + 7,
            text as u32 + 2,
            label,
            14,
            ink,
            true,
            Align::Left,
        );
        let action = w.action(&format!("content:clock:{id}"));
        if id == "alarms" {
            p.insensitive(tab, &action, label);
        } else {
            p.region(tab, &action, label);
        }
        p.announce("tab", None);
        p.select_last(on);
    }
    // Clocks' primary menu holds its keyboard shortcuts and About, which the model lacks.
    primary_menu(
        p,
        ctx,
        w,
        r.x + r.width as i32 - 164,
        "Main Menu",
        false,
        ink,
    );
}

/// GNOME Calendar's header bar over its sidebar and month view (measured on Calendar 46
/// at 1212 px): the Calendars and primary menus over the sidebar, then Today, the Week
/// and Month switcher, Search Events and Add Event. Today is insensitive on today; the
/// model has no calendar list, primary menu, week view or search, so those are too.
fn calendar_header(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    let right = r.x + r.width as i32;
    let sidebar = Rect::new(r.x + 1, r.y + 1, 303, 45);
    p.box_(sidebar, Color::rgb(235, 235, 235), 0);
    p.box_(
        Rect::new(r.x + 304, r.y + 1, r.width.saturating_sub(305), 45),
        Color::WHITE,
        0,
    );
    p.vline(r.x + 304, r.y + 1, 45, Color::rgb(218, 218, 218));
    // GTK places a header bar's MenuButton 5 px up and left of where it paints it, and
    // lists its inner toggle where it is painted.
    let calendars = Rect::new(r.x + 106, r.y + 11, 98, 34);
    let text = p.measure("Calendars", 14, true) as i32;
    let tx = calendars.x + (98 - text - 16) / 2;
    p.left(tx, calendars.y + 8, text as u32 + 2, "Calendars", 14, ink);
    p.symbol("chevron-down", tx + text + 6, calendars.y + 13, 10, ink);
    p.insensitive(
        Rect::new(r.x + 101, r.y + 6, 98, 34),
        &w.action("content:cal:calendars"),
        "Calendars",
    );
    p.insensitive(
        calendars,
        &w.action("content:cal:calendars-toggle"),
        "Calendars",
    );
    primary_menu(p, ctx, w, r.x + 260, "Main Menu", false, ink);
    let today = ctx.clock_us / crate::apps::calendar::DAY_US;
    let on_today = w.chrome("cursor") == Some(today.to_string().as_str());
    let today_button = Rect::new(r.x + 315, r.y + 11, 84, 34);
    let today_ink = if on_today { DIM } else { ink };
    p.symbol(
        "calendar",
        today_button.x + 6,
        today_button.y + 9,
        16,
        today_ink,
    );
    p.left(
        today_button.x + 27,
        today_button.y + 8,
        52,
        "Today",
        14,
        today_ink,
    );
    if on_today {
        p.insensitive(today_button, &w.action("content:cal:today:shown"), "Today");
    } else {
        if ctx.hovered(today_button) {
            p.box_(today_button, Color(0, 0, 0, 18), 8);
        }
        p.region(today_button, &w.action("content:cal:today"), "Today");
    }
    // The switcher: a list of no action of its own between its two tabs.
    p.region(
        Rect::new(r.x + 630, r.y + 6, 243, 34),
        &w.action("content:shell:noop"),
        "",
    );
    p.announce("tablist", None);
    let month = w.chrome("view") != Some("day");
    for (i, (id, label)) in [("week", "Week"), ("month", "Month")]
        .into_iter()
        .enumerate()
    {
        let tab = Rect::new(r.x + 630 + i as i32 * 123, r.y + 6, 120, 34);
        let painted = Rect::new(tab.x + 5, tab.y + 5, 120, 34);
        let on = id == "month" && month;
        if on {
            p.box_(painted, Color::rgb(235, 235, 235), 8);
        }
        let text = p.measure(label, 14, true) as i32;
        let x = painted.x + (120 - text - 22) / 2;
        p.symbol("calendar", x, painted.y + 9, 16, ink);
        p.label(
            x + 22,
            painted.y + 7,
            text as u32 + 2,
            label,
            14,
            ink,
            true,
            Align::Left,
        );
        let action = w.action(&format!("content:cal:{id}"));
        if id == "week" {
            p.insensitive(tab, &action, label);
        } else {
            p.region(tab, &action, label);
        }
        p.announce("tab", None);
        p.select_last(on);
    }
    let search = Rect::new(right - 194, r.y + 16, 34, 34);
    p.symbol("search", search.x + 4, search.y + 4, 16, ink);
    p.insensitive(search, &w.action("content:cal:search"), "Search Events");
    let add = Rect::new(right - 159, r.y + 11, 34, 34);
    header_button(p, ctx, add, "plus", ink);
    p.region(add, &w.action("content:cal:new"), "Add Event");
}

/// A libadwaita primary menu button at `x`, 6 px down. GTK lists a MenuButton and its
/// inner toggle as two buttons of one name, the toggle 5 px further right and down,
/// where its icon is painted: the toggle opens the menu, and the strip of the outer
/// button that shows past it does nothing. A menu with nothing the model can do in it
/// (`opens` false) is announced insensitive, both buttons.
fn primary_menu(
    p: &mut Painter,
    ctx: &ShellContext<'_>,
    w: &WindowView,
    x: i32,
    name: &str,
    opens: bool,
    ink: Color,
) {
    let r = w.rect;
    let outer = Rect::new(x, r.y + 6, 34, 34);
    let toggle = Rect::new(x + 5, r.y + 11, 34, 34);
    if !opens {
        p.symbol("menu", toggle.x + 9, toggle.y + 9, 16, DIM);
        p.insensitive(outer, &w.action("content:menu:primary"), name);
        p.insensitive(toggle, &w.action("content:menu:primary-toggle"), name);
        return;
    }
    p.region(outer, &w.action("content:shell:noop"), name);
    header_button(p, ctx, toggle, "menu", ink);
    p.region_above(toggle, &w.action("content:shell:panel:app-menu"), name);
}

/// Flat libadwaita header bar button with its hover plate.
fn header_button(p: &mut Painter, ctx: &ShellContext<'_>, r: Rect, symbol: &str, ink: Color) {
    if ctx.hovered(r) {
        p.box_(r, Color(128, 128, 128, 50), 8);
    }
    p.symbol(
        symbol,
        r.x + (r.width as i32 - 16) / 2,
        r.y + (r.height as i32 - 16) / 2,
        16,
        ink,
    );
}

/// GNOME's path bar: one button per component, each navigating to that exact folder.
/// Inside the home folder the trail starts at Home, as Files starts it; a place that is
/// not a folder (Recent, Starred, Trash) is a single named button.
fn path_bar(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, bar: Rect, ink: Color) {
    p.box_(bar, Color(0, 0, 0, 18), 8);
    let home = w.home.trim_end_matches('/');
    let under_home =
        !home.is_empty() && (w.document == home || w.document.starts_with(&format!("{home}/")));
    let (mut crumbs, rest, mut path) = if !w.caption.is_empty() {
        (vec![(w.caption.clone(), String::new())], "", String::new())
    } else if under_home {
        (
            vec![("Home".to_owned(), home.to_owned())],
            &w.document[home.len()..],
            home.to_owned(),
        )
    } else {
        (
            vec![("Computer".to_owned(), "/".to_owned())],
            w.document.as_str(),
            String::new(),
        )
    };
    for part in rest.split('/').filter(|s| !s.is_empty()) {
        path.push('/');
        path.push_str(part);
        crumbs.push((part.to_owned(), path.clone()));
    }
    let lead = if !w.caption.is_empty() {
        None
    } else if under_home {
        Some("home")
    } else {
        Some("drive")
    };
    let last = crumbs.len() - 1;
    let widths: Vec<u32> = crumbs
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            p.measure(name, 13, true)
                + if i == last { 24 } else { 36 }
                + if i == 0 && lead.is_some() { 22 } else { 0 }
        })
        .collect();
    // Elide from the front when the bar is short: the deepest components are the useful ones.
    let mut first = 0;
    while first < last && widths[first..].iter().sum::<u32>() > bar.width.saturating_sub(8) {
        first += 1;
    }
    let mut cx = bar.x + 4;
    for (i, (name, target)) in crumbs.iter().enumerate().skip(first) {
        let room = bar.width.saturating_sub(8).max(24);
        let width = widths[i].min(room);
        let chip = Rect::new(cx, bar.y + 3, width - if i == last { 0 } else { 12 }, 26);
        if ctx.hovered(chip) {
            p.box_(chip, Color(0, 0, 0, 24), 6);
        }
        let (text_x, text_width) = match lead.filter(|_| i == 0) {
            Some(symbol) => {
                p.symbol(symbol, chip.x + 10, chip.y + 5, 16, ink);
                (chip.x + 22, chip.width.saturating_sub(22))
            }
            None => (chip.x, chip.width),
        };
        p.label(
            text_x,
            chip.y + 5,
            text_width,
            name,
            13,
            ink,
            i == last,
            Align::Center,
        );
        if target.is_empty() {
            // A named place re-reads itself: Recent and Starred refresh from state.
            p.region(
                chip,
                &w.action("content:files-reload"),
                &format!("Reload {name}"),
            );
        } else {
            p.region(
                chip,
                &w.action(&format!("content:files-location:{target}")),
                &format!("Go to {name}"),
            );
        }
        if i < last {
            p.symbol(
                "chevron-right",
                chip.x + chip.width as i32,
                bar.y + 11,
                12,
                DIM,
            );
        }
        cx += width as i32;
        if cx >= bar.x + bar.width as i32 {
            break;
        }
    }
}

/// A control GTK announces at bounds of its own: a label's box or an image's.
fn announced(p: &mut Painter, r: Rect, role: &str, name: &str) {
    p.node(r, cw_scene::Primitive::Region, None);
    if let Some(n) = p.scene.nodes.last_mut() {
        n.semantic = Some(cw_scene::Semantic {
            role: role.into(),
            label: name.into(),
            value: None,
            disabled: false,
            focusable: false,
        });
    }
}

/// Nautilus 46's header bars, laid out from the real one at 890 px: over the sidebar,
/// Search Everywhere, the Files title and the Main Menu; over the view, a tool bar
/// holding Back and Forward, the path bar with its Current Folder Menu, Search Current
/// Folder and View Options. GTK 4 lists a menu button and its inner toggle as two
/// buttons, so Main Menu, Current Folder Menu and the View Options arrow are each
/// announced twice, and both of each pair act alike. Searching turns the path bar into
/// the search entry. Global search and the View Options menu are announced insensitive:
/// the file manager has neither.
fn files_header(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    let (x, y) = (r.x, r.y);
    let right = r.x + r.width as i32;
    let side = FILES_SIDEBAR as i32;
    // The view's header bar is the view's white; the sidebar's is the sidebar's grey.
    p.box_(
        Rect::new(x + side, y + 1, r.width - side as u32 - 1, 46),
        Color::WHITE,
        0,
    );
    p.box_(Rect::new(x + 1, y + 1, side as u32 - 1, 46), HEADER, 0);
    p.vline(x + side - 1, y + 1, 46, Color::rgb(218, 218, 218));
    // The tool bar is the header bar over the view; pressing its empty part still drags
    // the window, through the move grip painted above it.
    p.z -= 1;
    p.region(
        Rect::new(
            x + side - 5,
            y + 3,
            (right - 10 - (x + side - 5)).max(0) as u32,
            40,
        ),
        &w.action("content:files-header"),
        "",
    );
    p.announce("toolbar", None);
    p.z += 1;
    let glyph = |p: &mut Painter, symbol: &str, cx: i32, cy: i32, color: Color| {
        p.symbol(symbol, cx - 8, cy - 8, 16, color)
    };
    let everywhere = Rect::new(x + 11, y + 11, 34, 34);
    glyph(p, "search", x + 28, y + 28, DIM);
    p.insensitive(
        everywhere,
        &w.action("content:files-search-everywhere"),
        "Search Everywhere",
    );
    p.strong_center(x + 70, y + 18, 64, "Files", 14, ink);
    p.presentation();
    announced(p, Rect::new(x + 70, y + 14, 64, 18), "label", "Files");
    glyph(p, "menu", x + 162, y + 28, ink);
    p.region(
        Rect::new(x + 140, y + 6, 34, 34),
        &w.action("content:shell:panel:primary-menu"),
        "Main Menu",
    );
    p.region(
        Rect::new(x + 145, y + 11, 34, 34),
        &w.action("content:shell:panel:app-menu"),
        "Main Menu",
    );
    // History, greyed with nothing to go back or forward to.
    for (bx, symbol, command, label, live) in [
        (191, "chevron-left", "files-back", "Back", w.can_go_back),
        (
            231,
            "chevron-right",
            "files-forward",
            "Forward",
            w.can_go_forward,
        ),
    ] {
        let hit = Rect::new(x + bx, y + 11, 34, 34);
        if live && ctx.hovered(hit) {
            p.box_(hit, Color(0, 0, 0, 20), 8);
        }
        glyph(
            p,
            symbol,
            x + bx + 17,
            y + 28,
            if live { ink } else { Color::rgb(200, 200, 200) },
        );
        let action = w.action(&format!("content:{command}"));
        if live {
            p.region(hit, &action, label);
        } else {
            p.insensitive(hit, &action, label);
        }
    }
    let bar = Rect::new(
        x + 289,
        y + 11,
        (right - 247 - (x + 289)).max(80) as u32,
        33,
    );
    p.box_(bar, HEADER, 6);
    let bar_right = bar.x + bar.width as i32;
    if let Some(location) = w.chrome("location") {
        // The location entry, its icon at the right, and Cancel over the folder menu's
        // place.
        let field = Rect::new(
            x + 293,
            y + 6,
            (bar_right - 28 - (x + 293)).max(40) as u32,
            34,
        );
        p.box_(bar, Color::WHITE, 6);
        p.border(bar, Color::TRANSPARENT, 6, Color::rgb(53, 132, 228));
        let shown = p.left(
            x + 300,
            y + 19,
            bar.width.saturating_sub(80),
            location,
            14,
            ink,
        );
        p.box_(Rect::new(x + 301 + shown as i32, y + 19, 1, 18), ink, 0);
        p.region(field, &w.action("content:files-locate"), "");
        p.announce("textbox", Some(location));
        let icon = Rect::new(field.x + field.width as i32 - 34, y + 6, 16, 34);
        p.symbol("folder", icon.x, y + 20, 16, DIM);
        announced(p, icon, "img", "");
        let cancel = Rect::new(bar_right - 33, y + 11, 34, 34);
        glyph(p, "close", bar_right - 16, y + 28, DIM);
        p.region(cancel, &w.action("content:files-locate-cancel"), "Cancel");
    } else if w.editing {
        let field = Rect::new(
            x + 293,
            y + 6,
            (bar_right - 28 - (x + 293)).max(40) as u32,
            34,
        );
        let query = w.chrome("query").unwrap_or_default();
        p.box_(bar, Color::WHITE, 6);
        p.border(bar, Color::TRANSPARENT, 6, Color::rgb(53, 132, 228));
        glyph(p, "search", x + 306, y + 28, DIM);
        let shown = p.left(
            x + 322,
            y + 19,
            bar.width.saturating_sub(70),
            query,
            14,
            ink,
        );
        p.box_(Rect::new(x + 323 + shown as i32, y + 19, 1, 18), ink, 0);
        p.region(field, &w.action("content:files-search"), "");
        p.announce("textbox", Some(query));
        let clear = Rect::new(bar_right - 33, y + 11, 34, 34);
        glyph(p, "close", bar_right - 16, y + 28, DIM);
        p.region(clear, &w.action("content:files-search-clear"), "Cancel");
    } else {
        gnome_path_bar(p, ctx, w, bar, ink);
        // The folder's own menu is the view's context menu, as in Nautilus.
        glyph(p, "more-vertical", bar_right - 17, y + 28, ink);
        p.region(
            Rect::new(bar_right - 39, y + 6, 35, 34),
            &w.action("content:shell:panel:folder-menu"),
            "Current Folder Menu",
        );
        p.region(
            Rect::new(bar_right - 30, y + 11, 36, 34),
            &w.action("content:shell:panel:context"),
            "Current Folder Menu",
        );
        announced(p, Rect::new(bar_right - 30, y + 11, 16, 24), "img", "");
    }
    let search = Rect::new(right - 235, y + 16, 34, 34);
    glyph(p, "search", right - 218, y + 28, ink);
    p.region(
        search,
        &w.action("content:files-search"),
        "Search Current Folder",
    );
    let view = Rect::new(right - 182, y + 11, 34, 34);
    glyph(
        p,
        if w.view_grid {
            "list-view"
        } else {
            "grid-view"
        },
        right - 165,
        y + 28,
        ink,
    );
    p.region(view, &w.action("content:files-view"), "View Options");
    glyph(p, "chevron-down", right - 138, y + 28, DIM);
    p.insensitive(
        Rect::new(right - 153, y + 6, 23, 34),
        &w.action("content:files-view-options"),
        "View Options",
    );
    p.insensitive(
        Rect::new(right - 150, y + 11, 24, 34),
        &w.action("content:files-view-options:toggle"),
        "View Options",
    );
}

/// Nautilus's path bar: a button for each folder from Home (or the computer's root)
/// down, each named by its folder and going there; the current folder's button fills the
/// bar. A place that is not a folder (Recent, Starred, Trash) is its one named button.
fn gnome_path_bar(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, bar: Rect, ink: Color) {
    let home = w.home.trim_end_matches('/');
    let under_home =
        !home.is_empty() && (w.document == home || w.document.starts_with(&format!("{home}/")));
    let (mut crumbs, rest, mut path) = if !w.caption.is_empty() {
        (vec![(w.caption.clone(), String::new())], "", String::new())
    } else if under_home {
        (
            vec![("Home".to_owned(), home.to_owned())],
            &w.document[home.len()..],
            home.to_owned(),
        )
    } else {
        (
            vec![("Computer".to_owned(), "/".to_owned())],
            w.document.as_str(),
            String::new(),
        )
    };
    for part in rest.split('/').filter(|s| !s.is_empty()) {
        path.push('/');
        path.push_str(part);
        crumbs.push((part.to_owned(), path.clone()));
    }
    let lead = match (w.caption.is_empty(), under_home) {
        (false, _) => None,
        (true, true) => Some("home"),
        (true, false) => Some("drive"),
    };
    let last = crumbs.len() - 1;
    let end = bar.x + bar.width as i32 - 32;
    let widths: Vec<i32> = crumbs
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            p.measure(name, 14, i == last) as i32
                + 16
                + if i == 0 && lead.is_some() { 22 } else { 0 }
        })
        .collect();
    // The deepest folders are the useful ones: drop from the front when the bar is short.
    let mut first = 0;
    while first < last && widths[first..].iter().map(|w| w + 14).sum::<i32>() > end - bar.x - 8 {
        first += 1;
    }
    let mut cx = bar.x + 8;
    for (i, (name, target)) in crumbs.iter().enumerate().skip(first) {
        let width = if i == last {
            (end - cx).max(24)
        } else {
            widths[i]
        };
        let chip = Rect::new(cx, bar.y - 2, width as u32, 28);
        if ctx.hovered(chip) && i != last {
            p.box_(
                Rect::new(chip.x, bar.y + 3, chip.width, 26),
                Color(0, 0, 0, 20),
                6,
            );
        }
        let mut tx = chip.x + 8;
        if let Some(symbol) = lead.filter(|_| i == 0) {
            p.symbol(symbol, chip.x + 6, bar.y + 9, 16, ink);
            tx += 20;
        }
        p.label(
            tx,
            bar.y + 7,
            (width - (tx - chip.x) - 4).max(0) as u32,
            name,
            14,
            ink,
            i == last,
            Align::Left,
        );
        // The current folder's own button opens the location entry, as in Nautilus.
        let action = if target.is_empty() {
            w.action("content:files-reload")
        } else if i == last {
            w.action("content:files-locate")
        } else {
            w.action(&format!("content:files-location:{target}"))
        };
        // The header bar is the application's own surface, laid above its view (the
        // content's layer, ten above the frame's): a control named like a sidebar label
        // is the one on top, as AT-SPI's smallest-control rule finds it in Nautilus.
        p.z += 11;
        p.region(chip, &action, name);
        p.z -= 11;
        if lead.is_some() && i == 0 {
            announced(p, Rect::new(chip.x, chip.y, 16, 28), "img", "");
        }
        if i < last {
            p.label(
                chip.x + width,
                bar.y + 7,
                14,
                "/",
                14,
                DIM,
                false,
                Align::Center,
            );
            p.presentation();
        }
        cx += width + 14;
    }
}

/// The rule under a light header bar, drawn over the first row of the window's content as
/// GTK draws it, and libadwaita's shade fading below it. Measured on Ubuntu 24.04 (fidelity
/// run ub2-ref2): Text Editor's and Calculator's 46 px white header bars end in a rule 10 %
/// darker than the content, then four rows 3.5 %, 2.4 %, 1.2 % and 0.4 % darker.
pub fn header_rule(p: &mut Painter, w: &WindowView) {
    let plain = !matches!(
        w.kind.as_str(),
        "photos" | "browser" | "code" | "contacts" | "clock" | "settings"
    ) && !(w.kind == "weather" && w.chrome("welcome") == Some("1"));
    if !plain {
        return;
    }
    let r = w.rect;
    let x = r.x + 1;
    let width = r.width.saturating_sub(2);
    let adwaita = matches!(w.kind.as_str(), "editor" | "calculator");
    p.hline(
        x,
        r.y + 46,
        width,
        Color(0, 0, 0, if adwaita { 26 } else { 30 }),
    );
    if adwaita {
        for (dy, alpha) in [(47, 9), (48, 6), (49, 3), (50, 1)] {
            p.hline(x, r.y + dy, width, Color(0, 0, 0, alpha));
        }
    }
}

pub fn window_frame(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView) {
    let r = w.rect;
    let radius = super::corner_radius(ctx.theme, w.maximized);
    if !w.maximized {
        if w.focused {
            p.drop_shadow(r, radius, 30, 110, 12);
        } else {
            p.drop_shadow(r, radius, 16, 70, 5);
        }
    }
    // Image Viewer (Eye of GNOME) draws a dark header bar over its dark canvas.
    let viewer = w.kind == "photos";
    // Every other header bar is Yaru's light one, GNOME Terminal's included (3.52
    // draws a light header over its dark view).
    let dark = viewer;
    // GNOME Weather's welcome page paints its blue under a transparent header bar.
    let welcome = w.kind == "weather" && w.chrome("welcome") == Some("1");
    // Clocks' header bar is flat: the window's own background, with no edge below it.
    let flat = w.kind == "clock";
    let header = match (dark, w.focused) {
        _ if welcome => crate::apps::weather::WELCOME_BLUE,
        _ if flat => crate::apps::clock::GNOME_WINDOW,
        (true, true) if viewer => EOG_HEADER,
        (true, true) => DARK_HEADER,
        (true, false) => Color::rgb(58, 58, 58),
        // libadwaita's header bar is white under Yaru (GNOME Maps, Text Editor and
        // Calculator 46 on Ubuntu 24.04); GTK 3's (GNOME Terminal's) is grey.
        (false, true) if matches!(w.kind.as_str(), "maps" | "editor" | "calculator") => {
            Color::WHITE
        }
        (false, true) => HEADER,
        (false, false) => HEADER_BACKDROP,
    };
    let ink = match (dark, w.focused) {
        _ if welcome => Color::WHITE,
        (true, true) => Color::rgb(246, 246, 246),
        (true, false) => Color::rgb(160, 160, 160),
        (false, true) => INK,
        (false, false) => DIM,
    };
    p.border(
        r,
        if viewer {
            EOG_CANVAS
        } else if w.kind == "terminal" {
            Color::rgb(48, 10, 36)
        } else {
            Color::rgb(250, 250, 250)
        },
        radius,
        Color(0, 0, 0, if w.focused { 110 } else { 70 }),
    );
    if w.kind == "code" {
        return crate::apps::code::title_bar(p, ctx, w);
    }
    // Firefox draws its own 44 px tab strip where the title bar would be.
    let firefox = w.kind == "browser";
    let head = if firefox { FF_TABS_H as u32 - 1 } else { 45 };
    // The header bar fills the 45 rows under the window's top hairline; its rule lies on
    // the content's first row (46 px down), drawn over the content (`header_rule`).
    let inner = Rect::new(r.x + 1, r.y + 1, r.width.saturating_sub(2), head);
    let header = if firefox { FF_TABS_BG } else { header };
    p.box_(inner, header, radius.saturating_sub(1));
    p.box_(
        Rect::new(inner.x, r.y + 24, inner.width, head.saturating_sub(23)),
        header,
        0,
    );
    if w.kind == "contacts" {
        // Contacts' sidebar runs up through the header bar; its content pane is lighter.
        p.box_(
            Rect::new(r.x + 264, inner.y, (r.width as i32 - 265).max(0) as u32, 45),
            Color::rgb(250, 250, 250),
            radius.saturating_sub(1),
        );
        p.box_(
            Rect::new(r.x + 264, inner.y, 24, 45),
            Color::rgb(250, 250, 250),
            0,
        );
        p.vline(r.x + 264, inner.y, 46, Color::rgb(218, 218, 218));
    } else if viewer {
        p.hline(inner.x, r.y + 45, inner.width, Color::rgb(24, 24, 24));
    }
    p.region(
        Rect::new(r.x + 1, r.y + 1, r.width.saturating_sub(2), head - 1),
        &w.action("drag"),
        &format!("Move {}", w.title),
    );
    let right = r.x + r.width as i32;
    match w.kind.as_str() {
        "browser" => return firefox_tab_strip(p, ctx, w, ink),
        "files" if r.width > 470 => files_header(p, ctx, w, ink),
        "files" => {
            let sidebar = r.width > 470;
            let x = r.x + if sidebar { FILES_SIDEBAR as i32 } else { 0 };
            if sidebar {
                p.box_(
                    Rect::new(inner.x, inner.y, FILES_SIDEBAR, 45),
                    LIGHT,
                    radius.saturating_sub(1),
                );
                p.box_(
                    Rect::new(inner.x + 14, inner.y, FILES_SIDEBAR - 14, 45),
                    LIGHT,
                    0,
                );
                p.box_(Rect::new(inner.x, r.y + 24, FILES_SIDEBAR, 22), LIGHT, 0);
                p.vline(x, inner.y, 45, Color(0, 0, 0, 24));
                // `files-search` opens the tab's own query field and really filters the
                // rows below, so this is the search the sidebar always claimed to be.
                let find = Rect::new(r.x + 8, r.y + 7, 32, 32);
                header_button(p, ctx, find, "search", ink);
                p.region(
                    find,
                    &w.action("content:files-search"),
                    "Search this folder",
                );
                // A heading, not a control: GNOME's sidebar title does nothing.
                p.strong_center(r.x + 44, r.y + 14, FILES_SIDEBAR - 88, "Files", 14, ink);
                // GNOME's primary menu is gone rather than greyed: everything it would
                // hold is already on this header bar, and nothing opens a popover here.
            }
            // Files' arrows are history, like a browser's; the path bar climbs.
            let back = Rect::new(x + 8, r.y + 7, 34, 32);
            header_button(
                p,
                ctx,
                back,
                "chevron-left",
                if w.can_go_back { ink } else { DIM },
            );
            if w.can_go_back {
                p.region(
                    back,
                    &w.action("content:files-back"),
                    "Back to the previous folder",
                );
            } else {
                p.disabled("Back to the previous folder");
            }
            let forward = Rect::new(x + 46, r.y + 7, 34, 32);
            header_button(
                p,
                ctx,
                forward,
                "chevron-right",
                if w.can_go_forward { ink } else { DIM },
            );
            // Nothing to go forward to is a greyed control, not one that refuses.
            if w.can_go_forward {
                p.region(
                    forward,
                    &w.action("content:files-forward"),
                    "Forward to the next folder",
                );
            } else {
                p.disabled("Forward to the next folder");
            }
            let bar = Rect::new(x + 84, r.y + 7, (right - 210 - x - 84).max(60) as u32, 32);
            path_bar(p, ctx, w, bar, ink);
            if r.width > 560 {
                // `files-view` swaps the tab between its list and grid layouts for real.
                // The ⋮ beside it is gone: preferences and shortcuts do not exist, and
                // the header already carries every action the file manager has.
                let view = Rect::new(right - 200, r.y + 7, 48, 32);
                if ctx.hovered(view) {
                    p.box_(view, Color(128, 128, 128, 50), 8);
                }
                p.symbol("list-view", view.x + 6, r.y + 15, 16, ink);
                p.symbol("chevron-down", view.x + 28, r.y + 18, 10, ink);
                p.region(view, &w.action("content:files-view"), "Change view");
            }
        }
        "editor" => {
            // GNOME Text Editor 46's header bar, laid out after the real one on Ubuntu 24.04.
            // GTK 4 announces each menu button twice (the button and the toggle inside it),
            // the outer one 9 px left of and 5 px above the inner; both are listed here
            // with those bounds, and both act alike.
            let at = |x: i32, y: i32, width: u32, height: u32| {
                Rect::new(r.x + x, r.y + y, width, height)
            };
            let open = at(15, 11, 76, 34);
            if ctx.hovered(open) {
                p.box_(at(15, 7, 72, 32), Color(0, 0, 0, 18), 8);
            }
            // This shell's file chooser is the Files application: documents open from there.
            if ctx.installed("files") {
                p.region(
                    at(6, 6, 76, 34),
                    &w.action("content:shell:launch:files"),
                    "Open",
                );
                p.region(open, &w.action("shell:launch:files"), "Open");
            } else {
                // Files is not installed, so there is nowhere to pick a document.
                p.insensitive(at(6, 6, 76, 34), &w.action("editor:off:open"), "Open");
                p.insensitive(open, &w.action("content:editor:off:open"), "Open");
            }
            p.strong(r.x + 21, r.y + 14, 40, "Open", 13, ink);
            p.symbol("chevron-down", r.x + 64, r.y + 18, 10, ink);
            let new = at(93, 11, 34, 34);
            if ctx.hovered(new) {
                p.box_(at(94, 7, 32, 32), Color(128, 128, 128, 50), 8);
            }
            p.symbol("new-tab", r.x + 102, r.y + 15, 16, ink);
            p.region(new, &w.action("content:shell:new"), "New tab");
            // The title is the document's name, a bullet before it while it has unsaved
            // changes, centred over the folder (or "Draft") beneath it.
            let name = editor_name(w);
            let centre = r.x + (r.width as i32 - 10) / 2;
            let tw = p.measure(&name, 13, true) as i32;
            if w.modified {
                p.left(centre - tw / 2 - 11, r.y + 6, 8, "•", 13, ink);
            }
            p.strong_center(centre - tw / 2 - 2, r.y + 6, tw as u32 + 4, &name, 13, ink);
            let folder = editor_folder(w);
            let fw = p.measure(&folder, 11, false) as i32;
            p.center(
                centre - fw / 2 - 2,
                r.y + 24,
                fw as u32 + 4,
                &folder,
                11,
                DIM,
            );
            if r.width > 420 {
                // View (zoom, wrapping, the colour scheme) has no model here: shown insensitive.
                let view = at(r.width as i32 - 199, 11, 34, 34);
                p.symbol("sun", view.x + 9, r.y + 15, 16, DIM);
                p.insensitive(
                    at(r.width as i32 - 204, 6, 34, 34),
                    &w.action("editor:off:view"),
                    "View",
                );
                p.insensitive(view, &w.action("content:editor:off:view"), "View");
                // Text Editor's main menu: new window, save, wrap, date and close.
                let menu = at(r.width as i32 - 159, 11, 34, 34);
                if ctx.hovered(menu) {
                    p.box_(
                        at(r.width as i32 - 158, 7, 32, 32),
                        Color(128, 128, 128, 50),
                        8,
                    );
                }
                p.symbol("menu", menu.x + 9, r.y + 15, 16, ink);
                p.region(
                    at(r.width as i32 - 164, 6, 34, 34),
                    &w.action("shell:panel:app-menu"),
                    "Main menu",
                );
                p.region(menu, &w.action("content:shell:panel:app-menu"), "Main menu");
            }
        }
        // GNOME Terminal 3.52's header bar: New Tab at the left, the title, then Find and
        // the primary Menu before the window buttons; the terminal sits in a notebook,
        // a tab list of one tab. One shell per window and no scrollback search: New Tab
        // and Find are announced insensitive, and the Menu's New Window opens a shell.
        "terminal" => {
            // Its header buttons are framed: white, a grey edge, 34 px square.
            let framed = |p: &mut Painter, at: Rect, symbol: &str, color: Color| {
                let frame = Rect::new(at.x + 1, r.y + 10, 34, 34);
                p.border(frame, Color::WHITE, 6, Color::rgb(199, 199, 199));
                p.symbol(symbol, frame.x + 9, frame.y + 9, 16, color);
            };
            let new = Rect::new(r.x + 10, r.y + 4, 36, 46);
            framed(p, new, "new-tab", DIM);
            p.insensitive(new, &w.action("content:terminal-new-tab"), "New Tab");
            p.strong_center(
                r.x + 120,
                r.y + 14,
                r.width.saturating_sub(240),
                &window_title(w),
                13,
                ink,
            );
            if r.width > 420 {
                let find = Rect::new(right - 215, r.y + 4, 36, 46);
                framed(p, find, "search", DIM);
                p.insensitive(find, &w.action("content:terminal-find"), "Find");
                // Terminal's primary menu: new window, full screen, reset and clear.
                let menu = Rect::new(right - 173, r.y + 4, 36, 46);
                framed(p, menu, "menu", ink);
                p.region(menu, &w.action("content:shell:panel:app-menu"), "Menu");
            }
            p.z -= 1;
            p.region(
                Rect::new(
                    r.x + 5,
                    r.y + 51,
                    r.width.saturating_sub(10),
                    r.height.saturating_sub(56),
                ),
                &w.action("content:terminal-tabs"),
                "",
            );
            p.announce("tablist", None);
            p.z += 1;
        }
        "contacts" => contacts_header(p, ctx, w, ink),
        "maps" => maps_header(p, ctx, w, ink),
        "weather" if welcome => {
            // The welcome page's header bar holds only the primary menu, unnamed.
            primary_menu(p, ctx, w, right - 164, "", true, ink);
        }
        "clock" => clocks_header(p, ctx, w, ink),
        "calendar" => calendar_header(p, ctx, w, ink),
        "calculator" => calculator_header(p, ctx, w, ink),
        "settings" if r.width > 400 => settings_header(p, ctx, w, ink),
        kind => {
            if kind == "photos" {
                viewer_header(p, ctx, w, ink);
            }
            p.strong_center(
                r.x + 120,
                r.y + 14,
                r.width.saturating_sub(240),
                &w.title,
                13,
                ink,
            );
            // LibreOffice, DB Browser for SQLite and Kdenlive are framed by the window
            // manager, whose title is no part of the application's tree.
            if matches!(kind, "docs" | "spreadsheet" | "database" | "kdenlive") {
                p.presentation();
            }
        }
    }
    for (offset, action, label) in [
        (104, "minimize", "Minimize window"),
        (70, "maximize", "Maximize or restore window"),
        (36, "close", "Close window"),
    ] {
        let cx = right - offset;
        let hit = Rect::new(cx, r.y + 9, 28, 28);
        let hovered = ctx.hovered(hit);
        p.region(hit, &w.action(action), label);
        let plate = match (dark || welcome, hovered) {
            (true, true) => Color(255, 255, 255, 60),
            (true, false) => Color(255, 255, 255, 30),
            (false, true) => Color(0, 0, 0, 50),
            (false, false) => Color(0, 0, 0, 22),
        };
        p.circle(cx + 14, r.y + 23, 12, plate);
        let (mx, my) = (cx + 14, r.y + 23);
        match action {
            "minimize" => p.line(vec![(mx - 4, my + 3), (mx + 4, my + 3)], ink, 1),
            "maximize" => {
                if w.maximized {
                    p.border(Rect::new(mx - 2, my - 5, 7, 7), Color::TRANSPARENT, 1, ink);
                    p.box_(Rect::new(mx - 5, my - 2, 7, 7), header, 1);
                    p.border(Rect::new(mx - 5, my - 2, 7, 7), Color::TRANSPARENT, 1, ink);
                } else {
                    p.border(Rect::new(mx - 4, my - 4, 9, 9), Color::TRANSPARENT, 1, ink);
                }
            }
            _ => {
                p.line(vec![(mx - 4, my - 4), (mx + 4, my + 4)], ink, 1);
                p.line(vec![(mx + 4, my - 4), (mx - 4, my + 4)], ink, 1);
            }
        }
    }
}

/// Firefox's tab strip, drawn where the title bar would be and announced as Firefox 159
/// announces it: a "Browser tabs" toolbar holding the tab list (each tab 225 px at most, its
/// "Close tab" button), "Open a new tab (Ctrl+T)", "List all tabs", and Firefox's own
/// Minimize, Restore Down (Maximize) and Close buttons at the right.
fn firefox_tab_strip(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    let right = r.x + r.width as i32;
    let strip = Rect::new(r.x, r.y, r.width, FF_TABS_H as u32);
    p.container(strip, "toolbar", "Browser tabs");
    // The tab list leaves room for "List all tabs" and the window buttons.
    let room = r.width.saturating_sub(190);
    let count = w.tabs.len().max(1) as u32;
    let width = (225.min(room / count)).max(76).min(room);
    let list_end = r.x + (width * count) as i32 + 36;
    p.container(
        Rect::new(r.x, r.y, (list_end - r.x).max(0) as u32, FF_TABS_H as u32),
        "tablist",
        "",
    );
    let mut x = r.x;
    for (index, label) in w.tabs.iter().enumerate() {
        if x + width as i32 > right - 190 {
            break;
        }
        // The environment calls an empty tab "New tab"; Firefox says "New Tab".
        let label = if label == "New tab" { "New Tab" } else { label };
        let tab = Rect::new(x, r.y, width, FF_TABS_H as u32);
        let selected = index == w.active_tab;
        let plate = Rect::new(x + 2, r.y + 4, width.saturating_sub(4), 36);
        if selected {
            p.drop_shadow(plate, 4, 4, 40, 1);
            p.box_(plate, Color::WHITE, 4);
        } else if ctx.hovered(tab) {
            p.box_(plate, Color(0, 0, 0, 16), 4);
        }
        p.region(tab, &w.tab_select(index), label);
        p.announce("tab", None);
        p.select_last(selected);
        p.symbol("globe", tab.x + 12, tab.y + 14, 16, DIM);
        p.left(
            tab.x + 36,
            tab.y + 13,
            width.saturating_sub(66),
            label,
            12,
            ink,
        );
        p.presentation();
        let close = Rect::new(tab.x + width as i32 - 26, tab.y + 12, 20, 20);
        p.button(
            close,
            Color::TRANSPARENT,
            4,
            &w.tab_close(index),
            "Close tab",
        );
        p.symbol("close", close.x + 5, close.y + 5, 10, ink);
        x += width as i32;
    }
    let plus = Rect::new(x, r.y, 36, FF_TABS_H as u32);
    p.button(
        plus,
        Color::TRANSPARENT,
        4,
        &w.tab_new(),
        "Open a new tab (Ctrl+T)",
    );
    p.symbol("plus", plus.x + 10, plus.y + 14, 16, ink);
    // Firefox lists the tabs in a menu; the simulator's strip shows every one, so the
    // button is announced and drawn but has nothing to open.
    let list = Rect::new(right - 150, r.y, 36, FF_TABS_H as u32);
    p.symbol("chevron-down", list.x + 10, list.y + 14, 16, ink);
    p.container(list, "button", "List all tabs");
    for (left, width, action, label) in [
        (114, 37, "minimize", "Minimize"),
        (
            77,
            37,
            "maximize",
            if w.maximized {
                "Restore Down"
            } else {
                "Maximize"
            },
        ),
        (40, 40, "close", "Close"),
    ] {
        let hit = Rect::new(right - left, r.y, width, FF_TABS_H as u32);
        if ctx.hovered(hit) {
            p.box_(hit, Color(0, 0, 0, 24), 0);
        }
        p.region(hit, &w.action(action), label);
        let (mx, my) = (hit.x + width as i32 / 2, r.y + FF_TABS_H / 2);
        p.circle(mx, my, 12, Color(0, 0, 0, 22));
        match action {
            "minimize" => p.line(vec![(mx - 4, my + 3), (mx + 4, my + 3)], ink, 1),
            "maximize" if w.maximized => {
                p.border(Rect::new(mx - 2, my - 5, 7, 7), Color::TRANSPARENT, 1, ink);
                p.box_(Rect::new(mx - 5, my - 2, 7, 7), FF_TABS_BG, 1);
                p.border(Rect::new(mx - 5, my - 2, 7, 7), Color::TRANSPARENT, 1, ink);
            }
            "maximize" => p.border(Rect::new(mx - 4, my - 4, 9, 9), Color::TRANSPARENT, 1, ink),
            _ => {
                p.line(vec![(mx - 4, my - 4), (mx + 4, my + 4)], ink, 1);
                p.line(vec![(mx + 4, my - 4), (mx - 4, my + 4)], ink, 1);
            }
        }
    }
}

/// Firefox's navigation toolbar beneath its tab strip, at Firefox 159's geometry and with its
/// names: Back, Forward and Reload (greyed when they cannot act), the address bar (a
/// combobox, "Search or enter address" on an empty tab and "Search with Google or enter
/// address" over a page, whose value is the full URL), the site information button ("Not
/// Secure" over plain HTTP), "Bookmark this page (Ctrl+D)", Extensions and the "Firefox"
/// menu.
pub fn browser_chrome(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView) {
    let frame = w.rect;
    let top = frame.y + FF_TABS_H;
    let ink = if w.focused { INK } else { DIM };
    let bar = Rect::new(frame.x, top, frame.width, FF_NAV_H as u32 + 1);
    p.box_(bar, FF_NAV_BG, 0);
    p.hline(frame.x, top + FF_NAV_H, frame.width, Color(0, 0, 0, 26));
    p.container(
        Rect::new(frame.x, top, frame.width, FF_NAV_H as u32),
        "toolbar",
        "Navigation",
    );
    let page = !w.document.is_empty();
    // Firefox greys what it cannot do: Back and Forward follow the tab's real history,
    // and Reload needs a page to re-request.
    for (x, width, symbol, action, label, ready) in [
        (0, 40, "arrow-left", "back", "Back", w.can_go_back),
        (
            40,
            36,
            "arrow-right",
            "forward",
            "Forward",
            w.can_go_forward,
        ),
        (76, 36, "reload", "reload", "Reload", page),
    ] {
        let hit = Rect::new(frame.x + x, top, width, FF_NAV_H as u32);
        if ready && ctx.hovered(hit) {
            p.box_(
                Rect::new(hit.x + 2, hit.y + 4, width - 4, 32),
                Color(0, 0, 0, 16),
                4,
            );
        }
        p.symbol(
            symbol,
            hit.x + width as i32 / 2 - 8,
            hit.y + 12,
            16,
            if ready { ink } else { DIM },
        );
        if ready {
            p.region(hit, &w.action(&format!("content:shell:{action}")), label);
        } else {
            // Announced greyed at the button's own size, not the glyph's.
            p.box_(hit, Color::TRANSPARENT, 0);
            p.disabled(label);
        }
    }
    let right = frame.x + frame.width as i32;
    let narrow = frame.width < 640;
    let bar_x = frame.x + if narrow { 116 } else { 217 };
    let bar_end = right - if narrow { 84 } else { 181 };
    let field_bg = Rect::new(bar_x, top + 4, (bar_end - bar_x).max(40) as u32, 32);
    p.box_(field_bg, if w.editing { Color::WHITE } else { FF_FIELD }, 5);
    if w.editing {
        p.border(field_bg, Color::TRANSPARENT, 5, Color::rgb(0, 96, 223));
    }
    let typed = w
        .title
        .split_once(" — ")
        .map_or(w.title.as_str(), |(_, a)| a);
    // The identity button at the bar's left: "Pick a search engine" on an empty tab, the
    // site information (with "Not Secure" over plain HTTP) over a page.
    let insecure = typed.starts_with("http://");
    let (info_w, info_label) = match (page, insecure) {
        (false, _) => (50, "Pick a search engine"),
        (true, true) => (108, "View site information"),
        (true, false) => (24, "View site information"),
    };
    let info = Rect::new(bar_x + 4, top + 8, info_w, 24);
    p.symbol(
        if page { "shield" } else { "search" },
        info.x + 4,
        info.y + 5,
        14,
        DIM,
    );
    // It opens Firefox's identity or search-engine panel, which the simulator does not
    // have: announced, with nothing to open.
    p.container(info, "button", info_label);
    if page && insecure {
        p.left(info.x + 23, info.y + 4, 77, "Not Secure", 13, INK);
        p.container(
            Rect::new(info.x + 23, info.y + 4, 77, 17),
            "label",
            "Not Secure",
        );
    }
    let field_x = info.x + info_w as i32 + 6;
    let field_end = if page { bar_end - 34 } else { bar_end - 10 };
    let field = Rect::new(field_x, top + 4, (field_end - field_x).max(20) as u32, 32);
    let name = if page {
        "Search with Google or enter address"
    } else {
        "Search or enter address"
    };
    p.region(field, &w.action("content:shell:address"), name);
    p.announce("combobox", (!typed.is_empty()).then_some(typed));
    if typed.is_empty() {
        p.left(field.x + 2, field.y + 8, field.width, name, 13, DIM);
    } else {
        let end = p.left(field.x + 2, field.y + 8, field.width, typed, 13, INK);
        if w.editing {
            p.box_(
                Rect::new(field.x + 3 + end as i32, field.y + 8, 1, 16),
                INK,
                0,
            );
        }
    }
    // The star saves the page to the machine's real bookmark list and reads its own
    // position back; Firefox shows it only over a page.
    if page {
        let star = Rect::new(field_end + 6, top + 8, 24, 24);
        let saved = ctx.bookmarked && w.focused;
        if ctx.hovered(star) {
            p.box_(star, Color(0, 0, 0, 16), 4);
        }
        p.symbol(
            if saved { "star" } else { "star-outline" },
            star.x + 4,
            star.y + 4,
            15,
            if saved { ORANGE } else { ink },
        );
        p.region(
            star,
            "shell:bookmark",
            if saved {
                "Edit this bookmark (Ctrl+D)"
            } else {
                "Bookmark this page (Ctrl+D)"
            },
        );
    }
    // Downloads appears once something was downloaded, as Firefox's does; its count is of
    // files the browser really wrote (saved with `shell:download`). It has no panel here.
    if !ctx.downloads.is_empty() {
        let downloads = Rect::new(right - 112, top, 36, FF_NAV_H as u32);
        p.symbol("download", downloads.x + 10, downloads.y + 12, 16, ink);
        p.container(downloads, "button", "Downloads");
        p.circle(downloads.x + 27, downloads.y + 10, 7, ORANGE);
        p.label(
            downloads.x + 20,
            downloads.y + 4,
            14,
            &ctx.downloads.len().min(99).to_string(),
            9,
            Color::WHITE,
            true,
            Align::Center,
        );
    }
    let extensions = Rect::new(right - 76, top, 36, FF_NAV_H as u32);
    // Extensions is announced as Firefox announces it; the simulator installs none, so
    // it has nothing to open and takes no click.
    p.symbol("apps", extensions.x + 10, extensions.y + 12, 16, ink);
    p.container(extensions, "button", "Extensions");
    // Firefox's ☰ is its settings menu, so it opens the machine's Settings panel.
    let menu = Rect::new(right - 40, top, 40, FF_NAV_H as u32);
    if ctx.hovered(menu) {
        p.box_(
            Rect::new(menu.x + 2, menu.y + 4, 36, 32),
            Color(0, 0, 0, 16),
            4,
        );
    }
    p.symbol("menu", menu.x + 12, menu.y + 12, 16, ink);
    p.region(menu, "shell:settings", "Firefox");
}

fn popover(p: &mut Painter, r: Rect) {
    p.drop_shadow(r, 20, 24, 90, 8);
    p.border(r, MENU_BG, 20, MENU_EDGE);
    p.region(r, "shell:noop", "System menu");
}

fn panel_surface(p: &mut Painter, ctx: &ShellContext<'_>, panel: &str) {
    if let Some(selected) = panel.strip_prefix("switcher:") {
        switcher(p, ctx, selected.parse().unwrap_or(1));
        return;
    }
    if let Some(command) = panel.strip_prefix("polkit:") {
        polkit(p, ctx, command);
        return;
    }
    if panel == "about:calculator" {
        calculator_about(p, ctx);
        return;
    }
    p.region(
        Rect::new(
            68,
            32,
            ctx.width.saturating_sub(68),
            ctx.height.saturating_sub(32),
        ),
        "shell:dismiss",
        "Dismiss system menu",
    );
    match panel {
        "calendar" | "notifications" => calendar(p, ctx),
        "settings" => settings(p, ctx),
        "context" => context_menu(p, ctx),
        "app-menu" => app_menu(p, ctx),
        _ => quick_settings(p, ctx),
    }
}

/// polkit's authentication prompt, as GNOME Shell 46 draws it for a program asking for
/// administrator rights (`pkexec`): the screen dimmed, the dialog modal over it. The
/// password typed is shown as dots; Authenticate runs the command as the super user, as
/// the machine's `sudo` does, and Cancel refuses it.
fn polkit(p: &mut Painter, ctx: &ShellContext<'_>, command: &str) {
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    p.box_(full, Color(0, 0, 0, 170), 0);
    p.region(full, "shell:noop", "Authentication Required");
    let r = Rect::new(
        ctx.width as i32 / 2 - 206,
        ctx.height as i32 / 2 - 195,
        411,
        389,
    );
    p.drop_shadow(r, 14, 24, 120, 6);
    p.border(r, MENU_BG, 14, MENU_EDGE);
    let inner = Rect::new(r.x + 40, r.y, 331, r.height);
    p.center(
        inner.x,
        r.y + 34,
        inner.width,
        "Authentication Required",
        18,
        INK,
    );
    let program = command.split_whitespace().next().unwrap_or(command);
    p.paragraph(
        inner.x,
        r.y + 76,
        inner.width,
        &format!("Authentication is needed to run `{program}' as the super user"),
        14,
        INK,
    );
    p.circle(r.x + 205, r.y + 176, 32, Color::rgb(232, 232, 232));
    p.symbol("person", r.x + 191, r.y + 162, 28, INK);
    p.center(inner.x, r.y + 216, inner.width, ctx.user, 14, INK);
    let field = Rect::new(r.x + 51, r.y + 259, 310, 38);
    p.border(field, Color::rgb(232, 232, 232), 8, ORANGE);
    p.region(field, "shell:noop", "Password");
    if ctx.search.is_empty() {
        p.left(
            field.x + 10,
            field.y + 10,
            field.width - 50,
            "Password",
            14,
            MENU_FAINT,
        );
    } else {
        let dots: String = "●".repeat(ctx.search.chars().count().min(32));
        p.left(field.x + 10, field.y + 10, field.width - 50, &dots, 12, INK);
    }
    p.symbol(
        "eye",
        field.x + field.width as i32 - 30,
        field.y + 11,
        16,
        INK,
    );
    let row = r.y + r.height as i32 - 42;
    let cancel = Rect::new(r.x, row, 205, 42);
    let auth = Rect::new(r.x + 206, row, 205, 42);
    p.box_(
        Rect::new(r.x + 1, row, r.width - 2, 41),
        Color::rgb(230, 230, 230),
        0,
    );
    p.vline(r.x + 205, row, 42, Color::rgb(218, 218, 218));
    p.button(
        cancel,
        if ctx.hovered(cancel) {
            Color::rgb(218, 218, 218)
        } else {
            Color::TRANSPARENT
        },
        0,
        "shell:polkit:cancel",
        "Cancel",
    );
    p.center(cancel.x, cancel.y + 12, cancel.width, "Cancel", 14, INK);
    if ctx.search.is_empty() {
        p.box_(auth, Color::TRANSPARENT, 0);
        p.disabled("Authenticate");
        p.center(
            auth.x,
            auth.y + 12,
            auth.width,
            "Authenticate",
            14,
            MENU_FAINT,
        );
    } else {
        p.button(
            auth,
            if ctx.hovered(auth) {
                Color::rgb(218, 218, 218)
            } else {
                Color::TRANSPARENT
            },
            0,
            "shell:polkit:authenticate",
            "Authenticate",
        );
        p.center(auth.x, auth.y + 12, auth.width, "Authenticate", 14, INK);
    }
}

/// GNOME Calculator 46's header bar in Basic mode: Undo (insensitive: the model keeps no
/// undo), the mode menu (Basic is the only mode the model has) and the Main Menu.
fn calculator_header(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    p.symbol("undo", r.x + 22, r.y + 20, 14, DIM);
    p.left(r.x + 40, r.y + 19, 50, "Undo", 14, DIM);
    p.insensitive(
        Rect::new(r.x + 15, r.y + 11, 79, 34),
        &w.action("content:calc:undo"),
        "Undo",
    );
    let mode = Rect::new(r.x + 123, r.y + 11, 74, 34);
    p.strong(mode.x + 10, mode.y + 8, 44, "Basic", 14, ink);
    p.symbol("chevron-down", mode.x + 56, mode.y + 13, 10, ink);
    p.insensitive(
        Rect::new(r.x + 114, r.y + 6, 98, 34),
        &w.action("content:calc:mode"),
        "Basic",
    );
    primary_menu(p, ctx, w, r.x + 206, "Main Menu", true, ink);
}

/// GNOME Calculator 46's Main Menu, dropped from its button: New Window, the number
/// formats (the model shows Automatic only), Preferences, Keyboard Shortcuts, Help and
/// About Calculator. What the model cannot do is listed insensitive.
fn calculator_menu(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView) {
    let r = Rect::new(w.rect.x + 102, w.rect.y + 52, 242, 358);
    p.drop_shadow(r, 12, 18, 110, 6);
    p.border(r, Color::rgb(250, 250, 250), 12, Color(0, 0, 0, 50));
    p.region(r, "shell:noop", "Main Menu");
    let left = r.x + 41;
    let shortcut = |p: &mut Painter, y: i32, text: &str| {
        p.right(r.x + 150, y, 73, text, 13, MENU_FAINT);
    };
    let row = |y: i32| Rect::new(r.x + 6, y - 8, r.width - 12, 32);
    // New Window opens another Calculator; it is the menu's first, focused, item.
    let new = row(r.y + 22);
    p.box_(new, Color(0, 0, 0, 18), 6);
    p.left(left, new.y + 7, 140, "New Window", 14, INK);
    shortcut(p, new.y + 7, "Ctrl+N");
    p.region(new, "shell:new", "New Window");
    p.left(left, r.y + 59, 160, "Number format", 14, MENU_FAINT);
    for (i, name) in ["Automatic", "Fixed", "Scientific", "Engineering"]
        .iter()
        .enumerate()
    {
        let item = row(r.y + 99 + i as i32 * 32);
        p.ring(r.x + 25, item.y + 16, 7, 1, Color::rgb(120, 120, 120));
        if i == 0 {
            p.circle(r.x + 25, item.y + 16, 3, INK);
        }
        p.left(left, item.y + 7, 160, name, 14, INK);
        p.insensitive(item, &w.action(&format!("content:calc:format:{i}")), name);
    }
    p.hline(r.x + 1, r.y + 218, r.width - 2, Color(0, 0, 0, 25));
    for (i, (name, keys)) in [
        ("Preferences", "Ctrl+,"),
        ("Keyboard Shortcuts", "Ctrl+?"),
        ("Help", "F1"),
    ]
    .iter()
    .enumerate()
    {
        let item = row(r.y + 240 + i as i32 * 32);
        p.left(left, item.y + 7, 150, name, 14, INK);
        shortcut(p, item.y + 7, keys);
        p.insensitive(item, &w.action(&format!("content:calc:menu:{i}")), name);
    }
    let about = row(r.y + 336);
    if ctx.hovered(about) {
        p.box_(about, Color(0, 0, 0, 18), 6);
    }
    p.left(left, about.y + 7, 150, "About Calculator", 14, INK);
    p.region(about, "shell:about", "About Calculator");
}

/// Calculator's About window (libadwaita's AdwAboutWindow, as Calculator 46 shows it) over
/// its parent: the icon, name, developer and version, then Website and Report an Issue
/// (which need the internet) and the Credits and Legal pages, which the model lacks.
fn calculator_about(p: &mut Painter, ctx: &ShellContext<'_>) {
    let parent = ctx
        .windows
        .iter()
        .find(|w| w.kind == "calculator" && !w.minimized)
        .map(|w| w.rect)
        .unwrap_or_else(|| Rect::new(66, 32, 360, 503));
    let height = 583.min(ctx.height.saturating_sub(32));
    let r = Rect::new(
        parent.x.clamp(0, (ctx.width as i32 - 360).max(0)),
        parent
            .y
            .clamp(32, (ctx.height as i32 - height as i32).max(32)),
        360,
        height,
    );
    p.drop_shadow(r, 12, 24, 120, 6);
    p.border(r, Color::rgb(250, 250, 250), 12, Color(0, 0, 0, 40));
    p.region(r, "shell:noop", "About");
    let close = Rect::new(r.x + 325, r.y + 10, 24, 24);
    p.circle(
        close.x + 12,
        close.y + 12,
        12,
        Color(0, 0, 0, if ctx.hovered(close) { 40 } else { 18 }),
    );
    p.symbol("close", close.x + 7, close.y + 7, 10, INK);
    p.region(close, "shell:dismiss", "Close");
    p.asset(
        Rect::new(r.x + 123, r.y + 65, 114, 114),
        "icon/ubuntu/calculator",
    );
    p.label(
        r.x + 12,
        r.y + 192,
        336,
        "Calculator",
        26,
        INK,
        true,
        Align::Center,
    );
    p.center(r.x + 12, r.y + 231, 336, "The GNOME Project", 14, INK);
    let version = Rect::new(r.x + 148, r.y + 258, 64, 30);
    p.box_(version, Color::rgb(250, 228, 218), 15);
    p.label(
        version.x,
        version.y + 6,
        64,
        "46.3",
        14,
        ORANGE,
        true,
        Align::Center,
    );
    let card = |p: &mut Painter, y: i32, h: u32| {
        p.border(
            Rect::new(r.x + 12, r.y + y, 336, h),
            Color::WHITE,
            12,
            Color::rgb(230, 230, 230),
        );
    };
    card(p, 280 + 32, 56);
    card(p, 352 + 32, 56);
    card(p, 424 + 32, 112);
    p.hline(r.x + 13, r.y + 512, 334, Color::rgb(230, 230, 230));
    for (i, (y, name, symbol)) in [
        (314, "Website", "link"),
        (386, "Report an Issue", "link"),
        (458, "Credits", "chevron-right"),
        (513, "Legal", "chevron-right"),
    ]
    .iter()
    .enumerate()
    {
        let row = Rect::new(r.x + 14, r.y + y, 336, 54);
        if row.y + 54 > r.y + r.height as i32 {
            break;
        }
        p.left(row.x + 12, row.y + 17, 260, name, 14, INK);
        p.symbol(symbol, row.x + 302, row.y + 20, 14, INK);
        p.insensitive(row, &format!("shell:about:{i}"), name);
    }
}

/// The windows Alt+Tab walks, most recently used first (the focused one, then down the
/// stack), as GNOME's switcher lists them.
pub(crate) fn switcher_order(windows: &[WindowView]) -> Vec<&WindowView> {
    let mut order: Vec<&WindowView> = windows.iter().rev().collect();
    order.sort_by_key(|w| (!w.focused, w.minimized));
    order
}

/// GNOME's window switcher, on screen while Alt is held after Tab: a preview of each
/// window with its application's icon over the corner, the selected one lit and named.
fn switcher(p: &mut Painter, ctx: &ShellContext<'_>, selected: usize) {
    // Alt+Tab has already raised the selection, so the list as it stood before the
    // switch has the two most recent windows the other way round.
    let mut order = switcher_order(ctx.windows);
    if order.is_empty() {
        return;
    }
    if order.len() > 1 {
        order.swap(0, 1);
    }
    let selected = selected % order.len();
    let n = order
        .len()
        .min(((ctx.width as i32 - 40) / 152).max(1) as usize);
    let width = n as i32 * 152 + 12;
    let r = Rect::new(
        ctx.width as i32 / 2 - width / 2,
        ctx.height as i32 / 2 - 97,
        width as u32,
        195,
    );
    p.drop_shadow(r, 20, 24, 110, 6);
    p.box_(r, Color::rgb(29, 29, 29), 20);
    for (i, w) in order.iter().take(n).enumerate() {
        let item = Rect::new(r.x + 12 + i as i32 * 152, r.y + 12, 140, 140);
        if i == selected {
            p.box_(item, Color::rgb(72, 72, 72), 16);
        }
        // The window's preview, scaled to fit, its header bar then its content.
        let (ww, wh) = (w.rect.width.max(1) as i32, w.rect.height.max(1) as i32);
        let scale = (110 * 1000 / ww).min(102 * 1000 / wh);
        let (tw, th) = (ww * scale / 1000, wh * scale / 1000);
        let thumb = Rect::new(
            item.x + 70 - tw / 2,
            item.y + 69 - th / 2,
            tw.max(4) as u32,
            th.max(4) as u32,
        );
        p.box_(thumb, Color::rgb(250, 250, 250), 3);
        let head = (46 * scale / 1000).min(th / 3);
        p.box_(
            Rect::new(thumb.x, thumb.y, thumb.width, head.max(1) as u32),
            HEADER,
            3,
        );
        if let Some(content) = &w.content {
            let body = Rect::new(
                thumb.x,
                thumb.y + head,
                thumb.width,
                (th - head).max(1) as u32,
            );
            p.thumbnail(content, body, 0);
        }
        p.asset(
            Rect::new(item.x + 87, item.y + 87, 45, 45),
            &format!("icon/ubuntu/{}", w.kind),
        );
        p.region(item, &w.action("focus"), &window_title(w));
    }
    if let Some(w) = order.get(selected) {
        p.label(
            r.x,
            r.y + 164,
            r.width,
            &window_title(w),
            15,
            Color::rgb(230, 230, 230),
            false,
            Align::Center,
        );
    }
}

/// The primary (hamburger) menu of the focused Text Editor or Terminal window, dropped
/// from the button that opened it. Every entry acts on the window, its document or a
/// real machine setting.
fn app_menu(p: &mut Painter, ctx: &ShellContext<'_>) {
    let Some(w) = ctx.windows.iter().find(|w| w.focused) else {
        return;
    };
    if w.kind == "calculator" {
        return calculator_menu(p, ctx, w);
    }
    let date = ctx.date();
    let entries: Vec<(String, String)> = match w.kind.as_str() {
        "editor" => vec![
            ("New Window".into(), "shell:new".into()),
            ("Save".into(), "shell:save".into()),
            (
                if ctx.settings.word_wrap {
                    "✓ Wrap Text".into()
                } else {
                    "Wrap Text".into()
                },
                "shell:toggle:word_wrap".into(),
            ),
            (
                "Insert Date and Time".into(),
                format!(
                    "shell:insert:{:04}-{:02}-{:02} {}",
                    date.year,
                    date.month,
                    date.day,
                    ctx.time()
                ),
            ),
            ("Close".into(), w.action("close")),
        ],
        // Nautilus's Main Menu, as far as the file manager goes: a new window, hidden
        // files shown or not, and closing.
        "files" => vec![
            ("New Window".into(), "shell:new".into()),
            (
                if w.chrome("hidden") == Some("1") {
                    "✓ Show Hidden Files".into()
                } else {
                    "Show Hidden Files".into()
                },
                w.action("content:files-hidden"),
            ),
            ("Close Window".into(), w.action("close")),
        ],
        "terminal" => vec![
            ("New Window".into(), "shell:new".into()),
            (
                if w.maximized {
                    "Leave Full Screen".into()
                } else {
                    "Full Screen".into()
                },
                w.action("maximize"),
            ),
            ("Reset and Clear".into(), "shell:terminal:clear".into()),
            ("Close Window".into(), w.action("close")),
        ],
        // Weather's primary menu: what its model can do from here.
        "weather" => {
            let metric = w.chrome("units") == Some("metric");
            vec![
                ("Refresh".into(), w.action("content:weather:reload")),
                (
                    format!("{}Fahrenheit", if metric { "" } else { "✓ " }),
                    w.action("content:weather:units:imperial"),
                ),
                (
                    format!("{}Celsius", if metric { "✓ " } else { "" }),
                    w.action("content:weather:units:metric"),
                ),
                ("Close".into(), w.action("close")),
            ]
        }
        _ => return,
    };
    let right = w.rect.x + w.rect.width as i32;
    let r = Rect::new(
        (right - 240).clamp(72, ctx.width as i32 - 224),
        w.rect.y + 44,
        220,
        entries.len() as u32 * 34 + 12,
    );
    p.drop_shadow(r, 12, 18, 110, 6);
    p.border(r, Color::rgb(250, 250, 250), 12, Color(0, 0, 0, 50));
    p.region(r, "shell:noop", "Menu");
    for (i, (label, action)) in entries.iter().enumerate() {
        let row = Rect::new(r.x + 6, r.y + 6 + i as i32 * 34, r.width - 12, 34);
        if ctx.hovered(row) {
            p.box_(row, Color(0, 0, 0, 18), 8);
        }
        p.left(row.x + 12, row.y + 9, row.width - 24, label, 13, INK);
        p.region(row, action, label);
    }
}

/// The date menu: notifications on the left, the calendar, events, world clocks and
/// weather on the right, as GNOME Shell 46 lays it out under Yaru's light style.
fn calendar(p: &mut Painter, ctx: &ShellContext<'_>) {
    let date = ctx.date();
    // The heading reports the world's own day; the grid below follows the paged month.
    let shown = ctx.panel_date();
    let paged = ctx.panel_month != 0;
    let two_column = ctx.width >= 900;
    let left = if two_column { 442 } else { 0 };
    let width = (left + 312).min(ctx.width.saturating_sub(8) as i32) as u32;
    // Centred under the clock, kept on screen.
    let centre = ctx.width as i32 / 2 + 34;
    let x = (centre - width as i32 / 2).clamp(4, (ctx.width as i32 - width as i32 - 4).max(4));
    let r = Rect::new(x, 38, width, 610.min(ctx.height.saturating_sub(46)));
    popover(p, r);
    if two_column {
        // The left column lists what applications really posted; when nothing has been
        // posted it says so, instead of saying so whatever the machine holds.
        if ctx.notifications.is_empty() {
            p.symbol("bell", x + 187, r.y + 249, 72, Color::rgb(137, 137, 137));
            p.center(
                x,
                r.y + 349,
                left as u32,
                "No Notifications",
                18,
                Color::rgb(128, 128, 128),
            );
        } else {
            let clear = Rect::new(x + left - 116, r.y + 14, 96, 30);
            p.button(
                clear,
                if ctx.hovered(clear) {
                    MENU_BUTTON_HOVER
                } else {
                    MENU_BUTTON
                },
                15,
                "shell:notifications:seen",
                "Mark all as read",
            );
            p.center(clear.x, clear.y + 7, clear.width, "Clear", 13, INK);
            for (i, notice) in ctx.notifications.iter().take(6).enumerate() {
                let row = Rect::new(x + 16, r.y + 54 + i as i32 * 70, left as u32 - 32, 62);
                p.border(
                    row,
                    if ctx.hovered(row) {
                        Color::rgb(242, 242, 242)
                    } else {
                        Color::WHITE
                    },
                    12,
                    Color::rgb(237, 237, 237),
                );
                p.asset(
                    Rect::new(row.x + 14, row.y + 15, 32, 32),
                    &format!("icon/ubuntu/{}", notice.app),
                );
                p.strong(
                    row.x + 58,
                    row.y + 11,
                    row.width - 90,
                    &notice.title,
                    13,
                    INK,
                );
                p.left(
                    row.x + 58,
                    row.y + 32,
                    row.width - 90,
                    &notice.body,
                    13,
                    MENU_DIM,
                );
                // Unread is a real flag on the notice, not a guess from its age.
                if !notice.seen {
                    p.circle(row.x + row.width as i32 - 18, row.y + 18, 4, ORANGE);
                }
                p.region(row, &format!("shell:notice:{i}"), &notice.title);
            }
        }
        p.vline(x + left, r.y + 11, r.height - 22, Color::rgb(199, 199, 199));
        let dnd_y = r.y + r.height as i32 - 40;
        p.left(x + 23, dnd_y, 160, "Do Not Disturb", 14, INK);
        let on = ctx.switch("do_not_disturb");
        let switch = Rect::new(x + 130, dnd_y - 4, 46, 26);
        p.button(
            switch,
            if on {
                ORANGE
            } else {
                Color::rgb(186, 186, 186)
            },
            13,
            "shell:toggle:do_not_disturb",
            if on {
                "Turn Do Not Disturb off"
            } else {
                "Turn Do Not Disturb on"
            },
        );
        // The knob sits where the switch really is.
        p.circle(
            switch.x + if on { 33 } else { 13 },
            switch.y + 13,
            11,
            Color::rgb(248, 247, 247),
        );
    }
    let cx = x + left + 15;
    let col: u32 = 282;
    p.left(
        cx + 10,
        r.y + 25,
        col - 20,
        date.weekday_name(),
        14,
        MENU_DIM,
    );
    p.left(
        cx + 10,
        r.y + 43,
        col - 20,
        &format!("{} {} {}", date.month_name(), date.day, date.year),
        19,
        MENU_DIM,
    );
    // Both chevrons page the grid's month for real.
    for (i, (symbol, action, label)) in [
        ("chevron-left", "shell:month:prev", "Previous month"),
        ("chevron-right", "shell:month:next", "Next month"),
    ]
    .into_iter()
    .enumerate()
    {
        let hit = Rect::new(cx + 1 + i as i32 * 242, r.y + 89, 38, 38);
        if ctx.hovered(hit) {
            p.box_(hit, Color(0, 0, 0, 18), 19);
        }
        p.symbol(symbol, hit.x + 13, hit.y + 13, 12, Color::rgb(80, 80, 80));
        p.region(hit, action, label);
    }
    // GNOME's month heading returns to today, which only moves when the grid has left it;
    // it names the year only when the shown month is in another.
    let heading = Rect::new(cx + 67, r.y + 91, 147, 34);
    if paged && ctx.hovered(heading) {
        p.box_(heading, Color(0, 0, 0, 18), 17);
    }
    p.center(
        heading.x,
        heading.y + 8,
        heading.width,
        &if shown.year != date.year {
            format!("{} {}", shown.month_name(), shown.year)
        } else {
            shown.month_name().to_owned()
        },
        14,
        INK,
    );
    if paged {
        p.region(
            heading,
            "shell:month:today",
            &format!("Back to {} {}", date.month_name(), date.year),
        );
    }
    // en_US weeks begin on Sunday; the grid always shows six of them, the neighbouring
    // months' days greyed.
    for (i, day) in ["S", "M", "T", "W", "T", "F", "S"].iter().enumerate() {
        p.center(
            cx + 5 + i as i32 * 40,
            r.y + 134,
            32,
            day,
            11,
            Color::rgb(186, 186, 186),
        );
    }
    let offset = shown.first_weekday as i64;
    let before = super::shared::CalendarDate::shifted_months(ctx.clock_us, ctx.panel_month - 1)
        .days_in_month as i64;
    for slot in 0..42i64 {
        let (dx, dy) = (
            cx + 3 + (slot % 7) as i32 * 40,
            r.y + 159 + (slot / 7) as i32 * 40,
        );
        let n = slot - offset + 1;
        let (day, here) = if n < 1 {
            (before + n, false)
        } else if n > shown.days_in_month as i64 {
            (n - shown.days_in_month as i64, false)
        } else {
            (n, true)
        };
        let weekend = slot % 7 == 0 || slot % 7 == 6;
        let ink = match (here, weekend) {
            (false, _) => MENU_FAINT,
            (true, true) => Color::rgb(120, 120, 120),
            (true, false) => INK,
        };
        // Each day opens Calendar on that day; days before the world began are only text.
        if here {
            if let Some(open) = ctx.open_day(day as u64) {
                p.region_above(
                    Rect::new(dx, dy, 36, 36),
                    &open,
                    &format!("{} {day}", shown.month_name()),
                );
            }
        }
        // Only the world's own day is ringed, and only on its own month.
        let today = here && !paged && day as u64 == date.day;
        if today {
            p.circle(dx + 18, dy + 18, 18, ORANGE);
        }
        p.label(
            dx,
            dy + 9,
            36,
            &format!("{day:02}"),
            13,
            if today { Color::WHITE } else { ink },
            today,
            Align::Center,
        );
    }
    // The events of the day, then world clocks and weather, each a button opening its
    // application when the machine has it.
    let cards: [(i32, u32, &str, &str); 3] = [
        (r.y + 412, 68, "calendar", "Open calendar application"),
        (r.y + 494, 44, "clock", "Add world clocks…"),
        (r.y + 552, 44, "weather", "Select weather location…"),
    ];
    for (i, (y, h, app, label)) in cards.into_iter().enumerate() {
        let card = Rect::new(cx, y, col, h);
        if y + h as i32 > r.y + r.height as i32 - 4 {
            break;
        }
        let live = ctx.installed(app);
        p.border(
            card,
            if live && ctx.hovered(card) {
                Color::rgb(242, 242, 242)
            } else {
                Color::WHITE
            },
            10,
            Color::rgb(237, 237, 237),
        );
        if live {
            p.region(card, &format!("shell:launch:{app}"), label);
        }
        if i == 0 {
            p.left(card.x + 13, card.y + 13, col - 26, "Today", 14, MENU_DIM);
            p.left(
                card.x + 13,
                card.y + 37,
                col - 26,
                "No Events",
                13,
                MENU_DIM,
            );
        } else {
            p.left(card.x + 13, card.y + 13, col - 26, label, 14, INK);
        }
    }
}

/// The system menu (quick settings) of GNOME Shell 46 under Yaru's light style: the
/// round system buttons, the sliders the machine has, then two columns of toggles.
fn quick_settings(p: &mut Painter, ctx: &ShellContext<'_>) {
    let width = 402.min(ctx.width.saturating_sub(84));
    let x = ctx.width as i32 - width as i32 - 6;
    let inner = width as i32 - 38;
    let r = Rect::new(x, 38, width, 376);
    popover(p, r);
    let on_text = Color::WHITE;
    let button = |p: &mut Painter, hit: Rect| {
        p.circle(
            hit.x + 19,
            hit.y + 19,
            19,
            if ctx.hovered(hit) {
                MENU_BUTTON_HOVER
            } else {
                MENU_BUTTON
            },
        );
    };
    // Top row: a laptop's battery pill, then round system buttons. A desktop computer
    // has no battery, so GNOME shows no pill there. The machine keeps no charge model:
    // a laptop here is on mains power and full, which is what the pill says.
    let pill = if ctx.battery { 100 } else { 0 };
    if ctx.battery {
        let b = Rect::new(x + 19, r.y + 19, 88, 38);
        p.box_(
            b,
            if ctx.hovered(b) {
                MENU_BUTTON_HOVER
            } else {
                MENU_BUTTON
            },
            19,
        );
        p.symbol("battery", b.x + 12, b.y + 11, 16, INK);
        p.left(b.x + 34, b.y + 10, 50, "100 %", 13, INK);
        p.region(b, "shell:settings", "Power settings, fully charged");
    }
    // Screenshot rasterises the display and writes a real PNG to ~/Pictures; every
    // button in this row changes the machine rather than decorating the panel.
    for (i, (symbol, action, label)) in [
        ("screenshot", "shell:screenshot", "Take Screenshot"),
        ("gear", "shell:settings", "Settings"),
        ("lock", "shell:power:lock", "Lock Screen"),
        ("power", "shell:power:off", "Power Off Menu"),
    ]
    .iter()
    .enumerate()
    {
        // GNOME 46: screenshot and settings from the left, lock and power at the right.
        let bx = match i {
            0 | 1 => x + 19 + pill + i as i32 * 50,
            _ => x + 19 + inner - 38 - (3 - i as i32) * 50,
        };
        let hit = Rect::new(bx, r.y + 19, 38, 38);
        button(p, hit);
        p.symbol(symbol, bx + 11, r.y + 30, 16, INK);
        p.region(hit, action, label);
    }
    // Sliders are twenty-one discrete stops, so a click lands on an exact level.
    const STOPS: i32 = 20;
    for (i, (symbol, name)) in [("volume", "volume"), ("sun", "brightness")]
        .iter()
        .enumerate()
    {
        let sy = r.y + 77 + i as i32 * 44;
        p.symbol(symbol, x + 25, sy, 16, INK);
        let track = Rect::new(x + 56, sy + 6, width - 80, 4);
        let span = track.width as i32;
        for step in 0..=STOPS {
            let lo = if step == 0 {
                0
            } else {
                span * (2 * step - 1) / (2 * STOPS)
            };
            let hi = if step == STOPS {
                span
            } else {
                span * (2 * step + 1) / (2 * STOPS)
            };
            let percent = step * 100 / STOPS;
            p.button(
                Rect::new(track.x + lo, sy - 2, (hi - lo).max(1) as u32, 24),
                Color::TRANSPARENT,
                0,
                &format!("shell:set:{name}:{percent}"),
                &format!("Set {name} to {percent} %"),
            );
        }
        // The filled portion is the level the machine really holds.
        let fill = track.width * u32::from(ctx.level(name)).min(100) / 100;
        p.box_(track, Color::rgb(210, 210, 210), 2);
        p.box_(Rect::new(track.x, track.y, fill, 4), ORANGE, 2);
        p.circle(track.x + fill as i32, track.y + 2, 10, MENU_EDGE);
        p.circle(track.x + fill as i32, track.y + 2, 9, Color::WHITE);
    }
    let pill = (inner as u32 - 12) / 2;
    for (i, (symbol, name, switch)) in [
        ("wifi", "Wi-Fi", "wifi"),
        ("bluetooth", "Bluetooth", "bluetooth"),
        ("leaf", "Power Mode", "battery_saver"),
        ("night-light", "Night Light", "night_light"),
        ("moon", "Dark Style", "dark_mode"),
        ("airplane", "Airplane Mode", "airplane_mode"),
    ]
    .iter()
    .enumerate()
    {
        let on = ctx.switch(switch);
        let px = x + 19 + (i % 2) as i32 * (pill as i32 + 12);
        let py = r.y + 165 + (i / 2) as i32 * 60;
        let slot = Rect::new(px, py, pill, 48);
        p.button(
            slot,
            match (on, ctx.hovered(slot)) {
                (true, true) => Color::rgb(243, 104, 52),
                (true, false) => ORANGE,
                (false, true) => MENU_BUTTON_HOVER,
                (false, false) => MENU_BUTTON,
            },
            24,
            &format!("shell:toggle:{switch}"),
            &format!("Turn {name} {}", if on { "off" } else { "on" }),
        );
        let ink = if on { on_text } else { INK };
        p.symbol(symbol, px + 16, py + 16, 16, ink);
        // Each pill states the position the switch is really in.
        let detail = match (*switch, on) {
            ("wifi", true) => "Connected",
            ("battery_saver", true) => "Power Saver",
            ("battery_saver", false) => "Balanced",
            (_, true) => "On",
            (_, false) => "Off",
        };
        p.left(px + 40, py + 7, pill - 50, name, 14, ink);
        p.left(px + 40, py + 25, pill - 50, detail, 12, ink);
    }
}

fn settings(p: &mut Painter, ctx: &ShellContext<'_>) {
    let width = 560.min(ctx.width.saturating_sub(90));
    let height = 360.min(ctx.height.saturating_sub(60));
    let x = 68 + (ctx.width as i32 - 68 - width as i32) / 2;
    let y = 32 + (ctx.height as i32 - 32 - height as i32) / 2;
    let r = Rect::new(x, y, width, height);
    p.drop_shadow(r, 12, 30, 110, 12);
    p.border(r, Color::rgb(250, 250, 250), 12, Color(0, 0, 0, 110));
    p.region(r, "shell:noop", "Settings");
    p.box_(Rect::new(x + 1, y + 1, width - 2, 45), HEADER, 11);
    p.box_(Rect::new(x + 1, y + 24, width - 2, 22), HEADER, 0);
    p.strong_center(x, y + 14, width, "About", 14, INK);
    let close = Rect::new(x + width as i32 - 36, y + 9, 28, 28);
    p.circle(
        close.x + 14,
        close.y + 14,
        12,
        Color(0, 0, 0, if ctx.hovered(close) { 50 } else { 22 }),
    );
    p.symbol("close", close.x + 9, close.y + 9, 10, INK);
    p.region(close, "shell:dismiss", "Close Settings");
    p.strong_center(x, y + 70, width, "Ubuntu 24.04 LTS", 22, INK);
    let card = Rect::new(x + 40, y + 120, width - 80, 144);
    p.border(card, Color::WHITE, 12, Color(0, 0, 0, 30));
    // GNOME's About rows are read-only facts, so they are painted, never clickable.
    for (i, (key, value)) in [
        ("Device Name", "alice-ubuntu".to_owned()),
        ("Display", format!("{} × {}", ctx.width, ctx.height)),
        ("Windowing System", "Wayland".to_owned()),
        ("Open Windows", ctx.windows.len().to_string()),
    ]
    .iter()
    .enumerate()
    {
        let ry = card.y + 10 + i as i32 * 34;
        p.left(card.x + 16, ry, 200, key, 13, INK);
        p.right(card.x + card.width as i32 - 216, ry, 200, value, 13, DIM);
        if i < 3 {
            p.hline(card.x + 1, ry + 25, card.width - 2, Color(0, 0, 0, 20));
        }
    }
    let launch = Rect::new(x + width as i32 / 2 - 90, y + height as i32 - 60, 180, 34);
    p.button(launch, ORANGE, 8, "shell:launcher", "Show Applications");
    p.strong_center(
        launch.x,
        launch.y + 9,
        180,
        "Show Applications",
        13,
        Color::WHITE,
    );
}

fn context_menu(p: &mut Painter, ctx: &ShellContext<'_>) {
    let (mx, my) = ctx
        .anchor
        .or(ctx.hover)
        .unwrap_or((ctx.width as i32 / 2, ctx.height as i32 / 3));
    let mut entries: Vec<(String, String)> = Vec::new();
    // Over Files with a file selected, the menu is about that file first, as Nautilus's
    // is: open it, and star or unstar it (the grid has no star column).
    if let Some(w) = ctx
        .windows
        .iter()
        .find(|w| w.focused && !w.minimized && w.kind == "files" && !w.selection.is_empty())
    {
        entries.push(("Open".into(), w.action("content:files-open")));
        let starred = w.chrome("starred") == Some("1");
        entries.push((
            if starred { "Unstar" } else { "Star" }.into(),
            w.action("content:files-star"),
        ));
        // Nautilus's own pair: Move to Trash in a folder, Restore in the Trash. Which
        // one is offered comes from the window, so the menu never names an action the
        // file manager would refuse.
        entries.push(if w.chrome("trash") == Some("1") {
            ("Restore".into(), w.action("content:files-restore"))
        } else {
            (
                "Move to Trash".into(),
                w.action("content:files-move-to-trash"),
            )
        });
    }
    for (label, action) in [
        ("New Window", "shell:new"),
        ("Open in Terminal", "shell:launch:terminal"),
        ("Show Applications", "shell:launcher"),
        ("Settings", "shell:settings"),
    ] {
        entries.push((label.into(), action.into()));
    }
    let tall = entries.len() as i32 * 34 + 12;
    let r = Rect::new(
        mx.clamp(72, ctx.width as i32 - 224),
        my.clamp(36, (ctx.height as i32 - tall - 2).max(36)),
        220,
        tall as u32,
    );
    p.drop_shadow(r, 12, 18, 110, 6);
    p.border(r, Color::rgb(250, 250, 250), 12, Color(0, 0, 0, 50));
    p.region(r, "shell:noop", "Menu");
    for (i, (label, action)) in entries.iter().enumerate() {
        let row = Rect::new(r.x + 6, r.y + 6 + i as i32 * 34, r.width - 12, 34);
        if ctx.hovered(row) {
            p.box_(row, Color(0, 0, 0, 18), 8);
        }
        p.left(row.x + 12, row.y + 9, row.width - 24, label, 13, INK);
        p.region(row, action, label);
    }
}

/// A header-bar button whose function the application lacks: drawn greyed and announced
/// insensitive, its action one the application refuses. GTK 4 lists a menu button twice
/// (the button and the toggle inside it, 5 px in), so `menu` announces both.
fn gnome_insensitive(
    p: &mut Painter,
    w: &WindowView,
    (x, y, height): (i32, i32, u32),
    symbol: &str,
    command: &str,
    label: &str,
    menu: bool,
) {
    let r = w.rect;
    let inner = Rect::new(r.x + x, r.y + y, 34, height);
    p.symbol(
        symbol,
        inner.x + 9,
        inner.y + (height as i32 - 16) / 2,
        16,
        DIM,
    );
    if menu {
        p.insensitive(
            Rect::new(inner.x - 5, inner.y - 5, 34, height),
            &w.action(&format!("content:{command}")),
            label,
        );
    }
    p.insensitive(
        inner,
        &w.action(&format!(
            "content:{command}{}",
            if menu { "-toggle" } else { "" }
        )),
        label,
    );
}

/// GNOME Contacts 46's header bar over its sidebar (measured on the real application,
/// fidelity/tours): Add New Contact, the "Contacts" title, Select Contacts and the main
/// menu. The address book is the people the machine's services know, so nothing here
/// adds, selects or exports: each is shown insensitive.
fn contacts_header(p: &mut Painter, _ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    gnome_insensitive(
        p,
        w,
        (11, 11, 34),
        "plus",
        "contacts:add",
        "Add New Contact",
        false,
    );
    p.strong_center(r.x + 45, r.y + 15, 140, "Contacts", 14, ink);
    gnome_insensitive(
        p,
        w,
        (185, 11, 34),
        "check",
        "contacts:select",
        "Select Contacts",
        false,
    );
    gnome_insensitive(
        p,
        w,
        (225, 11, 34),
        "menu",
        "contacts:menu",
        "Main Menu",
        true,
    );
}

/// GNOME Settings 46's header bars (measured on the real window, fidelity/tours): over
/// the 242 px sidebar, in the sidebar's grey, Search, the Settings title and the Main
/// Menu (GTK 4 lists a menu button and its toggle); over the panel, flat in the panel's
/// own 250 with no rule below, the panel's title, a label centred 618 px in. Settings has
/// no search or menu model here, so both are insensitive.
fn settings_header(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    let radius = super::corner_radius(ctx.theme, w.maximized).saturating_sub(1);
    let pane = Rect::new(r.x + 242, r.y + 1, (r.width as i32 - 243).max(0) as u32, 46);
    let flat = Color::rgb(250, 250, 250);
    p.box_(pane, flat, radius);
    p.box_(Rect::new(pane.x, pane.y, 24, pane.height), flat, 0);
    p.box_(
        Rect::new(pane.x, pane.y + 24, pane.width, pane.height - 24),
        flat,
        0,
    );
    // The sidebar's half runs on into the sidebar, with no rule either.
    let side = if w.focused { HEADER } else { HEADER_BACKDROP };
    p.box_(Rect::new(r.x + 1, r.y + 45, 241, 1), side, 0);
    p.vline(r.x + 242, r.y + 1, 46, Color(0, 0, 0, 24));
    gnome_insensitive(
        p,
        w,
        (11, 11, 34),
        "search",
        "settings:search",
        "Search",
        false,
    );
    p.strong_center(r.x + 93, r.y + 14, 81, "Settings", 14, ink);
    p.presentation();
    primary_menu(p, ctx, w, r.x + 202, "Main Menu", false, ink);
    let title = w.chrome("panel").unwrap_or("Settings");
    let tw = p.measure(title, 14, true).max(1);
    let tx = r.x + 618 - tw as i32 / 2;
    p.strong(tx, r.y + 14, tw, title, 14, ink);
    p.presentation();
    // GTK's box for the title is the text and 27 px of the window title's margins.
    announced(p, Rect::new(tx - 13, r.y + 14, tw + 27, 18), "label", title);
}

/// GNOME Maps 46's header bar (measured on the real application, fidelity/tours): Current
/// Location and Layers at the left, the search entry centred, then Explore Nearby Places,
/// Favorites, Route Planner and the main menu. The search entry searches the service's
/// places for real; the model has no position, map layers, popovers or menu, so the
/// other buttons are insensitive.
fn maps_header(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    let right = r.width as i32;
    gnome_insensitive(
        p,
        w,
        (11, 11, 34),
        "location",
        "maps:location",
        "Current Location",
        false,
    );
    gnome_insensitive(p, w, (51, 11, 34), "layers", "maps:layers", "Layers", true);
    let field = Rect::new(r.x + right / 2 - 243, r.y + 6, 462, 35);
    let typing = w.chrome("typing") == Some("1");
    let drawn = Rect::new(field.x - 4, field.y + 5, field.width - 4, 34);
    if typing {
        p.border(drawn, Color::rgb(235, 235, 235), 8, ORANGE);
    } else {
        p.box_(drawn, Color::rgb(235, 235, 235), 8);
    }
    p.region(field, &w.action("content:maps:search-field"), "");
    p.announce("textbox", None);
    p.symbol("search", drawn.x + 10, drawn.y + 9, 16, DIM);
    let query = w.chrome("query").unwrap_or("");
    // The placeholder and the query are the entry's own text, not labels of their own.
    if query.is_empty() {
        p.left(drawn.x + 31, drawn.y + 8, 200, "Search", 14, DIM);
    } else {
        p.left(
            drawn.x + 31,
            drawn.y + 8,
            drawn.width.saturating_sub(40),
            query,
            14,
            ink,
        );
    }
    p.presentation();
    // Explore Nearby Places sits joined to the entry's end.
    let explore = Rect::new(field.x + field.width as i32 - 5, r.y + 11, 34, 34);
    p.box_(explore, Color::rgb(242, 242, 242), 8);
    p.symbol("chevron-down", explore.x + 11, explore.y + 12, 12, DIM);
    p.insensitive(
        explore,
        &w.action("content:maps:explore"),
        "Explore Nearby Places",
    );
    let _ = ctx;
    gnome_insensitive(
        p,
        w,
        (right - 239, 11, 34),
        "star-filled",
        "maps:favorites",
        "Favorites",
        true,
    );
    gnome_insensitive(
        p,
        w,
        (right - 199, 11, 34),
        "fork",
        "maps:route-planner",
        "Route Planner",
        false,
    );
    gnome_insensitive(
        p,
        w,
        (right - 159, 11, 35),
        "menu",
        "maps:menu",
        "Main Menu",
        true,
    );
}

/// Image Viewer's (Eye of GNOME 45) header bar: the zoom group (Zoom Out, the zoom level
/// entry with its drop-down, Zoom In) at the left, Fullscreen and the menu at the right.
/// The viewer has no zoom or full-screen view of its own, so they are insensitive, as
/// they are in the real one with no image open.
fn viewer_header(p: &mut Painter, _ctx: &ShellContext<'_>, w: &WindowView, ink: Color) {
    let r = w.rect;
    let right = r.width as i32;
    let group = Rect::new(r.x + 10, r.y + 10, 160, 34);
    p.border(group, Color::rgb(40, 40, 40), 8, Color::rgb(28, 28, 28));
    let zoom = |p: &mut Painter, x: i32, width: u32, symbol: &str, command: &str, label: &str| {
        let b = Rect::new(r.x + x, r.y + 4, width, 46);
        p.symbol(symbol, b.x + (width as i32 - 16) / 2, b.y + 15, 16, DIM);
        p.insensitive(b, &w.action(&format!("content:photos:{command}")), label);
    };
    zoom(p, 10, 35, "zoom-out", "zoom-out", "Zoom Out");
    let entry = Rect::new(r.x + 45, r.y + 4, 89, 46);
    p.vline(entry.x, r.y + 10, 34, Color::rgb(28, 28, 28));
    p.vline(entry.x + 89, r.y + 10, 34, Color::rgb(28, 28, 28));
    p.left(entry.x + 22, entry.y + 14, 40, "100%", 13, DIM);
    p.presentation();
    p.insensitive(entry, &w.action("content:photos:zoom-level"), "");
    p.announce("textbox", None);
    p.symbol("chevron-down", r.x + 110, r.y + 19, 16, DIM);
    p.container(
        Rect::new(r.x + 110, r.y + 19, 16, 16),
        "img",
        "go-down-symbolic",
    );
    if let Some(s) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
        s.disabled = true;
    }
    zoom(p, 134, 36, "zoom-in", "zoom-in", "Zoom In");
    let full = Rect::new(r.x + right - 215, r.y + 10, 36, 34);
    p.border(full, Color::rgb(40, 40, 40), 8, Color::rgb(28, 28, 28));
    zoom(p, right - 215, 36, "resize", "fullscreen", "Fullscreen");
    let menu = Rect::new(r.x + right - 173, r.y + 10, 36, 34);
    p.box_(menu, Color::rgb(58, 58, 58), 8);
    p.symbol("menu", menu.x + 10, menu.y + 9, 16, ink);
    p.insensitive(
        Rect::new(menu.x, r.y + 4, 36, 46),
        &w.action("content:photos:menu"),
        "Menu",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_scene::DesktopTheme;

    fn context<'a>(windows: &'a [WindowView], active: bool) -> ShellContext<'a> {
        ShellContext {
            theme: DesktopTheme::Ubuntu,
            width: 1024,
            height: 768,
            clock_us: 0,
            title: "",
            launcher_open: false,
            active,
            windows,
            installed_apps: &[],
            panel: None,
            search: "",
            hover: None,
            desktop_selection: None,
            settings: &crate::SystemSettings::DEFAULT,
            screen: crate::ScreenState::Active,
            panel_month: 0,
            text_entry: false,
            keyboard: crate::KeyboardState::default(),
            bookmarks: &[],
            downloads: &[],
            notifications: &[],
            workspaces: 1,
            workspace: 0,
            library_group: None,
            bookmarked: false,
            panel_over_launcher: false,
            home_page: 0,
            typed: "",
            user: "alice",
            home: "/Users/alice",
            recents: &[],
            battery: false,
            anchor: None,
            overview: Default::default(),
        }
    }
    #[test]
    fn titlebar_controls_do_not_get_captured_by_drag_region() {
        let windows = [WindowView {
            id: 42,
            title: "Files".into(),
            kind: "files".into(),
            rect: Rect::new(120, 90, 700, 430),
            focused: true,
            can_go_back: true,
            ..Default::default()
        }];
        let ctx = context(&windows, true);
        let mut painter = Painter::new(1024, 768);
        window_frame(&mut painter, &ctx, &windows[0]);
        for (x, expected) in [
            (320, "content:files-back"),
            (397, "drag"),
            (730, "minimize"),
            (764, "maximize"),
            (798, "close"),
        ] {
            assert_eq!(
                painter
                    .scene
                    .hit_test(x, 113)
                    .and_then(|n| n.interaction.as_deref()),
                Some(format!("window:42:{expected}").as_str())
            );
        }
    }
    #[test]
    fn dock_restores_latest_running_window_and_launches_missing_apps() {
        let windows = [7, 19].map(|id| WindowView {
            id,
            title: "Terminal".into(),
            kind: "terminal".into(),
            rect: Rect::new(120, 90, 600, 400),
            minimized: true,
            ..Default::default()
        });
        let ctx = context(&windows, false);
        let mut painter = Painter::new(1024, 768);
        chrome(&mut painter, &ctx);
        // The Terminal slot, wherever the dock's order puts it, raises the latest window.
        let slot = painter
            .scene
            .nodes
            .iter()
            .find(|n| n.semantic.as_ref().is_some_and(|s| s.label == "Terminal"))
            .expect("a Terminal slot");
        let b = slot.bounds;
        assert_eq!(
            painter
                .scene
                .hit_test(32, b.y + b.height as i32 / 2)
                .and_then(|n| n.interaction.as_deref()),
            Some("window:19:focus")
        );
        assert_eq!(
            painter
                .scene
                .hit_test(32, 68)
                .and_then(|n| n.interaction.as_deref()),
            Some("shell:launch:browser")
        );
    }
    #[test]
    fn system_menu_absorbs_clicks_and_dismisses_outside() {
        let mut ctx = context(&[], false);
        ctx.panel = Some("quick");
        let mut painter = Painter::new(1024, 768);
        chrome(&mut painter, &ctx);
        let hit = |x, y| {
            painter
                .scene
                .hit_test(x, y)
                .and_then(|n| n.interaction.as_deref())
        };
        // Bare popover background between Settings and Lock Screen.
        assert_eq!(hit(800, 76), Some("shell:noop"));
        assert_eq!(hit(300, 500), Some("shell:dismiss"));
        // The 402 px menu sits 6 px from the right; Settings is its second button.
        assert_eq!(
            hit(1024 - 6 - 402 + 19 + 50 + 19, 76),
            Some("shell:settings")
        );
    }
    #[test]
    fn quick_settings_pills_switches_and_power_buttons_carry_real_ids() {
        let mut ctx = context(&[], false);
        ctx.panel = Some("quick");
        let mut painter = Painter::new(1024, 768);
        chrome(&mut painter, &ctx);
        let hit = |x, y| {
            painter
                .scene
                .hit_test(x, y)
                .and_then(|n| n.interaction.as_deref())
        };
        // Six pills, two per row, in the order the panel paints them: 176 px wide,
        // 12 apart, 19 px in from the 402 px menu's edge.
        let x = 1024 - 6 - 402;
        let (left, right) = (x + 19 + 40, x + 19 + 188 + 40);
        for (row, (a, b)) in [
            ("wifi", "bluetooth"),
            ("battery_saver", "night_light"),
            ("dark_mode", "airplane_mode"),
        ]
        .iter()
        .enumerate()
        {
            let y = 38 + 165 + row as i32 * 60 + 24;
            assert_eq!(hit(left, y), Some(format!("shell:toggle:{a}").as_str()));
            assert_eq!(hit(right, y), Some(format!("shell:toggle:{b}").as_str()));
        }
        // No battery pill: the machine has no battery. Screenshot, Settings, lock
        // and power off all change the machine.
        // GNOME 46: Screenshot and Settings at the left, Lock Screen and Power Off at the
        // right, 38 px buttons 50 apart.
        assert_eq!(hit(x + 200, 76), Some("shell:noop"));
        assert_eq!(hit(x + 19 + 19, 76), Some("shell:screenshot"));
        assert_eq!(hit(x + 19 + 50 + 19, 76), Some("shell:settings"));
        assert_eq!(hit(x + 402 - 19 - 50 - 19, 76), Some("shell:power:lock"));
        assert_eq!(hit(x + 402 - 19 - 19, 76), Some("shell:power:off"));
    }
    #[test]
    fn sliders_are_discrete_stops_that_set_the_level_they_paint() {
        let mut ctx = context(&[], false);
        ctx.panel = Some("quick");
        let settings = crate::SystemSettings {
            volume: 25,
            ..crate::SystemSettings::DEFAULT
        };
        ctx.settings = &settings;
        let mut painter = Painter::new(1024, 768);
        chrome(&mut painter, &ctx);
        // The 402 px menu's tracks start 56 px in, 77 px under its top, 322 px long.
        let track = Rect::new(1024 - 6 - 402 + 56, 38 + 77 + 6, 402 - 80, 4);
        let hit = |x, y| {
            painter
                .scene
                .hit_test(x, y)
                .and_then(|n| n.interaction.as_deref())
        };
        assert_eq!(hit(track.x, track.y), Some("shell:set:volume:0"));
        assert_eq!(
            hit(track.x + track.width as i32 / 2, track.y),
            Some("shell:set:volume:50")
        );
        assert_eq!(
            hit(track.x + track.width as i32 - 1, track.y),
            Some("shell:set:volume:100")
        );
        assert_eq!(
            hit(track.x + 64, 38 + 77 + 44 + 6),
            Some("shell:set:brightness:20")
        );
        // The orange fill reports the level the machine really holds.
        let fill = track.width * 25 / 100;
        assert!(painter
            .scene
            .nodes
            .iter()
            .any(|n| n.bounds == Rect::new(track.x, track.y, fill, 4)));
    }
    #[test]
    fn do_not_disturb_switch_toggles_and_shows_its_real_position() {
        let on = crate::SystemSettings {
            do_not_disturb: true,
            ..crate::SystemSettings::DEFAULT
        };
        let mut positions = Vec::new();
        for settings in [&crate::SystemSettings::DEFAULT, &on] {
            let mut ctx = context(&[], false);
            ctx.panel = Some("calendar");
            ctx.settings = settings;
            let mut painter = Painter::new(1024, 768);
            chrome(&mut painter, &ctx);
            // The 754 px date menu is centred under the clock (34 px right of the
            // screen's centre); Do Not Disturb sits 40 px over its bottom.
            let x = 1024 / 2 + 34 - 754 / 2;
            let switch = Rect::new(x + 130, 38 + 610 - 44, 46, 26);
            assert_eq!(
                painter
                    .scene
                    .hit_test(switch.x + 22, switch.y + 12)
                    .and_then(|n| n.interaction.as_deref()),
                Some("shell:toggle:do_not_disturb")
            );
            positions.push(
                painter
                    .scene
                    .nodes
                    .iter()
                    .filter(|n| n.bounds.width == 22 && n.bounds.y == switch.y + 2)
                    .map(|n| n.bounds.x)
                    .next_back()
                    .expect("the knob is painted"),
            );
        }
        assert!(positions[0] < positions[1]);
    }
    #[test]
    fn locked_and_powered_off_screens_cover_everything_with_a_real_way_back() {
        for screen in [crate::ScreenState::Locked, crate::ScreenState::Off] {
            let windows = [WindowView {
                id: 3,
                kind: "terminal".into(),
                rect: Rect::new(120, 90, 600, 400),
                ..Default::default()
            }];
            let mut ctx = context(&windows, true);
            ctx.screen = screen;
            ctx.panel = Some("quick");
            let mut painter = Painter::new(1024, 768);
            painter.z = 1_000_000;
            chrome(&mut painter, &ctx);
            for (x, y) in [(20, 20), (512, 384), (1000, 700)] {
                assert_eq!(
                    painter
                        .scene
                        .hit_test(x, y)
                        .and_then(|n| n.interaction.as_deref()),
                    Some("shell:power:wake"),
                    "{screen:?} leaks a control at {x},{y}"
                );
            }
            // Neither the dock nor the system menu survives behind the shield.
            assert!(!painter
                .scene
                .nodes
                .iter()
                .any(|n| n.interaction.as_deref() == Some("shell:launcher")));
        }
    }
    #[test]
    fn files_header_navigates_by_breadcrumb_and_offers_forward() {
        let window = |width: u32| WindowView {
            id: 42,
            title: "Files".into(),
            kind: "files".into(),
            document: "/home/alice/work".into(),
            rect: Rect::new(60, 90, width, 430),
            focused: true,
            can_go_back: true,
            can_go_forward: true,
            ..Default::default()
        };
        let crumbs = |w: &WindowView| {
            let windows = [w.clone()];
            let ctx = context(&windows, true);
            let mut painter = Painter::new(1024, 768);
            window_frame(&mut painter, &ctx, &windows[0]);
            let trail: Vec<String> = painter
                .scene
                .nodes
                .iter()
                .filter_map(|n| n.interaction.as_deref())
                .filter(|a| a.contains("files-location:"))
                .map(str::to_owned)
                .collect();
            let hit = |x: i32| {
                painter
                    .scene
                    .hit_test(x, 113)
                    .and_then(|n| n.interaction.as_deref())
                    .map(str::to_owned)
            };
            (trail, hit(265), hit(305))
        };
        let labelled_action = |w: &WindowView, name: &str| {
            let windows = [w.clone()];
            let ctx = context(&windows, true);
            let mut painter = Painter::new(1024, 768);
            window_frame(&mut painter, &ctx, &windows[0]);
            labelled(&painter, name).and_then(|n| n.interaction.clone())
        };
        let (trail, back, forward) = crumbs(&window(900));
        // Files' arrows walk history, as a browser's do; the path bar climbs.
        assert_eq!(back.as_deref(), Some("window:42:content:files-back"));
        assert_eq!(forward.as_deref(), Some("window:42:content:files-forward"));
        assert_eq!(
            trail,
            [
                "window:42:content:files-location:/",
                "window:42:content:files-location:/home",
                "window:42:content:files-location:/home/alice",
            ]
        );
        // A short bar elides from the front and always keeps the folder in view.
        let (short, _, _) = crumbs(&window(620));
        assert!(short.len() < trail.len());
        assert!(labelled_action(&window(620), "work").is_some_and(|a| a.ends_with("files-locate")));
        // Inside the home folder the trail starts at Home, as Files starts it.
        let mut homed = window(900);
        homed.home = "/home/alice".into();
        let (trail, _, _) = crumbs(&homed);
        assert_eq!(trail, ["window:42:content:files-location:/home/alice"]);
        // The current folder's own button opens the location entry.
        assert!(labelled_action(&homed, "work").is_some_and(|a| a.ends_with("files-locate")));
        // A named place is one button that re-reads it, not a trail of folders.
        homed.caption = "Starred".into();
        let (trail, _, _) = crumbs(&homed);
        assert!(trail.is_empty());
        // With no history, Back is announced insensitive: what it would send the
        // file manager refuses.
        let mut fresh = window(900);
        fresh.can_go_back = false;
        let windows = [fresh];
        let ctx = context(&windows, true);
        let mut painter = Painter::new(1024, 768);
        window_frame(&mut painter, &ctx, &windows[0]);
        let back = labelled(&painter, "Back").expect("Back is announced");
        assert!(back.semantic.as_ref().unwrap().disabled);
    }
    #[test]
    fn editor_and_terminal_header_controls_open_real_windows() {
        {
            let (kind, label) = ("editor", "New tab");
            let windows = [WindowView {
                id: 8,
                title: "Text Editor".into(),
                kind: kind.into(),
                rect: Rect::new(100, 80, 640, 420),
                focused: true,
                ..Default::default()
            }];
            let ctx = context(&windows, true);
            let mut painter = Painter::new(1024, 768);
            window_frame(&mut painter, &ctx, &windows[0]);
            let new = painter
                .scene
                .nodes
                .iter()
                .find(|n| n.semantic.as_ref().is_some_and(|s| s.label == label))
                .expect("the header paints a new-window control");
            assert_eq!(
                new.interaction.as_deref(),
                Some("window:8:content:shell:new")
            );
            assert!(!new.semantic.as_ref().unwrap().disabled);
        }
        let windows = [WindowView {
            id: 8,
            kind: "editor".into(),
            rect: Rect::new(100, 80, 640, 420),
            focused: true,
            ..Default::default()
        }];
        let ctx = context(&windows, true);
        let mut painter = Painter::new(1024, 768);
        window_frame(&mut painter, &ctx, &windows[0]);
        // Open, as GTK 4 lists it: the menu button and its toggle, both opening Files.
        let opens: Vec<&str> = painter
            .scene
            .nodes
            .iter()
            .filter(|n| n.semantic.as_ref().is_some_and(|s| s.label == "Open"))
            .filter_map(|n| n.interaction.as_deref())
            .collect();
        assert_eq!(
            opens,
            [
                "window:8:content:shell:launch:files",
                "window:8:shell:launch:files"
            ]
        );
        assert_eq!(
            painter
                .scene
                .hit_test(120, 103)
                .and_then(|n| n.interaction.as_deref())
                .map(|a| a.ends_with(":shell:launch:files")),
            Some(true)
        );
        // Main menu opens the app menu panel; View, which has no model, is insensitive.
        for n in &painter.scene.nodes {
            let Some(s) = n.semantic.as_ref() else {
                continue;
            };
            let action = n.interaction.as_deref().unwrap_or_default();
            match s.label.as_str() {
                "Main menu" => {
                    assert!(action.ends_with("shell:panel:app-menu"), "{action}");
                    assert!(!s.disabled);
                }
                "View" => {
                    assert!(action.contains("editor:off:"), "{action}");
                    assert!(s.disabled);
                }
                _ => {}
            }
        }
    }
    fn actions(p: &Painter) -> Vec<&str> {
        p.scene
            .nodes
            .iter()
            .filter_map(|n| n.interaction.as_deref())
            .collect()
    }
    /// Every string the painter drew, bold or not.
    fn texts(p: &Painter) -> Vec<&str> {
        p.scene
            .nodes
            .iter()
            .filter_map(|n| match &n.primitive {
                cw_scene::Primitive::UiText { text, .. }
                | cw_scene::Primitive::UiTextBold { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
    fn labelled<'a>(p: &'a Painter, label: &str) -> Option<&'a cw_scene::Node> {
        p.scene
            .nodes
            .iter()
            .find(|n| n.semantic.as_ref().is_some_and(|s| s.label == label))
    }
    /// Hit-test the middle of a labelled control, so occlusion counts too.
    fn hit_labelled<'a>(p: &'a Painter, label: &str) -> Option<&'a str> {
        let node = labelled(p, label)?;
        p.scene
            .hit_test(
                node.bounds.x + node.bounds.width as i32 / 2,
                node.bounds.y + node.bounds.height as i32 / 2,
            )
            .and_then(|n| n.interaction.as_deref())
    }
    #[test]
    fn activities_carries_the_whole_grid_and_the_dock_only_its_favourites() {
        let mut ctx = context(&[], true);
        ctx.launcher_open = true;
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        // The grid pages; its dots reach the rest, and every page together is the grid.
        let mut launches: Vec<String> = actions(&p).iter().map(|a| a.to_string()).collect();
        let pages = launches
            .iter()
            .filter(|a| a.starts_with("shell:home-page:"))
            .count()
            .max(1);
        for page in 1..pages {
            ctx.home_page = page as u32;
            let mut next = Painter::new(1024, 768);
            chrome(&mut next, &ctx);
            launches.extend(actions(&next).iter().map(|a| a.to_string()));
        }
        for (kind, _) in APPS {
            assert!(
                launches.contains(&format!("shell:launch:{kind}")),
                "Activities hides {kind}"
            );
        }
        // The dock keeps its favourites; the rest is one "Show applications" away.
        let ctx = context(&[], true);
        let mut docked = Painter::new(1024, 768);
        chrome(&mut docked, &ctx);
        let dock = actions(&docked);
        for kind in ["notes", "contacts", "calculator", "clock", "settings"] {
            assert!(!dock.contains(&format!("shell:launch:{kind}").as_str()));
        }
        assert!(dock.contains(&"shell:launcher"));
        // Only what the machine has installed is ever offered.
        let apps: Vec<String> = vec!["files".into(), "clock".into()];
        let mut ctx = context(&[], true);
        ctx.installed_apps = &apps;
        ctx.launcher_open = true;
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        for action in actions(&p) {
            if let Some(kind) = action.strip_prefix("shell:launch:") {
                assert!(apps.iter().any(|a| a == kind), "{kind} is not installed");
            }
        }
    }
    #[test]
    fn the_calendar_grid_pages_the_month_the_panel_reports() {
        let mut ctx = context(&[], true);
        ctx.panel = Some("calendar");
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert_eq!(hit_labelled(&p, "Previous month"), Some("shell:month:prev"));
        assert_eq!(hit_labelled(&p, "Next month"), Some("shell:month:next"));
        assert!(texts(&p).contains(&"September"));
        assert!(!actions(&p).contains(&"shell:month:today"));
        // One month on, the grid is February's, drawn from `panel_date`.
        ctx.panel_month = 17;
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        let shown = ctx.panel_date();
        assert_eq!(
            (shown.year, shown.month, shown.days_in_month),
            (2028, 2, 29)
        );
        assert!(texts(&p).contains(&"February 2028"));
        assert_eq!(
            hit_labelled(&p, "Back to September 2026"),
            Some("shell:month:today")
        );
        let days: Vec<&str> = texts(&p)
            .into_iter()
            .filter(|t| t.len() == 2 && t.parse::<u64>().is_ok_and(|d| (1..=31).contains(&d)))
            .collect();
        // Six weeks from Sunday 30 January: a leap February between two days of January
        // and eleven of March, and no cell of it is a target.
        assert_eq!(days.len(), 42);
        assert_eq!(days.iter().filter(|d| **d == "29").count(), 1);
        assert_eq!(days.iter().filter(|d| **d == "01").count(), 2);
    }
    #[test]
    fn firefox_greys_history_and_reload_it_cannot_perform() {
        let fresh = [WindowView {
            id: 5,
            kind: "browser".into(),
            rect: Rect::new(60, 60, 880, 600),
            focused: true,
            tabs: vec!["New tab".into()],
            ..Default::default()
        }];
        let ctx = context(&fresh, true);
        let mut p = Painter::new(1024, 768);
        browser_chrome(&mut p, &ctx, &fresh[0]);
        for label in ["Back", "Forward", "Reload"] {
            let node = labelled(&p, label).unwrap();
            let semantic = node.semantic.as_ref().unwrap();
            assert!(semantic.disabled && !semantic.focusable, "{label}");
            assert!(node.interaction.is_none(), "{label}");
            assert_eq!(hit_labelled(&p, label), None, "{label}");
        }
        let loaded = [WindowView {
            document: "http://example.test/".into(),
            can_go_back: true,
            can_go_forward: true,
            ..fresh[0].clone()
        }];
        let ctx = context(&loaded, true);
        let mut p = Painter::new(1024, 768);
        browser_chrome(&mut p, &ctx, &loaded[0]);
        for (label, action) in [
            ("Back", "back"),
            ("Forward", "forward"),
            ("Reload", "reload"),
        ] {
            assert_eq!(
                hit_labelled(&p, label),
                Some(format!("window:5:content:shell:{action}").as_str())
            );
        }
    }
    #[test]
    fn nothing_decorative_is_left_clickable() {
        let windows = [WindowView {
            id: 5,
            kind: "browser".into(),
            rect: Rect::new(60, 60, 880, 600),
            focused: true,
            tabs: vec!["Start".into()],
            ..Default::default()
        }];
        let mut ctx = context(&windows, true);
        ctx.launcher_open = true;
        let mut painter = Painter::new(1024, 768);
        chrome(&mut painter, &ctx);
        window_frame(&mut painter, &ctx, &windows[0]);
        browser_chrome(&mut painter, &ctx, &windows[0]);
        let announced: Vec<_> = painter
            .scene
            .nodes
            .iter()
            .filter(|n| n.semantic.as_ref().is_some_and(|s| s.disabled))
            .collect();
        // What is left greyed is only what this window genuinely cannot do: an empty
        // tab has no page to reload (and Firefox shows no star without one).
        let labels: Vec<&str> = announced
            .iter()
            .map(|n| n.semantic.as_ref().unwrap().label.as_str())
            .collect();
        assert!(labels.contains(&"Reload"), "{labels:?}");
        assert!(labelled(&painter, "Bookmark this page (Ctrl+D)").is_none());
        for n in announced {
            assert!(n.interaction.is_none());
            assert!(!n.semantic.as_ref().unwrap().focusable);
        }
        // The dock's trash and the overview's workspace tile are real now, and the
        // app grid carries no page indicator at all.
        let live = actions(&painter);
        assert!(live.contains(&"shell:trash"));
        assert!(live.contains(&"shell:workspace:0"));
        assert!(!labels.contains(&"Page 1 of 1"));
    }

    /// Nautilus's context menu offers Move to Trash on a file and Restore on a trashed
    /// one, and never both: which it is comes from the window it is over.
    #[test]
    fn the_context_menu_trashes_what_it_is_over_and_restores_what_is_in_the_trash() {
        for (trash, want, absent) in [
            (
                "0",
                "window:3:content:files-move-to-trash",
                "window:3:content:files-restore",
            ),
            (
                "1",
                "window:3:content:files-restore",
                "window:3:content:files-move-to-trash",
            ),
        ] {
            let windows = [WindowView {
                id: 3,
                kind: "files".into(),
                rect: Rect::new(60, 60, 800, 500),
                focused: true,
                selection: "/home/alice/notes.txt".into(),
                chrome: vec![
                    ("trash".to_owned(), trash.to_owned()),
                    ("starred".to_owned(), "0".to_owned()),
                ],
                ..Default::default()
            }];
            let mut ctx = context(&windows, true);
            ctx.panel = Some("context");
            let mut p = Painter::new(1024, 768);
            chrome(&mut p, &ctx);
            let live = actions(&p);
            assert!(live.contains(&want), "{trash}: {live:?}");
            assert!(!live.contains(&absent), "{trash}: {live:?}");
        }
        // Over nothing in particular the menu is about the desktop, not about a file.
        let mut ctx = context(&[], true);
        ctx.panel = Some("context");
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert!(!actions(&p)
            .iter()
            .any(|a| a.contains("files-move-to-trash") || a.contains("files-restore")));
    }

    fn notice(app: &str, title: &str, seen: bool) -> crate::Notice {
        crate::Notice {
            app: app.into(),
            title: title.into(),
            body: "Saved to ~/Pictures".into(),
            time_us: 0,
            action: Some("shell:launch:files".into()),
            seen,
        }
    }

    #[test]
    fn the_calendar_panel_lists_the_notices_the_machine_really_posted() {
        let posted = [notice("files", "Screenshot taken", false)];
        let mut ctx = context(&[], true);
        ctx.width = 1024;
        ctx.panel = Some("calendar");
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert!(texts(&p).contains(&"No Notifications"));
        assert!(!actions(&p).iter().any(|a| a.starts_with("shell:notice:")));
        ctx.notifications = &posted;
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert!(!texts(&p).contains(&"No Notifications"));
        assert_eq!(hit_labelled(&p, "Screenshot taken"), Some("shell:notice:0"));
        assert_eq!(
            hit_labelled(&p, "Mark all as read"),
            Some("shell:notifications:seen")
        );
    }

    #[test]
    fn the_activities_switcher_shows_the_real_desktops() {
        let windows = [WindowView {
            id: 5,
            kind: "editor".into(),
            rect: Rect::new(120, 90, 500, 360),
            focused: true,
            ..Default::default()
        }];
        // One desktop: switching to it is real, and the last one cannot be closed.
        let mut ctx = context(&windows, true);
        ctx.launcher_open = true;
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert_eq!(hit_labelled(&p, "Desktop 1"), Some("shell:workspace:0"));
        assert_eq!(hit_labelled(&p, "New desktop"), Some("shell:workspace:new"));
        assert!(labelled(&p, "Close desktop 1").is_none());
        // Three desktops, sitting on the second: each tile carries its own index, and
        // only the one on screen offers the close the handler would accept.
        ctx.workspaces = 3;
        ctx.workspace = 1;
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        for index in 0..3 {
            assert_eq!(
                hit_labelled(&p, &format!("Desktop {}", index + 1)),
                Some(format!("shell:workspace:{index}").as_str()),
                "desktop {index}"
            );
        }
        assert!(labelled(&p, "Desktop 4").is_none());
        assert_eq!(
            hit_labelled(&p, "Close desktop 2"),
            Some("shell:workspace:close")
        );
        assert!(labelled(&p, "Close desktop 3").is_none());
        // At the machine's limit the plus is announced unavailable and cannot be hit.
        ctx.workspaces = crate::WORKSPACE_LIMIT;
        ctx.workspace = 0;
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        let node = labelled(&p, "New desktop").unwrap();
        assert!(node.semantic.as_ref().unwrap().disabled);
        assert!(node.interaction.is_none() && !node.semantic.as_ref().unwrap().focusable);
        assert_ne!(hit_labelled(&p, "New desktop"), Some("shell:workspace:new"));
    }

    #[test]
    fn the_terminal_header_is_gnome_terminals() {
        let windows = [WindowView {
            id: 8,
            kind: "terminal".into(),
            rect: Rect::new(100, 80, 824, 586),
            focused: true,
            ..Default::default()
        }];
        let ctx = context(&windows, true);
        let mut p = Painter::new(1280, 800);
        window_frame(&mut p, &ctx, &windows[0]);
        // One shell per window and no scrollback search: New Tab and Find are greyed,
        // and what they send the model refuses.
        for (greyed, action) in [
            ("New Tab", "window:8:content:terminal-new-tab"),
            ("Find", "window:8:content:terminal-find"),
        ] {
            let node = labelled(&p, greyed).expect(greyed);
            assert!(node.semantic.as_ref().unwrap().disabled, "{greyed}");
            assert_eq!(node.interaction.as_deref(), Some(action));
        }
        assert_eq!(
            hit_labelled(&p, "Menu"),
            Some("window:8:content:shell:panel:app-menu")
        );
        assert!(p
            .scene
            .nodes
            .iter()
            .any(|n| n.semantic.as_ref().is_some_and(|s| s.role == "tablist")));
    }
    #[test]
    fn the_files_header_searches_and_swaps_view_for_real() {
        let windows = [WindowView {
            id: 4,
            title: "Files".into(),
            kind: "files".into(),
            rect: Rect::new(0, 0, 900, 600),
            focused: true,
            document: "/home/alice".into(),
            ..Default::default()
        }];
        let ctx = context(&windows, true);
        let mut p = Painter::new(1024, 768);
        window_frame(&mut p, &ctx, &windows[0]);
        assert_eq!(
            hit_labelled(&p, "Search Current Folder"),
            Some("window:4:content:files-search")
        );
        assert_eq!(
            hit_labelled(&p, "View Options"),
            Some("window:4:content:files-view")
        );
        // Both halves of each menu button open the same menu.
        assert!(hit_labelled(&p, "Main Menu").is_some_and(|a| a.ends_with("shell:panel:app-menu")));
        assert!(hit_labelled(&p, "Current Folder Menu")
            .is_some_and(|a| a.ends_with("shell:panel:context")));
        // GTK 4 lists a menu button and its inner toggle: two of each.
        let named = |name: &str| {
            p.scene
                .nodes
                .iter()
                .filter(|n| n.interaction.is_some())
                .filter(|n| n.semantic.as_ref().is_some_and(|s| s.label == name))
                .count()
        };
        assert_eq!(named("Main Menu"), 2);
        assert_eq!(named("View Options"), 3);
        // What the file manager has no model for is announced insensitive.
        let everywhere = labelled(&p, "Search Everywhere").expect("Search Everywhere");
        assert!(everywhere.semantic.as_ref().unwrap().disabled);
        // The view's header bar is announced as GTK announces it: a tool bar.
        assert!(p
            .scene
            .nodes
            .iter()
            .any(|n| n.semantic.as_ref().is_some_and(|s| s.role == "toolbar")));
    }

    #[test]
    fn the_firefox_toolbar_bookmarks_pages_and_counts_downloads() {
        let loaded = [WindowView {
            id: 5,
            title: "Example — http://example.test/".into(),
            kind: "browser".into(),
            rect: Rect::new(60, 60, 880, 600),
            focused: true,
            document: "http://example.test/".into(),
            tabs: vec!["Example".into()],
            ..Default::default()
        }];
        let downloads = [crate::Download {
            name: "index.html".into(),
            path: "/home/alice/Downloads/index.html".into(),
            url: "http://example.test/".into(),
            bytes: 12,
        }];
        let mut ctx = context(&loaded, true);
        ctx.downloads = &downloads;
        let mut p = Painter::new(1024, 768);
        browser_chrome(&mut p, &ctx, &loaded[0]);
        assert_eq!(
            hit_labelled(&p, "Bookmark this page (Ctrl+D)"),
            Some("shell:bookmark")
        );
        // Firefox's Downloads button appears once something was downloaded.
        assert!(labelled(&p, "Downloads").is_some());
        assert_eq!(hit_labelled(&p, "Firefox"), Some("shell:settings"));
        // The address bar is Firefox's combobox, holding the page's whole address.
        let bar = labelled(&p, "Search with Google or enter address").unwrap();
        let sem = bar.semantic.as_ref().unwrap();
        assert_eq!(sem.role, "combobox");
        assert_eq!(sem.value.as_deref(), Some("http://example.test/"));
        assert!(labelled(&p, "Not Secure").is_some());
        // Saved already: the same star edits the bookmark, as Firefox's says.
        ctx.bookmarked = true;
        let mut p = Painter::new(1024, 768);
        browser_chrome(&mut p, &ctx, &loaded[0]);
        assert_eq!(
            hit_labelled(&p, "Edit this bookmark (Ctrl+D)"),
            Some("shell:bookmark")
        );
        // An empty tab's bar is "Search or enter address", with no star.
        let empty = [WindowView {
            document: String::new(),
            title: String::new(),
            ..loaded[0].clone()
        }];
        let ctx = context(&empty, true);
        let mut p = Painter::new(1024, 768);
        browser_chrome(&mut p, &ctx, &empty[0]);
        assert!(labelled(&p, "Search or enter address").is_some());
        assert!(labelled(&p, "Pick a search engine").is_some());
    }

    #[test]
    fn super_shows_the_window_overview_and_show_applications_the_grid() {
        let windows = [WindowView {
            id: 4,
            kind: "editor".into(),
            rect: Rect::new(120, 90, 500, 360),
            focused: true,
            ..Default::default()
        }];
        // The window overview: the window on screen, reachable, and no app grid.
        let mut ctx = context(&windows, true);
        ctx.panel = Some("overview");
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        let focus = windows[0].action("focus");
        assert_eq!(
            hit_labelled(&p, &window_title(&windows[0])),
            Some(focus.as_str())
        );
        assert!(!actions(&p).contains(&"shell:launch:calculator"));
        assert!(texts(&p).contains(&"Type to search"));
        // The app grid: every installed application the dock does not carry, A to Z.
        ctx.panel = None;
        ctx.launcher_open = true;
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        let grid: Vec<&str> = p
            .scene
            .nodes
            .iter()
            .filter(|n| n.bounds.y >= 261 && n.bounds.x >= DOCK_WIDTH)
            .filter_map(|n| n.interaction.as_deref()?.strip_prefix("shell:launch:"))
            .collect();
        assert!(!grid.is_empty());
        for kind in &grid {
            assert!(!DOCK.contains(kind), "{kind} is in the dock and the grid");
        }
        let names: Vec<String> = grid
            .iter()
            .map(|k| app_name(k).unwrap().to_lowercase())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        let mut seen = names.clone();
        seen.dedup();
        assert_eq!(seen, sorted, "the grid is alphabetical");
        // Typing searches: one row of results, the first what Enter opens.
        ctx.search = "calc";
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert!(actions(&p).contains(&"shell:launch:calculator"));
    }
    #[test]
    fn alt_tab_shows_the_window_it_switched_to_selected_second() {
        // Alt+Tab from Text Editor has raised Calculator; the switcher still lists Text
        // Editor first, as it stood when Tab was pressed, with Calculator selected.
        let windows = [
            WindowView {
                id: 2,
                kind: "editor".into(),
                rect: Rect::new(116, 82, 700, 520),
                ..Default::default()
            },
            WindowView {
                id: 1,
                kind: "calculator".into(),
                rect: Rect::new(66, 32, 360, 503),
                focused: true,
                ..Default::default()
            },
        ];
        let order: Vec<u64> = switcher_order(&windows).iter().map(|w| w.id).collect();
        assert_eq!(order, vec![1, 2]);
        let mut ctx = context(&windows, true);
        ctx.panel = Some("switcher:1");
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        // The selected window is named under the previews, and each preview is its window.
        assert!(texts(&p).contains(&"Calculator"));
        let focus = windows[1].action("focus");
        assert_eq!(hit_labelled(&p, "Calculator"), Some(focus.as_str()));
        assert!(!actions(&p).contains(&"shell:dismiss"));
    }

    #[test]
    fn the_polkit_prompt_is_modal_and_authenticates_only_with_a_password() {
        let mut ctx = context(&[], true);
        ctx.panel = Some("polkit:/bin/true");
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert!(texts(&p).contains(&"Authentication Required"));
        assert!(texts(&p).contains(&"alice"));
        assert_eq!(hit_labelled(&p, "Cancel"), Some("shell:polkit:cancel"));
        // Nothing behind the prompt answers, the dock included.
        assert_eq!(
            p.scene
                .hit_test(30, 300)
                .and_then(|n| n.interaction.as_deref()),
            Some("shell:noop")
        );
        // Authenticate is insensitive until a password is typed, which shows as dots.
        let node = labelled(&p, "Authenticate").unwrap();
        assert!(node.semantic.as_ref().unwrap().disabled);
        assert!(!actions(&p).contains(&"shell:polkit:authenticate"));
        ctx.search = "secret";
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert_eq!(
            hit_labelled(&p, "Authenticate"),
            Some("shell:polkit:authenticate")
        );
        assert!(!texts(&p).contains(&"secret"));
    }

    #[test]
    fn calculators_main_menu_opens_its_about_window() {
        let windows = [WindowView {
            id: 7,
            kind: "calculator".into(),
            title: "Calculator".into(),
            rect: Rect::new(66, 32, 360, 503),
            focused: true,
            ..Default::default()
        }];
        let mut ctx = context(&windows, true);
        let mut p = Painter::new(1024, 768);
        window_frame(&mut p, &ctx, &windows[0]);
        let menu = windows[0].action("content:shell:panel:app-menu");
        assert!(actions(&p).contains(&menu.as_str()));
        ctx.panel = Some("app-menu");
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        assert_eq!(hit_labelled(&p, "About Calculator"), Some("shell:about"));
        assert_eq!(hit_labelled(&p, "New Window"), Some("shell:new"));
        ctx.panel = Some("about:calculator");
        let mut p = Painter::new(1024, 768);
        chrome(&mut p, &ctx);
        for text in [
            "Calculator",
            "The GNOME Project",
            "46.3",
            "Website",
            "Legal",
        ] {
            assert!(texts(&p).contains(&text), "{text}");
        }
        assert_eq!(hit_labelled(&p, "Close"), Some("shell:dismiss"));
        // Its links need the internet and its pages the model lacks.
        assert!(
            labelled(&p, "Website")
                .unwrap()
                .semantic
                .as_ref()
                .unwrap()
                .disabled
        );
    }
}
