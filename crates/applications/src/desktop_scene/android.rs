//! Pixel / Android 15 Material You presentation, rendered entirely by the Rust scene
//! engine: tonal surfaces derived from the wallpaper, a scalloped clock widget, a
//! real notification shade with Quick Settings, and three-button navigation.
use super::shared::{arc_points, cos1024, sin1024, Align, Painter, ShellContext, WindowView};
use cw_scene::{Color, Rect};

const INK: Color = Color::rgb(26, 28, 22);
const SOFT: Color = Color::rgb(68, 72, 60);
const PAPER: Color = Color::rgb(248, 250, 240);
const CONTAINER: Color = Color::rgb(232, 236, 222);
const PRIMARY: Color = Color::rgb(76, 102, 43);
const PRIMARY_CONTAINER: Color = Color::rgb(205, 237, 163);
const SHADE: Color = Color::rgb(18, 20, 16);
const SHADE_TILE: Color = Color::rgb(50, 54, 46);
const SHADE_INK: Color = Color::rgb(226, 228, 216);
/// (application id, name). The id is also the icon id, and every application in the
/// catalogue now has Pixel artwork of its own, so the drawer can show the whole roster.
/// The hotseat below still keeps to four.
/// This platform's applications in the order its shell shows them, from the catalog
/// (packages/catalog.json) this build compiled in.
const APPS: &[(&str, &str)] = crate::apps::catalog::SHELF2_ANDROID;

/// The title this shell paints in a window's app bar. It is left empty where the
/// application's own content carries the name instead.
pub(super) fn app_bar_title(w: &WindowView) -> String {
    match w.kind.as_str() {
        "files" => w
            .document
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("Internal storage")
            .to_owned(),
        "editor" => String::new(),
        // A Settings page is titled by the page (Network & internet, Display).
        "settings" if !w.title.is_empty() => w.title.clone(),
        kind => app_name(kind).map_or_else(|| w.title.clone(), str::to_owned),
    }
}
/// The name this window goes by, for a reader that needs one even where the bar
/// is left blank.
pub(super) fn window_title(w: &WindowView) -> String {
    let painted = app_bar_title(w);
    if !painted.is_empty() {
        return painted;
    }
    if w.kind == "editor" && !w.document.is_empty() {
        return w
            .document
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or(&w.document)
            .to_owned();
    }
    app_name(&w.kind).map_or_else(|| w.title.clone(), str::to_owned)
}
pub(super) fn app_name(kind: &str) -> Option<&'static str> {
    APPS.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n)
}

/// Whether an application's window follows the system's dark theme: all but the terminal,
/// which is dark already.
pub fn follows_theme(kind: &str) -> bool {
    kind != "terminal"
}

/// One colour in the dark theme. Material's dark scheme takes each role's tone from the
/// other end of its tonal palette (a surface at tone 98 becomes tone 6, text at tone 10 becomes
/// tone 90, a light container a dark one, the primary a light primary), keeping the hue. The
/// simulated applications paint those roles as constants, so the dark theme is that swap
/// applied to what they painted: the lightness inverts, compressed into tones 6 to 90, with
/// each channel's offset kept for a neutral colour and scaled for a hued one; a hued colour
/// of middle lightness (a badge, an accent) and every image stay as they are.
pub fn dark_tone(c: Color) -> Color {
    let (hi, lo) = (c.0.max(c.1).max(c.2) as i32, c.0.min(c.1).min(c.2) as i32);
    let l = (hi + lo) / 2;
    let target = 16 + (255 - l) * (230 - 16) / 255;
    if hi - lo <= 40 {
        let mean = (c.0 as i32 + c.1 as i32 + c.2 as i32) / 3;
        let f = |v: u8| (target + (v as i32 - mean)).clamp(0, 255) as u8;
        return Color(f(c.0), f(c.1), f(c.2), c.3);
    }
    if l > 160 {
        // A light container darkens toward black, its hue kept.
        let f = |v: u8| (v as i32 * target / l.max(1)).clamp(0, 255) as u8;
        Color(f(c.0), f(c.1), f(c.2), c.3)
    } else if l < 110 {
        // A dark hued colour (the primary, its text) lightens toward white.
        let f = |v: u8| {
            (255 - (255 - v as i32) * (255 - target) / (255 - l).max(1)).clamp(0, 255) as u8
        };
        Color(f(c.0), f(c.1), f(c.2), c.3)
    } else {
        c
    }
}

/// The dark theme over painted nodes (an application's window, its app bar and content):
/// every colour through `dark_tone`.
pub fn dark_theme(nodes: &mut [cw_scene::Node]) {
    use cw_scene::Primitive as P;
    for n in nodes {
        match &mut n.primitive {
            P::Box { fill, border, .. } | P::RoundedBox { fill, border, .. } => {
                *fill = dark_tone(*fill);
                if let Some(b) = border {
                    *b = dark_tone(*b);
                }
            }
            P::UiText { color, .. }
            | P::UiTextBold { color, .. }
            | P::Text { color, .. }
            | P::Symbol { color, .. } => *color = dark_tone(*color),
            P::Path { fill, stroke, .. } => {
                for c in [fill, stroke].into_iter().flatten() {
                    *c = dark_tone(*c);
                }
            }
            _ => {}
        }
    }
}

/// Publish the position of the control just painted, so its state is observable.
fn announce(p: &mut Painter, value: &str) {
    if let Some(s) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
        s.value = Some(value.into());
    }
}

fn google(p: &mut Painter, x: i32, y: i32, size: i32) {
    // Original vector construction, not a font glyph or remotely loaded logo.
    let colors = [
        Color::rgb(66, 133, 244),
        Color::rgb(52, 168, 83),
        Color::rgb(251, 188, 5),
        Color::rgb(234, 67, 53),
    ];
    let (cx, cy, r) = (x + size / 2, y + size / 2, size / 2 - 1);
    let stroke = (size / 5).max(2) as u16;
    // Angles run clockwise from 12 o'clock.
    for (from, to, color) in [
        (90, 185, colors[0]),
        (175, 260, colors[1]),
        (250, 305, colors[2]),
        (295, 400, colors[3]),
    ] {
        p.line(arc_points(cx, cy, r, from, to, 6), color, stroke);
    }
    p.line(vec![(cx, cy), (cx + r, cy)], colors[0], stroke);
}

fn app(p: &mut Painter, x: i32, y: i32, size: u32, kind: &str, label: &str, ink: Option<Color>) {
    p.platform_icon(
        Rect::new(x, y, size, size),
        "android",
        kind,
        &format!("shell:launch:{kind}"),
        label,
    );
    if let Some(ink) = ink {
        let (lx, ly, width) = (x - 16, y + size as i32 + 7, size + 32);
        if ink == Color::WHITE {
            p.label(
                lx,
                ly + 1,
                width,
                label,
                12,
                Color(0, 0, 0, 150),
                false,
                Align::Center,
            );
        }
        p.label(lx, ly, width, label, 12, ink, false, Align::Center);
    }
}
fn short_date(ctx: &ShellContext<'_>) -> String {
    let date = ctx.date();
    format!(
        "{}, {} {}",
        &date.weekday_name()[..3],
        &date.month_name()[..3],
        date.day
    )
}
fn clock(ctx: &ShellContext<'_>) -> String {
    let (hour, minute) = ctx.hour_minute();
    format!(
        "{}:{minute:02}",
        if hour % 12 == 0 { 12 } else { hour % 12 }
    )
}

pub fn background(p: &mut Painter, ctx: &ShellContext<'_>) {
    p.asset(Rect::new(0, 0, ctx.width, ctx.height), "wallpaper/android");
    if ctx.active || ctx.launcher_open {
        return;
    }
    let width = ctx.width as i32;
    let height = ctx.height as i32;
    // At a Glance and the clock widget live on the first page; later pages are apps.
    let pages = home_pages(ctx.installed_apps, ctx.width, ctx.height);
    if ctx.home_page.min(pages.saturating_sub(1)) > 0 {
        launcher_pages(p, ctx);
        return;
    }
    // At a Glance and the clock widget are calendar surfaces: they open the Calendar
    // application when it is installed, and the shell's own month grid when it is not.
    let calendar = if ctx.installed("calendar") {
        "shell:launch:calendar"
    } else {
        "shell:panel:calendar"
    };
    p.left(
        26,
        62,
        ctx.width.saturating_sub(52),
        &short_date(ctx),
        19,
        INK,
    );
    p.region(
        Rect::new(20, 54, ctx.width.saturating_sub(40).min(240), 34),
        calendar,
        "At a Glance, open the calendar",
    );
    // Android 12's characteristic tonal, scalloped clock widget.
    let diameter = (ctx.width * 56 / 100).clamp(148, 250) as i32;
    let cx = width / 2;
    let cy = (height * 33 / 100).max(190);
    let points = (0..180)
        .map(|i| {
            let a = i * 2;
            let radius = diameter * (940 + 60 * cos1024(a * 12) / 1024) / 2000;
            (
                cx + radius * sin1024(a) / 1024,
                cy - radius * cos1024(a) / 1024,
            )
        })
        .collect();
    p.path(points, PRIMARY_CONTAINER);
    let (hour, minute) = ctx.hour_minute();
    let size = (diameter * 36 / 100) as u16;
    let ink = Color::rgb(36, 54, 16);
    for (i, part) in [if hour % 12 == 0 { 12 } else { hour % 12 }, minute]
        .iter()
        .enumerate()
    {
        p.label(
            cx - diameter / 2,
            cy - diameter * 43 / 100 + i as i32 * diameter * 36 / 100,
            diameter as u32,
            &format!("{part:02}"),
            size,
            ink,
            false,
            Align::Center,
        );
    }
    let widget = Rect::new(
        cx - diameter / 2,
        cy - diameter / 2,
        diameter as u32,
        diameter as u32,
    );
    p.region(widget, calendar, "Clock widget, open the calendar");
    launcher_pages(p, ctx);
}

/// Pixel Launcher's layout at a screen size: the hotseat's four favourites, then the
/// other installed applications on home screen pages in a grid of four columns (five
/// on a wide screen). The first page keeps At a Glance and the clock widget above its
/// apps; later pages are all grid. The router swipes through exactly these pages.
struct Launcher {
    icon: u32,
    columns: i32,
    pitch: i32,
    dock_y: i32,
    dots_y: i32,
    first_y: i32,
    dock: Vec<(&'static str, &'static str)>,
    pages: Vec<Vec<(&'static str, &'static str)>>,
}
impl Launcher {
    fn new(installed: impl Fn(&str) -> bool, width: u32, height: u32) -> Self {
        let (w, h) = (width as i32, height as i32);
        let icon = (width / 7).clamp(43, 60);
        let columns = if width >= 480 { 5 } else { 4 };
        let pitch = icon as i32 + 42;
        let all: Vec<(&'static str, &'static str)> = APPS
            .iter()
            .copied()
            .filter(|(kind, _)| installed(kind))
            .collect();
        // Hotseat favourites first; the remaining applications fill the pages.
        let preferred = ["messages", "browser", "mail", "files"];
        let mut dock: Vec<_> = preferred
            .iter()
            .filter_map(|k| all.iter().copied().find(|(kind, _)| kind == k))
            .collect();
        let mut rest: Vec<_> = all.iter().copied().filter(|a| !dock.contains(a)).collect();
        while dock.len() < 4 && !rest.is_empty() {
            dock.push(rest.remove(0));
        }
        let dock_y = h - 194;
        let dots_y = dock_y - 14;
        // The first page's apps sit under the clock widget, as they always have.
        let diameter = (width * 56 / 100).clamp(148, 250) as i32;
        let cy = (h * 33 / 100).max(190);
        let first_y = (h - 300).max(cy + diameter / 2 + 24);
        let fits = |top: i32| ((dots_y - 10 - top - icon as i32 - 26) / pitch + 1).max(1) as usize;
        let first = fits(first_y) * columns as usize;
        let per_page = fits(70) * columns as usize;
        let mut pages = vec![rest.iter().copied().take(first).collect::<Vec<_>>()];
        for chunk in rest[first.min(rest.len())..].chunks(per_page.max(1)) {
            pages.push(chunk.to_vec());
        }
        let _ = w;
        Self {
            icon,
            columns,
            pitch,
            dock_y,
            dots_y,
            first_y,
            dock,
            pages,
        }
    }
    fn column(&self, width: u32, i: i32) -> i32 {
        let w = width as i32;
        w * (i * 2 + 1) / (self.columns * 2) - self.icon as i32 / 2
    }
}

/// Home screen pages at this size, for these installed applications (empty meaning all).
pub fn home_pages(installed: &[String], width: u32, height: u32) -> u32 {
    Launcher::new(
        |kind| installed.is_empty() || installed.iter().any(|a| a == kind),
        width,
        height,
    )
    .pages
    .len() as u32
}

/// The page of the home screen the user swiped to, its dots, and the hotseat.
fn launcher_pages(p: &mut Painter, ctx: &ShellContext<'_>) {
    let l = Launcher::new(|kind| ctx.installed(kind), ctx.width, ctx.height);
    let page = (ctx.home_page as usize).min(l.pages.len() - 1);
    let top = if page == 0 { l.first_y } else { 70 };
    for (i, (kind, label)) in l.pages[page].iter().enumerate() {
        let (col, row) = (i as i32 % l.columns, i as i32 / l.columns);
        app(
            p,
            l.column(ctx.width, col),
            top + row * l.pitch,
            l.icon,
            kind,
            label,
            Some(Color::WHITE),
        );
    }
    // The page indicator: one dot a page, the current one filled; a dot is a real
    // control that goes to its page, as tapping the indicator does.
    let count = l.pages.len() as i32;
    if count > 1 {
        let gap = 16;
        let x0 = ctx.width as i32 / 2 - (count - 1) * gap / 2;
        for i in 0..count {
            let cx = x0 + i * gap;
            // A soft shadow keeps the white dots legible over a light wallpaper.
            p.circle(cx, l.dots_y + 1, 5, Color(0, 0, 0, 40));
            if i as usize == page {
                p.circle(cx, l.dots_y, 4, Color::WHITE);
            } else {
                p.circle(cx, l.dots_y, 3, Color(255, 255, 255, 120));
            }
            p.region(
                Rect::new(cx - 8, l.dots_y - 10, 16, 20),
                &format!("shell:home-page:{i}"),
                &format!("Page {} of {count}", i + 1),
            );
        }
    }
    // The hotseat and the search bar sit just above the navigation bar on every page.
    let dock_column = |i: i32| ctx.width as i32 * (i * 2 + 1) / 8 - l.icon as i32 / 2;
    for (i, (kind, label)) in l.dock.iter().enumerate() {
        app(
            p,
            dock_column(i as i32),
            l.dock_y,
            l.icon,
            kind,
            label,
            None,
        );
    }
    search_bar(p, ctx);
}

fn search_bar(p: &mut Painter, ctx: &ShellContext<'_>) {
    let height = ctx.height as i32;
    let search = Rect::new(18, height - NAV_BAR - 66, ctx.width.saturating_sub(36), 54);
    p.drop_shadow(search, 27, 8, 40, 2);
    p.box_(search, PAPER, 27);
    google(p, search.x + 18, search.y + 16, 22);
    // The search surface searches; the swipe-up strip below still opens the drawer.
    p.region(
        Rect::new(search.x, search.y, search.width.saturating_sub(92), 54),
        "shell:search",
        "Search",
    );
    p.symbol(
        "mic",
        search.x + search.width as i32 - 78,
        search.y + 16,
        22,
        PRIMARY,
    );
    // Nothing in the protocol captures audio, so a microphone has nothing to send.
    p.disabled("Voice search");
    p.symbol(
        "screenshot",
        search.x + search.width as i32 - 42,
        search.y + 16,
        22,
        PRIMARY,
    );
    // Lens needs a camera capture; there is no camera application and no capture model.
    p.disabled("Google Lens");
}

fn status(p: &mut Painter, ctx: &ShellContext<'_>, ink: Color) {
    let w = ctx.width as i32;
    p.left(24, 11, 70, &clock(ctx), 14, ink);
    if ctx.switch("do_not_disturb") {
        p.symbol("dnd", 96, 11, 15, ink);
    }
    // The badge appears only when a notice the user has not seen is really waiting.
    if ctx.unseen_notices() > 0 {
        p.symbol("bell", 118, 11, 15, ink);
    }
    // Pixel's centred camera aperture stays in the system-owned safe area.
    p.circle(w / 2, 19, 7, Color::rgb(8, 10, 9));
    p.circle(w / 2, 19, 2, Color::rgb(28, 34, 44));
    // Status glyphs report the real switches rather than a fixed happy path.
    if ctx.switch("airplane_mode") {
        p.symbol("airplane", w - 84, 11, 16, ink);
    } else {
        let wifi = if ctx.switch("wifi") {
            "wifi-fill"
        } else {
            "wifi"
        };
        p.symbol(wifi, w - 84, 11, 16, ink);
        p.symbol("signal", w - 62, 11, 15, ink);
    }
    let battery = if ctx.switch("battery_saver") {
        "leaf"
    } else {
        "battery-vertical"
    };
    p.symbol(battery, w - 42, 10, 17, ink);
}

fn app_drawer(p: &mut Painter, ctx: &ShellContext<'_>) {
    let w = ctx.width as i32;
    p.glass(
        Rect::new(0, 0, ctx.width, ctx.height),
        0,
        30,
        Color(248, 250, 240, 235),
        None,
    );
    // Tapping outside the drawer closes it, as it does on the device.
    p.region(
        Rect::new(0, 0, ctx.width, ctx.height),
        "shell:dismiss",
        "Close all applications",
    );
    p.box_(Rect::new(w / 2 - 16, 42, 32, 4), Color(68, 72, 60, 110), 2);
    p.region(
        Rect::new(w / 2 - 40, 30, 80, 24),
        "shell:dismiss",
        "Swipe down to close",
    );
    let field = Rect::new(14, 72, ctx.width.saturating_sub(28), 48);
    p.box_(field, CONTAINER, 24);
    google(p, field.x + 18, field.y + 13, 22);
    if ctx.search.is_empty() {
        p.left(
            field.x + 56,
            field.y + 14,
            field.width - 110,
            "Search apps, web and more",
            16,
            SOFT,
        );
    } else {
        let end = p.left(
            field.x + 56,
            field.y + 14,
            field.width - 110,
            ctx.search,
            16,
            INK,
        );
        p.box_(
            Rect::new(field.x + 58 + end as i32, field.y + 13, 2, 22),
            PRIMARY,
            1,
        );
    }
    p.region(field, "shell:search", "Search installed applications");
    p.symbol(
        "more-vertical",
        field.x + field.width as i32 - 38,
        field.y + 14,
        20,
        SOFT,
    );
    p.region(
        Rect::new(field.x + field.width as i32 - 48, field.y + 4, 40, 40),
        "shell:panel:context",
        "More options",
    );
    let query = ctx.search.to_lowercase();
    // Five columns on a phone, as Launcher3's and Pixel's drawers have; more on a wide screen.
    let columns = (ctx.width as i32 / 82).clamp(5, 8);
    let size = (ctx.width / 7).clamp(40, 56);
    let matches: Vec<_> = APPS
        .iter()
        .filter(|(kind, label)| {
            ctx.installed(kind)
                && (query.is_empty()
                    || label.to_lowercase().contains(&query)
                    || kind.contains(&query))
        })
        .collect();
    if matches.is_empty() {
        p.center(0, 150, ctx.width, "No apps found", 15, SOFT);
    }
    for (i, (kind, label)) in matches.iter().enumerate() {
        let x = (i as i32 % columns * 2 + 1) * w / (columns * 2) - size as i32 / 2;
        let y = 128 + (i as i32 / columns) * 120;
        app(p, x, y, size, kind, label, Some(INK));
    }
}

/// Brightness as twenty discrete steps: every segment carries the exact level it sets,
/// and the fill is painted from the level the device actually holds. Android 16's slider is
/// a thick track filled to the level, its thumb a bar just past the fill.
fn brightness(p: &mut Painter, ctx: &ShellContext<'_>, c: &ShadeInk, track: Rect) {
    const STEPS: u32 = 20;
    let level = u32::from(ctx.level("brightness")).min(100);
    let filled = (track.width * level / 100).max(track.height / 2);
    p.box_(track, c.tile, 16);
    p.box_(
        Rect::new(track.x, track.y, filled, track.height),
        c.icon_on,
        16,
    );
    let thumb = (track.x + filled as i32 + 6).min(track.x + track.width as i32 - 6);
    p.box_(
        Rect::new(thumb, track.y + 4, 4, track.height.saturating_sub(8)),
        c.icon_on,
        2,
    );
    p.symbol(
        "sun",
        track.x + 16,
        track.y + track.height as i32 / 2 - 11,
        22,
        if filled > 52 { c.glyph_on } else { c.ink },
    );
    for i in 0..STEPS {
        let x = track.x + (track.width * i / STEPS) as i32;
        let percent = (i + 1) * 100 / STEPS;
        p.button(
            Rect::new(
                x,
                track.y,
                (track.x + (track.width * (i + 1) / STEPS) as i32 - x).max(1) as u32,
                track.height,
            ),
            Color::TRANSPARENT,
            0,
            &format!("shell:set:brightness:{percent}"),
            &format!("Brightness {percent}%"),
        );
    }
    p.container(track, "slider", "Display brightness");
    announce(p, &format!("{level}%"));
}

/// The shade's colours. Android 16's shade (AOSP's System UI, which Pixel's shares) follows
/// the system theme: a light or dark scrim over the blurred screen, tonal tiles whose icon sits
/// in a container that takes the primary colour when the tile is on.
struct ShadeInk {
    scrim: Color,
    tile: Color,
    icon_off: Color,
    icon_on: Color,
    glyph_on: Color,
    ink: Color,
    soft: Color,
    sheet: Color,
}
fn shade_ink(ctx: &ShellContext<'_>) -> ShadeInk {
    if ctx.switch("dark_mode") {
        ShadeInk {
            scrim: Color(SHADE.0, SHADE.1, SHADE.2, 232),
            tile: SHADE_TILE,
            icon_off: Color::rgb(70, 75, 64),
            icon_on: Color::rgb(178, 209, 138),
            glyph_on: Color::rgb(30, 54, 8),
            ink: SHADE_INK,
            soft: Color::rgb(196, 200, 186),
            sheet: Color::rgb(34, 37, 31),
        }
    } else {
        ShadeInk {
            scrim: Color(204, 208, 196, 228),
            tile: Color::rgb(240, 242, 234),
            icon_off: Color::rgb(252, 253, 248),
            icon_on: PRIMARY,
            glyph_on: Color::WHITE,
            ink: INK,
            soft: SOFT,
            sheet: Color::rgb(228, 231, 220),
        }
    }
}

/// Mark the last node checked or not (a switch tile).
fn check_last(p: &mut Painter, on: bool) {
    if let Some(n) = p.scene.nodes.last_mut() {
        n.state.get_or_insert_with(Default::default).checked = Some(on);
    }
}

/// A Quick Settings tile: one real system switch. A large tile is a pill with its icon in a
/// container and its name and state beside it; a small one is the icon alone, the whole tile
/// coloured when on.
struct Tile {
    symbol: &'static str,
    name: &'static str,
    flag: &'static str,
    on_text: &'static str,
    off_text: &'static str,
}
/// Android 16's tiles in their order, the ones the simulated phone has switches for: the
/// large ones (Wi-Fi, Bluetooth, two, Modes, one), then a row of small ones. AOSP's own are
/// Wi-Fi, Bluetooth, Mobile data, Cast, Modes, Wallet; Airplane mode, Auto-rotate, Flashlight,
/// Battery Saver. A leading `!` marks a tile whose "on" face is the flag being off:
/// auto-rotate is rotation unlocked.
const LARGE_TILES: [Tile; 6] = [
    Tile {
        symbol: "wifi-fill",
        name: "Wi-Fi",
        flag: "wifi",
        on_text: "Connected",
        off_text: "Off",
    },
    Tile {
        symbol: "bluetooth",
        name: "Bluetooth",
        flag: "bluetooth",
        on_text: "On",
        off_text: "Off",
    },
    Tile {
        symbol: "hotspot",
        name: "Hotspot",
        flag: "hotspot",
        on_text: "On",
        off_text: "Off",
    },
    Tile {
        symbol: "night-light",
        name: "Night Light",
        flag: "night_light",
        on_text: "On",
        off_text: "Off",
    },
    Tile {
        symbol: "dnd",
        name: "Modes",
        flag: "do_not_disturb",
        on_text: "Do Not Disturb",
        off_text: "",
    },
    Tile {
        symbol: "contrast",
        name: "Dark theme",
        flag: "dark_mode",
        on_text: "On",
        off_text: "Off",
    },
];
const SMALL_TILES: [Tile; 4] = [
    Tile {
        symbol: "airplane",
        name: "Airplane mode",
        flag: "airplane_mode",
        on_text: "On",
        off_text: "Off",
    },
    Tile {
        symbol: "rotate-lock",
        name: "Auto-rotate screen",
        flag: "!rotation_lock",
        on_text: "On",
        off_text: "Off",
    },
    Tile {
        symbol: "flashlight",
        name: "Flashlight",
        flag: "flashlight",
        on_text: "On",
        off_text: "Off",
    },
    Tile {
        symbol: "leaf",
        name: "Battery Saver",
        flag: "battery_saver",
        on_text: "On",
        off_text: "Off",
    },
];

fn tile(p: &mut Painter, ctx: &ShellContext<'_>, c: &ShadeInk, t: &Tile, r: Rect, large: bool) {
    let (flag, inverted) = t
        .flag
        .strip_prefix('!')
        .map_or((t.flag, false), |f| (f, true));
    let on = ctx.switch(flag) != inverted;
    let (symbol, state) = match flag {
        "wifi" if !on && ctx.switch("airplane_mode") => ("wifi", "Airplane mode"),
        "wifi" if !on => ("wifi", t.off_text),
        _ if on => (t.symbol, t.on_text),
        _ => (t.symbol, t.off_text),
    };
    let label = if state.is_empty() {
        t.name.to_owned()
    } else {
        format!("{}, {state}", t.name)
    };
    let action = format!("shell:toggle:{flag}");
    let h = r.height as i32;
    if large {
        p.button(r, c.tile, r.height / 2, &action, &label);
        check_last(p, on);
        let icon = Rect::new(r.x + 8, r.y + (h - 56) / 2, 56, 56);
        p.box_(
            icon,
            if on { c.icon_on } else { c.icon_off },
            if on { 18 } else { 28 },
        );
        p.symbol(
            symbol,
            icon.x + 17,
            icon.y + 17,
            22,
            if on { c.glyph_on } else { c.ink },
        );
        let text_w = r.width.saturating_sub(82);
        if state.is_empty() {
            p.strong(r.x + 70, r.y + h / 2 - 10, text_w, t.name, 14, c.ink);
        } else {
            p.strong(r.x + 70, r.y + h / 2 - 19, text_w, t.name, 14, c.ink);
            p.left(r.x + 70, r.y + h / 2 + 2, text_w, state, 12, c.soft);
        }
    } else {
        p.button(r, if on { c.icon_on } else { c.tile }, 28, &action, &label);
        check_last(p, on);
        p.symbol(
            symbol,
            r.x + r.width as i32 / 2 - 12,
            r.y + h / 2 - 12,
            24,
            if on { c.glyph_on } else { c.ink },
        );
    }
}

/// Where the shade's parts go at a screen size. A phone stacks Quick Settings over the
/// notifications; a wide screen (a tablet) splits them side by side, Quick Settings on the
/// left, as System UI's split shade does.
struct ShadeLayout {
    split: bool,
    /// Quick Settings' column.
    qs: Rect,
    /// The notifications' column.
    notices: Rect,
    columns: i32,
}
fn shade_layout(ctx: &ShellContext<'_>) -> ShadeLayout {
    let (w, h) = (ctx.width as i32, ctx.height as i32);
    if w >= 600 {
        let half = (w - 2 * 40 - 56) / 2;
        ShadeLayout {
            split: true,
            qs: Rect::new(40, 0, half as u32, h as u32),
            notices: Rect::new(40 + half + 56, 0, half as u32, h as u32),
            columns: 3,
        }
    } else {
        ShadeLayout {
            split: false,
            qs: Rect::new(16, 0, (w - 32) as u32, h as u32),
            notices: Rect::new(16, 0, (w - 32) as u32, h as u32),
            columns: 2,
        }
    }
}

/// Notification shade. `expanded` shows the full Quick Settings: the clock, brightness, every
/// tile and the footer; collapsed, the first tiles over the notifications. On a wide screen
/// the shade is split and Quick Settings is whole at the first pull.
fn shade(p: &mut Painter, ctx: &ShellContext<'_>, expanded: bool) {
    let c = shade_ink(ctx);
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    p.glass(full, 0, 24, c.scrim, None);
    p.region(full, "shell:dismiss", "Close notification shade");
    let l = shade_layout(ctx);
    let expanded = expanded || l.split;
    let (qx, qw) = (l.qs.x, l.qs.width as i32);
    let gap = 8;
    let cols = l.columns;
    let tile_w = (qw - gap * (cols - 1)) / cols;
    let mut y = 24;
    if l.split {
        // The split shade keeps the status bar's row as its header.
        brightness(p, ctx, &c, Rect::new(qx, 94, qw as u32, 52));
        y = 168;
    } else if expanded {
        p.left(qx, 6, 220, &clock(ctx), 40, c.ink);
        p.left(qx, 52, 220, &short_date(ctx), 14, c.ink);
        // Tapping the date opens the calendar, exactly as it does on the device.
        p.region(
            Rect::new(qx, 48, 160, 24),
            "shell:panel:calendar",
            "Open the calendar",
        );
        y = 126;
        brightness(p, ctx, &c, Rect::new(qx, y, qw as u32, 52));
        y = 200;
    }
    let large = if expanded { LARGE_TILES.len() } else { 4 };
    for (i, t) in LARGE_TILES.iter().take(large).enumerate() {
        let (col, row) = (i as i32 % cols, i as i32 / cols);
        let r = Rect::new(qx + col * (tile_w + gap), y + row * 80, tile_w as u32, 72);
        tile(p, ctx, &c, t, r, true);
    }
    y += (large as i32 + cols - 1) / cols * 80;
    if expanded {
        let small_cols = cols * 2;
        let small_w = (qw - gap * (small_cols - 1)) / small_cols;
        for (i, t) in SMALL_TILES.iter().enumerate() {
            let r = Rect::new(qx + i as i32 * (small_w + gap), y, small_w as u32, 72);
            tile(p, ctx, &c, t, r, false);
        }
        y += 80;
        // Every tile fits one page, so one dot is the truth about how many there are.
        p.circle(qx + qw / 2, y + 16, 3, c.ink);
        // Rearranging tiles needs a tile order on the desktop to rearrange; there is none,
        // and this shell must not invent one, so the button is announced off.
        let edit = Rect::new(qx + qw - 48, y - 7, 48, 48);
        p.symbol("edit", edit.x + 14, edit.y + 14, 20, c.ink);
        p.disabled("Edit tiles");
        let foot = if l.split {
            ctx.height as i32 - 140
        } else {
            ctx.height as i32 - NAV_BAR - 51
        };
        for (i, (symbol, action, label)) in [
            ("gear", "shell:settings", "Open settings"),
            ("power", "shell:panel:power", "Power menu"),
        ]
        .iter()
        .enumerate()
        {
            let hit = Rect::new(qx + qw - (2 - i as i32) * 48, foot, 48, 48);
            let power = i == 1;
            p.circle(
                hit.x + 24,
                hit.y + 24,
                21,
                if power { c.icon_on } else { c.tile },
            );
            p.symbol(
                symbol,
                hit.x + 14,
                hit.y + 14,
                20,
                if power { c.glyph_on } else { c.ink },
            );
            p.region(hit, action, label);
        }
    }
    if !expanded || l.split {
        let top = if l.split { 72 } else { y + 32 };
        notices(p, ctx, &c, Rect::new(l.notices.x, top, l.notices.width, 0));
    }
}

/// The notifications: each a card in the shade's sheet, then the footer's history, Clear all
/// and settings buttons. Each card is a notice an application really posted; tapping one
/// opens it and dispatches the action that notice carries.
fn notices(p: &mut Painter, ctx: &ShellContext<'_>, c: &ShadeInk, at: Rect) {
    let (x, w) = (at.x, at.width);
    let mut y = at.y;
    if ctx.notifications.is_empty() {
        p.center(x, y + 8, w, "No notifications", 15, c.soft);
        return;
    }
    let floor = ctx.height as i32 - NAV_BAR - 64;
    let sheet = Rect::new(x - 8, y - 8, w + 16, (floor + 56 - y + 8).max(0) as u32);
    p.box_(sheet, c.sheet, 28);
    for (i, notice) in ctx.notifications.iter().enumerate() {
        let card = Rect::new(x, y, w, 100);
        if card.y + 100 > floor {
            break;
        }
        p.button(
            card,
            if notice.seen { c.sheet } else { c.tile },
            24,
            &format!("shell:notice:{i}"),
            &notice.title,
        );
        p.circle(card.x + 36, card.y + 36, 20, c.icon_on);
        p.symbol(
            notice_symbol(&notice.app),
            card.x + 26,
            card.y + 26,
            20,
            c.glyph_on,
        );
        let text_w = w.saturating_sub(88);
        p.left(
            card.x + 72,
            card.y + 14,
            text_w,
            app_name(&notice.app).unwrap_or(&notice.app),
            12,
            c.soft,
        );
        p.strong(card.x + 72, card.y + 34, text_w, &notice.title, 14, c.ink);
        p.left(card.x + 72, card.y + 58, text_w, &notice.body, 13, c.ink);
        y += 108;
    }
    y += 6;
    let history = Rect::new(x, y, 60, 48);
    p.box_(history, c.tile, 24);
    p.symbol("clock", history.x + 20, history.y + 14, 20, c.ink);
    // The notifications a person dismissed are not kept anywhere to show again.
    p.disabled("Notification history");
    let clear = Rect::new(x + 68, y, w.saturating_sub(136), 48);
    p.button(
        clear,
        c.tile,
        24,
        "shell:notifications:clear",
        "Clear all notifications",
    );
    p.center(clear.x, clear.y + 14, clear.width, "Clear all", 15, c.ink);
    let gear = Rect::new(x + w as i32 - 60, y, 60, 48);
    p.box_(gear, c.tile, 24);
    p.symbol("bell", gear.x + 20, gear.y + 14, 20, c.ink);
    p.region(gear, "shell:settings", "Notification settings");
}

/// The volume panel the volume keys raise: Android 16's vertical pill at the screen's right
/// edge, the ringer button over the media volume slider and the panel's settings below.
fn volume_panel(p: &mut Painter, ctx: &ShellContext<'_>) {
    let c = shade_ink(ctx);
    let (w, h) = (ctx.width as i32, ctx.height as i32);
    p.region(
        Rect::new(0, 0, ctx.width, ctx.height),
        "shell:dismiss",
        "Close the volume panel",
    );
    let panel = Rect::new(w - 70, h * 31 / 100, 62, 364);
    p.drop_shadow(panel, 31, 12, 50, 2);
    p.box_(panel, c.icon_off, 31);
    let ring = Rect::new(panel.x + 7, panel.y + 7, 48, 48);
    p.circle(ring.x + 24, ring.y + 24, 24, c.icon_on);
    p.symbol("bell", ring.x + 13, ring.y + 13, 22, c.glyph_on);
    // Sound modes need a ringer the simulated phone does not have.
    p.disabled("Ring");
    let track = Rect::new(panel.x + 9, panel.y + 70, 44, 248);
    p.box_(track, c.tile, 22);
    let level = i32::from(ctx.level("volume")).min(100);
    let filled = (track.height as i32 * level / 100).max(22);
    p.box_(
        Rect::new(
            track.x,
            track.y + track.height as i32 - filled,
            44,
            filled as u32,
        ),
        c.icon_on,
        22,
    );
    p.symbol(
        "music",
        track.x + 12,
        track.y + 12,
        20,
        if level >= 92 { c.glyph_on } else { c.ink },
    );
    const STEPS: i32 = 15;
    for i in 0..STEPS {
        let percent = (STEPS - i) * 100 / STEPS;
        let top = track.y + track.height as i32 * i / STEPS;
        let bottom = track.y + track.height as i32 * (i + 1) / STEPS;
        p.region(
            Rect::new(track.x, top, 44, (bottom - top).max(1) as u32),
            &format!("shell:set:volume:{percent}"),
            &format!("Media volume {percent}%"),
        );
    }
    p.container(track, "slider", "Media volume");
    announce(p, &format!("{level}%"));
    p.symbol(
        "sliders",
        panel.x + 20,
        panel.y + panel.height as i32 - 36,
        22,
        c.ink,
    );
    p.disabled("Sound settings");
}

/// Glyph for whatever posted a notice. The set is small on purpose: a notice names an
/// application, not an icon, and inventing artwork for one would be worse than a bell.
fn notice_symbol(app: &str) -> &'static str {
    match app {
        "chat" | "messages" => "chat",
        "mail" => "mail",
        "browser" => "globe",
        "calendar" => "calendar",
        "files" => "folder",
        "screenshot" | "photos" => "image",
        _ => "bell",
    }
}

/// The power menu. Every entry really moves the display between power states, and it
/// now has a panel name of its own rather than borrowing the `apple` system menu slot.
fn power_menu(p: &mut Painter, ctx: &ShellContext<'_>) {
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    p.glass(full, 0, 18, Color(SHADE.0, SHADE.1, SHADE.2, 190), None);
    p.region(full, "shell:dismiss", "Close power menu");
    let card = Rect::new(
        (ctx.width as i32 - 232).max(8),
        70,
        216.min(ctx.width - 16),
        180,
    );
    p.drop_shadow(card, 28, 18, 90, 6);
    p.box_(card, SHADE_TILE, 28);
    for (i, (symbol, action, label)) in [
        ("lock", "shell:power:lock", "Lock"),
        ("reload", "shell:power:restart", "Restart"),
        ("power", "shell:power:off", "Power off"),
    ]
    .iter()
    .enumerate()
    {
        let row = Rect::new(card.x + 8, card.y + 8 + i as i32 * 56, card.width - 16, 52);
        p.button(row, Color(255, 255, 255, 12), 26, action, label);
        p.symbol(symbol, row.x + 16, row.y + 16, 20, SHADE_INK);
        p.left(row.x + 52, row.y + 16, row.width - 68, label, 16, SHADE_INK);
    }
}

/// Long-press and overflow sheet, the arm for panel `context`. Its entries follow the
/// application in front, so every row reaches something that exists right now.
fn context_sheet(p: &mut Painter, ctx: &ShellContext<'_>) {
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    p.glass(full, 0, 18, Color(SHADE.0, SHADE.1, SHADE.2, 150), None);
    p.region(full, "shell:dismiss", "Close menu");
    let mut rows: Vec<(&str, String, String)> = Vec::new();
    match ctx.windows.iter().find(|w| w.focused && !w.minimized) {
        Some(w) if w.kind == "browser" => {
            rows.push(("new-tab", w.tab_new(), "New tab".to_owned()));
            rows.push((
                "tabs",
                "shell:panel:window".to_owned(),
                format!("Tabs ({})", w.tabs.len().max(1)),
            ));
            rows.push((
                "reload",
                w.action("content:shell:reload"),
                "Reload".to_owned(),
            ));
            rows.push(("close", w.action("close"), "Close Chrome".to_owned()));
        }
        Some(w) => {
            rows.push(("plus", "shell:new".to_owned(), "New window".to_owned()));
            if w.kind == "editor" {
                rows.push(("document", "shell:save".to_owned(), "Save".to_owned()));
            }
            rows.push((
                "close",
                w.action("close"),
                format!("Close {}", app_name(&w.kind).unwrap_or("app")),
            ));
        }
        None => {
            rows.push(("apps", "shell:launcher".to_owned(), "All apps".to_owned()));
            rows.push((
                "clock",
                "shell:overview".to_owned(),
                "Recent apps".to_owned(),
            ));
        }
    }
    rows.push(("gear", "shell:settings".to_owned(), "Settings".to_owned()));
    rows.push((
        "lock",
        "shell:power:lock".to_owned(),
        "Lock screen".to_owned(),
    ));
    // The sheet stops short of the navigation bar so no row fights it for taps.
    let height = 44 + rows.len() as i32 * 56;
    let top = ctx.height as i32 - height - NAV_BAR;
    p.box_(
        Rect::new(0, top, ctx.width, (height + 56) as u32),
        Color::rgb(30, 34, 28),
        28,
    );
    p.box_(
        Rect::new(ctx.width as i32 / 2 - 16, top + 14, 32, 4),
        Color(226, 228, 216, 110),
        2,
    );
    for (i, (symbol, action, label)) in rows.iter().enumerate() {
        let row = Rect::new(8, top + 34 + i as i32 * 56, ctx.width - 16, 52);
        p.button(row, Color(255, 255, 255, 10), 26, action, label);
        p.symbol(symbol, row.x + 18, row.y + 16, 20, SHADE_INK);
        p.left(row.x + 56, row.y + 15, row.width - 72, label, 16, SHADE_INK);
    }
}

/// Locked and powered-off displays, drawn over everything with a real way back. There is
/// no authentication model in the world, so the lock screen swipes open rather than asking.
fn sleeping(p: &mut Painter, ctx: &ShellContext<'_>) {
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    let (w, h) = (ctx.width as i32, ctx.height as i32);
    if ctx.screen == crate::ScreenState::Off {
        p.box_(full, Color::rgb(0, 0, 0), 0);
        let dim = Color::rgb(86, 90, 82);
        p.symbol("power", w / 2 - 14, h / 2 - 14, 28, dim);
        p.center(0, h / 2 + 26, ctx.width, "Tap to turn on", 14, dim);
        p.region(full, "shell:power:wake", "Turn the display on");
        return;
    }
    p.asset(full, "wallpaper/android");
    p.box_(full, Color(8, 10, 8, 150), 0);
    let faint = Color(255, 255, 255, 200);
    let (hour, minute) = ctx.hour_minute();
    let hour = if hour % 12 == 0 { 12 } else { hour % 12 };
    // System UI's keyguard clock, which Pixel's shares: with notifications waiting, one
    // line at the top left over the date and the notifications' icons; without, the large
    // clock, hours over minutes, in the middle of the screen.
    if ctx.notifications.is_empty() {
        for (i, part) in [hour, minute].iter().enumerate() {
            p.label(
                0,
                h * 33 / 100 + i as i32 * 146,
                ctx.width,
                &format!("{part:02}"),
                150,
                Color::WHITE,
                false,
                Align::Center,
            );
        }
        p.center(0, h * 33 / 100 - 34, ctx.width, &short_date(ctx), 18, faint);
    } else {
        p.label(
            28,
            42,
            ctx.width.saturating_sub(56),
            &format!("{hour:02}:{minute:02}"),
            92,
            Color::WHITE,
            false,
            Align::Left,
        );
        p.left(
            32,
            158,
            ctx.width.saturating_sub(64),
            &short_date(ctx),
            18,
            faint,
        );
        for (i, notice) in ctx.notifications.iter().take(6).enumerate() {
            let x = 16 + i as i32 * 48;
            p.symbol(notice_symbol(&notice.app), x + 14, 220, 20, faint);
        }
    }
    p.region(full, "shell:power:wake", "Swipe up to unlock");
    p.symbol("lock", w / 2 - 12, h - 133, 24, faint);
    p.container(Rect::new(w / 2 - 36, h - 145, 72, 72), "button", "Locked");
}

fn settings(p: &mut Painter, ctx: &ShellContext<'_>) {
    p.box_(Rect::new(0, 0, ctx.width, ctx.height), PAPER, 0);
    // Settings is a full screen, not a sheet: the backdrop absorbs taps rather than
    // letting them fall through to the windows it covers.
    p.region(
        Rect::new(0, 0, ctx.width, ctx.height),
        "shell:noop",
        "Settings",
    );
    p.symbol("arrow-left", 18, 58, 24, INK);
    p.region(Rect::new(8, 48, 48, 44), "shell:dismiss", "Back");
    p.left(24, 112, ctx.width - 48, "About phone", 32, INK);
    // Device facts are readings, not rows. They carry no interaction and no semantic at
    // all, so nothing announces them as a control, and they wear no chevron either.
    let mut y = 184;
    for (key, value) in [
        ("Device name", "Pixel".to_owned()),
        ("Android version", "15".to_owned()),
        ("Display", format!("{} × {} pixels", ctx.width, ctx.height)),
    ] {
        p.left(24, y, ctx.width - 48, key, 18, INK);
        p.left(24, y + 25, ctx.width - 48, &value, 14, SOFT);
        y += 62;
    }
    p.hline(24, y + 2, ctx.width - 48, Color(0, 0, 0, 28));
    y += 22;
    // Below the rule, every row really opens the surface it names, and says so with the
    // chevron the readings above deliberately do not have.
    let unseen = ctx.unseen_notices();
    for (key, value, action) in [
        (
            "Display & brightness",
            format!("{}%", ctx.level("brightness")),
            "shell:quick-settings",
        ),
        (
            "Notifications",
            match (ctx.notifications.len(), unseen) {
                (0, _) => "None".to_owned(),
                (total, 0) => format!("{total} read"),
                (_, unseen) => format!("{unseen} unread"),
            },
            "shell:notifications",
        ),
        (
            "Open applications",
            ctx.windows.len().to_string(),
            "shell:overview",
        ),
    ] {
        p.left(24, y, ctx.width - 88, key, 18, INK);
        p.left(24, y + 25, ctx.width - 88, &value, 14, SOFT);
        p.symbol("chevron-right", ctx.width as i32 - 44, y + 12, 18, SOFT);
        p.region(
            Rect::new(16, y - 8, ctx.width - 32, 62),
            action,
            &format!("{key}, {value}"),
        );
        y += 62;
    }
}

/// The month grid behind the shade's date. It draws the month the panel is paging —
/// `ctx.panel_date()` — rather than the world's own, and the chevrons move it for real.
fn calendar(p: &mut Painter, ctx: &ShellContext<'_>) {
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    p.box_(full, PAPER, 0);
    // A full screen, not a sheet: the backdrop absorbs the taps it covers.
    p.region(full, "shell:noop", "Calendar");
    p.symbol("arrow-left", 18, 58, 24, INK);
    p.region(Rect::new(8, 48, 48, 44), "shell:dismiss", "Back");
    let shown = ctx.panel_date();
    let today = ctx.date();
    let w = ctx.width as i32;
    p.left(
        24,
        108,
        ctx.width.saturating_sub(160),
        &format!("{} {}", shown.month_name(), shown.year),
        28,
        INK,
    );
    for (i, (symbol, action, label)) in [
        ("chevron-left", "shell:month:prev", "Previous month"),
        ("chevron-right", "shell:month:next", "Next month"),
    ]
    .iter()
    .enumerate()
    {
        let hit = Rect::new(w - 112 + i as i32 * 52, 102, 44, 44);
        p.circle(hit.x + 22, hit.y + 22, 22, CONTAINER);
        p.symbol(symbol, hit.x + 12, hit.y + 12, 20, INK);
        p.region(hit, action, label);
    }
    // Returning to today is a move only when the grid has left it.
    let mut y = 164;
    if ctx.panel_month != 0 {
        let chip = Rect::new(24, y, 104, 40);
        p.button(
            chip,
            PRIMARY_CONTAINER,
            20,
            "shell:month:today",
            &format!("Back to {} {}", today.month_name(), today.year),
        );
        p.strong_center(
            chip.x,
            chip.y + 12,
            104,
            "Today",
            14,
            Color::rgb(36, 54, 16),
        );
        y += 56;
    }
    let cell = ((ctx.width - 32) / 7) as i32;
    for (i, day) in ["S", "M", "T", "W", "T", "F", "S"].iter().enumerate() {
        p.center(16 + i as i32 * cell, y, cell as u32, day, 12, SOFT);
    }
    let rows = (shown.first_weekday + shown.days_in_month)
        .div_ceil(7)
        .max(1) as i32;
    let step = cell.min(54);
    // Each day opens Calendar on that day. Without Calendar installed, or before the
    // world began, a day is shown and not offered: nothing could honour the tap.
    if !ctx.installed("calendar") {
        p.box_(
            Rect::new(16, y + 28, ctx.width - 32, (rows * step) as u32),
            Color::TRANSPARENT,
            0,
        );
        p.disabled("Calendar days");
    }
    for day in 1..=shown.days_in_month {
        let slot = (day - 1 + shown.first_weekday) as i32;
        let x = 16 + (slot % 7) * cell;
        let cy = y + 32 + (slot / 7) * step;
        if let Some(open) = ctx.open_day(day) {
            p.region_above(
                Rect::new(x, cy - 4, cell as u32, (step - 2).max(1) as u32),
                &open,
                &format!("{} {day}", shown.month_name()),
            );
        }
        let is_today = ctx.panel_month == 0 && day == today.day;
        if is_today {
            p.circle(x + cell / 2, cy + 10, 18, PRIMARY);
        }
        p.label(
            x,
            cy,
            cell as u32,
            &day.to_string(),
            15,
            if is_today { Color::WHITE } else { INK },
            false,
            Align::Center,
        );
    }
    if ctx.installed("calendar") {
        let open = Rect::new(24, y + 36 + rows * step, 180, 44);
        if open.y + 44 < ctx.height as i32 - NAV_BAR - 8 {
            p.button(open, PRIMARY, 22, "shell:launch:calendar", "Open Calendar");
            p.strong_center(open.x, open.y + 14, 180, "Open Calendar", 14, Color::WHITE);
        }
    }
}

fn overview(p: &mut Painter, ctx: &ShellContext<'_>) {
    let w = ctx.width as i32;
    let h = ctx.height as i32;
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    p.glass(full, 0, 30, Color(225, 232, 210, 170), None);
    // Tapping the space around the cards goes home, as it does on a Pixel.
    p.region(full, "shell:home", "Home screen");
    if ctx.windows.is_empty() {
        p.center(0, h / 2 - 12, ctx.width, "No recent items", 18, INK);
        return;
    }
    let focused = ctx
        .windows
        .iter()
        .position(|window| window.focused)
        .unwrap_or(ctx.windows.len() - 1) as i32;
    // The carousel is centred on the focused card until it is swiped; past the oldest
    // card (to the left) is the Clear all slot.
    let centred = ctx
        .overview
        .slot
        .unwrap_or(focused)
        .clamp(-1, ctx.windows.len() as i32 - 1);
    // Launcher3's recents, which Pixel's shares: cards two thirds of the screen, 16 apart,
    // each with its application's chip at its top left.
    let card_width = ctx.width * 68 / 100;
    let card_height = ctx.height * 68 / 100;
    let stride = card_width as i32 + 16;
    let slot_x = |i: i32| (w - card_width as i32) / 2 + (i - centred) * stride;
    let y = ctx.height as i32 * 8 / 100;
    let clear = Rect::new(
        slot_x(-1) + card_width as i32 / 2 - 60,
        y + card_height as i32 / 2 - 22,
        120,
        44,
    );
    if clear.x + clear.width as i32 > 0 && clear.x < w {
        p.button(clear, PAPER, 22, "shell:recents:clear", "Clear all");
        p.label(
            clear.x,
            clear.y + 12,
            clear.width,
            "Clear all",
            14,
            INK,
            true,
            Align::Center,
        );
    }
    for (i, window) in ctx.windows.iter().enumerate() {
        let x = slot_x(i as i32);
        if x + card_width as i32 <= 0 || x >= w {
            continue;
        }
        let card = Rect::new(x, y, card_width, card_height);
        p.drop_shadow(card, 24, 16, 70, 6);
        if let Some(content) = &window.content {
            p.thumbnail(content, card, 24);
            if ctx.overview.select && i as i32 == centred {
                select_highlights(p, content, card);
            }
        } else {
            p.box_(card, PAPER, 24);
        }
        p.region(
            card,
            &window.action("focus"),
            &format!("Resume {}, swipe up to dismiss", window.title),
        );
        let name = app_name(&window.kind).unwrap_or(&window.title);
        let chip = Rect::new(x + 4, y + 4, 164.min(card_width - 8), 52);
        p.box_(chip, Color(248, 250, 240, 235), 26);
        p.asset(
            Rect::new(chip.x + 10, chip.y + 10, 32, 32),
            &format!("icon/android/{}", window.kind),
        );
        p.left(
            chip.x + 50,
            chip.y + 17,
            chip.width.saturating_sub(84),
            name,
            14,
            INK,
        );
        p.symbol(
            "chevron-down",
            chip.x + chip.width as i32 - 30,
            chip.y + 17,
            18,
            SOFT,
        );
        p.container(chip, "button", name);
    }
    // The action row under the centred card: Screenshot captures that application and
    // Select picks up the text it shows; in Select mode, Copy takes it to the clipboard.
    // There is no Close chip: a card is dismissed by swiping it up.
    if centred >= 0 {
        let row_y = y + card_height as i32 + 24;
        let chips: [(&str, &str, &str); 2] = if ctx.overview.select {
            [
                ("copy", "Copy", "shell:recents:copy"),
                ("check", "Done", "shell:recents:select"),
            ]
        } else {
            [
                ("screenshot", "Screenshot", "shell:recents:screenshot"),
                ("text-tool", "Select", "shell:recents:select"),
            ]
        };
        let chip_w = 124;
        let x0 = w / 2 - chip_w - 6;
        for (i, (symbol, label, action)) in chips.iter().enumerate() {
            let chip = Rect::new(x0 + i as i32 * (chip_w + 12), row_y, chip_w as u32, 48);
            p.button(chip, PAPER, 24, action, label);
            p.symbol(symbol, chip.x + 16, chip.y + 16, 16, INK);
            p.left(chip.x + 40, chip.y + 15, 80, label, 14, INK);
        }
    }
    // Every live app is addressable, including those outside the horizontal card viewport.
    let count = ctx.windows.len().max(1) as i32;
    let icon_size = (w / (count + 1)).clamp(22, 40) as u32;
    for (i, window) in ctx.windows.iter().enumerate() {
        let x = w * (i as i32 + 1) / (count + 1) - icon_size as i32 / 2;
        p.platform_icon(
            Rect::new(x, h - NAV_BAR - 52, icon_size, icon_size),
            "android",
            &window.kind,
            &window.action("focus"),
            &format!("Resume {}", window.title),
        );
    }
}

/// Select mode marks every run of text on the card, the way Pixel outlines what it
/// can pick up from an app's screenshot.
fn select_highlights(p: &mut Painter, content: &cw_scene::Scene, card: Rect) {
    let scale = (i64::from(card.width) * 1024 / i64::from(content.width.max(1))) as i32;
    let s = |v: i32| v * scale / 1024;
    for n in &content.nodes {
        if n.painted_text().is_none_or(|t| t.trim().is_empty()) {
            continue;
        }
        let b = n.painted_bounds();
        let r = Rect::new(
            card.x + s(b.x),
            card.y + s(b.y),
            s(b.width as i32).max(2) as u32,
            s(b.height as i32).max(2) as u32,
        );
        if let Some(r) = r.intersection(card) {
            p.box_(r, Color(76, 102, 43, 70), 2);
        }
    }
}

/// Topmost visible window: the one a keystroke would reach.
fn front<'a>(ctx: &'a ShellContext<'_>) -> Option<&'a WindowView> {
    ctx.windows
        .iter()
        .rev()
        .find(|w| w.focused && !w.minimized)
        .or_else(|| ctx.windows.iter().rev().find(|w| !w.minimized))
}

/// Gboard's rows. Letters type lowercase and there is no `?123` plane: a shift and a/// What the enter key does, which is also the glyph Gboard wears for it. The keystroke
/// is `Enter` whichever one it is.
fn enter_label(ctx: &ShellContext<'_>) -> &'static str {
    if ctx.launcher_open || ctx.panel == Some("search") {
        "Search"
    } else if front(ctx).is_some_and(|w| w.kind == "browser" && w.editing) {
        "Go"
    } else {
        "Enter"
    }
}

/// Gboard's three planes, top row first. Between them they carry every printable ASCII
/// character, so nothing needs a long press that this keyboard has no way to express.
fn plane_rows(plane: crate::Plane) -> [&'static str; 3] {
    match plane {
        crate::Plane::Letters => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        crate::Plane::Numbers => ["1234567890", "@#$_&-+()/", "*\"':;!?"],
        crate::Plane::Symbols => [
            "~`|\u{2022}\u{f7}\u{d7}\u{b6}\u{b0}\u{a7}\u{b5}",
            "\u{a3}\u{a2}\u{20ac}\u{a5}^%={}\\",
            "\u{a9}\u{ae}\u{2122}[]<>",
        ],
    }
}

/// Key grid at this width: gap, key width, key height, and the keyboard's own height
/// including the suggestion strip. Every plane is four rows, so switching one never
/// moves what sits above the keyboard.
fn key_grid(width: u32) -> (i32, i32, i32, i32) {
    let gap = (width as i32 / 70).clamp(4, 8);
    let key_w = (width as i32 - 8 - gap * 9) / 10;
    let key_h = (key_w * 6 / 5).clamp(34, 48);
    (gap, key_w, key_h, key_h * 4 + gap * 3 + 60)
}

/// Pixels the keyboard takes from the bottom of the screen, 0 when it is down, including
/// the navigation bar it leaves clear beneath itself. `ctx.text_entry` is the keystroke
/// router's own answer, hoisted before the scene exists, so a painted keyboard and a real
/// keystroke cannot disagree; a dark display is the one case it does not cover.
fn keyboard_height(ctx: &ShellContext<'_>) -> i32 {
    let total = key_grid(ctx.width).3 + NAV_BAR;
    if !ctx.text_entry || !ctx.awake() || total * 2 > ctx.height as i32 {
        return 0;
    }
    // The app drawer takes typing to search it, but shows no keyboard until its search field
    // is tapped or something is typed (Launcher3's and Pixel's alike).
    if ctx.launcher_open && ctx.panel != Some("search") && ctx.search.is_empty() {
        return 0;
    }
    total
}

/// One key. The whole cell takes the tap, so the gaps between keys are not dead, and the
/// transparent hit target is painted last so it is the topmost thing over its own face.
#[allow(clippy::too_many_arguments)]
fn key(
    p: &mut Painter,
    face: Rect,
    gap: i32,
    cap: &str,
    size: u16,
    fill: Color,
    action: &str,
    label: &str,
) {
    p.box_(face, fill, 8);
    if !cap.is_empty() {
        p.label(
            face.x,
            face.y + (face.height as i32 - i32::from(size) * 3 / 2) / 2,
            face.width,
            cap,
            size,
            INK,
            false,
            Align::Center,
        );
    }
    p.button(
        Rect::new(
            face.x - gap / 2,
            face.y - gap / 2,
            face.width + gap as u32,
            face.height + gap as u32,
        ),
        Color::TRANSPARENT,
        0,
        action,
        label,
    );
}

/// The shift key in the position it really holds: plain when off, tonal for the next
/// character only, and tonal with a bar beneath it when locked. One tap moves it on,
/// because `shell:key:Shift` cycles the same three positions the device does.
fn shift_key(p: &mut Painter, ctx: &ShellContext<'_>, r: Rect, gap: i32, off: Color) {
    let shift = ctx.keyboard.shift;
    let lit = shift != crate::Shift::Off;
    let fill = if lit { PRIMARY_CONTAINER } else { off };
    key(p, r, gap, "", 16, fill, "shell:key:Shift", "Shift");
    announce(
        p,
        match shift {
            crate::Shift::Off => "Off",
            crate::Shift::Once => "Next character",
            crate::Shift::Lock => "Locked",
        },
    );
    let ink = if lit { Color::rgb(36, 54, 16) } else { SOFT };
    let (cx, cy) = (r.x + r.width as i32 / 2, r.y + r.height as i32 / 2);
    p.symbol("arrow-up", cx - 11, cy - 13, 22, ink);
    if shift == crate::Shift::Lock {
        p.box_(Rect::new(cx - 8, cy + 10, 16, 2), ink, 1);
    }
}

/// Gboard. Every key dispatches into the same `keyboard.v1` pipeline an actor's own
/// keyboard action uses, so a painted key and a scripted keystroke cannot diverge. A
/// letter key always sends its lower-case character: the shift is applied by the handler,
/// which is what spends a one-shot shift exactly once.
fn keyboard(p: &mut Painter, ctx: &ShellContext<'_>) {
    if keyboard_height(ctx) == 0 {
        return;
    }
    let (gap, kw, kh, total) = key_grid(ctx.width);
    let w = ctx.width as i32;
    let full = kw * 10 + gap * 9;
    let left = (w - full) / 2;
    let top = ctx.height as i32 - NAV_BAR - total;
    p.box_(
        Rect::new(0, top, ctx.width, total as u32),
        Color::rgb(236, 240, 228),
        0,
    );
    // The suggestion strip completes the word being typed from a fixed word list; a chip
    // types the rest of it and a space through `shell:insert:`, the same `keyboard.v1`
    // pipeline a key uses. Voice typing is off for the reason it is off everywhere here.
    let offered = ctx.suggestions(3);
    let strip = (w - 112 - left).max(0);
    let chip = strip / 3;
    for (i, (word, action)) in offered.iter().enumerate() {
        let r = Rect::new(left + i as i32 * chip, top + 4, chip.max(1) as u32, 38);
        if ctx.hovered(r) {
            p.box_(r, Color(0, 0, 0, 14), 19);
        }
        p.button(r, Color::TRANSPARENT, 19, action, word);
        p.center(
            r.x,
            r.y + 10,
            r.width,
            word,
            15,
            if i == 0 { INK } else { Color(68, 72, 60, 230) },
        );
        if i > 0 {
            p.vline(r.x, r.y + 10, 18, Color(0, 0, 0, 30));
        }
    }
    p.symbol("mic", w - 96, top + 12, 20, SOFT);
    p.disabled("Voice typing");
    let gear = Rect::new(w - 56, top + 6, 40, 36);
    p.symbol("gear", gear.x + 10, gear.y + 8, 20, SOFT);
    p.region(gear, "shell:settings", "Settings");
    p.hline(0, top + 44, ctx.width, Color(0, 0, 0, 18));
    let face = PAPER;
    let heavy = Color::rgb(214, 220, 204);
    let plane = ctx.keyboard.plane;
    let upper = ctx.shifted();
    let wide = kw * 3 / 2;
    let mut y = top + 52;
    for (row, keys) in plane_rows(plane).iter().enumerate() {
        let n = keys.chars().count() as i32;
        let x0 = left + (full - (n * kw + (n - 1) * gap)) / 2;
        for (i, ch) in keys.chars().enumerate() {
            let cap = if upper {
                ch.to_uppercase().to_string()
            } else {
                ch.to_string()
            };
            key(
                p,
                Rect::new(x0 + i as i32 * (kw + gap), y, kw as u32, kh as u32),
                gap,
                &cap,
                20,
                face,
                &format!("shell:type:{ch}"),
                &cap,
            );
        }
        if row == 2 {
            // Left of the bottom row: shift on the letters, the other symbol plane
            // elsewhere, which is where Gboard puts each of them.
            let slot = Rect::new(left, y, wide as u32, kh as u32);
            match plane {
                crate::Plane::Letters => shift_key(p, ctx, slot, gap, heavy),
                crate::Plane::Numbers => {
                    key(
                        p,
                        slot,
                        gap,
                        "=\\<",
                        15,
                        heavy,
                        "shell:plane:symbols",
                        "=\\<",
                    );
                }
                crate::Plane::Symbols => {
                    key(
                        p,
                        slot,
                        gap,
                        "?123",
                        15,
                        heavy,
                        "shell:plane:numbers",
                        "?123",
                    );
                }
            }
            let back = Rect::new(left + full - wide, y, wide as u32, kh as u32);
            key(p, back, gap, "", 16, heavy, "shell:key:Backspace", "Delete");
            p.symbol(
                "backspace",
                back.x + back.width as i32 / 2 - 11,
                y + kh / 2 - 11,
                22,
                SOFT,
            );
        }
        y += kh + gap;
    }
    let (plane_cap, plane_action) = match plane {
        crate::Plane::Letters => ("?123", "shell:plane:numbers"),
        _ => ("ABC", "shell:plane:letters"),
    };
    let enter = enter_label(ctx);
    let mut x = left;
    for (cap, action, label, width, fill) in [
        (plane_cap, plane_action, plane_cap, kw * 3 / 2, heavy),
        (",", "shell:type:,", "comma", kw, heavy),
        ("", "shell:type: ", "space", kw * 4 + gap * 4 + kw / 2, face),
        (".", "shell:type:.", "period", kw, heavy),
        (
            "",
            "shell:key:Enter",
            enter,
            kw * 2 + gap,
            PRIMARY_CONTAINER,
        ),
    ] {
        let r = Rect::new(x, y, width as u32, kh as u32);
        key(p, r, gap, cap, 18, fill, action, label);
        if action == "shell:type: " {
            // The space bar wears Gboard's bare rule.
            p.hline(
                r.x + r.width as i32 / 2 - 26,
                y + kh / 2,
                52,
                Color(68, 72, 60, 120),
            );
        }
        if action == "shell:key:Enter" {
            p.symbol(
                if enter == "Search" { "search" } else { "send" },
                r.x + r.width as i32 / 2 - 11,
                y + kh / 2 - 11,
                22,
                Color::rgb(36, 54, 16),
            );
        }
        x += width + gap;
    }
}

pub fn chrome(p: &mut Painter, ctx: &ShellContext<'_>) {
    // A dark or off display covers everything, and owns the only way back.
    if !ctx.awake() {
        sleeping(p, ctx);
        return;
    }
    // The dark theme reaches the shell's own light surfaces too: the drawer, recents, the
    // keyboard (the shade and the volume panel choose their colours themselves).
    let night = ctx.switch("dark_mode");
    let mark = p.scene.nodes.len();
    if ctx.launcher_open || ctx.panel == Some("search") {
        app_drawer(p, ctx);
    }
    if night {
        dark_theme(&mut p.scene.nodes[mark..]);
    }
    let browser_open = ctx
        .windows
        .iter()
        .any(|w| w.kind == "browser" && !w.minimized);
    let dark = match ctx.panel {
        Some("overview") => {
            let mark = p.scene.nodes.len();
            overview(p, ctx);
            if night {
                dark_theme(&mut p.scene.nodes[mark..]);
            }
            night
        }
        Some("quick") => {
            shade(p, ctx, true);
            ctx.switch("dark_mode")
        }
        Some("notifications") => {
            shade(p, ctx, false);
            ctx.switch("dark_mode")
        }
        Some("calendar") => {
            calendar(p, ctx);
            false
        }
        Some("settings") => {
            settings(p, ctx);
            false
        }
        Some("power") => {
            power_menu(p, ctx);
            true
        }
        Some("context") => {
            context_sheet(p, ctx);
            true
        }
        Some("volume") => {
            volume_panel(p, ctx);
            false
        }
        // Chrome draws its own tab switcher; the drawer draws the search panel.
        Some("window") if browser_open => true,
        Some("search") | None => false,
        Some(_) => {
            // No arm renders this panel, so at least never strand the user in an
            // invisible modal state that only Escape can clear.
            let full = Rect::new(0, 0, ctx.width, ctx.height);
            p.glass(full, 0, 8, Color(SHADE.0, SHADE.1, SHADE.2, 130), None);
            p.region(full, "shell:dismiss", "Close menu");
            true
        }
    };
    let terminal = ctx.panel.is_none()
        && !ctx.launcher_open
        && ctx
            .windows
            .iter()
            .rev()
            .find(|w| !w.minimized)
            .is_some_and(|w| w.kind == "terminal");
    // The buttons are drawn over whatever is beneath: light over a dark surface or the
    // wallpaper of the home screen, dark over a light application or sheet.
    let home = !ctx.active && !ctx.launcher_open && ctx.panel.is_none();
    let dark = dark || terminal || (night && !home && ctx.panel.is_none());
    let ink = if dark { SHADE_INK } else { INK };
    status(p, ctx, ink);
    // The status bar is where the shade is pulled down from; a tap on it opens nothing,
    // as on the device. See `shell:gesture:notifications`.
    gesture(
        p,
        Rect::new(0, 0, ctx.width, 38),
        "shell:gesture:notifications",
        "Status bar, swipe down for notifications",
    );
    let mark = p.scene.nodes.len();
    keyboard(p, ctx);
    if night {
        dark_theme(&mut p.scene.nodes[mark..]);
    }
    let buttons = if dark {
        SHADE_INK
    } else if home {
        Color::WHITE
    } else {
        SOFT
    };
    navigation_bar(p, ctx, buttons);
}

/// A gesture affordance: an interaction target a pointer reaches by dragging from it,
/// not by tapping it. The router turns a swipe that starts here into the gesture and
/// treats a tap as the device does, as nothing; `application.v1 shell` performs it.
fn gesture(p: &mut Painter, r: Rect, action: &str, label: &str) {
    p.region(r, action, label);
    if let Some(s) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
        s.role = "gesture".into();
    }
}

/// Height of the three-button navigation bar: 48 dp, the platform's own.
const NAV_BAR: i32 = super::ANDROID_NAV_BAR;

/// Pixel's three-button navigation: Back, Home and Recents, outlined glyphs spread over
/// the bar. Back is the platform back action — the panel, then the application's own
/// history, then out of the application — Home goes home, Recents opens the overview.
fn navigation_bar(p: &mut Painter, ctx: &ShellContext<'_>, ink: Color) {
    let (w, h) = (ctx.width as i32, ctx.height as i32);
    let bar = Rect::new(0, h - NAV_BAR, ctx.width, NAV_BAR as u32);
    // The bar carries the surface beneath it: the keyboard's plate when it is up.
    let ink = if keyboard_height(ctx) > 0 { SOFT } else { ink };
    if keyboard_height(ctx) > 0 {
        p.box_(bar, Color::rgb(236, 240, 228), 0);
    }
    let cy = bar.y + NAV_BAR / 2;
    for (i, (action, label)) in [
        ("shell:mobile-back", "Back"),
        ("shell:home", "Home"),
        ("shell:overview", "Recents"),
    ]
    .iter()
    .enumerate()
    {
        let cx = w * (i as i32 + 1) / 4;
        match i {
            0 => p.line(
                vec![
                    (cx + 6, cy - 8),
                    (cx - 8, cy),
                    (cx + 6, cy + 8),
                    (cx + 6, cy - 8),
                ],
                ink,
                2,
            ),
            1 => p.ring(cx, cy, 8, 2, ink),
            _ => p.node(
                Rect::new(cx - 7, cy - 7, 14, 14),
                cw_scene::Primitive::RoundedBox {
                    fill: Color::TRANSPARENT,
                    border: Some(ink),
                    border_width: 2,
                    radius: 2,
                },
                None,
            ),
        }
        p.region(Rect::new(cx - 40, bar.y, 80, NAV_BAR as u32), action, label);
    }
}

pub fn window_frame(p: &mut Painter, ctx: &ShellContext<'_>, window: &WindowView) {
    let r = window.rect;
    let dark = window.kind == "terminal";
    p.box_(r, if dark { Color::rgb(18, 20, 22) } else { PAPER }, 0);
    if window.kind == "browser" || window.kind == "music" || window.kind == "calculator" {
        return;
    }
    let ink = if dark { SHADE_INK } else { INK };
    // Material 3 small top app bar beneath the status bar. Only a screen with a parent
    // wears an up arrow — the file manager inside a folder; an application's root
    // screen has none, because leaving it is the navigation bar's Back.
    let files_up = window.kind == "files" && !window.document.trim_end_matches('/').is_empty();
    // An application's leading control: an up arrow on a screen with a parent (an open
    // conversation, an open message), Gmail's menu button for its drawer.
    let nav = window.chrome("nav").and_then(|nav| {
        let mut parts = nav.splitn(3, '\t');
        let symbol = match parts.next()? {
            "back" => "arrow-left",
            "menu" => "menu",
            _ => return None,
        };
        let target = parts.next()?;
        let label = match parts.next().unwrap_or("") {
            "" if symbol == "arrow-left" => "Navigate up",
            "" => "Open menu",
            label => label,
        };
        Some((symbol, target, label))
    });
    if let Some((symbol, target, label)) = nav {
        p.symbol(symbol, r.x + 16, r.y + 56, 24, ink);
        p.region(
            Rect::new(r.x + 4, r.y + 44, 48, 48),
            &window.action(&format!("content:{target}")),
            label,
        );
    }
    if files_up {
        p.symbol("arrow-left", r.x + 16, r.y + 56, 24, ink);
        p.region(
            Rect::new(r.x + 4, r.y + 44, 48, 48),
            &window.action("content:files-up"),
            "Navigate up",
        );
    }
    let title = app_bar_title(window);
    let title_x = if files_up || nav.is_some() {
        r.x + 60
    } else {
        r.x + 20
    };
    p.left(
        title_x,
        r.y + 55,
        r.width.saturating_sub(160),
        &title,
        22,
        ink,
    );
    let right = r.x + r.width as i32;
    if window.kind == "files" {
        let hit = Rect::new(right - 96, r.y + 48, 40, 40);
        p.symbol("search", hit.x + 9, hit.y + 10, 22, ink);
        // The file manager's own folder search: the same field its content paints.
        p.region(
            hit,
            &window.action("content:files-search"),
            "Search this folder",
        );
    }
    p.symbol("more-vertical", right - 44, r.y + 58, 22, ink);
    p.region(
        Rect::new(right - 52, r.y + 48, 40, 40),
        "shell:panel:context",
        "More options",
    );
    let _ = ctx;
}

/// Chrome's toolbar occupies the application bar beneath the status bar.
pub fn browser_chrome(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView) {
    let r = w.rect;
    let right = r.x + r.width as i32;
    for (i, (symbol, action, label, live)) in [
        ("arrow-left", "back", "Back", w.can_go_back),
        ("arrow-right", "forward", "Forward", w.can_go_forward),
    ]
    .iter()
    .enumerate()
    {
        let hit = Rect::new(r.x + 6 + i as i32 * 38, r.y + 46, 36, 44);
        p.symbol(
            symbol,
            hit.x + 8,
            hit.y + 12,
            20,
            if *live { SOFT } else { Color(68, 72, 60, 80) },
        );
        if *live {
            p.region(hit, &w.action(&format!("content:shell:{action}")), label);
        } else {
            // A tab with no history behind it greys out rather than being refused.
            p.disabled(label);
        }
    }
    let field = Rect::new(
        r.x + 86,
        r.y + 46,
        (right - 86 - 84 - r.x).max(60) as u32,
        44,
    );
    p.button(
        field,
        if w.editing { Color::WHITE } else { CONTAINER },
        22,
        &w.action("content:shell:address"),
        "Address and search",
    );
    let typed = w
        .title
        .split_once(" — ")
        .map_or(w.title.as_str(), |(_, a)| a);
    let inner = field.width.saturating_sub(84);
    if typed.is_empty() {
        p.left(
            field.x + 18,
            field.y + 13,
            inner + 30,
            "Search or type URL",
            15,
            SOFT,
        );
    } else {
        let shown = if w.editing {
            typed
        } else {
            typed
                .split_once("://")
                .map_or(typed, |(_, rest)| rest)
                .trim_end_matches('/')
        };
        p.symbol(
            if w.editing { "search" } else { "lock" },
            field.x + 16,
            field.y + 15,
            14,
            SOFT,
        );
        let end = p.left(field.x + 38, field.y + 13, inner, shown, 15, INK);
        if w.editing {
            p.box_(
                Rect::new(field.x + 40 + end as i32, field.y + 12, 2, 20),
                PRIMARY,
                1,
            );
        }
    }
    let reload = Rect::new(field.x + field.width as i32 - 42, field.y + 2, 40, 40);
    p.symbol("reload", reload.x + 11, reload.y + 11, 18, SOFT);
    p.region(reload, &w.action("content:shell:reload"), "Reload");
    // The counter is the browser session's real tab count, and opens the switcher.
    let count = w.tabs.len().max(1);
    p.border(
        Rect::new(right - 74, r.y + 57, 22, 22),
        Color::TRANSPARENT,
        6,
        SOFT,
    );
    p.border(
        Rect::new(right - 73, r.y + 58, 20, 20),
        Color::TRANSPARENT,
        5,
        SOFT,
    );
    p.label(
        right - 74,
        r.y + 60,
        22,
        &count.to_string(),
        12,
        SOFT,
        true,
        Align::Center,
    );
    p.region(
        Rect::new(right - 82, r.y + 51, 36, 36),
        "shell:panel:window",
        &format!("Tab switcher, {count} open"),
    );
    p.symbol("more-vertical", right - 40, r.y + 57, 22, SOFT);
    p.region(
        Rect::new(right - 46, r.y + 51, 36, 36),
        "shell:panel:context",
        "More options",
    );
    p.hline(r.x, r.y + 95, r.width, Color(0, 0, 0, 20));
    if ctx.panel == Some("window") {
        tab_switcher(p, ctx, w);
    }
}

/// Chrome's tab grid over the whole display: the session's real tabs, each selectable
/// and closable, plus a new-tab button.
fn tab_switcher(p: &mut Painter, ctx: &ShellContext<'_>, w: &WindowView) {
    let full = Rect::new(0, 0, ctx.width, ctx.height);
    p.glass(full, 0, 24, Color(SHADE.0, SHADE.1, SHADE.2, 242), None);
    p.region(full, "shell:dismiss", "Close tab switcher");
    let width = ctx.width as i32;
    let count = w.tabs.len().max(1);
    p.left(
        20,
        50,
        200,
        &format!("{count} tab{}", if count == 1 { "" } else { "s" }),
        20,
        SHADE_INK,
    );
    for (rect, action, symbol, label) in [
        (
            Rect::new(width - 104, 42, 40, 40),
            w.tab_new(),
            "plus",
            "New tab".to_owned(),
        ),
        (
            Rect::new(width - 56, 42, 40, 40),
            "shell:dismiss".to_owned(),
            "close",
            "Close tab switcher".to_owned(),
        ),
    ] {
        p.circle(rect.x + 20, rect.y + 20, 20, SHADE_TILE);
        p.symbol(symbol, rect.x + 11, rect.y + 11, 18, SHADE_INK);
        p.region(rect, &action, &label);
    }
    let card_width = ctx.width.saturating_sub(48) / 2;
    let card_height = card_width * 5 / 4;
    for (i, label) in w.tabs.iter().enumerate() {
        let y = 100 + (i / 2) as i32 * (card_height as i32 + 20);
        if y + card_height as i32 > ctx.height as i32 - NAV_BAR - 8 {
            break;
        }
        let x = 16 + (i % 2) as i32 * (card_width as i32 + 16);
        let active = i == w.active_tab;
        p.button(
            Rect::new(x, y, card_width, card_height),
            if active {
                PRIMARY_CONTAINER
            } else {
                Color::rgb(40, 44, 38)
            },
            16,
            &w.tab_select(i),
            label,
        );
        let ink = if active {
            Color::rgb(36, 54, 16)
        } else {
            SHADE_INK
        };
        p.left(
            x + 12,
            y + 11,
            card_width.saturating_sub(52),
            label,
            13,
            ink,
        );
        let close = Rect::new(x + card_width as i32 - 34, y + 6, 28, 28);
        p.symbol("close", close.x + 7, close.y + 7, 14, ink);
        p.region(close, &w.tab_close(i), &format!("Close {label}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop_scene::DesktopTheme;
    fn context<'a>(panel: Option<&'a str>) -> ShellContext<'a> {
        ShellContext {
            theme: DesktopTheme::Android,
            width: 412,
            height: 892,
            clock_us: 0,
            title: "",
            launcher_open: false,
            active: false,
            windows: &[],
            installed_apps: &[],
            panel,
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
            battery: true,
            anchor: None,
            overview: Default::default(),
        }
    }
    /// Every id the scene can actually dispatch, ignoring announced-disabled paint.
    fn actions(p: &Painter) -> Vec<String> {
        p.scene
            .nodes
            .iter()
            .filter(|n| !n.semantic.as_ref().is_some_and(|s| s.disabled))
            .filter_map(|n| n.interaction.clone())
            .collect()
    }
    fn shell(ctx: &ShellContext<'_>) -> Painter {
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        background(&mut p, ctx);
        chrome(&mut p, ctx);
        p
    }
    fn target<'a>(p: &'a Painter, id: &str) -> &'a cw_scene::Node {
        p.scene
            .nodes
            .iter()
            .find(|n| n.interaction.as_deref() == Some(id))
            .unwrap_or_else(|| panic!("no control emits {id}"))
    }
    /// Whether some node paints exactly this line of text.
    fn shows(p: &Painter, needle: &str) -> bool {
        p.scene.nodes.iter().any(|n| {
            matches!(
                &n.primitive,
                cw_scene::Primitive::UiText { text, .. }
                    | cw_scene::Primitive::UiTextBold { text, .. } if text == needle
            )
        })
    }
    /// Announced disabled: named, unreachable and out of the focus order.
    fn greyed(p: &Painter, label: &str) -> bool {
        p.scene.nodes.iter().any(|n| {
            n.interaction.is_none()
                && n.semantic
                    .as_ref()
                    .is_some_and(|s| s.label == label && s.disabled && !s.focusable)
        })
    }
    fn phone_window(kind: &str) -> WindowView {
        WindowView {
            id: 4,
            title: kind.into(),
            kind: kind.into(),
            rect: Rect::new(0, 0, 412, 892),
            focused: true,
            ..WindowView::default()
        }
    }
    fn browser_window() -> WindowView {
        WindowView {
            id: 7,
            title: "Chrome — https://example.com".into(),
            kind: "browser".into(),
            rect: Rect::new(0, 0, 412, 892),
            focused: true,
            tabs: vec!["Example".into(), "Docs".into(), "Mail".into()],
            active_tab: 1,
            ..WindowView::default()
        }
    }
    #[test]
    fn android_date_tracks_world_time_across_months_and_leap_years() {
        let mut ctx = context(None);
        assert_eq!(short_date(&ctx), "Thu, Sep 17");
        for (days, expected) in [
            (14, "Thu, Oct 1"),
            (105, "Thu, Dec 31"),
            (106, "Fri, Jan 1"),
        ] {
            ctx.clock_us = days * 86_400_000_000;
            assert_eq!(short_date(&ctx), expected);
        }
    }
    #[test]
    fn android_home_has_native_asset_and_action_regions() {
        let ctx = context(None);
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        background(&mut p, &ctx);
        chrome(&mut p, &ctx);
        let json = serde_json::to_string(&p.scene).unwrap();
        assert!(json.contains("wallpaper/android"));
        assert!(json.contains("shell:home"));
        assert!(json.contains("shell:launch:browser"));
        // The app drawer is a swipe up the home screen, not an invisible tap strip.
        assert!(!json.contains("shell:launcher"));
    }

    #[test]
    fn three_button_navigation_is_back_home_and_recents_everywhere() {
        let windows = [phone_window("files")];
        let notices = [notice("chat", "Ready", false)];
        let mut contexts = vec![context(None)];
        for panel in [None, Some("quick"), Some("notifications"), Some("overview")] {
            let mut ctx = context(panel);
            ctx.windows = &windows;
            ctx.active = true;
            ctx.notifications = &notices;
            contexts.push(ctx);
        }
        let mut typing = context(None);
        typing.windows = &windows;
        typing.active = true;
        typing.text_entry = true;
        contexts.push(typing);
        for ctx in &contexts {
            let p = shell(ctx);
            let bar = 892 - NAV_BAR / 2;
            for (x, expected) in [
                (103, "shell:mobile-back"),
                (206, "shell:home"),
                (309, "shell:overview"),
            ] {
                let hit = p.scene.hit_test(x, bar).unwrap();
                assert_eq!(
                    hit.interaction.as_deref(),
                    Some(expected),
                    "panel {:?}, keyboard {}",
                    ctx.panel,
                    ctx.text_entry
                );
            }
            // No gesture handle and no tap-to-open strips pretending to be gestures.
            if ctx.panel.is_none() {
                let ids = actions(&p);
                assert!(!ids.iter().any(|a| a == "shell:quick-settings"));
                assert!(!ids.iter().any(|a| a == "shell:notifications"));
                assert_eq!(
                    p.scene.hit_test(200, 12).unwrap().interaction.as_deref(),
                    Some("shell:gesture:notifications")
                );
            }
        }
        // The status bar is a gesture affordance, announced as one.
        let p = shell(&context(None));
        let status = target(&p, "shell:gesture:notifications");
        assert_eq!(status.semantic.as_ref().unwrap().role, "gesture");
        // Applications end above the bar rather than running beneath it.
        let frame = Rect::new(0, 0, 412, 892);
        let content = super::super::window_content_rect(DesktopTheme::Android, frame);
        assert_eq!(content.y + content.height as i32, 892 - NAV_BAR);
    }
    #[test]
    fn notification_shade_is_drawn_and_dismissible() {
        for panel in ["quick", "notifications"] {
            let ctx = context(Some(panel));
            let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
            background(&mut p, &ctx);
            let home = p.scene.nodes.len();
            chrome(&mut p, &ctx);
            assert!(p.scene.nodes[home..]
                .iter()
                .any(|n| matches!(n.primitive, cw_scene::Primitive::Backdrop { .. })));
            assert_eq!(
                p.scene.hit_test(206, 700).unwrap().interaction.as_deref(),
                Some("shell:dismiss")
            );
            assert_eq!(
                p.scene.hit_test(206, 880).unwrap().interaction.as_deref(),
                Some("shell:home")
            );
        }
    }
    #[test]
    fn quick_settings_tiles_flip_real_switches_and_report_their_position() {
        let ctx = context(Some("quick"));
        let p = shell(&ctx);
        let ids = actions(&p);
        for name in [
            "wifi",
            "bluetooth",
            "do_not_disturb",
            "flashlight",
            "rotation_lock",
            "battery_saver",
            "airplane_mode",
            "hotspot",
        ] {
            assert!(
                ids.contains(&format!("shell:toggle:{name}")),
                "no tile toggles {name}"
            );
        }
        assert!(!ids.iter().any(|a| a == "shell:noop"));
        let on = target(&p, "shell:toggle:wifi");
        assert_eq!(on.state.as_ref().and_then(|s| s.checked), Some(true));
        assert!(on.semantic.as_ref().unwrap().label.contains("Connected"));
        // The on tile's icon sits in a container of the primary colour.
        assert!(p.scene.nodes.iter().any(|n| n.bounds.x == on.bounds.x + 8
            && n.bounds.width == 56
            && matches!(n.primitive, cw_scene::Primitive::RoundedBox { fill, .. } if fill == PRIMARY)));
        // The same tile has to go neutral and honest once the switch is off.
        let off = crate::SystemSettings {
            wifi: false,
            ..crate::SystemSettings::DEFAULT
        };
        let mut ctx = context(Some("quick"));
        ctx.settings = &off;
        let p = shell(&ctx);
        let tile = target(&p, "shell:toggle:wifi");
        assert_eq!(tile.state.as_ref().and_then(|s| s.checked), Some(false));
        assert!(tile.semantic.as_ref().unwrap().label.ends_with("Off"));
        // The shade follows the system theme: light by day, dark in the dark theme.
        let lit = target(&p, "shell:toggle:bluetooth");
        assert!(
            matches!(lit.primitive, cw_scene::Primitive::RoundedBox { fill, .. } if fill.0 > 200)
        );
        let dark = crate::SystemSettings {
            dark_mode: true,
            ..crate::SystemSettings::DEFAULT
        };
        let mut ctx = context(Some("quick"));
        ctx.settings = &dark;
        let p = shell(&ctx);
        let tile = target(&p, "shell:toggle:bluetooth");
        assert!(
            matches!(tile.primitive, cw_scene::Primitive::RoundedBox { fill, .. } if fill == SHADE_TILE)
        );
    }
    #[test]
    fn brightness_steps_set_exact_levels_and_the_fill_follows_the_device() {
        let dim = crate::SystemSettings {
            brightness: 25,
            ..crate::SystemSettings::DEFAULT
        };
        let mut ctx = context(Some("quick"));
        ctx.settings = &dim;
        let p = shell(&ctx);
        let ids = actions(&p);
        for percent in [5, 50, 100] {
            assert!(ids.contains(&format!("shell:set:brightness:{percent}")));
        }
        // A quarter of the track is filled because the device is at a quarter.
        assert!(p.scene.nodes.iter().any(|n| n.bounds.width == 380 * 25 / 100
            && matches!(n.primitive, cw_scene::Primitive::RoundedBox { fill, .. } if fill == PRIMARY)));
    }
    #[test]
    fn the_search_bar_searches_and_the_drawer_closes_on_an_outside_tap() {
        let p = shell(&context(None));
        assert_eq!(
            p.scene.hit_test(120, 830).unwrap().interaction.as_deref(),
            Some("shell:search")
        );
        let mut ctx = context(None);
        ctx.launcher_open = true;
        ctx.text_entry = true;
        // The drawer shows no keyboard until its field is tapped.
        assert!(!actions(&shell(&ctx))
            .iter()
            .any(|a| a.starts_with("shell:type:")));
        ctx.panel = Some("search");
        ctx.search = "zzz";
        let p = shell(&ctx);
        assert!(actions(&p).iter().any(|a| a.starts_with("shell:type:")));
        // Below the icon grid and above the keyboard the query field raises.
        assert_eq!(
            p.scene.hit_test(206, 500).unwrap().interaction.as_deref(),
            Some("shell:dismiss")
        );
    }
    #[test]
    fn every_panel_state_is_drawn_and_escapable() {
        let ctx = context(Some("context"));
        let p = shell(&ctx);
        let ids = actions(&p);
        for id in [
            "shell:dismiss",
            "shell:launcher",
            "shell:overview",
            "shell:settings",
            "shell:power:lock",
        ] {
            assert!(ids.contains(&id.to_owned()), "context sheet lacks {id}");
        }
        // Panels this shell does not render still leave a way out.
        for panel in ["file", "edit", "view", "help", "apple", "window"] {
            let p = shell(&context(Some(panel)));
            assert_eq!(
                p.scene.hit_test(206, 300).unwrap().interaction.as_deref(),
                Some("shell:dismiss"),
                "panel {panel} traps the user"
            );
        }
        // And every panel this shell does render dispatches a real dismissal, whatever
        // it has grown to hold — notices, a month grid or a list of device facts.
        let notices = [notice("chat", "Ready to share", false)];
        for panel in [
            "quick",
            "notifications",
            "calendar",
            "settings",
            "power",
            "context",
        ] {
            let mut ctx = context(Some(panel));
            ctx.notifications = &notices;
            assert!(
                actions(&shell(&ctx)).contains(&"shell:dismiss".to_owned()),
                "panel {panel} has no way out"
            );
        }
        // Tapping around the overview's cards goes home, as it does on a Pixel.
        let p = shell(&context(Some("overview")));
        assert_eq!(
            p.scene.hit_test(206, 100).unwrap().interaction.as_deref(),
            Some("shell:home")
        );
    }
    #[test]
    fn the_power_menu_and_the_sleeping_display_are_real() {
        let ids = actions(&shell(&context(Some("power"))));
        for id in ["shell:power:lock", "shell:power:restart", "shell:power:off"] {
            assert!(ids.contains(&id.to_owned()), "power menu lacks {id}");
        }
        for screen in [crate::ScreenState::Locked, crate::ScreenState::Off] {
            let mut ctx = context(None);
            ctx.screen = screen;
            let p = shell(&ctx);
            // The dark display covers the home screen it is drawn over.
            for point in [(206, 446), (120, 830)] {
                assert_eq!(
                    p.scene
                        .hit_test(point.0, point.1)
                        .unwrap()
                        .interaction
                        .as_deref(),
                    Some("shell:power:wake")
                );
            }
            assert!(!actions(&p).iter().any(|a| a == "shell:home"));
        }
    }
    #[test]
    fn chrome_counts_its_real_tabs_and_the_switcher_reaches_them() {
        let windows = [browser_window()];
        let mut ctx = context(None);
        ctx.windows = &windows;
        ctx.active = true;
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        browser_chrome(&mut p, &ctx, &windows[0]);
        assert!(p.scene.nodes.iter().any(
            |n| matches!(&n.primitive, cw_scene::Primitive::UiTextBold { text, .. } if text == "3")
        ));
        assert!(actions(&p).contains(&"shell:panel:window".to_owned()));
        let mut ctx = context(Some("window"));
        ctx.windows = &windows;
        ctx.active = true;
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        browser_chrome(&mut p, &ctx, &windows[0]);
        let ids = actions(&p);
        for id in [
            "shell:tab:select:0",
            "shell:tab:select:1",
            "shell:tab:select:2",
            "shell:tab:close:2",
            "shell:tab:new",
            "shell:dismiss",
        ] {
            assert!(ids.contains(&id.to_owned()), "tab switcher lacks {id}");
        }
        // The shell must not scrim over a switcher the browser draws itself.
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        chrome(&mut p, &ctx);
        assert!(!p
            .scene
            .nodes
            .iter()
            .any(|n| matches!(n.primitive, cw_scene::Primitive::Backdrop { .. })));
    }
    #[test]
    fn gboard_follows_the_routers_own_answer_about_text_entry() {
        // `ctx.text_entry` is the keystroke router's decision, so the shell asks it
        // rather than guessing from the window in front.
        for (text_entry, screen, up) in [
            (false, crate::ScreenState::Active, false),
            (true, crate::ScreenState::Active, true),
            // A dark or locked display is the one case the router does not cover.
            (true, crate::ScreenState::Locked, false),
            (true, crate::ScreenState::Off, false),
        ] {
            let mut ctx = context(None);
            ctx.text_entry = text_entry;
            ctx.screen = screen;
            assert_eq!(
                actions(&shell(&ctx))
                    .iter()
                    .any(|a| a.starts_with("shell:type:")),
                up,
                "text_entry={text_entry} screen={screen:?}"
            );
        }
        let mut ctx = context(Some("search"));
        ctx.launcher_open = true;
        ctx.text_entry = true;
        let p = shell(&ctx);
        let ids = actions(&p);
        for ch in "abcdefghijklmnopqrstuvwxyz,. ".chars() {
            assert!(
                ids.contains(&format!("shell:type:{ch}")),
                "no key types {ch:?}"
            );
        }
        for id in [
            "shell:key:Backspace",
            "shell:key:Enter",
            "shell:key:Shift",
            "shell:plane:numbers",
            "shell:settings",
        ] {
            assert!(ids.contains(&id.to_owned()), "the keyboard lacks {id}");
        }
        // With nothing typed there is no word to complete, so the strip offers no chips;
        // the mic needs audio the simulation does not have.
        assert!(!ids.iter().any(|id| id.starts_with("shell:insert:")));
        assert!(greyed(&p, "Voice typing"));
        // Every key lands on screen, clear of the status bar and the gesture handle.
        for n in p.scene.nodes.iter().filter(|n| {
            n.interaction.as_deref().is_some_and(|a| {
                a.starts_with("shell:type:")
                    || a.starts_with("shell:key:")
                    || a.starts_with("shell:plane:")
            })
        }) {
            let b = n.bounds;
            assert!(
                b.x >= 0 && b.x + b.width as i32 <= 412,
                "{:?} runs off the edge",
                n.interaction
            );
            assert!(
                b.y > 44 && b.y + b.height as i32 <= 862,
                "{:?} escapes the keyboard",
                n.interaction
            );
        }
    }

    #[test]
    fn the_three_planes_between_them_type_every_printable_character() {
        let mut reachable = std::collections::BTreeSet::new();
        for (plane, switches) in [
            (
                crate::Plane::Letters,
                ["shell:plane:numbers", "shell:key:Shift"],
            ),
            (
                crate::Plane::Numbers,
                ["shell:plane:letters", "shell:plane:symbols"],
            ),
            (
                crate::Plane::Symbols,
                ["shell:plane:letters", "shell:plane:numbers"],
            ),
        ] {
            let mut ctx = context(None);
            ctx.text_entry = true;
            ctx.keyboard = crate::KeyboardState {
                plane,
                ..Default::default()
            };
            let p = shell(&ctx);
            let ids = actions(&p);
            // Every cap is a glyph the bundled font really has: a missing one would
            // paint a key that looks blank and says nothing about what it types.
            for row in plane_rows(plane) {
                for ch in row.chars() {
                    assert!(
                        p.measure(&ch.to_string(), 20, false) > 0,
                        "{ch:?} has no glyph"
                    );
                }
            }
            // Every plane can be left again, and keeps delete and return.
            for id in switches
                .iter()
                .chain(["shell:key:Backspace", "shell:key:Enter"].iter())
            {
                assert!(ids.contains(&(*id).to_owned()), "{plane:?} lacks {id}");
            }
            for typed in ids.iter().filter_map(|a| a.strip_prefix("shell:type:")) {
                assert_eq!(typed.chars().count(), 1, "{typed:?} is not one character");
                assert!(!typed.chars().any(char::is_control));
                reachable.insert(typed.chars().next().unwrap());
            }
        }
        // Shift supplies the capitals, so lower case is all the keys have to carry.
        for ch in (0x20u8..0x7f)
            .map(char::from)
            .filter(|c| !c.is_ascii_uppercase())
        {
            assert!(reachable.contains(&ch), "no plane types {ch:?}");
        }
    }

    #[test]
    fn the_shift_key_changes_the_cap_but_never_the_character_it_sends() {
        for (shift, cap, state) in [
            (crate::Shift::Off, "q", "Off"),
            (crate::Shift::Once, "Q", "Next character"),
            (crate::Shift::Lock, "Q", "Locked"),
        ] {
            let mut ctx = context(None);
            ctx.text_entry = true;
            ctx.keyboard = crate::KeyboardState {
                shift,
                ..Default::default()
            };
            let p = shell(&ctx);
            // The id stays lower case whatever the cap reads: the handler applies the
            // modifier, which is what spends a one-shot shift exactly once.
            let ids = actions(&p);
            assert!(ids.contains(&"shell:type:q".to_owned()), "{shift:?}");
            assert!(!ids.contains(&"shell:type:Q".to_owned()), "{shift:?}");
            assert!(shows(&p, cap), "the q key should read {cap}");
            assert_eq!(
                target(&p, "shell:key:Shift")
                    .semantic
                    .as_ref()
                    .and_then(|s| s.value.as_deref()),
                Some(state)
            );
        }
    }

    #[test]
    fn a_tapped_key_types_the_character_it_shows_into_the_focused_window() {
        let windows = [phone_window("terminal")];
        // The same desktop the shell action reaches, driven only by what the keys say.
        let mut desktop = crate::DesktopState::default();
        let (id, _) = desktop.launch("terminal", "").unwrap();
        desktop.focused = Some(id);
        for (plane, keys) in [
            (
                crate::Plane::Letters,
                ["shell:type:l", "shell:type:s", "shell:type: "].as_slice(),
            ),
            (
                crate::Plane::Numbers,
                ["shell:type:.", "shell:key:Backspace"].as_slice(),
            ),
        ] {
            let mut ctx = context(None);
            ctx.windows = &windows;
            ctx.active = true;
            ctx.text_entry = true;
            ctx.keyboard = crate::KeyboardState {
                plane,
                ..Default::default()
            };
            let p = shell(&ctx);
            for want in keys {
                let node = target(&p, want);
                let (cx, cy) = (
                    node.bounds.x + node.bounds.width as i32 / 2,
                    node.bounds.y + node.bounds.height as i32 / 2,
                );
                assert_eq!(
                    p.scene.hit_test(cx, cy).unwrap().interaction.as_deref(),
                    Some(*want),
                    "{want} is not the topmost control at its own centre"
                );
                match want.strip_prefix("shell:type:") {
                    Some(text) => desktop.text(text).unwrap(),
                    None => {
                        desktop.key(want.trim_start_matches("shell:key:")).unwrap();
                    }
                }
            }
        }
        let effects = desktop.key("Enter").unwrap();
        assert!(
            matches!(effects.as_slice(), [crate::AppEffect::Execute { command, .. }] if command == "ls "),
            "{effects:?}"
        );
    }

    #[test]
    fn chrome_greys_back_and_forward_until_the_tab_has_history() {
        let fresh = browser_window();
        let windows = [fresh.clone()];
        let mut ctx = context(None);
        ctx.windows = &windows;
        ctx.active = true;
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        browser_chrome(&mut p, &ctx, &fresh);
        for label in ["Back", "Forward"] {
            assert!(greyed(&p, label), "{label} must be greyed, not refused");
        }
        assert!(!actions(&p)
            .iter()
            .any(|a| a.ends_with("shell:back") || a.ends_with("shell:forward")));
        let visited = WindowView {
            can_go_back: true,
            can_go_forward: true,
            ..browser_window()
        };
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        browser_chrome(&mut p, &ctx, &visited);
        let ids = actions(&p);
        for id in [
            "window:7:content:shell:back",
            "window:7:content:shell:forward",
        ] {
            assert!(ids.contains(&id.to_owned()), "missing {id}");
        }
    }

    #[test]
    fn the_calendar_screen_pages_its_month_and_draws_the_one_it_is_showing() {
        let p = shell(&context(Some("calendar")));
        let ids = actions(&p);
        for id in ["shell:month:prev", "shell:month:next", "shell:dismiss"] {
            assert!(ids.contains(&id.to_owned()), "the calendar lacks {id}");
        }
        // The world's own month is already today, so there is nowhere to return to.
        assert!(!ids.contains(&"shell:month:today".to_owned()));
        assert!(shows(&p, "September 2026"));
        assert!(shows(&p, "30") && !shows(&p, "31"));
        // A day opens Calendar on it; a day before the world began offers nothing.
        assert!(ids.contains(&"shell:launch:calendar/2026-09-30".to_owned()));
        assert!(ids.contains(&"shell:launch:calendar/2026-09-17".to_owned()));
        assert!(!ids.contains(&"shell:launch:calendar/2026-09-16".to_owned()));
        assert!(!greyed(&p, "Calendar days"));
        let mut ctx = context(Some("calendar"));
        ctx.panel_month = -1;
        let p = shell(&ctx);
        assert!(shows(&p, "August 2026") && shows(&p, "31"));
        assert!(!actions(&p)
            .iter()
            .any(|id| id.starts_with("shell:launch:calendar/")));
        assert!(actions(&p).contains(&"shell:month:today".to_owned()));
        // Quick Settings' date reaches it, and so does the home screen when there is no
        // Calendar application to launch instead.
        assert!(
            actions(&shell(&context(Some("quick")))).contains(&"shell:panel:calendar".to_owned())
        );
        let installed = ["files".to_owned()];
        let mut ctx = context(None);
        ctx.installed_apps = &installed;
        assert!(actions(&shell(&ctx)).contains(&"shell:panel:calendar".to_owned()));
    }

    #[test]
    fn the_files_app_bar_reaches_the_folder_search_and_the_folder_above() {
        let ctx = context(None);
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        let window = WindowView {
            document: "/home/alice/Documents".into(),
            ..phone_window("files")
        };
        window_frame(&mut p, &ctx, &window);
        let ids = actions(&p);
        for id in [
            "window:4:content:files-search",
            "window:4:content:files-up",
            "shell:panel:context",
        ] {
            assert!(ids.contains(&id.to_owned()), "the app bar lacks {id}");
        }
        // At the root there is no folder above, so the app bar has no up arrow: leaving
        // the application is the navigation bar's Back.
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        window_frame(&mut p, &ctx, &phone_window("files"));
        assert!(!actions(&p)
            .iter()
            .any(|a| a == "shell:mobile-back" || a.ends_with("files-up")));
        // Nor has any other application's root screen.
        let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
        window_frame(&mut p, &ctx, &phone_window("calendar"));
        assert!(!actions(&p).iter().any(|a| a == "shell:mobile-back"));
    }

    #[test]
    fn overview_and_settings_only_paint_controls_that_reach_something() {
        let windows = [browser_window()];
        let mut ctx = context(Some("overview"));
        ctx.windows = &windows;
        let p = shell(&ctx);
        let ids = actions(&p);
        // A card is dismissed by swiping it up; there is no Close chip on a Pixel.
        assert!(!ids.contains(&"window:7:close".to_owned()));
        assert!(ids.contains(&"window:7:focus".to_owned()));
        // The Screenshot chip really captures the centred application (the grant
        // decides, not the shell), Select picks up its text, and past the oldest card
        // is Clear all.
        for chip in ["shell:recents:screenshot", "shell:recents:select"] {
            assert!(ids.contains(&chip.to_owned()), "{chip}");
        }
        assert!(!greyed(&p, "Screenshot"));
        ctx.overview.slot = Some(-1);
        let ids = actions(&shell(&ctx));
        assert!(ids.contains(&"shell:recents:clear".to_owned()));
        assert!(!ids.contains(&"shell:recents:select".to_owned()));
        assert!(!ids.iter().any(|a| a == "shell:noop"));
        let mut ctx = context(Some("settings"));
        ctx.windows = &windows;
        let p = shell(&ctx);
        let ids = actions(&p);
        for expected in [
            "shell:overview",
            "shell:quick-settings",
            "shell:notifications",
        ] {
            assert!(
                ids.contains(&expected.to_owned()),
                "settings lacks {expected}"
            );
        }
        // Settings has no shortcut to the app drawer; a Pixel's does not either.
        assert!(!ids.contains(&"shell:launcher".to_owned()));
        assert!(shows(&p, "15"));
        // Device facts are text. They are neither tappable nor announced as a disabled
        // control, because a reading is not a control that happens to be off.
        for reading in ["Device name", "Android version"] {
            assert!(shows(&p, reading));
            assert!(
                !p.scene.nodes.iter().any(|n| n
                    .semantic
                    .as_ref()
                    .is_some_and(|s| s.label.contains(reading))),
                "{reading} is announced as a control"
            );
        }
    }

    fn notice(app: &str, title: &str, seen: bool) -> crate::Notice {
        crate::Notice {
            app: app.into(),
            title: title.into(),
            body: "body".into(),
            time_us: 0,
            action: None,
            seen,
        }
    }
    fn wears(p: &Painter, symbol: &str) -> bool {
        let asset = format!("symbol/{symbol}");
        p.scene.nodes.iter().any(
            |n| matches!(&n.primitive, cw_scene::Primitive::Symbol { asset: a, .. } if *a == asset),
        )
    }

    #[test]
    fn the_volume_panel_is_the_media_volume_in_fifteen_steps() {
        let ctx = context(Some("volume"));
        let p = shell(&ctx);
        let ids = actions(&p);
        for percent in [6, 53, 100] {
            assert!(
                ids.contains(&format!("shell:set:volume:{percent}")),
                "{percent}"
            );
        }
        assert!(ids.contains(&"shell:dismiss".to_owned()));
        assert!(greyed(&p, "Ring") && greyed(&p, "Sound settings"));
        let slider = p
            .scene
            .nodes
            .iter()
            .find(|n| n.semantic.as_ref().is_some_and(|s| s.role == "slider"))
            .unwrap();
        assert_eq!(
            slider.semantic.as_ref().unwrap().value.as_deref(),
            Some("60%")
        );
        // At the right edge of the screen, over the home screen.
        assert!(slider.bounds.x > 300);
    }
    #[test]
    fn the_lock_screen_clock_is_large_without_notifications_and_small_with_them() {
        let lock = |notices: &[crate::Notice]| {
            let mut ctx = context(None);
            ctx.screen = crate::ScreenState::Locked;
            ctx.notifications = notices;
            let mut p = Painter::themed(DesktopTheme::Android, 412, 892, 1);
            chrome(&mut p, &ctx);
            p
        };
        let p = lock(&[]);
        assert!(shows(&p, "09") && shows(&p, "00"));
        let p = lock(&[notice("chat", "Ready", false)]);
        assert!(shows(&p, "09:00"));
        assert_eq!(
            p.scene.hit_test(206, 400).unwrap().interaction.as_deref(),
            Some("shell:power:wake")
        );
    }

    #[test]
    fn the_shade_shows_the_notices_the_machine_really_posted() {
        let notices = [
            notice("chat", "Ready to share", false),
            notice("screenshot", "Screenshot saved", true),
        ];
        let mut ctx = context(Some("notifications"));
        ctx.notifications = &notices;
        let p = shell(&ctx);
        let ids = actions(&p);
        for expected in [
            "shell:notice:0",
            "shell:notice:1",
            "shell:notifications:clear",
        ] {
            assert!(ids.contains(&expected.to_owned()), "shade lacks {expected}");
        }
        assert!(greyed(&p, "Notification history"));
        assert!(shows(&p, "Ready to share") && shows(&p, "Screenshot saved"));
        assert!(!shows(&p, "No notifications"));
        // The status badge tracks unseen notices rather than the feed being non-empty.
        assert!(wears(&p, "bell"));
        let seen = [notice("mail", "Ready to share", true)];
        let mut ctx = context(Some("notifications"));
        ctx.notifications = &seen;
        let p = shell(&ctx);
        assert!(!actions(&p).iter().any(|a| a == "shell:notifications:seen"));
        // An empty feed says so and paints no rows at all.
        let p = shell(&context(Some("notifications")));
        assert!(shows(&p, "No notifications"));
        assert!(!actions(&p).iter().any(|a| a.starts_with("shell:notice")));
    }

    #[test]
    fn everything_without_a_capture_or_an_order_behind_it_is_announced_off() {
        // Home: the two glyphs in the search pill need audio and a camera.
        let p = shell(&context(None));
        for label in ["Voice search", "Google Lens"] {
            assert!(greyed(&p, label), "{label} is not announced disabled");
        }
        // The keyboard: the mic needs audio.
        let mut ctx = context(None);
        ctx.text_entry = true;
        let p = shell(&ctx);
        assert!(
            greyed(&p, "Voice typing"),
            "Voice typing is not announced disabled"
        );
        // Quick settings: rearranging tiles needs a tile order that does not exist.
        let p = shell(&context(Some("quick")));
        assert!(greyed(&p, "Edit tiles"));
        // None of them leaks an id anywhere in the scene.
        let mut ctx = context(Some("quick"));
        ctx.text_entry = true;
        let ids = actions(&shell(&ctx));
        for fragment in ["voice", "lens", "dictat", "suggest", "tile-order"] {
            assert!(
                !ids.iter().any(|a| a.contains(fragment)),
                "something dispatches {fragment}"
            );
        }
    }

    #[test]
    fn the_suggestion_strip_completes_the_word_being_typed() {
        let mut ctx = context(None);
        ctx.text_entry = true;
        ctx.typed = "See you at the Mee";
        let p = shell(&ctx);
        let ids = actions(&p);
        // The chip types the rest of the word, keeping the capital that was typed.
        assert!(ids.contains(&"shell:insert:ting ".to_owned()), "{ids:?}");
        assert!(shows(&p, "Meeting"));
        assert!(!greyed(&p, "Suggestions"));
        // Nothing to complete: a space, or no field at all, offers no chips.
        ctx.typed = "See you ";
        assert!(!actions(&shell(&ctx))
            .iter()
            .any(|i| i.starts_with("shell:insert:")));
        ctx.typed = "Mee";
        ctx.text_entry = false;
        assert!(!actions(&shell(&ctx))
            .iter()
            .any(|i| i.starts_with("shell:insert:")));
    }
}
