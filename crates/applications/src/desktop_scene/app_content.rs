//! Native client-area projection. No host data and no invented file metadata: every
//! row, path and counter comes from application state, and only real actions are
//! interactive. Window frames own titles and tabs; these scenes own the content.
use super::{
    shared::{Align, CalendarDate, Painter},
    DesktopTheme,
};
use crate::entry_name;
use cw_scene::{Color, Primitive, Rect, Scene};

const INK: Color = Color::rgb(29, 29, 31);
const MUTED: Color = Color::rgb(112, 114, 120);
const FAINT: Color = Color::rgb(160, 162, 168);
const LINE: Color = Color(0, 0, 0, 26);
/// The pane a plain-text editor's rows scroll in.
pub const EDITOR_PANE: &str = "text";

struct Look {
    accent: Color,
    folder: Color,
    sidebar: Color,
    sidebar_width: u32,
    selection: Color,
    row: u32,
    root: &'static str,
}
/// The file managers' sidebar widths, shared by the shells that draw them.
pub const FINDER_SIDEBAR: u32 = 172;
pub const FILES_SIDEBAR: u32 = 185;
fn look(t: DesktopTheme) -> Look {
    match t {
        DesktopTheme::Macos => Look {
            accent: Color::rgb(0, 122, 255),
            folder: Color::rgb(58, 160, 240),
            sidebar: Color::rgb(232, 231, 234),
            sidebar_width: FINDER_SIDEBAR,
            selection: Color(0, 0, 0, 24),
            row: 24,
            root: "Macintosh HD",
        },
        DesktopTheme::Windows => Look {
            accent: Color::rgb(0, 95, 184),
            folder: Color::rgb(245, 187, 64),
            sidebar: Color::rgb(249, 249, 249),
            sidebar_width: 184,
            selection: Color::rgb(229, 241, 251),
            row: 32,
            root: "This PC",
        },
        DesktopTheme::Ubuntu => Look {
            accent: Color::rgb(233, 84, 32),
            folder: Color::rgb(233, 84, 32),
            sidebar: Color::rgb(235, 235, 235),
            sidebar_width: FILES_SIDEBAR,
            selection: Color(0, 0, 0, 22),
            row: 40,
            root: "Computer",
        },
        DesktopTheme::Ios => Look {
            accent: Color::rgb(0, 122, 255),
            folder: Color::rgb(64, 168, 250),
            sidebar: Color::WHITE,
            sidebar_width: 0,
            selection: Color(0, 0, 0, 16),
            row: 60,
            root: "On My iPhone",
        },
        DesktopTheme::Android => Look {
            accent: Color::rgb(76, 102, 43),
            folder: Color::rgb(95, 99, 104),
            sidebar: Color::WHITE,
            sidebar_width: 0,
            selection: Color(0, 0, 0, 16),
            row: 64,
            root: "Internal storage",
        },
    }
}
fn mono(p: &mut Painter, r: Rect, s: &str, size: u16, color: Color) {
    p.node(
        r,
        Primitive::Text {
            text: s.into(),
            size,
            color,
        },
        None,
    );
}
/// Widths of the metadata columns a list view draws to the right of the name. Each one
/// is dropped, narrowest window first, rather than squeezed into nothing.
const KIND_COLUMN: i32 = 130;
const SIZE_COLUMN: i32 = 80;
const DATE_COLUMN: i32 = 124;
/// A byte count as a file manager writes it. Integer arithmetic only: the same listing
/// prints the same string on every target.
fn size_label(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} {}", if bytes == 1 { "byte" } else { UNITS[0] });
    }
    let (mut value, mut tenths, mut unit) = (bytes, 0, 0);
    while value >= 1024 && unit + 1 < UNITS.len() {
        tenths = (value % 1024) * 10 / 1024;
        value /= 1024;
        unit += 1;
    }
    if value >= 100 || tenths == 0 {
        format!("{value} {}", UNITS[unit])
    } else {
        format!("{value}.{tenths} {}", UNITS[unit])
    }
}
/// A modification time as the world clock has it, `2026-09-17 09:00`. Derived from the
/// simulated microsecond tick alone; no host clock is consulted anywhere on this path.
fn date_label(clock_us: u64) -> String {
    let date = CalendarDate::from_clock(clock_us);
    let minutes = (clock_us / 60_000_000 + 9 * 60) % (24 * 60);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        date.year,
        date.month,
        date.day,
        minutes / 60,
        minutes % 60
    )
}
/// A control that is real only when `enabled`; otherwise it is drawn greyed and
/// announced as disabled, never painted as an affordance that would be refused.
fn control(p: &mut Painter, r: Rect, action: &str, label: &str, enabled: bool) {
    if enabled {
        p.button(r, Color::TRANSPARENT, 4, action, label);
    } else {
        p.box_(r, Color::TRANSPARENT, 4);
        p.disabled(label);
    }
}
fn components(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).filter(|s| !s.is_empty()).collect()
}
fn current<'a>(path: &'a str, root: &'a str) -> &'a str {
    components(path).last().copied().unwrap_or(root)
}
fn parent<'a>(path: &'a str, root: &'a str) -> &'a str {
    let parts = components(path);
    if parts.len() >= 2 {
        parts[parts.len() - 2]
    } else {
        root
    }
}
fn has_parent(path: &str) -> bool {
    !components(path).is_empty()
}
/// Breadcrumb trail as (label, absolute path) pairs, root first.
fn crumbs(path: &str, root: &str) -> Vec<(String, String)> {
    let mut out = vec![(root.to_owned(), "/".to_owned())];
    let mut prefix = String::new();
    for part in components(path) {
        prefix.push('/');
        prefix.push_str(part);
        out.push((part.to_owned(), prefix.clone()));
    }
    out
}
/// Finder's tab bar. Returns the height it consumed.
fn tab_strip(
    p: &mut Painter,
    l: &Look,
    x: i32,
    width: u32,
    tabs: &[crate::FileTab],
    active: usize,
) -> u32 {
    const HEIGHT: u32 = 28;
    p.box_(Rect::new(x, 0, width, HEIGHT), Color::rgb(236, 236, 238), 0);
    p.hline(x, HEIGHT as i32 - 1, width, LINE);
    let plus = 30;
    let each = (width.saturating_sub(plus) / tabs.len().max(1) as u32).clamp(60, 240);
    for (i, tab) in tabs.iter().enumerate() {
        let r = Rect::new(x + i as i32 * each as i32, 0, each, HEIGHT - 1);
        p.button(
            r,
            if i == active {
                Color::WHITE
            } else {
                Color::TRANSPARENT
            },
            0,
            &format!("files-tab:{i}"),
            tab.name(),
        );
        p.vline(r.x + r.width as i32 - 1, 4, HEIGHT - 9, LINE);
        p.label(
            r.x + 10,
            5,
            r.width.saturating_sub(34),
            tab.name(),
            12,
            if i == active { INK } else { MUTED },
            i == active,
            Align::Left,
        );
        let close = Rect::new(r.x + r.width as i32 - 24, 5, 18, 18);
        p.button(
            close,
            Color::TRANSPARENT,
            4,
            &format!("files-closetab:{i}"),
            "Close tab",
        );
        p.symbol("close", close.x + 4, close.y + 4, 10, MUTED);
    }
    let plus_rect = Rect::new(x + width as i32 - plus as i32, 3, 22, 22);
    p.button(plus_rect, Color::TRANSPARENT, 4, "files-newtab", "New tab");
    p.symbol("plus", plus_rect.x + 5, plus_rect.y + 5, 12, l.accent);
    HEIGHT
}
fn entry_icon(p: &mut Painter, l: &Look, x: i32, y: i32, size: u32, directory: bool) {
    if directory {
        p.symbol("folder", x, y, size, l.folder);
    } else {
        p.symbol("document", x, y, size, Color::rgb(126, 132, 142));
    }
}

/// One row of a file manager sidebar: its name, glyph, glyph tint and the command it
/// runs. `current` lights the row for the place the tab is showing.
struct Place {
    label: String,
    symbol: &'static str,
    tint: Option<Color>,
    action: String,
    current: bool,
    pinned: bool,
}
enum Side {
    Heading(&'static str),
    Row(Place),
    Gap,
}
/// Sidebar glyph for a standard home folder.
fn folder_symbol(name: &str) -> &'static str {
    match name {
        "Desktop" => "desktop",
        "Documents" => "document",
        "Downloads" => "download",
        "Music" => "music",
        "Pictures" => "image",
        "Videos" | "Movies" => "film",
        _ => "folder",
    }
}
/// Explorer tints its known folders the way their shell icons are coloured.
fn explorer_tint(name: &str) -> Color {
    match name {
        "Desktop" => Color::rgb(0, 120, 212),
        "Downloads" => Color::rgb(16, 137, 62),
        "Documents" => Color::rgb(96, 110, 128),
        "Pictures" => Color::rgb(0, 153, 188),
        "Music" => Color::rgb(232, 99, 43),
        "Videos" => Color::rgb(135, 100, 184),
        _ => Color::rgb(245, 187, 64),
    }
}
/// Where a row of a platform's standard sidebar leads. Lists the desktop keeps
/// (Recents, Starred) and Explorer's views (Home, Gallery) are not folders; the rest
/// are absolute folders on the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceKind {
    Recents,
    Starred,
    /// Explorer's Home: the pinned Quick access folders (and, in Explorer, the lists).
    QuickAccess,
    /// Explorer's Gallery: the images in Pictures.
    Gallery,
    Home(String),
    Folder(String),
    Trash(String),
    /// The computer: Macintosh HD, This PC, Other Locations.
    Root,
}
/// One row of a standard sidebar, before any application decides what clicking it does.
#[derive(Clone, Debug, PartialEq)]
pub struct StandardPlace {
    pub label: String,
    pub symbol: &'static str,
    pub tint: Option<Color>,
    /// Explorer's pinned Quick access folders carry a pin.
    pub pinned: bool,
    pub kind: PlaceKind,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SideItem {
    Heading(&'static str),
    Place(StandardPlace),
    Gap,
}
/// The platform's standard sidebar, holding only places that can really be opened:
/// the lists the desktop keeps, the home folder, the standard folders that exist in it
/// right now, the Trash and the computer. AirDrop, iCloud, Tags, OneDrive and Network
/// have nothing behind them in the simulator and are left out rather than faked.
/// Files, Finder and Explorer draw it; an application's native file dialog lists the
/// same places, so the two can never disagree about what the machine holds.
pub fn standard_places(t: DesktopTheme, files: &crate::FilesEnv<'_>) -> Vec<SideItem> {
    let row = |label: &str, symbol: &'static str, kind: PlaceKind| {
        SideItem::Place(StandardPlace {
            label: label.to_owned(),
            symbol,
            tint: None,
            pinned: false,
            kind,
        })
    };
    let standard = |names: &[&str], explorer: bool| -> Vec<SideItem> {
        names
            .iter()
            .filter(|name| files.has(name))
            .map(|name| {
                SideItem::Place(StandardPlace {
                    label: (*name).to_owned(),
                    symbol: folder_symbol(name),
                    tint: explorer.then(|| explorer_tint(name)),
                    pinned: explorer,
                    kind: PlaceKind::Folder(files.folder(name)),
                })
            })
            .collect()
    };
    let has_home = !files.home.is_empty() && files.home != "/";
    let mut out = Vec::new();
    match t {
        DesktopTheme::Ubuntu => {
            out.push(row("Recent", "clock", PlaceKind::Recents));
            out.push(row("Starred", "star", PlaceKind::Starred));
            if has_home {
                out.push(row("Home", "home", PlaceKind::Home(files.home.to_owned())));
            }
            // GTK's places sidebar lists Desktop of its own and the other home folders as
            // bookmarks after the Trash, in the order xdg-user-dirs-gtk-update writes them
            // at an Ubuntu desktop's first login.
            out.extend(standard(&["Desktop"], false));
            if !files.trash.is_empty() {
                out.push(row("Trash", "trash", PlaceKind::Trash(files.trash.clone())));
            }
            let bookmarks = standard(
                &["Documents", "Music", "Pictures", "Videos", "Downloads"],
                false,
            );
            if !bookmarks.is_empty() {
                out.push(SideItem::Gap);
                out.extend(bookmarks);
            }
            out.push(SideItem::Gap);
            // Files opens Other Locations, which lists the computer; a file dialog,
            // which has no such page, opens the computer's root directly.
            out.push(row("Other Locations", "plus", PlaceKind::Root));
        }
        DesktopTheme::Macos => {
            out.push(SideItem::Heading("Favorites"));
            out.push(row("Recents", "clock", PlaceKind::Recents));
            out.extend(standard(&["Desktop", "Documents", "Downloads"], false));
            out.push(SideItem::Heading("Locations"));
            out.push(row("Macintosh HD", "drive", PlaceKind::Root));
        }
        DesktopTheme::Windows => {
            if has_home {
                out.push(row("Home", "home", PlaceKind::QuickAccess));
                if files.has("Pictures") {
                    out.push(row("Gallery", "image", PlaceKind::Gallery));
                }
            }
            out.push(SideItem::Gap);
            out.extend(standard(&crate::QUICK_ACCESS, true));
            out.push(SideItem::Gap);
            out.push(row("This PC", "desktop", PlaceKind::Root));
        }
        _ => {}
    }
    out
}
/// The file manager's sidebar: each standard place as the command that opens it in
/// this tab, lit when it is the place the tab is showing.
fn places(t: DesktopTheme, env: &crate::AppEnv<'_>, tab: &crate::FileTab) -> Vec<Side> {
    use crate::FileScope;
    let folder_at = |path: &str| {
        tab.scope == FileScope::Folder
            && !path.is_empty()
            && tab.path.trim_end_matches('/') == path.trim_end_matches('/')
    };
    standard_places(t, &env.files)
        .into_iter()
        .map(|item| match item {
            SideItem::Heading(h) => Side::Heading(h),
            SideItem::Gap => Side::Gap,
            SideItem::Place(place) => {
                let (action, current) = match &place.kind {
                    PlaceKind::Recents => ("files-recents".into(), tab.scope == FileScope::Recents),
                    PlaceKind::Starred => ("files-starred".into(), tab.scope == FileScope::Starred),
                    PlaceKind::QuickAccess => (
                        "files-quick-access".into(),
                        tab.scope == FileScope::QuickAccess,
                    ),
                    PlaceKind::Gallery => ("files-gallery".into(), tab.scope == FileScope::Gallery),
                    PlaceKind::Home(path) => ("files-home".into(), folder_at(path)),
                    PlaceKind::Folder(path) => (format!("files-location:{path}"), folder_at(path)),
                    PlaceKind::Trash(path) => ("files-trash".into(), folder_at(path)),
                    PlaceKind::Root if t == DesktopTheme::Ubuntu => (
                        "files-other-locations".into(),
                        tab.scope == FileScope::OtherLocations,
                    ),
                    PlaceKind::Root => ("files-root".into(), folder_at("/")),
                };
                Side::Row(Place {
                    label: place.label,
                    symbol: place.symbol,
                    tint: place.tint,
                    action,
                    current,
                    pinned: place.pinned,
                })
            }
        })
        .collect()
}
/// Draw the sidebar `places` describes. Every row is a real command; the lit row is
/// the place the tab is showing, so the sidebar and the listing cannot disagree.
fn sidebar(p: &mut Painter, env: &crate::AppEnv<'_>, l: &Look, h: u32, tab: &crate::FileTab) {
    let t = env.theme;
    if t == DesktopTheme::Ubuntu {
        return gnome_sidebar(p, env, l, h, tab);
    }
    let side = l.sidebar_width;
    p.box_(Rect::new(0, 0, side, h), l.sidebar, 0);
    p.vline(side as i32 - 1, 0, h, LINE);
    let (row, text, icon, radius) = match t {
        DesktopTheme::Ubuntu => (36, 13, 16, 6),
        DesktopTheme::Windows => (32, 12, 16, 4),
        _ => (26, 13, 16, 5),
    };
    let mut y = if t == DesktopTheme::Macos { 4 } else { 8 };
    for item in places(t, env, tab) {
        if y + row as i32 > h as i32 {
            break;
        }
        match item {
            Side::Heading(title) => {
                y += 8;
                p.strong(18, y, side - 28, title, 11, FAINT);
                y += 20;
            }
            Side::Gap => {
                y += 6;
                p.hline(12, y, side - 24, LINE);
                y += 7;
            }
            Side::Row(place) => {
                let plate = Rect::new(8, y, side - 16, row - 2);
                p.button(
                    plate,
                    if place.current {
                        l.selection
                    } else {
                        Color::TRANSPARENT
                    },
                    radius,
                    &place.action,
                    &place.label,
                );
                let tint = place.tint.unwrap_or(match t {
                    DesktopTheme::Ubuntu => INK,
                    _ => l.accent,
                });
                let ix = if t == DesktopTheme::Windows { 20 } else { 18 };
                p.symbol(
                    place.symbol,
                    ix,
                    y + (row as i32 - 2 - icon as i32) / 2,
                    icon,
                    tint,
                );
                let tx = ix + icon as i32 + 10;
                p.left(
                    tx,
                    y + (row as i32 - 2 - 18) / 2,
                    (side as i32 - tx - if place.pinned { 30 } else { 12 }).max(0) as u32,
                    &place.label,
                    text,
                    INK,
                );
                if place.pinned {
                    // Explorer marks pinned Quick access folders with a pin.
                    p.symbol(
                        "pin",
                        side as i32 - 32,
                        y + (row as i32 - 14) / 2,
                        12,
                        FAINT,
                    );
                }
                y += row as i32;
            }
        }
    }
}

/// What GTK 4's places sidebar names each row for assistive technology: the row's
/// tooltip, not its label.
fn gnome_place_name(label: &str, action: &str) -> String {
    match action {
        "files-recents" => "Recent Files".into(),
        "files-starred" => "Starred Files".into(),
        "files-home" => "Open Personal Folder".into(),
        "files-trash" => "Open Trash".into(),
        "files-other-locations" => "Show Other Locations".into(),
        _ if label == "Desktop" => "Open the contents of your desktop in a folder".into(),
        // A bookmark is named by its tooltip, the folder's path.
        _ if action.starts_with("files-location:") => action["files-location:".len()..].into(),
        _ => label.into(),
    }
}

/// A node announced as `role` with the bounds its toolkit gives it, apart from what is
/// painted inside: a GTK label's box, an image's.
fn announced(p: &mut Painter, r: Rect, role: &str, name: &str) {
    p.node(r, Primitive::Region, None);
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

/// Nautilus 46's sidebar, as AT-SPI lists it: each place a list item named by its
/// tooltip, holding an image and a label, 36 px tall on a 38 px pitch, with a separator
/// before Other Locations; the row of the place on screen is selected. GTK paints each
/// row 3 px left of the bounds it announces, and the sidebar's edge 6 px inside them.
fn gnome_sidebar(p: &mut Painter, env: &crate::AppEnv<'_>, l: &Look, h: u32, tab: &crate::FileTab) {
    let side = l.sidebar_width;
    p.box_(Rect::new(0, 0, side - 6, h), l.sidebar, 0);
    p.vline(side as i32 - 7, 0, h, Color::rgb(218, 218, 218));
    let mut y = 6;
    for item in places(env.theme, env, tab) {
        if y + 36 > h as i32 {
            break;
        }
        match item {
            Side::Heading(_) => {}
            Side::Gap => {
                p.hline(11, y + 11, side - 22, Color::rgb(206, 206, 206));
                y += 13;
            }
            Side::Row(place) => {
                let plate = Rect::new(5, y, 168, 36);
                p.box_(
                    Rect::new(2, y, 168, 36),
                    if place.current {
                        Color::rgb(217, 217, 217)
                    } else {
                        Color::TRANSPARENT
                    },
                    6,
                );
                p.region(
                    plate,
                    &place.action,
                    &gnome_place_name(&place.label, &place.action),
                );
                p.announce("option", None);
                if place.current {
                    p.select_last(true);
                }
                p.symbol(place.symbol, 20, y + 10, 16, Color::rgb(46, 52, 54));
                announced(p, Rect::new(19, y, 24, 36), "img", "");
                p.left(44, y + 9, 112, &place.label, 14, Color::rgb(46, 52, 54));
                p.presentation();
                announced(p, Rect::new(43, y, 116, 36), "label", &place.label);
                y += 38;
            }
        }
    }
}

fn files(p: &mut Painter, env: &crate::AppEnv<'_>, tabs: &[crate::FileTab], active: usize) {
    let (t, w, h) = (env.theme, env.width, env.height);
    let (clipboard, share_to) = (env.clipboard, env.share_to);
    p.scene.background = Color::WHITE;
    let Some(tab) = tabs.get(active).or_else(|| tabs.first()) else {
        return;
    };
    let (path, entries) = (tab.path.as_str(), tab.entries.as_slice());
    // Screen order, resolved once: every row, every `open:<i>` and the selection
    // highlight all read from this list, so they cannot disagree.
    let rows_shown = tab.display();
    if t.mobile() {
        return files_mobile(p, env, tab, &rows_shown);
    }
    // The Trash is not just another folder: what it offers is Restore and Empty, and
    // each of its rows knows where it came from.
    let in_trash = tab.in_trash(&env.files.trash);
    let has_selection = tab.selection().is_some();
    let folder_scope = tab.scope == crate::FileScope::Folder;
    let l = look(t);
    let side = if w > 470 { l.sidebar_width } else { 0 };
    if side > 0 {
        sidebar(p, env, &l, h, tab);
    }
    // What the platform calls this place, and whether it is one (a list, a view, the
    // Trash) rather than a folder to show a trail of.
    let title = tab.title(t, env.files.home, &env.files.trash);
    let place = tab.is_place(&env.files.trash);
    let absolute = tab.scope.absolute();
    let x = side as i32;
    let content = w.saturating_sub(side);
    let mut top = 0;
    // Finder keeps its tab bar under the toolbar; Explorer and Files put theirs in
    // the title bar, which the window frame draws.
    if t == DesktopTheme::Macos {
        top = tab_strip(p, &l, x, content, tabs, active) as i32;
    }
    if t == DesktopTheme::Windows {
        // Navigation row with breadcrumb address, then the command bar.
        p.box_(Rect::new(x, 0, content, 88), Color::rgb(249, 250, 252), 0);
        for (i, (symbol, action, label, enabled)) in [
            ("arrow-left", "files-back", "Back", tab.can_go_back()),
            (
                "arrow-right",
                "files-forward",
                "Forward",
                tab.can_go_forward(),
            ),
            // Home and Gallery are views, not folders in a tree: Up is greyed there.
            (
                "arrow-up",
                "files-up",
                "Up one level",
                has_parent(path) && tab.scope == crate::FileScope::Folder,
            ),
            ("reload", "files-reload", "Refresh", true),
        ]
        .into_iter()
        .enumerate()
        {
            let hit = Rect::new(x + 8 + i as i32 * 34, 6, 32, 32);
            if enabled {
                p.button(hit, Color::TRANSPARENT, 4, action, label);
            }
            p.symbol(
                symbol,
                hit.x + 8,
                hit.y + 8,
                16,
                if enabled { INK } else { FAINT },
            );
            if !enabled {
                p.disabled(label);
            }
        }
        let address = Rect::new(x + 150, 7, content.saturating_sub(150 + 190), 30);
        p.border(address, Color::WHITE, 4, LINE);
        p.symbol(
            if tab.scope == crate::FileScope::QuickAccess {
                "home"
            } else if tab.scope == crate::FileScope::Gallery {
                "image"
            } else {
                "desktop"
            },
            address.x + 10,
            address.y + 8,
            14,
            MUTED,
        );
        let mut cx = address.x + 32;
        let trail = if place {
            // A view is its own root: Explorer's address reads "> Home", not a path.
            vec![(title.clone(), String::new())]
        } else {
            crumbs(path, l.root)
        };
        for (part, target) in trail {
            p.symbol("chevron-right", cx, address.y + 10, 10, MUTED);
            cx += 16;
            let width = (address.x + address.width as i32 - cx - 8).max(0) as u32;
            let text = p.left(cx, address.y + 7, width, &part, 12, INK);
            if !target.is_empty() {
                p.region(
                    Rect::new(cx - 3, address.y + 4, text + 6, 24),
                    &format!("files-location:{target}"),
                    &part,
                );
            }
            cx += text as i32 + 8;
        }
        // Real query field: what is typed lands in `tab.query` and the listing below is
        // filtered by it, so the field and the rows can never disagree.
        let search = Rect::new(x + content as i32 - 182, 7, 172, 30);
        p.border(search, Color::WHITE, 4, LINE);
        p.region(search, "files-search", "Search this folder");
        p.symbol("search", search.x + 8, search.y + 8, 13, FAINT);
        let typing = tab.searching && tab.rename.is_none();
        let shown = if tab.query.is_empty() && !typing {
            format!("Search {title}")
        } else {
            tab.query.clone()
        };
        let text = p.left(
            search.x + 26,
            search.y + 7,
            118,
            &shown,
            12,
            if tab.query.is_empty() { FAINT } else { INK },
        );
        if typing {
            p.box_(
                Rect::new(search.x + 27 + text as i32, search.y + 7, 1, 16),
                INK,
                0,
            );
        }
        if !tab.query.is_empty() || typing {
            let clear = Rect::new(search.x + search.width as i32 - 24, search.y + 6, 18, 18);
            p.button(clear, Color::TRANSPARENT, 9, "files-search-clear", "Clear");
            p.symbol("close", clear.x + 4, clear.y + 4, 10, MUTED);
        }
        p.hline(x, 44, content, LINE);
        // Command bar. Everything here dispatches a real effect, and what needs a
        // selection or a clipboard is greyed when it has none rather than refusing.
        control(
            p,
            Rect::new(x + 10, 50, 74, 28),
            "files-new-folder",
            "New folder",
            folder_scope,
        );
        p.symbol(
            "plus",
            x + 16,
            58,
            14,
            if folder_scope { INK } else { FAINT },
        );
        p.left(
            x + 36,
            56,
            44,
            "New",
            12,
            if folder_scope { INK } else { FAINT },
        );
        for (i, (symbol, label, action, enabled)) in [
            (
                "scissors",
                "Cut",
                "files-cut",
                has_selection && folder_scope,
            ),
            ("copy", "Copy", "files-copy", has_selection && folder_scope),
            (
                "paste",
                "Paste",
                "files-paste",
                clipboard.is_some_and(|c| !c.paths.is_empty()) && folder_scope,
            ),
            (
                "rename",
                "Rename",
                "files-rename",
                has_selection && folder_scope,
            ),
            // In the Recycle Bin, Delete has nothing left to do — the file is already
            // there — and Restore has everything, so the slot carries Restore instead
            // of a control whose only outcome would be a refusal.
            if in_trash {
                (
                    "restore",
                    "Restore the selected items",
                    "files-restore",
                    has_selection,
                )
            } else {
                (
                    "trash",
                    "Delete",
                    "files-delete",
                    has_selection && folder_scope,
                )
            },
        ]
        .into_iter()
        .enumerate()
        {
            let hit = Rect::new(x + 94 + i as i32 * 36, 50, 32, 28);
            control(p, hit, action, label, enabled);
            p.symbol(symbol, hit.x + 6, 57, 16, if enabled { INK } else { FAINT });
        }
        // Share hands the selected item to Messages or Mail, whichever the machine has;
        // with no selection, or nothing installed to receive it, it is greyed.
        let share = Rect::new(x + 268, 50, 28, 28);
        match share_to.filter(|_| has_selection) {
            Some(kind) => control(
                p,
                share,
                &format!("shell:share:{kind}"),
                if kind == "messages" {
                    "Share with Messages"
                } else {
                    "Share with Mail"
                },
                true,
            ),
            None => control(p, share, "shell:share", "Share", false),
        }
        p.symbol(
            "share",
            x + 274,
            57,
            16,
            if share_to.is_some() && has_selection {
                INK
            } else {
                FAINT
            },
        );
        // The Recycle Bin's own command, in the slot New file has everywhere else:
        // the one thing a file manager does that really destroys data.
        let (symbol, label, action, enabled) = if in_trash {
            (
                "trash",
                "Empty Recycle Bin",
                "files-empty-trash",
                !tab.entries.is_empty(),
            )
        } else {
            ("document", "New file", "files-new-file", folder_scope)
        };
        control(p, Rect::new(x + 306, 50, 28, 28), action, label, enabled);
        p.symbol(symbol, x + 310, 57, 16, if enabled { INK } else { FAINT });
        if content > 520 {
            let arrow = if tab.descending {
                "\u{2193}"
            } else {
                "\u{2191}"
            };
            // Sort flips the direction of the key that is in force; the column headers
            // choose the key. Both are the same `files-sort:<key>` command.
            for (dx, symbol, label, action) in [
                (
                    350,
                    "sort",
                    format!("Sort {arrow}"),
                    format!("files-sort:{}", tab.sort.id()),
                ),
                (
                    434,
                    if tab.view == crate::FileView::Grid {
                        "list-view"
                    } else {
                        "grid-view"
                    },
                    if tab.view == crate::FileView::Grid {
                        "List".to_owned()
                    } else {
                        "Grid".to_owned()
                    },
                    "files-view".to_owned(),
                ),
            ] {
                let hit = Rect::new(x + dx - 6, 50, 78, 28);
                p.button(hit, Color::TRANSPARENT, 4, &action, &label);
                p.symbol(symbol, x + dx, 57, 16, INK);
                p.left(x + dx + 22, 56, 50, &label, 12, INK);
            }
        }
        p.hline(x, 88, content, LINE);
        top = 89;
    }
    if side == 0 {
        // Narrow windows lose the sidebar, so the header carries both real actions.
        p.box_(Rect::new(0, top, w, 30), Color::rgb(248, 248, 249), 0);
        let up = has_parent(path);
        if up {
            p.button(
                Rect::new(6, top + 2, 72, 26),
                Color::TRANSPARENT,
                5,
                "files-up",
                "Enclosing folder",
            );
        }
        p.symbol(
            "chevron-left",
            10,
            top + 8,
            13,
            if up { l.accent } else { FAINT },
        );
        p.left(
            26,
            top + 6,
            50,
            "Back",
            13,
            if up { l.accent } else { FAINT },
        );
        if !up {
            p.disabled("Enclosing folder");
        }
        p.button(
            Rect::new(w.saturating_sub(118) as i32, top + 2, 112, 26),
            Color::TRANSPARENT,
            5,
            "files-root",
            l.root,
        );
        p.right(
            w.saturating_sub(116) as i32,
            top + 6,
            106,
            l.root,
            13,
            l.accent,
        );
        top += 30;
    }
    // Finder and Files keep Move to Trash and Put Back in menus; Explorer keeps them
    // in its command bar, which is drawn above. A menu an observer cannot see is not
    // an affordance, so the shells without a command bar carry the pair themselves,
    // greyed whenever there is nothing for them to act on.
    // Files keeps them in the context menu (the shell's), and in the Trash shows Restore
    // and Empty above the listing, so on Ubuntu the bar is the Trash's alone.
    let gnome = t == DesktopTheme::Ubuntu;
    if t != DesktopTheme::Windows && (!gnome || in_trash) {
        p.box_(Rect::new(x, top, content, 32), Color::rgb(250, 250, 250), 0);
        let mut bx = x + 10;
        for (label, action, enabled) in [
            (
                "Move to Trash",
                "files-move-to-trash",
                has_selection && folder_scope && !in_trash,
            ),
            (
                if t == DesktopTheme::Macos {
                    "Put Back"
                } else {
                    "Restore"
                },
                "files-restore",
                has_selection && in_trash,
            ),
            (
                "Empty Trash",
                "files-empty-trash",
                in_trash && !tab.entries.is_empty(),
            ),
        ] {
            let width = p.measure(label, 12, false) + 22;
            control(p, Rect::new(bx, top + 3, width, 26), action, label, enabled);
            p.left(
                bx + 11,
                top + 9,
                width.saturating_sub(18),
                label,
                12,
                if enabled { l.accent } else { FAINT },
            );
            bx += width as i32 + 8;
        }
        p.hline(x, top + 32, content, LINE);
        top += 33;
    }
    // Column headers are the sort controls: each one selects its key, and clicking the
    // key already in force reverses it. The arrow says which, so the order on screen is
    // always accounted for by something visible. Size and Date Modified are real keys
    // now: the listing carries the machine's own `stat` for every row.
    // Files keeps a star column at the right edge of its list; the other columns move
    // over for it.
    let star_column = t == DesktopTheme::Ubuntu && tab.view == crate::FileView::List;
    let (show_kind, show_size, show_date) = (content > 330, content > 480, content > 620);
    let mut edge = x + content as i32 - if star_column { 50 } else { 10 };
    if show_date {
        edge -= DATE_COLUMN;
    }
    let date_x = edge;
    if show_size {
        edge -= SIZE_COLUMN;
    }
    let size_x = edge;
    if show_kind {
        edge -= KIND_COLUMN;
    }
    let kind_x = edge;
    let mark = |key: crate::SortKey| {
        if tab.sort != key {
            ""
        } else if tab.descending {
            " \u{2193}"
        } else {
            " \u{2191}"
        }
    };
    let name_width = (kind_x - (x + 44)).max(40) as u32;
    // Files' icon view has no column headers: its sort lives in View Options.
    let other_locations = gnome && tab.scope == crate::FileScope::OtherLocations;
    let headers = !(gnome && tab.view == crate::FileView::Grid) && !other_locations;
    if headers {
        p.button(
            Rect::new(x + 40, top, name_width + 4, 26),
            Color::TRANSPARENT,
            0,
            "files-sort:name",
            "Sort by name",
        );
        p.left(
            x + 44,
            top + 6,
            name_width,
            &format!("Name{}", mark(crate::SortKey::Name)),
            11,
            MUTED,
        );
        for (shown, cx, width, label, key, action, hint) in [
            (
                show_kind,
                kind_x,
                KIND_COLUMN,
                if t == DesktopTheme::Windows {
                    "Type"
                } else {
                    "Kind"
                },
                crate::SortKey::Kind,
                "files-sort:kind",
                "Sort by kind",
            ),
            (
                show_size,
                size_x,
                SIZE_COLUMN,
                "Size",
                crate::SortKey::Size,
                "files-sort:size",
                "Sort by size",
            ),
            (
                show_date,
                date_x,
                DATE_COLUMN,
                if t == DesktopTheme::Macos {
                    "Date Modified"
                } else {
                    "Date modified"
                },
                crate::SortKey::Modified,
                "files-sort:modified",
                "Sort by date modified",
            ),
        ] {
            if !shown {
                continue;
            }
            p.vline(cx - 10, top + 5, 16, LINE);
            p.button(
                Rect::new(cx - 4, top, width as u32, 26),
                Color::TRANSPARENT,
                0,
                action,
                hint,
            );
            p.left(
                cx,
                top + 6,
                (width - 8) as u32,
                &format!("{label}{}", mark(key)),
                11,
                MUTED,
            );
        }
        p.hline(x, top + 26, content, LINE);
    }
    let head = if headers { 27 } else { 0 };
    let footer = if t == DesktopTheme::Ubuntu { 0 } else { 26 };
    let viewport = Rect::new(
        x,
        top + head,
        content,
        h.saturating_sub((top + head) as u32 + footer).max(1),
    );
    // Every row is painted in a pane that scrolls; each folder keeps its own place, so
    // Back returns to where the list was.
    let pane = p.pane(&folder_pane(path, tab.scope), viewport);
    let body = pane.top();
    if tab.scope == crate::FileScope::OtherLocations && gnome {
        gnome_other_locations(p, Rect::new(x, top, content, h.saturating_sub(top as u32)));
    } else if tab.view == crate::FileView::Grid && gnome {
        gnome_grid(
            p,
            env,
            Rect::new(x, body, content, viewport.height),
            tab,
            &rows_shown,
        );
    } else if tab.view == crate::FileView::Grid {
        grid(
            p,
            env,
            &l,
            Rect::new(x, body, content, viewport.height),
            tab,
            &rows_shown,
        );
    }
    let rows = if tab.view == crate::FileView::Grid {
        0
    } else {
        rows_shown.len()
    };
    for (i, index) in rows_shown.iter().copied().take(rows).enumerate() {
        let entry = &entries[index];
        let meta = tab
            .row(index)
            .unwrap_or_else(|| crate::FileRow::named(entry));
        let origin = meta.original.clone();
        let y = body + (i as u32 * l.row) as i32;
        let directory = entry.ends_with('/');
        let inset = if t == DesktopTheme::Macos { 8 } else { 4 };
        let selected = tab.selected == Some(index);
        let bg = if selected {
            l.selection
        } else if t == DesktopTheme::Macos && i % 2 == 1 {
            Color::rgb(244, 245, 245)
        } else {
            Color::TRANSPARENT
        };
        p.button(
            Rect::new(
                x + inset,
                y,
                content.saturating_sub(inset as u32 * 2),
                l.row,
            ),
            bg,
            if t == DesktopTheme::Macos { 5 } else { 4 },
            &format!("open:{i}"),
            entry,
        );
        // Explorer's items are list items named as the file is (UI Automation's ListItem),
        // a folder without the trailing slash the listing marks it with.
        if t == DesktopTheme::Windows {
            if let Some(sem) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
                sem.role = "list item".into();
                sem.label = entry.trim_end_matches('/').into();
            }
        }
        let icon = if t == DesktopTheme::Ubuntu { 24 } else { 16 };
        if t == DesktopTheme::Macos && directory {
            p.symbol(
                "chevron-right",
                x + 12,
                y + (l.row as i32 - 9) / 2,
                9,
                MUTED,
            );
        }
        entry_icon(
            p,
            &l,
            x + if t == DesktopTheme::Macos { 24 } else { 16 },
            y + (l.row as i32 - icon as i32) / 2,
            icon,
            directory,
        );
        let name_x = x + if t == DesktopTheme::Ubuntu { 52 } else { 48 };
        if star_column {
            // A real star: `files-star:<row>` adds the row to, or takes it off, the
            // desktop's starred list, which the Starred place shows.
            let absolute_path = if absolute {
                entry_name(entry).to_owned()
            } else {
                tab.child(entry_name(entry))
            };
            let starred = env.files.starred(&absolute_path);
            let star = Rect::new(x + content as i32 - 40, y + (l.row as i32 - 28) / 2, 28, 28);
            p.button(
                star,
                Color::TRANSPARENT,
                14,
                &format!("files-star:{i}"),
                &format!(
                    "{} {}",
                    if starred { "Unstar" } else { "Star" },
                    entry_name(entry).rsplit('/').next().unwrap_or_default()
                ),
            );
            p.symbol(
                if starred { "star" } else { "star-outline" },
                star.x + 6,
                star.y + 6,
                16,
                if starred { INK } else { FAINT },
            );
        }
        // A rename replaces the row's name with the field collecting it, caret and all:
        // what is on screen is the buffer that Enter will commit.
        match &tab.rename {
            Some(rename) if selected => {
                let field = Rect::new(name_x - 4, y + 2, 200.min(content), l.row.saturating_sub(4));
                p.border(field, Color::WHITE, 3, l.accent);
                let width = p.left(
                    field.x + 4,
                    y + (l.row as i32 - 19) / 2,
                    190,
                    &rename.name,
                    13,
                    INK,
                );
                p.box_(
                    Rect::new(
                        field.x + 5 + width as i32,
                        y + (l.row as i32 - 15) / 2,
                        1,
                        15,
                    ),
                    INK,
                    0,
                );
            }
            // A list of paths from all over the machine, or a Trash row, which knows
            // where it came from: the name, then where it lives, the way Files' Recent
            // list and Nautilus's and Finder's Trash read. The original path is the
            // `.trashinfo` record's, so the Trash says what a restore would really do.
            _ if absolute || origin.is_some() => {
                let full = entry_name(entry);
                let (folder, name) = match &origin {
                    Some(from) => {
                        let (folder, _) = from.rsplit_once('/').unwrap_or(("", from.as_str()));
                        (folder, full)
                    }
                    None => {
                        let (folder, name) = full.rsplit_once('/').unwrap_or(("", full));
                        (folder, name)
                    }
                };
                let room = (kind_x - 16 - name_x).max(0) as u32;
                let used = p.left(name_x, y + (l.row as i32 - 19) / 2, room, name, 13, INK);
                let home = env.files.home.trim_end_matches('/');
                let folder = if folder.is_empty() {
                    "/".to_owned()
                } else if !home.is_empty() && t != DesktopTheme::Windows && folder.starts_with(home)
                {
                    format!("~{}", &folder[home.len()..])
                } else {
                    folder.to_owned()
                };
                p.left(
                    name_x + used as i32 + 10,
                    y + (l.row as i32 - 18) / 2,
                    room.saturating_sub(used + 10),
                    &folder,
                    12,
                    MUTED,
                );
            }
            _ => {
                p.left(
                    name_x,
                    y + (l.row as i32 - 19) / 2,
                    (kind_x - 16 - name_x).max(0) as u32,
                    entry.trim_end_matches('/'),
                    13,
                    INK,
                );
            }
        }
        // The metadata columns, every value read off the row the machine described. A
        // row the listing said nothing about draws an em dash rather than a zero.
        if show_kind {
            let kind = kind_label(t, &meta);
            p.left(
                kind_x,
                y + (l.row as i32 - 18) / 2,
                (KIND_COLUMN - 8) as u32,
                &kind,
                12,
                MUTED,
            );
        }
        if show_size {
            let size = meta.size.map_or_else(|| "\u{2014}".to_owned(), size_label);
            p.left(
                size_x,
                y + (l.row as i32 - 18) / 2,
                (SIZE_COLUMN - 8) as u32,
                &size,
                12,
                MUTED,
            );
        }
        if show_date {
            let when = meta
                .modified
                .map_or_else(|| "\u{2014}".to_owned(), date_label);
            p.left(
                date_x,
                y + (l.row as i32 - 18) / 2,
                (DATE_COLUMN - 8) as u32,
                &when,
                12,
                MUTED,
            );
        }
    }
    if rows_shown.is_empty() && !other_locations {
        // A filter that hides everything says so: an empty folder and a query with no
        // match look identical otherwise, and only one of them is the folder's fault.
        let empty = if !tab.query.is_empty() {
            "No items match your search."
        } else {
            match (t, tab.scope) {
                (DesktopTheme::Windows, crate::FileScope::Gallery) => {
                    "Photos you save to Pictures will appear here."
                }
                (DesktopTheme::Windows, crate::FileScope::Starred) => {
                    "Files you add to Favorites will appear here."
                }
                (DesktopTheme::Windows, _) => "This folder is empty.",
                (_, crate::FileScope::Recents) => "No Recent Files",
                (_, crate::FileScope::Starred) => "No Starred Files",
                _ if place => "Trash is Empty",
                _ => "Folder is Empty",
            }
        };
        p.center(x, body + 60, content, empty, 15, FAINT);
    }
    p.end_pane(pane, None);
    if gnome {
        gnome_status(p, w, h, tab);
    }
    if footer > 0 {
        let fy = h.saturating_sub(footer) as i32;
        p.box_(
            Rect::new(x, fy, content, footer),
            Color::rgb(248, 248, 249),
            0,
        );
        p.hline(x, fy, content, LINE);
        if t == DesktopTheme::Macos {
            // Path bar.
            let mut cx = x + 12;
            for (i, part) in std::iter::once(l.root).chain(components(path)).enumerate() {
                if i > 0 {
                    p.symbol("chevron-right", cx, fy + 9, 8, FAINT);
                    cx += 14;
                }
                p.symbol(
                    if i == 0 { "drive" } else { "folder" },
                    cx,
                    fy + 6,
                    13,
                    if i == 0 { MUTED } else { l.folder },
                );
                cx += 18;
                cx += p.left(
                    cx,
                    fy + 5,
                    (x + content as i32 - cx - 8).max(0) as u32,
                    part,
                    11,
                    MUTED,
                ) as i32
                    + 8;
            }
        } else {
            let note = format!(
                "{} item{}{}{}",
                rows_shown.len(),
                if rows_shown.len() == 1 { "" } else { "s" },
                // The filtered count is the count on screen; the total says what it
                // was filtered out of, rather than letting the two blur.
                if rows_shown.len() == tab.listed() {
                    String::new()
                } else {
                    format!(" of {}", tab.listed())
                },
                match tab.selection() {
                    Some(entry) => format!("  ·  {} selected", entry.trim_end_matches('/')),
                    None => String::new(),
                }
            );
            p.left(x + 14, fy + 5, content.saturating_sub(28), &note, 12, MUTED);
        }
    }
}

/// What each platform's Kind (Type) column calls an entry. A folder and a symbolic
/// link are named from what the filesystem said they are, not from what the name looks
/// like; only an ordinary file falls back to its extension, the way the platform's own
/// column does. Nothing here is a guess about the contents.
pub fn kind_label(t: DesktopTheme, row: &crate::FileRow) -> String {
    match row.kind {
        crate::EntryKind::Directory => {
            return if t == DesktopTheme::Windows {
                "File folder".into()
            } else {
                "Folder".into()
            }
        }
        crate::EntryKind::Symlink => {
            return match t {
                DesktopTheme::Macos | DesktopTheme::Ios => "Alias".into(),
                DesktopTheme::Windows => "Shortcut".into(),
                _ => "Link".into(),
            }
        }
        crate::EntryKind::File => {}
    }
    let entry = row.name();
    let name = entry.rsplit('/').next().unwrap_or(entry);
    let ext = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, ext)| ext.to_ascii_lowercase());
    match (t, ext.as_deref()) {
        (DesktopTheme::Windows, Some("txt")) => "Text Document".into(),
        (DesktopTheme::Windows, Some(ext)) => format!("{} File", ext.to_ascii_uppercase()),
        (DesktopTheme::Windows, None) => "File".into(),
        (DesktopTheme::Macos, Some("txt")) => "Plain Text Document".into(),
        (_, Some("txt")) => "Plain text document".into(),
        (DesktopTheme::Macos, Some("png")) => "PNG image".into(),
        (DesktopTheme::Macos, Some("jpg" | "jpeg")) => "JPEG image".into(),
        (_, Some("png")) => "PNG image".into(),
        (_, Some("jpg" | "jpeg")) => "JPEG image".into(),
        (_, Some("md")) => "Markdown document".into(),
        (_, Some("html" | "htm")) => "HTML document".into(),
        _ => "Document".into(),
    }
}

/// The scroll pane a folder's listing is painted in: one per place, named by a stable
/// digest of the path because a pane name is a single segment.
fn folder_pane(path: &str, scope: crate::FileScope) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in format!("{scope:?}{path}").bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("files-{hash:016x}")
}

/// Icon grid. Same rows, same order, same `open:<i>` targets as the list — only the
/// arrangement differs, so switching view cannot move a file out from under a click.
/// GNOME Files marks a starred item with a star on its icon, and the selected item
/// carries the star button too, so starring does not need the list's star column.
fn grid(
    p: &mut Painter,
    env: &crate::AppEnv<'_>,
    l: &Look,
    area: Rect,
    tab: &crate::FileTab,
    rows: &[usize],
) {
    const CELL: u32 = 96;
    let columns = (area.width / CELL).max(1);
    let stars = env.theme == DesktopTheme::Ubuntu;
    for (i, index) in rows.iter().copied().enumerate() {
        let entry = &tab.entries[index];
        let cell = Rect::new(
            area.x + (i as u32 % columns * CELL) as i32,
            area.y + (i as u32 / columns * CELL) as i32,
            CELL,
            CELL,
        );
        p.button(
            Rect::new(cell.x + 4, cell.y + 4, CELL - 8, CELL - 8),
            if tab.selected == Some(index) {
                l.selection
            } else {
                Color::TRANSPARENT
            },
            6,
            &format!("open:{i}"),
            entry,
        );
        entry_icon(p, l, cell.x + 30, cell.y + 14, 36, entry.ends_with('/'));
        p.label(
            cell.x + 6,
            cell.y + 58,
            CELL - 12,
            entry.trim_end_matches('/'),
            12,
            INK,
            false,
            Align::Center,
        );
        // In the Trash the icon grid says where each thing came from, exactly as the
        // list does: the `.trashinfo` record is what a Restore would act on, and it
        // must be readable in whichever view the file manager happens to be in.
        if let Some(from) = tab.original_of(index) {
            let home = env.files.home.trim_end_matches('/');
            let (folder, _) = from.rsplit_once('/').unwrap_or(("", from.as_str()));
            let folder = if folder.is_empty() {
                "/".to_owned()
            } else if !home.is_empty() && folder.starts_with(home) {
                format!("~{}", &folder[home.len()..])
            } else {
                folder.to_owned()
            };
            p.label(
                cell.x + 2,
                cell.y + 74,
                CELL - 4,
                &folder,
                10,
                MUTED,
                false,
                Align::Center,
            );
        }
        if stars {
            let path = if tab.scope.absolute() {
                entry_name(entry).to_owned()
            } else {
                tab.child(entry_name(entry))
            };
            let starred = env.files.starred(&path);
            if starred || tab.selected == Some(index) {
                let star = Rect::new(cell.x + CELL as i32 - 30, cell.y + 8, 22, 22);
                p.region_above(
                    star,
                    &format!("files-star:{i}"),
                    &format!(
                        "{} {}",
                        if starred { "Unstar" } else { "Star" },
                        entry_name(entry)
                            .trim_end_matches('/')
                            .rsplit('/')
                            .next()
                            .unwrap_or_default()
                    ),
                );
                p.symbol(
                    if starred { "star" } else { "star-outline" },
                    star.x + 3,
                    star.y + 3,
                    16,
                    if starred { INK } else { MUTED },
                );
            }
        }
    }
}

/// Nautilus 46's icon view: a grid of cells 144 by 132 px on a 167 by 138 px pitch, 20 px
/// in and 18 px down, each announced as a table cell named by the file and holding its
/// 96 px icon; the name under the icon is the cell's own text.
fn gnome_grid(
    p: &mut Painter,
    env: &crate::AppEnv<'_>,
    area: Rect,
    tab: &crate::FileTab,
    rows: &[usize],
) {
    let columns = ((area.width.saturating_sub(20) + 23) / 167).max(1);
    for (i, index) in rows.iter().copied().enumerate() {
        let entry = &tab.entries[index];
        let name = entry_name(entry);
        let selected = tab.selected == Some(index);
        let cell = Rect::new(
            area.x + 20 + (i as u32 % columns * 167) as i32,
            area.y + 18 + (i as u32 / columns * 138) as i32,
            144,
            132,
        );
        p.button(
            cell,
            if selected {
                Color::rgb(190, 215, 246)
            } else {
                Color::TRANSPARENT
            },
            10,
            &format!("open:{i}"),
            name,
        );
        p.announce("cell", None);
        if selected {
            p.select_last(true);
        }
        let icon = Rect::new(cell.x + 24, cell.y + 6, 96, 96);
        gnome_icon(p, icon, name, entry.ends_with('/'));
        announced(p, icon, "img", "");
        // The name is painted 4 px lower than a 96 px icon's foot.
        p.label(
            cell.x + 4,
            cell.y + 108,
            136,
            name,
            14,
            Color::rgb(46, 52, 54),
            false,
            Align::Center,
        );
        // In the Trash each item says where it came from (its `.trashinfo` record), so a
        // Restore is not a leap of faith.
        if let Some(from) = tab.original_of(index) {
            let home = env.files.home.trim_end_matches('/');
            let folder = from
                .rsplit_once('/')
                .map_or("/", |(f, _)| if f.is_empty() { "/" } else { f });
            let folder = match folder.strip_prefix(home).filter(|_| !home.is_empty()) {
                Some(rest) => format!("~{rest}"),
                None => folder.to_owned(),
            };
            p.label(
                cell.x + 4,
                cell.y + 120,
                136,
                &folder,
                11,
                MUTED,
                false,
                Align::Center,
            );
        }
        let path = if tab.scope.absolute() {
            name.to_owned()
        } else {
            tab.child(name)
        };
        let starred = env.files.starred(&path);
        if starred {
            // A starred file's star unstars it, as the list's column does; starring is the
            // context menu's.
            let star = Rect::new(cell.x + 144 - 30, cell.y + 8, 22, 22);
            p.region_above(
                star,
                &format!("files-star:{i}"),
                &format!("Unstar {}", name.rsplit('/').next().unwrap_or_default()),
            );
            p.symbol("star", star.x + 3, star.y + 3, 16, INK);
        }
    }
}

/// Nautilus 46's Other Locations: On This Device lists the computer, a 48 px row that
/// opens its root, with the mount point at the right; the Connect to Server bar under it
/// is announced insensitive, as the machine reaches no servers.
fn gnome_other_locations(p: &mut Painter, area: Rect) {
    let ink = Color::rgb(46, 52, 54);
    let (x, y) = (area.x, area.y);
    p.strong(x + 6, y + 6, 300, "On This Device", 14, ink);
    let row = Rect::new(x - 4, y + 33, area.width.saturating_sub(4), 48);
    p.button(row, Color::TRANSPARENT, 0, "files-root", "Computer");
    p.announce("option", None);
    p.symbol("drive", x + 8, y + 39, 32, ink);
    announced(p, Rect::new(x + 8, y + 39, 32, 32), "img", "");
    p.left(x + 52, y + 46, 200, "Computer", 14, ink);
    p.presentation();
    p.label(x + 535, y + 46, 111, "/", 14, MUTED, false, Align::Right);
    p.hline(x, row.y + 48, area.width, LINE);
    let bar = area.y + area.height as i32 - 57;
    p.hline(x, bar - 1, area.width, LINE);
    let field = Rect::new(x + 9, bar + 8, 312, 34);
    p.border(field, Color::rgb(240, 240, 240), 6, LINE);
    p.left(
        field.x + 10,
        bar + 16,
        290,
        "Enter server address…",
        14,
        FAINT,
    );
    p.presentation();
    p.insensitive(field, "files-connect-address", "");
    p.announce("textbox", None);
    let connect = Rect::new(x + 368, bar + 13, 91, 34);
    p.box_(connect, Color::rgb(235, 235, 235), 6);
    p.label(
        connect.x,
        bar + 21,
        91,
        "Connect",
        14,
        FAINT,
        true,
        Align::Center,
    );
    p.presentation();
    p.insensitive(connect, "files-connect", "Connect");
}

/// Yaru's icons at 96 px, drawn: a folder is a grey pocket in front of a plum-to-orange
/// back, the Desktop folder the gradient square alone, and a file a white page with lines.
fn gnome_icon(p: &mut Painter, r: Rect, name: &str, directory: bool) {
    let (x, y) = (r.x, r.y);
    if directory && name == "Desktop" {
        p.gradient(
            Rect::new(x + 8, y + 16, 80, 66),
            Color::rgb(118, 40, 110),
            Color::rgb(236, 112, 70),
            8,
        );
        p.box_(Rect::new(x + 8, y + 16, 14, 66), Color(70, 20, 70, 90), 0);
        return;
    }
    if directory {
        p.box_(Rect::new(x + 8, y + 14, 34, 14), Color::rgb(131, 45, 77), 6);
        p.box_(Rect::new(x + 8, y + 20, 80, 30), Color::rgb(170, 60, 70), 6);
        p.box_(
            Rect::new(x + 50, y + 20, 38, 30),
            Color::rgb(198, 78, 40),
            6,
        );
        p.box_(
            Rect::new(x + 8, y + 26, 80, 56),
            Color::rgb(112, 112, 112),
            6,
        );
        return;
    }
    p.border(
        Rect::new(x + 14, y + 4, 68, 88),
        Color::rgb(245, 245, 245),
        4,
        Color::rgb(200, 200, 200),
    );
    for line in 0..8 {
        p.hline(x + 28, y + 40 + line * 5, 40, Color::rgb(150, 150, 150));
    }
}

/// Files' floating status bar at the bottom right, shown while something is selected:
/// the selection's name, then its size when the listing carried one.
fn gnome_status(p: &mut Painter, w: u32, h: u32, tab: &crate::FileTab) {
    let Some(index) = tab.selected else {
        return;
    };
    let Some(entry) = tab.entries.get(index) else {
        return;
    };
    let name = entry_name(entry).rsplit('/').next().unwrap_or_default();
    let first = format!("\u{201c}{name}\u{201d} selected");
    let second = tab
        .row(index)
        .and_then(|row| row.size)
        .map(|bytes| format!("({bytes} bytes)"));
    let ink = Color::rgb(46, 52, 54);
    let right = w as i32 - 24;
    let second_width = second.as_ref().map_or(0, |s| p.measure(s, 14, false));
    let first_width = p.measure(&first, 14, false);
    let mut lx =
        right - second_width as i32 - if second.is_some() { 6 } else { 0 } - first_width as i32;
    lx = lx.max(0);
    let pill = Rect::new(
        lx - 11,
        h as i32 - 33,
        (w as i32 - lx + 11).max(0) as u32,
        33,
    );
    p.border(pill, Color::WHITE, 8, Color::rgb(218, 218, 218));
    p.left(lx, h as i32 - 26, first_width + 2, &first, 14, ink);
    if let Some(second) = second {
        p.left(
            right - second_width as i32,
            h as i32 - 26,
            second_width + 2,
            &second,
            14,
            ink,
        );
    }
}

fn files_mobile(
    p: &mut Painter,
    env: &crate::AppEnv<'_>,
    tab: &crate::FileTab,
    rows_shown: &[usize],
) {
    let (t, w, h) = (env.theme, env.width, env.height);
    let (path, entries) = (tab.path.as_str(), tab.entries.as_slice());
    let in_trash = tab.in_trash(&env.files.trash);
    let has_selection = tab.selection().is_some();
    let folder_scope = tab.scope == crate::FileScope::Folder;
    let l = look(t);
    let ios = t == DesktopTheme::Ios;
    p.scene.background = if ios {
        Color::WHITE
    } else {
        Color::rgb(248, 250, 240)
    };
    let recents = tab.scope == crate::FileScope::Recents;
    let tab_bar = if ios { 50 } else { 0 };
    let mut top;
    // The listing scrolls; on iOS the large title and the search field scroll with it,
    // and the title collapses into the navigation bar (which carries the crumb back to
    // the enclosing folder) once it has gone under it.
    let titled = ios.then(|| {
        let title = if recents {
            "Recents"
        } else {
            current(path, l.root)
        };
        let pane = p
            .pane(
                &folder_pane(path, tab.scope),
                Rect::new(0, 0, w, h.saturating_sub(tab_bar).max(1)),
            )
            .titled(title, 44);
        p.strong(16, pane.top() + 4, w.saturating_sub(32), title, 32, INK);
        pane
    });
    if let Some(pane) = &titled {
        let y0 = pane.top() - 90 + 54;
        // Real field: typing lands in `tab.query` and the rows below are what survives
        // it, so nothing on this screen is outside the filter it advertises.
        let field = Rect::new(16, y0 + 90, w.saturating_sub(32), 36);
        p.box_(field, Color(118, 118, 128, 30), 10);
        p.region(field, "files-search", "Search");
        p.symbol("search", field.x + 8, field.y + 10, 16, FAINT);
        let typing = tab.searching;
        let width = p.left(
            field.x + 30,
            field.y + 8,
            field.width.saturating_sub(70),
            if tab.query.is_empty() && !typing {
                "Search"
            } else {
                &tab.query
            },
            17,
            if tab.query.is_empty() { FAINT } else { INK },
        );
        if typing {
            p.box_(
                Rect::new(field.x + 31 + width as i32, field.y + 8, 2, 21),
                l.accent,
                0,
            );
        }
        if !tab.query.is_empty() || typing {
            let clear = Rect::new(field.x + field.width as i32 - 30, field.y + 8, 22, 22);
            p.button(clear, Color::TRANSPARENT, 11, "files-search-clear", "Clear");
            p.symbol("close", clear.x + 5, clear.y + 5, 12, MUTED);
        }
        top = y0 + 138;
    } else {
        // Breadcrumb chips: storage root, enclosing folder, current folder.
        let mut cx = 16;
        let depth = components(path).len();
        for (i, (name, action)) in [
            (l.root, Some("files-root")),
            (parent(path, l.root), Some("files-up")),
            (current(path, l.root), None),
        ]
        .iter()
        .enumerate()
        {
            if (i == 1 && depth < 2) || (i == 2 && depth < 1) {
                if i == 1 {
                    p.region(Rect::new(16, 4, 28, 32), "files-up", "Enclosing folder");
                }
                continue;
            }
            if i > 0 {
                p.symbol("chevron-right", cx, 15, 10, MUTED);
                cx += 16;
            }
            let width = p.measure(name, 14, action.is_none());
            if let Some(action) = action {
                p.button(
                    Rect::new(cx - 6, 4, width + 12, 32),
                    Color::TRANSPARENT,
                    16,
                    action,
                    name,
                );
            }
            p.label(
                cx,
                11,
                w.saturating_sub(cx as u32 + 8),
                name,
                14,
                if action.is_some() { MUTED } else { INK },
                action.is_none(),
                Align::Left,
            );
            cx += width as i32 + 10;
        }
        p.hline(0, 42, w, LINE);
        top = 0;
    }
    // Android's crumbs stay put over the list; iOS's title scrolls with it.
    let pane = match titled {
        Some(pane) => pane,
        None => {
            let pane = p.pane(
                &folder_pane(path, tab.scope),
                Rect::new(0, 43, w, h.saturating_sub(43).max(1)),
            );
            top = pane.top() + 7;
            pane
        }
    };
    // The phones get the same pair the desktops have. iOS Files and Google's Files
    // both put these on the screen rather than behind a menu, and each is greyed until
    // there is something for it to act on.
    let mut bx = 16;
    for (label, action, enabled) in [
        (
            "Move to Trash",
            "files-move-to-trash",
            has_selection && folder_scope && !in_trash,
        ),
        (
            if ios { "Put Back" } else { "Restore" },
            "files-restore",
            has_selection && in_trash,
        ),
    ] {
        let width = p.measure(label, 13, false) + 24;
        control(p, Rect::new(bx, top, width, 32), action, label, enabled);
        p.left(
            bx + 12,
            top + 8,
            width.saturating_sub(20),
            label,
            13,
            if enabled { l.accent } else { FAINT },
        );
        bx += width as i32 + 8;
    }
    top += 40;
    for (i, index) in rows_shown.iter().copied().enumerate() {
        let entry = &entries[index];
        let origin = tab.original_of(index);
        let y = top + (i as u32 * l.row) as i32;
        let directory = entry.ends_with('/');
        p.button(
            Rect::new(0, y, w, l.row),
            Color::TRANSPARENT,
            0,
            &format!("open:{i}"),
            entry,
        );
        if ios {
            entry_icon(p, &l, 16, y + 12, 36, directory);
        } else {
            p.circle(36, y + 32, 20, Color::rgb(232, 236, 222));
            entry_icon(p, &l, 25, y + 21, 22, directory);
        }
        let tx = if ios { 66 } else { 72 };
        // A recent is an absolute path, so the row shows the name and says where it
        // lives; inside a folder every row is already a child of the title.
        let trimmed = entry.trim_end_matches('/');
        // A trashed row says where it came from, exactly as a recent says where it
        // lives: the `.trashinfo` record is what a Put Back would act on.
        let (name, note) = match (recents, &origin) {
            (true, _) => (
                trimmed.rsplit('/').next().unwrap_or(trimmed),
                parent_label(trimmed, l.root),
            ),
            (false, Some(from)) => (trimmed, parent_label(from, l.root)),
            (false, None) => (
                trimmed,
                match tab.row(index).map(|r| r.kind) {
                    Some(crate::EntryKind::Symlink) => "Alias".to_owned(),
                    _ if directory => "Folder".to_owned(),
                    _ => tab
                        .row(index)
                        .and_then(|r| r.size)
                        .map_or_else(|| "Document".to_owned(), size_label),
                },
            ),
        };
        p.left(
            tx,
            y + if ios { 10 } else { 13 },
            w.saturating_sub(tx as u32 + 40),
            name,
            16,
            INK,
        );
        p.left(
            tx,
            y + if ios { 32 } else { 36 },
            w.saturating_sub(tx as u32 + 40),
            &note,
            13,
            MUTED,
        );
        if ios {
            p.symbol(
                "chevron-right",
                w.saturating_sub(26) as i32,
                y + 23,
                13,
                FAINT,
            );
            p.hline(tx, y + l.row as i32 - 1, w.saturating_sub(tx as u32), LINE);
        }
    }
    if rows_shown.is_empty() {
        let empty = if !tab.query.is_empty() {
            "No items match your search."
        } else if recents {
            "No recent documents"
        } else if ios {
            "Folder is Empty"
        } else {
            "No items"
        };
        p.center(0, top + 70, w, empty, 17, MUTED);
    }
    p.end_pane(pane, None);
    top = h.saturating_sub(tab_bar) as i32;
    if ios {
        p.box_(Rect::new(0, top, w, tab_bar), Color(249, 249, 249, 245), 0);
        p.hline(0, top, w, LINE);
        for (i, (symbol, name, action)) in [
            // Recents is a real list: every entry is a document this desktop opened.
            ("clock", "Recents", Some("files-recents")),
            // Shared stays inert. Nothing in this world can receive a shared file —
            // no recipient, no link, no service to hand it to — so there is nothing
            // for a Shared list to hold and nothing honest for the tab to dispatch.
            ("person", "Shared", None),
            ("folder", "Browse", Some("files-browse")),
        ]
        .iter()
        .enumerate()
        {
            let cx = (w as i32 / 3) * i as i32 + w as i32 / 6;
            let selected = (i == 0) == recents && i != 1;
            let tint = match (action.is_some(), selected) {
                (false, _) => FAINT,
                (true, true) => l.accent,
                (true, false) => MUTED,
            };
            if let Some(action) = action {
                p.button(
                    Rect::new(cx - 40, top + 2, 80, 46),
                    Color::TRANSPARENT,
                    8,
                    action,
                    name,
                );
            }
            p.symbol(symbol, cx - 12, top + 5, 24, tint);
            p.label(cx - 40, top + 30, 80, name, 10, tint, false, Align::Center);
            if action.is_none() {
                p.disabled(name);
            }
        }
    }
}
/// Enclosing folder of an absolute path, as a Recents subtitle shows it.
fn parent_label(path: &str, root: &str) -> String {
    match path.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => parent.to_owned(),
        _ => root.to_owned(),
    }
}

/// Wrap `text` to `cells` columns, tagging every line with the colour that says which
/// stream it came from.
/// Rows of `cells` terminal cells: wide characters (CJK, emoji) take two, combining
/// marks none, exactly as `Primitive::Text` lays them out.
fn wrap_into(lines: &mut Vec<(String, Color)>, text: &str, color: Color, cells: usize) {
    for line in text.lines() {
        for row in cw_scene::wrap_text(line, cells) {
            lines.push((row, color));
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn terminal(
    p: &mut Painter,
    t: DesktopTheme,
    w: u32,
    h: u32,
    input: &str,
    prompt_text: &str,
    transcript: &[crate::TerminalEntry],
    cursor: usize,
    scroll: usize,
) {
    let (bg, fg, prompt, err) = match t {
        DesktopTheme::Macos => (
            Color::WHITE,
            Color::rgb(0, 0, 0),
            Color::rgb(0, 0, 0),
            Color::rgb(190, 30, 30),
        ),
        DesktopTheme::Ubuntu => (
            Color::rgb(48, 10, 36),
            Color::rgb(255, 255, 255),
            Color::rgb(138, 226, 52),
            Color::rgb(239, 41, 41),
        ),
        DesktopTheme::Windows => (
            Color::rgb(12, 12, 12),
            Color::rgb(204, 204, 204),
            Color::rgb(204, 204, 204),
            Color::rgb(231, 72, 86),
        ),
        DesktopTheme::Ios => (
            Color::rgb(0, 0, 0),
            Color::rgb(235, 235, 235),
            Color::rgb(48, 209, 88),
            Color::rgb(255, 69, 58),
        ),
        DesktopTheme::Android => (
            Color::rgb(18, 20, 22),
            Color::rgb(220, 230, 228),
            Color::rgb(111, 220, 160),
            Color::rgb(242, 109, 109),
        ),
    };
    p.scene.background = bg;
    let pad: u32 = if t.mobile() { 14 } else { 8 };
    let size = 13;
    let cells = (w.saturating_sub(pad * 2) / 8).max(1) as usize;
    let mut lines: Vec<(String, Color)> = Vec::new();
    for entry in transcript {
        // Echo first: the prompt plus the command is the boundary an observer keys on.
        wrap_into(&mut lines, &entry.echo(), prompt, cells);
        if !entry.stdout.is_empty() {
            wrap_into(&mut lines, &entry.stdout, fg, cells);
        }
        if !entry.stderr.is_empty() {
            wrap_into(&mut lines, &entry.stderr, err, cells);
        }
        if entry.failed() {
            // Only failures are marked: a clean frame stays clean, and `[exit 1]` is a
            // literal to match where error prose is not.
            wrap_into(&mut lines, &entry.status(), err, cells);
        }
        // Command Prompt prints a blank line before its next prompt.
        if crate::prompt_gap(&entry.prompt).is_empty() {
            lines.push((String::new(), fg));
        }
    }
    let capacity = (h.saturating_sub(pad * 2 + 19) / 19).max(1) as usize;
    // `scroll` lifts the window off the tail. It is clamped here rather than in state
    // because only the view knows how many wrapped lines fit at this size.
    let hidden = lines.len().saturating_sub(capacity);
    let scroll = scroll.min(hidden);
    let first = hidden - scroll;
    // GNOME Terminal's VTE widget is one accessible of role terminal, named Terminal,
    // whose text interface holds what the screen shows; its rows are not accessibles of
    // their own. On Ubuntu the rows are drawn unannounced and the terminal's node
    // carries the screen's text as its value, so a reader still gets every row. Windows
    // Terminal's text area is one element too, whose text pattern gives the buffer's rows.
    let vte = matches!(t, DesktopTheme::Ubuntu | DesktopTheme::Windows);
    let mut screen = String::new();
    let mut y = pad as i32;
    for (line, color) in &lines[first..(first + capacity).min(lines.len())] {
        mono(
            p,
            Rect::new(pad as i32, y, w.saturating_sub(pad * 2), 19),
            line,
            size,
            *color,
        );
        if vte {
            p.presentation();
            screen.push_str(line);
            screen.push('\n');
        }
        y += 19;
    }
    // The machine's own prompt, not a per-theme sigil: what is drawn is what the shell
    // prints, so nothing on screen is a harness invention.
    let shown = crate::prompt_or_sigil(prompt_text);
    let sig = format!("{shown}{}", crate::prompt_gap(shown));
    if vte {
        screen.push_str(&sig);
        screen.push_str(input);
        p.region(Rect::new(0, 0, w, h), "terminal-input", "Terminal");
        p.announce("terminal", Some(&screen));
    } else {
        p.region(
            Rect::new(0, 0, w, h),
            "terminal-input",
            "Terminal command input",
        );
    }
    mono(
        p,
        Rect::new(pad as i32, y, w.saturating_sub(pad * 2), 19),
        &sig,
        size,
        prompt,
    );
    if vte {
        p.presentation();
    }
    let offset = cw_scene::text::terminal::columns(&sig) as i32 * 8;
    mono(
        p,
        Rect::new(
            pad as i32 + offset,
            y,
            w.saturating_sub(pad * 2 + offset as u32),
            19,
        ),
        input,
        size,
        fg,
    );
    if vte {
        p.presentation();
    }
    // The prompt line has its own target, laid over the body and starting at the first
    // character of the input, so `click_at` reads a column straight off `dx`. Clicking
    // anywhere else still focuses the shell and leaves the caret where it was.
    p.region(
        Rect::new(
            pad as i32 + offset,
            y,
            w.saturating_sub(pad * 2 + offset as u32).max(1),
            19,
        ),
        "terminal-line",
        "Terminal prompt line",
    );
    if vte {
        // VTE announces no such target: the line takes clicks, unannounced, inside the
        // terminal's own node.
        p.presentation();
    }
    // Where the caret really is: `cursor` is a byte offset into the input, and a click
    // on the line above moved it there.
    let before =
        cw_scene::text::terminal::columns(input.get(..cursor.min(input.len())).unwrap_or(input))
            as i32;
    let caret = offset + before * 8;
    // Block cursor on the desktops, a bar on touch keyboards.
    p.box_(
        Rect::new(
            pad as i32 + caret,
            y + 1,
            if t.mobile() { 2 } else { 8 },
            16,
        ),
        if t == DesktopTheme::Macos {
            Color(0, 0, 0, 110)
        } else {
            Color(fg.0, fg.1, fg.2, 200)
        },
        0,
    );
    if hidden > 0 {
        // A real scrollbar: the thumb sits where the view is, and the track above and
        // below it pages the scrollback by the height of one screen.
        let track = h.saturating_sub(8);
        let thumb = (track * capacity as u32 / lines.len().max(1) as u32).clamp(24, track.max(24));
        let travel = track.saturating_sub(thumb);
        let from_top = (travel as usize * (hidden - scroll) / hidden.max(1)) as i32;
        let bar = Rect::new(w.saturating_sub(9) as i32, 4, 8, track);
        let page = capacity.max(1);
        for (r, lines_to, label, live) in [
            (
                Rect::new(bar.x, bar.y, bar.width, from_top.max(0) as u32),
                scroll + page,
                "Scroll back",
                scroll < hidden,
            ),
            (
                Rect::new(
                    bar.x,
                    bar.y + from_top + thumb as i32,
                    bar.width,
                    travel.saturating_sub(from_top.max(0) as u32),
                ),
                scroll.saturating_sub(page),
                "Scroll forward",
                scroll > 0,
            ),
        ] {
            if r.height > 0 && live {
                p.button(
                    r,
                    Color::TRANSPARENT,
                    2,
                    &format!("terminal-scroll:{lines_to}"),
                    label,
                );
            }
        }
        p.box_(
            Rect::new(bar.x + 2, bar.y + from_top, 4, thumb),
            Color(128, 128, 128, 150),
            2,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn editor(
    p: &mut Painter,
    t: DesktopTheme,
    (w, h): (u32, u32),
    path: &str,
    text: &str,
    dirty: bool,
    (cursor, selected): (usize, bool),
    wrap: bool,
) {
    let mobile = t.mobile();
    let paper = match t {
        DesktopTheme::Ios => Color::rgb(255, 255, 255),
        DesktopTheme::Android => Color::rgb(248, 250, 240),
        _ => Color::WHITE,
    };
    p.scene.background = paper;
    // Desktop editors keep their commands in menus: Notepad's menu bar, TextEdit's
    // menu bar and GNOME Text Editor's primary menu each hold Save. A plain-text
    // TextEdit window and a Text Editor window are the document and nothing else.
    let toolbar: u32 = match t {
        DesktopTheme::Windows => 36,
        DesktopTheme::Macos | DesktopTheme::Ubuntu => 0,
        _ => 44,
    };
    let status: u32 = match t {
        DesktopTheme::Windows => 26,
        DesktopTheme::Ubuntu | DesktopTheme::Macos => 0,
        _ => 46,
    };
    let accent = look(t).accent;
    let save_label = match t {
        DesktopTheme::Ios => "Done",
        _ => "Save",
    };
    // (x, y, width) of the Save control on each platform.
    let save = match t {
        DesktopTheme::Ubuntu => Rect::new(
            w.saturating_sub(74) as i32,
            h.saturating_sub(27) as i32,
            66,
            24,
        ),
        DesktopTheme::Windows => Rect::new(150, 4, 52, 28),
        DesktopTheme::Macos => Rect::new(w.saturating_sub(62) as i32, 3, 54, 24),
        _ => Rect::new(w.saturating_sub(76) as i32, 6, 64, 32),
    };
    match t {
        DesktopTheme::Windows => {
            p.box_(Rect::new(0, 0, w, toolbar), Color::rgb(249, 250, 252), 0);
            // The shell owns menus: these open the same panels the desktop menu bar does.
            for (i, (menu, panel)) in [("File", "file"), ("Edit", "edit"), ("View", "view")]
                .into_iter()
                .enumerate()
            {
                let x = 8 + i as i32 * 46;
                p.button(
                    Rect::new(x, 3, 44, toolbar - 7),
                    Color::TRANSPARENT,
                    4,
                    &format!("shell:panel:{panel}"),
                    menu,
                );
                p.left(x + 6, 9, 40, menu, 13, INK);
            }
            // Notepad's settings: word wrap and the app theme, both real machine settings.
            p.button(
                Rect::new(w.saturating_sub(40) as i32, 3, 32, toolbar - 7),
                Color::TRANSPARENT,
                4,
                "shell:panel:app-settings",
                "Settings",
            );
            p.symbol("gear", w.saturating_sub(32) as i32, 10, 16, INK);
            p.hline(0, toolbar as i32 - 1, w, LINE);
        }
        DesktopTheme::Macos | DesktopTheme::Ubuntu => {}
        _ => {
            let name = components(path).last().copied().unwrap_or("New Note");
            p.strong(16, 11, w.saturating_sub(110), name, 17, INK);
        }
    }
    // Only a phone paints Save; see above.
    if mobile && path.is_empty() {
        p.left(
            save.x + 8,
            save.y + (save.height as i32 - 18) / 2,
            save.width,
            save_label,
            13,
            FAINT,
        );
        p.disabled(save_label);
    } else if mobile {
        let filled = matches!(t, DesktopTheme::Ubuntu) && dirty;
        p.button(
            save,
            if filled { accent } else { Color::TRANSPARENT },
            6,
            "editor-save",
            save_label,
        );
        p.label(
            save.x,
            save.y + (save.height as i32 - 18) / 2,
            save.width,
            save_label,
            13,
            if filled {
                Color::WHITE
            } else if mobile {
                Color::rgb(204, 149, 0)
            } else {
                accent
            },
            mobile,
            Align::Center,
        );
    }
    // Line numbers are off by default in every desktop editor modelled here.
    let gutter = 0;
    if gutter > 0 {
        p.box_(
            Rect::new(
                0,
                toolbar as i32,
                gutter,
                h.saturating_sub(toolbar + status),
            ),
            Color::rgb(250, 250, 250),
            0,
        );
    }
    let left = if mobile { 18 } else { gutter + 12 };
    let mut end = cursor.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = &text[..end];
    let row = before.bytes().filter(|b| *b == b'\n').count();
    let col = before.rsplit('\n').next().unwrap_or("").chars().count();
    // One 8x18 cell, the geometry `crate::caret_for_point` assumes: `Primitive::Text`
    // advances `text_cell(13).0` = 8 px, and rows are stepped 18 px apart. Keeping the
    // paint on that grid is what makes a click land on the character under the pointer.
    const CELL_W: i32 = 8;
    const ROW_H: u32 = 18;
    const SIZE: u16 = 13;
    // With word wrap on, rows are as many cells as the text area holds; `Ln`/`Col`
    // below stay logical, as Notepad's do, while the caret sits on its visual row.
    let text_width = w.saturating_sub(left + 12);
    let columns = if wrap {
        (text_width / CELL_W as u32).max(1) as usize
    } else {
        0
    };
    let rows = crate::editor_rows(text, columns);
    let (visual, visual_col) = crate::editor_caret_cell(text, end, columns);
    let capacity = (h.saturating_sub(toolbar + status + ROW_H) / ROW_H).max(1) as usize;
    // The view scrolls on its own (the wheel, the scroll bar, a finger), independent
    // of the caret, and always by whole rows so the click grid stays exact. An edit or
    // a caret move brings the caret back into view, moving the view as little as it
    // takes; before the view is first scrolled, it follows the caret the same way.
    let last_first = rows.len().saturating_sub(capacity);
    let stored = p.scroll.offsets.get(EDITOR_PANE).copied();
    let mut first = stored.map_or(0, |o| o.max(0) as usize / ROW_H as usize);
    first = first.min(last_first);
    let follow = stored.is_none() || p.scroll.reveals(EDITOR_PANE);
    if follow {
        if visual < first {
            first = visual;
        } else if visual >= first + capacity {
            first = visual + 1 - capacity;
        }
    }
    // The offset published is the one asked for when it is the view's (so a trackpad's
    // small turns accumulate), or the caret's row when the view followed the caret.
    let offset = match stored {
        Some(o) if !follow || o.max(0) as usize / ROW_H as usize == first => {
            o.clamp(0, (last_first as u32 * ROW_H) as i32)
        }
        _ => (first as u32 * ROW_H) as i32,
    };
    let origin = (left as i32, toolbar as i32 + 10);
    // The hit region starts at the first line's top-left corner, so the offsets
    // `click_at` reports are already relative to the text grid.
    p.region(
        Rect::new(
            origin.0,
            origin.1,
            w.saturating_sub(left + 12),
            h.saturating_sub(toolbar + status + 10),
        ),
        // The scroll position travels with the target, so a click on a scrolled
        // document still resolves to the character actually under the pointer.
        &if columns > 0 {
            format!("editor-text:{first}:{columns}")
        } else {
            format!("editor-text:{first}")
        },
        // GTK 4's text view (GNOME Text Editor) has no accessible name.
        if t == DesktopTheme::Ubuntu {
            ""
        } else {
            "Document text"
        },
    );
    // The rows are a pane: clipped to whole rows, published with the document's real
    // extent, and given the platform's scroll bar when the document does not fit.
    let mut pane = p.pane(
        EDITOR_PANE,
        Rect::new(0, origin.1, w, capacity as u32 * ROW_H),
    );
    pane.offset = offset;
    // Logical line of the first painted row; a gutter numbers each line once, on the
    // row it starts on, and leaves its soft-wrapped continuations blank.
    let mut line = text[..rows.get(first).map_or(0, |r| r.0)]
        .bytes()
        .filter(|b| *b == b'\n')
        .count();
    let rows_from = p.scene.nodes.len();
    for (i, (start, stop)) in rows.iter().enumerate().skip(first).take(capacity) {
        let y = origin.1 + ((i - first) as u32 * ROW_H) as i32;
        let starts_line = i == 0 || text.as_bytes()[start - 1] == b'\n';
        if starts_line && i > first {
            line += 1;
        }
        if gutter > 0 && starts_line {
            p.right(
                0,
                y + 1,
                gutter - 10,
                &(line + 1).to_string(),
                11,
                if line == row { INK } else { FAINT },
            );
        }
        // The whole text selected (Ctrl+A): each row's characters on the selection colour.
        let row_chars = text[*start..*stop].trim_end_matches('\n').chars().count() as u32;
        if selected && row_chars > 0 {
            p.box_(
                Rect::new(
                    origin.0,
                    y,
                    (row_chars * CELL_W as u32).min(text_width),
                    ROW_H,
                ),
                Color(53, 132, 228, 77),
                0,
            );
        }
        mono(
            p,
            Rect::new(origin.0, y, text_width, ROW_H),
            &text[*start..*stop],
            SIZE,
            INK,
        );
    }
    // GTK 4's text view announces its text as the view's content, not as labels.
    if t == DesktopTheme::Ubuntu {
        for n in &mut p.scene.nodes[rows_from..] {
            n.semantic = Some(cw_scene::Semantic {
                role: "presentation".into(),
                label: String::new(),
                value: None,
                disabled: false,
                focusable: false,
            });
        }
    }
    // Caret where the model says it is, on the same grid the click arrives on; scrolled
    // out of view, it is not painted at all.
    if (first..first + capacity).contains(&visual) {
        p.box_(
            Rect::new(
                origin.0 + visual_col as i32 * CELL_W,
                origin.1 + ((visual - first) as u32 * ROW_H) as i32,
                if mobile { 2 } else { 1 },
                ROW_H,
            ),
            if mobile { Color::rgb(204, 149, 0) } else { INK },
            0,
        );
    }
    p.end_pane(pane, Some(rows.len().max(1) as u32 * ROW_H));
    let sy = h.saturating_sub(status) as i32;
    let position = format!("Ln {}, Col {}", row + 1, col + 1);
    match t {
        DesktopTheme::Windows => {
            p.box_(Rect::new(0, sy, w, status), Color::rgb(249, 250, 252), 0);
            p.hline(0, sy, w, LINE);
            // Ln/Col and the character count are live; zoom, line endings and encoding
            // are fixed facts about the model (no zoom level, text kept with `\n`
            // line breaks, always UTF-8) and open no picker.
            p.left(14, sy + 5, 120, &position, 12, MUTED);
            let count = text.chars().count();
            if w > 560 {
                p.vline(146, sy + 5, 16, LINE);
                p.left(
                    158,
                    sy + 5,
                    140,
                    &format!("{count} character{}", if count == 1 { "" } else { "s" }),
                    12,
                    MUTED,
                );
            }
            if w > 420 {
                for (i, item) in ["100%", "Unix (LF)", "UTF-8"].iter().enumerate() {
                    let x = w as i32 - 300 + i as i32 * 100;
                    p.vline(x - 12, sy + 5, 16, LINE);
                    p.left(x, sy + 5, 100, item, 12, FAINT);
                    p.disabled(item);
                }
            }
        }
        DesktopTheme::Ubuntu | DesktopTheme::Macos => {}
        _ => {
            p.hline(0, sy, w, LINE);
            // Only "New note" is real — `shell:new` opens another window of this app.
            // Gallery view, the camera and markup have no model behind them.
            let live = if t == DesktopTheme::Ios {
                Color::rgb(204, 149, 0)
            } else {
                MUTED
            };
            for (i, (symbol, label, action)) in [
                ("list-view", "Gallery view", None),
                ("camera", "Insert photo", None),
                ("edit", "Markup", None),
                ("compose", "New note", Some("shell:new")),
            ]
            .into_iter()
            .enumerate()
            {
                let cx = (w as i32 / 4) * i as i32 + w as i32 / 8;
                if let Some(action) = action {
                    p.button(
                        Rect::new(cx - 24, sy + 6, 48, 34),
                        Color::TRANSPARENT,
                        8,
                        action,
                        label,
                    );
                }
                p.symbol(
                    symbol,
                    cx - 11,
                    sy + 12,
                    22,
                    if action.is_some() { live } else { FAINT },
                );
                if action.is_none() {
                    p.disabled(label);
                }
            }
        }
    }
}

/// Pure application projection; the compositor owns frame geometry and clipping.
/// GNOME Text Editor's dialog over its window ([`crate::EditorDialog`]), painted onto the
/// window's content as a surface of its own, as GTK opens it: a window transient for the
/// editor, centred on it, at the size the real one opens at on Ubuntu 24.04 (the coverage
/// tour's reference, fidelity/tours). `draft` is the document's title. Each control is
/// announced with the role, name and bounds GTK gives it; text a control or label already
/// announces is painted without being announced again. `files` gives the home folders the
/// file chooser's sidebar lists as bookmarks.
pub fn editor_dialog(
    content: &mut Scene,
    theme: DesktopTheme,
    dialog: crate::EditorDialog,
    draft: &str,
    files: &crate::FilesEnv<'_>,
    field: &str,
) {
    use crate::EditorDialog as D;
    // Size, and how far below the window's centre GTK places it.
    let (dw, dh, below) = match dialog {
        D::Open => (819, 384, 7),
        D::Print => (642, 503, 8),
        // Notepad's question on close, a content dialog over its window on Windows 11.
        D::Unsaved if theme == DesktopTheme::Windows => (440, 176, 0),
        D::Unsaved => (353, 289, 0),
        // GTK's file chooser in save mode: the Open chooser's size, with the Name row above.
        D::SaveAs if theme == DesktopTheme::Ubuntu => (819, 430, 7),
        // Notepad's Save As on Windows 11, no larger than the window it is over.
        D::SaveAs => (
            860.min(content.width.saturating_sub(24)),
            520.min(content.height.saturating_sub(24)),
            0,
        ),
    };
    // The content starts under the 46 px header bar; the dialog centres on the window.
    // Notepad's Save As centres on Notepad's content, which has no such header.
    let header_bar: i32 = if (dialog == D::SaveAs && theme != DesktopTheme::Ubuntu)
        || theme == DesktopTheme::Windows
    {
        0
    } else {
        46
    };
    let (w, h) = (content.width as i32, content.height as i32);
    let o = Rect::new(
        (w - dw as i32) / 2,
        (h + header_bar - dh as i32) / 2 - header_bar + below,
        dw,
        dh,
    );
    let mut p = Painter::themed(theme, content.width, content.height, 3_u64 << 52);
    p.z = content.nodes.iter().map(|n| n.z).max().unwrap_or(0) + 10;
    p.scene.surfaces.push(cw_scene::Surface { rect: o, z: p.z });
    let at = |x: i32, y: i32, rw: u32, rh: u32| Rect::new(o.x + x, o.y + y, rw, rh);
    const PANE: Color = Color::rgb(250, 250, 250);
    const SIDE: Color = Color::rgb(235, 235, 235);
    const EDGE: Color = Color(0, 0, 0, 36);
    const FLAT: Color = Color(0, 0, 0, 20);
    const BLUE: Color = Color::rgb(53, 132, 228);
    // The dialog itself, announced as GTK's dialog window.
    p.box_(o, PANE, 0);
    // Windows Notepad's question is titled by the application.
    let title = if dialog == D::Unsaved && theme == DesktopTheme::Windows {
        "Notepad"
    } else {
        dialog.title()
    };
    p.region(o, "editor:dialog:frame", title);
    if let Some(s) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
        s.role = "dialog".into();
    }
    let button = |p: &mut Painter, r: Rect, action: Option<&str>, label: &str, ink: Color| {
        p.box_(r, FLAT, 6);
        match action {
            Some(a) => p.region(r, a, label),
            None => p.insensitive(r, &format!("editor:dialog:off:{label}"), label),
        }
        p.label(
            r.x,
            r.y + (r.height as i32 - 20) / 2,
            r.width,
            label,
            13,
            ink,
            true,
            Align::Center,
        );
    };
    // A label or image GTK announces with its own bounds.
    let said = |p: &mut Painter, r: Rect, role: &str, name: &str| p.container(r, role, name);
    // Text painted but announced by the label or control around it.
    #[allow(clippy::too_many_arguments)]
    let quiet = |p: &mut Painter,
                 x: i32,
                 y: i32,
                 width: u32,
                 text: &str,
                 size: u16,
                 ink: Color,
                 bold: bool| {
        p.label(o.x + x, o.y + y, width, text, size, ink, bold, Align::Left);
        p.presentation();
    };
    // A control the simulator has no model for: listed with GTK's role, insensitive.
    let off = |p: &mut Painter, r: Rect, key: &str, role: &str, name: &str| {
        p.insensitive(r, &format!("editor:dialog:off:{key}"), name);
        p.announce(role, None);
    };
    let checked = |p: &mut Painter, on: bool| {
        if let Some(n) = p.scene.nodes.last_mut() {
            n.state.get_or_insert_with(Default::default).checked = Some(on);
        }
    };
    match dialog {
        D::Open => {
            p.box_(at(0, 0, 140, 286), SIDE, 0);
            p.box_(at(6, 6, 128, 36), FLAT, 6);
            // GTK's bookmarks of the home folders (xdg-user-dirs-gtk-update's, at an Ubuntu
            // desktop's first login) follow Desktop after a separator, each named by its
            // path; the list runs on under the dialog's bottom bar, where GTK still lists
            // what it scrolls out of sight.
            let bookmarks: Vec<(String, &str)> =
                ["Documents", "Music", "Pictures", "Videos", "Downloads"]
                    .into_iter()
                    .filter(|name| files.has(name))
                    .map(|name| (files.folder(name), name))
                    .collect();
            let mut places: Vec<(i32, &str, String, &str)> = vec![
                (6, "Recent", "Recent files".into(), "clock"),
                (44, "Home", "Open your personal folder".into(), "home"),
                (
                    82,
                    "Desktop",
                    "Open the contents of your desktop in a folder".into(),
                    "desktop",
                ),
            ];
            for (k, (path, name)) in bookmarks.iter().enumerate() {
                places.push((133 + 38 * k as i32, name, path.clone(), "folder"));
            }
            let others = if bookmarks.is_empty() {
                133
            } else {
                133 + 38 * bookmarks.len() as i32 + 13
            };
            places.push((
                others,
                "Other Locations",
                "Show other locations".into(),
                "plus",
            ));
            for (i, (y, place, name, symbol)) in places.into_iter().enumerate() {
                // Recent is the place shown; the others would browse folders the chooser
                // does not model.
                off(
                    &mut p,
                    at(6, y, 128, 36),
                    &format!("place:{i}"),
                    "option",
                    &name,
                );
                p.select_last(i == 0);
                // Rows under the bottom bar are listed, not drawn.
                if y + 36 <= 286 {
                    p.symbol(symbol, o.x + 24, o.y + y + 10, 16, INK);
                    quiet(&mut p, 44, y + 8, 90, place, 13, INK, false);
                }
                said(&mut p, at(20, y, 24, 36), "img", place);
                said(&mut p, at(44, y, 76, 36), "label", place);
            }
            p.hline(o.x + 6, o.y + 126, 128, EDGE);
            if !bookmarks.is_empty() {
                p.hline(o.x + 6, o.y + others - 7, 128, EDGE);
            }
            p.box_(at(162, 17, 89, 34), FLAT, 6);
            off(&mut p, at(162, 17, 89, 34), "path", "button", "Recent");
            p.symbol("clock", o.x + 170, o.y + 26, 16, INK);
            said(&mut p, at(162, 17, 16, 24), "img", "");
            quiet(&mut p, 192, 23, 56, "Recent", 13, INK, true);
            p.symbol("grid-view", o.x + 799, o.y + 26, 16, INK);
            off(&mut p, at(790, 17, 34, 34), "grid", "button", "Grid View");
            for (x, width, column) in [
                (147, 33, "Name"),
                (688, 23, "Size"),
                (724, 26, "Type"),
                (763, 50, "Modified"),
            ] {
                quiet(&mut p, x, 52, 60, column, 11, MUTED, false);
                said(&mut p, at(x, 49, width, 24), "label", column);
            }
            p.hline(o.x + 141, o.y + 76, 678, EDGE);
            p.hline(o.x, o.y + 286, dw, EDGE);
            quiet(&mut p, 10, 295, 134, "Character Encoding:", 13, INK, false);
            said(&mut p, at(6, 286, 134, 34), "label", "Character Encoding");
            p.box_(at(146, 291, 204, 34), FLAT, 6);
            off(
                &mut p,
                at(146, 286, 204, 34),
                "encoding",
                "combobox",
                "Automatically Detected",
            );
            p.strong(o.x + 156, o.y + 300, 170, "Automatically Detected", 13, INK);
            p.symbol("chevron-down", o.x + 330, o.y + 302, 10, INK);
            p.box_(at(707, 291, 106, 34), FLAT, 6);
            off(
                &mut p,
                at(707, 286, 106, 34),
                "filter",
                "combobox",
                "Text Files",
            );
            p.strong(o.x + 717, o.y + 300, 70, "Text Files", 13, INK);
            p.symbol("chevron-down", o.x + 793, o.y + 302, 10, INK);
            p.hline(o.x, o.y + 335, dw, EDGE);
            button(
                &mut p,
                at(667, 343, 79, 34),
                Some("editor:dialog:cancel"),
                "Cancel",
                INK,
            );
            // Open is insensitive until a file is chosen.
            button(&mut p, at(752, 343, 72, 34), None, "Open", MUTED);
        }
        D::Print => {
            p.region(at(4, 0, 642, 37), "editor:dialog:tabs", "");
            p.announce("tablist", None);
            for (i, (x, tw, tab)) in [
                (20, 76, "General"),
                (104, 99, "Page Setup"),
                (211, 54, "Job"),
                (273, 116, "Image Quality"),
                (397, 61, "Color"),
                (466, 82, "Finishing"),
                (556, 90, "Advanced"),
            ]
            .into_iter()
            .enumerate()
            {
                let r = at(x, 3, tw, 37);
                if i == 0 {
                    // The page shown: choosing it again changes nothing.
                    p.region(r, "editor:dialog:tabs:general", tab);
                    p.announce("tab", None);
                    p.select_last(true);
                    p.box_(at(x, 37, tw - 16, 3), INK, 0);
                } else {
                    // The other pages' settings are not modelled.
                    off(&mut p, r, &format!("tab:{i}"), "tab", tab);
                    p.select_last(false);
                }
                p.left(
                    o.x + x,
                    o.y + 10,
                    tw,
                    tab,
                    13,
                    if i == 0 { INK } else { MUTED },
                );
            }
            p.border(at(12, 49, 618, 396), Color::WHITE, 0, EDGE);
            for (x, width, column) in [(60, 33, "Name"), (151, 49, "Location"), (213, 37, "Status")]
            {
                quiet(&mut p, x, 68, 120, column, 11, MUTED, false);
                said(&mut p, at(x, 65, width, 24), "label", column);
            }
            p.hline(o.x + 25, o.y + 92, 592, EDGE);
            // The one printer, Print to File, selected: its cells take a click and keep it so.
            p.box_(at(25, 100, 592, 34), Color(53, 132, 228, 40), 0);
            for (i, (x, width, name)) in [
                (31, 28, ""),
                (60, 91, "Print to File"),
                (151, 62, ""),
                (213, 435, ""),
            ]
            .into_iter()
            .enumerate()
            {
                p.region(
                    at(x, 100, width, 34),
                    &format!("editor:dialog:printer:{i}"),
                    name,
                );
                p.announce("cell", None);
            }
            p.symbol("document", o.x + 31, o.y + 109, 16, INK);
            said(&mut p, at(31, 100, 16, 18), "img", "");
            p.left(o.x + 60, o.y + 106, 91, "Print to File", 13, INK);
            quiet(&mut p, 24, 331, 60, "Range", 13, INK, true);
            said(&mut p, at(24, 331, 43, 18), "label", "Range");
            quiet(&mut p, 368, 331, 60, "Copies", 13, INK, true);
            said(&mut p, at(368, 331, 46, 18), "label", "Copies");
            // Pages to print and copies to make: the settings of a printer not simulated.
            p.box_(at(42, 363, 18, 18), BLUE, 9);
            off(
                &mut p,
                at(40, 359, 253, 28),
                "all-pages",
                "radio",
                "All Pages",
            );
            checked(&mut p, true);
            p.left(o.x + 64, o.y + 361, 221, "All Pages", 13, INK);
            p.box_(at(42, 397, 18, 18), FLAT, 9);
            off(
                &mut p,
                at(40, 393, 253, 28),
                "current-page",
                "radio",
                "C_urrent Page",
            );
            checked(&mut p, false);
            quiet(&mut p, 64, 393, 221, "Current Page", 13, MUTED, false);
            said(&mut p, at(64, 393, 221, 20), "label", "Current Page");
            p.box_(at(42, 434, 18, 18), FLAT, 4);
            off(&mut p, at(39, 426, 73, 34), "pages", "checkbox", "Pages");
            checked(&mut p, false);
            quiet(&mut p, 63, 432, 43, "Pages:", 13, INK, false);
            p.border(at(130, 423, 168, 34), Color::WHITE, 6, EDGE);
            off(&mut p, at(130, 423, 168, 34), "range", "textbox", "Pages");
            quiet(&mut p, 380, 362, 49, "Copies:", 13, INK, false);
            said(&mut p, at(380, 355, 49, 34), "label", "Copies");
            p.border(at(476, 355, 106, 34), Color::WHITE, 6, EDGE);
            p.insensitive(at(476, 355, 106, 34), "editor:dialog:off:copies", "Copies");
            p.announce("spinbutton", Some("1"));
            quiet(&mut p, 486, 362, 30, "1", 13, INK, false);
            // The page ordering preview: two sheets, numbered.
            said(
                &mut p,
                at(476, 395, 106, 62),
                "img",
                "Page ordering preview",
            );
            p.border(at(510, 403, 20, 28), Color::WHITE, 0, EDGE);
            p.border(at(493, 420, 20, 28), Color::WHITE, 0, EDGE);
            quiet(&mut p, 516, 416, 10, "2", 9, MUTED, false);
            said(&mut p, at(516, 419, 8, 15), "label", "2");
            quiet(&mut p, 499, 433, 10, "1", 9, MUTED, false);
            said(&mut p, at(499, 436, 8, 15), "label", "1");
            p.box_(at(386, 404, 18, 18), BLUE, 4);
            off(
                &mut p,
                at(384, 399, 84, 28),
                "collate",
                "checkbox",
                "C_ollate",
            );
            checked(&mut p, true);
            quiet(&mut p, 408, 401, 52, "Collate", 13, MUTED, false);
            said(&mut p, at(408, 399, 52, 20), "label", "Collate");
            p.box_(at(386, 438, 18, 18), FLAT, 4);
            off(
                &mut p,
                at(384, 433, 84, 28),
                "reverse",
                "checkbox",
                "Reverse",
            );
            checked(&mut p, false);
            p.left(o.x + 408, o.y + 435, 52, "Reverse", 13, INK);
            // Printing and its preview are not simulated: shown, insensitive.
            button(&mut p, at(406, 468, 88, 34), None, "Preview", MUTED);
            button(
                &mut p,
                at(500, 468, 79, 34),
                Some("editor:dialog:cancel"),
                "Cancel",
                INK,
            );
            button(&mut p, at(585, 468, 68, 34), None, "Print", MUTED);
        }
        D::SaveAs if theme == DesktopTheme::Ubuntu => {
            // GTK 4's file chooser in save mode, as GNOME Text Editor opens it for a draft:
            // the Name entry on a row of its own, then the places and the folder it saves
            // into (Home), then Cancel and Save. Its layout follows the Open chooser the
            // coverage tour measured; the save mode itself was not captured.
            quiet(&mut p, 160, 22, 50, "Name", 13, INK, false);
            said(&mut p, at(156, 15, 40, 34), "label", "Name");
            let name = at(204, 15, 420, 34);
            p.border(
                name,
                Color::WHITE,
                6,
                if field.is_empty() { EDGE } else { BLUE },
            );
            p.region(name, "editor:dialog:field", "Name");
            p.announce("textbox", Some(field));
            p.left(
                name.x + 10,
                name.y + 8,
                name.width.saturating_sub(20),
                field,
                13,
                INK,
            );
            p.presentation();
            let body = 46;
            p.hline(o.x, o.y + 58, dw, EDGE);
            p.box_(at(0, 58, 140, 286), SIDE, 0);
            let places: [(&str, String, &str); 4] = [
                ("Recent", "Recent files".into(), "clock"),
                ("Home", "Open your personal folder".into(), "home"),
                (
                    "Desktop",
                    "Open the contents of your desktop in a folder".into(),
                    "desktop",
                ),
                ("Other Locations", "Show other locations".into(), "plus"),
            ];
            for (i, (place, name, symbol)) in places.into_iter().enumerate() {
                let y = 6 + body + 12 + 38 * i as i32;
                // Home is the folder shown; the others would browse folders not modelled.
                if i == 1 {
                    p.box_(at(6, y, 128, 36), FLAT, 6);
                }
                off(
                    &mut p,
                    at(6, y, 128, 36),
                    &format!("place:{i}"),
                    "option",
                    &name,
                );
                p.select_last(i == 1);
                p.symbol(symbol, o.x + 24, o.y + y + 10, 16, INK);
                quiet(&mut p, 44, y + 8, 90, place, 13, INK, false);
            }
            p.box_(at(162, 17 + body + 12, 89, 34), FLAT, 6);
            off(
                &mut p,
                at(162, 17 + body + 12, 89, 34),
                "path",
                "button",
                "Home",
            );
            p.symbol("home", o.x + 170, o.y + 26 + body + 12, 16, INK);
            quiet(&mut p, 192, 23 + body + 12, 56, "Home", 13, INK, true);
            for (x, column) in [
                (147, "Name"),
                (688, "Size"),
                (724, "Type"),
                (763, "Modified"),
            ] {
                quiet(&mut p, x, 52 + body + 12, 60, column, 11, MUTED, false);
            }
            p.hline(o.x + 141, o.y + 76 + body + 12, 678, EDGE);
            let foot = dh as i32 - 87;
            p.hline(o.x, o.y + foot, dw, EDGE);
            button(
                &mut p,
                at(645, foot + 8, 79, 34),
                Some("editor:dialog:cancel"),
                "Cancel",
                INK,
            );
            // Save is insensitive until the Name entry holds a name.
            if field.trim().is_empty() {
                button(&mut p, at(730, foot + 8, 82, 34), None, "Save", MUTED);
            } else {
                p.box_(at(730, foot + 8, 82, 34), BLUE, 6);
                p.region(at(730, foot + 8, 82, 34), "editor:dialog:save", "Save");
                p.label(
                    o.x + 730,
                    o.y + foot + 15,
                    82,
                    "Save",
                    13,
                    Color::WHITE,
                    true,
                    Align::Center,
                );
            }
        }
        D::SaveAs => {
            // Explorer's file dialog: the folder it opens in on a bar across the top, a
            // navigation pane and the folder's items, then the File name and Save as type
            // fields above Save and Cancel.
            let (w, h) = (dw as i32, dh as i32);
            p.box_(at(0, 0, dw, 40), Color::rgb(243, 243, 243), 0);
            p.left(o.x + 12, o.y + 11, 200, "Save As", 12, INK);
            p.border(
                at(140, 48, (w - 152).max(1) as u32, 30),
                Color::WHITE,
                4,
                EDGE,
            );
            p.left(
                o.x + 152,
                o.y + 54,
                400,
                "This PC  \u{203a}  Documents",
                12,
                INK,
            );
            p.box_(
                at(0, 86, 132, (h - 196).max(1) as u32),
                Color::rgb(249, 249, 249),
                0,
            );
            for (i, place) in ["Home", "Desktop", "Downloads", "Documents", "Pictures"]
                .into_iter()
                .enumerate()
            {
                p.left(o.x + 16, o.y + 96 + i as i32 * 28, 110, place, 12, INK);
            }
            p.hline(o.x, o.y + h - 110, dw, EDGE);
            p.left(o.x + 24, o.y + h - 96, 100, "File name:", 12, INK);
            let name = at(130, h - 102, (w - 150).max(1) as u32, 28);
            p.border(name, Color::WHITE, 4, EDGE);
            // Windows 11's File name is an editable combo box.
            p.region(name, "editor:dialog:field", "File name:");
            p.announce("combo box", Some(field));
            p.left(
                name.x + 8,
                name.y + 6,
                name.width.saturating_sub(16),
                field,
                12,
                INK,
            );
            p.left(o.x + 24, o.y + h - 62, 100, "Save as type:", 12, INK);
            let kind = at(130, h - 68, (w - 150).max(1) as u32, 28);
            p.border(kind, Color::WHITE, 4, EDGE);
            p.region(kind, "editor:dialog:field", "Save as type:");
            p.announce("combo box", Some("Text documents (*.txt)"));
            p.left(
                kind.x + 8,
                kind.y + 6,
                300,
                "Text documents (*.txt)",
                12,
                INK,
            );
            button(
                &mut p,
                at(w - 196, h - 34, 88, 28),
                Some("editor:dialog:save"),
                "Save",
                INK,
            );
            button(
                &mut p,
                at(w - 100, h - 34, 88, 28),
                Some("editor:dialog:cancel"),
                "Cancel",
                INK,
            );
        }
        D::Unsaved if theme == DesktopTheme::Windows => {
            // Windows 11 Notepad's ContentDialog: its title, the question naming the tab, and
            // Save (the accent, default button), Don't save and Cancel on a footer band.
            let (w, h) = (dw as i32, dh as i32);
            p.border(o, Color::WHITE, 8, EDGE);
            p.strong(o.x + 24, o.y + 20, dw - 48, "Notepad", 20, INK);
            p.presentation();
            said(&mut p, at(24, 20, dw - 48, 28), "label", "Notepad");
            let question = format!("Do you want to save changes to {draft}?");
            p.left(o.x + 24, o.y + 62, dw - 48, &question, 14, INK);
            p.presentation();
            said(&mut p, at(24, 62, dw - 48, 20), "label", &question);
            p.box_(at(0, h - 80, dw, 80), Color::rgb(243, 243, 243), 0);
            p.hline(o.x, o.y + h - 80, dw, EDGE);
            let bw = ((w - 48 - 16) / 3).max(1) as u32;
            for (i, (action, label)) in [
                ("editor:dialog:save", "Save"),
                ("editor:dialog:discard", "Don't save"),
                ("editor:dialog:cancel", "Cancel"),
            ]
            .into_iter()
            .enumerate()
            {
                let r = at(24 + i as i32 * (bw as i32 + 8), h - 56, bw, 32);
                let (fill, ink) = if i == 0 {
                    (Color::rgb(0, 95, 184), Color::WHITE)
                } else {
                    (Color::WHITE, INK)
                };
                p.border(r, fill, 4, EDGE);
                p.region(r, action, label);
                p.label(r.x, r.y + 7, r.width, label, 14, ink, false, Align::Center);
            }
        }
        D::Unsaved => {
            p.label(
                o.x + 30,
                o.y + 24,
                283,
                "Save Changes?",
                15,
                INK,
                true,
                Align::Center,
            );
            p.presentation();
            said(&mut p, at(30, 24, 283, 25), "label", "Save Changes?");
            let body = [
                "Open documents contain unsaved changes.",
                "Changes which are not saved will be",
                "permanently lost.",
            ];
            for (i, line) in body.iter().enumerate() {
                p.label(
                    o.x + 30,
                    o.y + 60 + 18 * i as i32,
                    283,
                    line,
                    13,
                    INK,
                    false,
                    Align::Center,
                );
                p.presentation();
            }
            said(&mut p, at(30, 59, 283, 54), "label", &body.join(" "));
            let name = format!("{draft} (new)");
            p.border(at(44, 149, 259, 54), Color::WHITE, 8, EDGE);
            // Which documents to save: saving is not simulated, so neither is the choice.
            off(&mut p, at(44, 149, 259, 54), "choose", "option", &name);
            p.box_(at(64, 165, 18, 18), BLUE, 4);
            off(&mut p, at(59, 152, 26, 50), "save-draft", "checkbox", &name);
            checked(&mut p, true);
            p.left(o.x + 94, o.y + 155, 193, &name, 13, INK);
            quiet(&mut p, 94, 175, 193, "/", 11, MUTED, false);
            said(&mut p, at(94, 176, 193, 16), "label", "/");
            p.hline(o.x, o.y + 245, dw, EDGE);
            button(
                &mut p,
                at(14, 245, 115, 44),
                Some("editor:dialog:cancel"),
                "Cancel",
                INK,
            );
            button(
                &mut p,
                at(128, 245, 116, 44),
                Some("editor:dialog:discard"),
                "Discard",
                Color::rgb(192, 28, 40),
            );
            // Saving a draft asks where, in a chooser not simulated: shown, insensitive.
            button(&mut p, at(243, 245, 114, 44), None, "Save", MUTED);
        }
    }
    content.nodes.extend(p.scene.nodes);
    content.surfaces.extend(p.scene.surfaces);
}

pub fn app_content(state: &crate::AppState, theme: DesktopTheme, width: u32, height: u32) -> Scene {
    app_content_with(
        state,
        &crate::AppEnv {
            theme,
            width,
            height,
            clock_us: 0,
            settings: &crate::SystemSettings::DEFAULT,
            clipboard: None,
            share_to: None,
            editor: None,
            pointer: None,
            files: Default::default(),
        },
    )
}
/// The same projection, with the machine facts a native application is allowed to read.
pub fn app_content_with(state: &crate::AppState, env: &crate::AppEnv<'_>) -> Scene {
    app_content_scrolled(state, env, &crate::Scroll::default())
}
/// The projection of a window whose panes are scrolled to `scroll`.
pub fn app_content_scrolled(
    state: &crate::AppState,
    env: &crate::AppEnv<'_>,
    scroll: &crate::Scroll,
) -> Scene {
    let (theme, width, height) = (env.theme, env.width, env.height);
    let mut p = Painter::themed(theme, width, height, 1_u64 << 52);
    p.scroll = scroll.clone();
    match state {
        crate::AppState::Native(app) => app.render(&mut p, env),
        crate::AppState::Files { tabs, active } => files(&mut p, env, tabs, *active),
        crate::AppState::Terminal {
            input,
            prompt,
            transcript,
            cursor,
            scroll,
            ..
        } => terminal(
            &mut p, theme, width, height, input, prompt, transcript, *cursor, *scroll,
        ),
        crate::AppState::Editor {
            path,
            text,
            dirty,
            cursor,
            selected,
        } => editor(
            &mut p,
            theme,
            (width, height),
            path,
            text,
            *dirty,
            (*cursor, *selected),
            // A phone's editor always wraps; there is no horizontal scroll to fall back on.
            env.settings.word_wrap || theme.mobile(),
        ),
        // Browser windows are projected by `cw_browser::Browser::scene`, which owns the
        // page, its start page and its chrome; this arm only keeps the match total.
        crate::AppState::Browser { .. } => {}
    }
    // An engine application's declaration, written onto what it drew, as a native one's is.
    if !matches!(state, crate::AppState::Native(_)) {
        if let Some(d) = crate::apps::declared(state.kind()) {
            crate::apps::declare::annotate(d, &mut p.scene.nodes);
        }
    }
    let bounds = Rect::new(0, 0, width, height);
    let surfaces = std::mem::take(&mut p.scene.surfaces);
    for n in &mut p.scene.nodes {
        // A node of a surface of its own (a toolkit's dialog) is kept within that surface,
        // which may reach past the window; composition places it.
        let bounds = surfaces
            .iter()
            .rev()
            .find(|s| n.z >= s.z)
            .map_or(bounds, |s| s.rect);
        // An application's own clip (a scrolled list, a viewport) is kept, within the
        // window; a node clipped away entirely keeps an empty clip and paints nothing.
        n.clip = Some(match n.clip {
            Some(c) => c.intersection(bounds).unwrap_or(Rect::new(0, 0, 0, 0)),
            None => bounds,
        });
        if let Some(s) = &mut n.semantic {
            if n.interaction.as_deref().is_some_and(|i| {
                i.starts_with("editor-text") || i == "terminal-input" || i == "files-search"
            }) && s.role != "terminal"
            {
                s.role = "textbox".into();
            }
        }
    }
    p.scene.surfaces = surfaces;
    p.scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{caret_for_point, AppState, FileTab};

    fn actions(scene: &Scene) -> Vec<&str> {
        scene
            .nodes
            .iter()
            .filter_map(|n| n.interaction.as_deref())
            .collect()
    }
    /// Every string the scene paints, in the order it paints them.
    fn text_of(scene: &Scene) -> Vec<&str> {
        scene
            .nodes
            .iter()
            .filter_map(|n| match &n.primitive {
                Primitive::Text { text, .. }
                | Primitive::UiText { text, .. }
                | Primitive::UiTextBold { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
    fn unavailable(scene: &Scene) -> Vec<&str> {
        scene
            .nodes
            .iter()
            .filter_map(|n| n.semantic.as_ref())
            .filter(|s| s.disabled)
            .map(|s| s.label.as_str())
            .collect()
    }

    /// The painted text grid and `caret_for_point` must be the same 8x18 grid, or a
    /// click places the caret somewhere the user did not point at.
    #[test]
    fn editor_click_grid_agrees_with_the_caret_model() {
        let text = "hello world\nsecond line\nthird".to_owned();
        let scene = app_content(
            &AppState::Editor {
                path: "/note.txt".into(),
                text: text.clone(),
                cursor: 0,
                dirty: false,
                selected: false,
            },
            DesktopTheme::Macos,
            400,
            300,
        );
        let region = scene
            .nodes
            .iter()
            .find(|n| n.interaction.as_deref() == Some("editor-text:0"))
            .unwrap()
            .bounds;
        let (dx, dy) = (7 * 8 + 3, 18 + 9);
        assert_eq!(
            scene
                .hit_test(region.x + dx, region.y + dy)
                .unwrap()
                .interaction
                .as_deref(),
            Some("editor-text:0")
        );
        let cursor = caret_for_point(&text, 0, dx, dy);
        assert_eq!(&text[cursor..cursor + 1], "l");
        let placed = app_content(
            &AppState::Editor {
                path: "/note.txt".into(),
                text,
                cursor,
                dirty: false,
                selected: false,
            },
            DesktopTheme::Macos,
            400,
            300,
        );
        let caret = placed
            .nodes
            .iter()
            .find(|n| n.bounds.width == 1 && n.bounds.height == 18)
            .unwrap()
            .bounds;
        assert_eq!((caret.x, caret.y), (region.x + 7 * 8, region.y + 18));
    }

    #[test]
    fn editor_chrome_dispatches_real_shell_actions() {
        let state = AppState::Editor {
            path: "/note.txt".into(),
            text: "note".into(),
            cursor: 0,
            dirty: false,
            selected: false,
        };
        let notepad = app_content(&state, DesktopTheme::Windows, 620, 400);
        for id in ["shell:panel:file", "shell:panel:edit", "shell:panel:view"] {
            assert!(actions(&notepad).contains(&id), "menu bar lost {id}");
        }
        assert!(actions(&notepad).contains(&"shell:panel:app-settings"));
        assert!(unavailable(&notepad).contains(&"UTF-8"));
        // Notes' compose button opens another window of the focused application.
        let notes = app_content(&state, DesktopTheme::Ios, 390, 700);
        assert!(actions(&notes).contains(&"shell:new"));
        assert!(unavailable(&notes).contains(&"Insert photo"));
        // Desktop editors keep Save in their menus (Notepad's File menu, TextEdit's
        // menu bar, Text Editor's primary menu), so the document area paints none.
        for theme in [
            DesktopTheme::Macos,
            DesktopTheme::Windows,
            DesktopTheme::Ubuntu,
        ] {
            let scene = app_content(&state, theme, 620, 400);
            assert!(!actions(&scene).contains(&"editor-save"), "{theme:?}");
        }
        // Notepad's status bar counts what is really there: `\n` endings, 4 chars.
        assert!(unavailable(&notepad).contains(&"Unix (LF)"));
    }

    /// Nothing that cannot act may look clickable. A command that needs a selection or
    /// a clipboard is greyed until it has one, and Share — which has no recipient
    /// anywhere in this world — is greyed always.
    #[test]
    fn file_manager_commands_are_live_only_when_they_can_act() {
        let mut tab = FileTab::new("/work");
        tab.entries = vec!["notes.txt".into(), "invoices/".into()];
        let idle = AppState::Files {
            tabs: vec![tab.clone()],
            active: 0,
        };
        let explorer = app_content(&idle, DesktopTheme::Windows, 900, 520);
        // Nothing is selected and the clipboard is empty, so none of these can act.
        for label in ["Cut", "Copy", "Paste", "Rename", "Delete"] {
            assert!(
                unavailable(&explorer).contains(&label),
                "{label} looks live with nothing selected"
            );
        }
        // These never need one.
        for action in [
            "files-new-folder",
            "files-new-file",
            "files-search",
            "files-sort:name",
            "files-sort:kind",
            "files-view",
        ] {
            assert!(actions(&explorer).contains(&action), "missing {action}");
        }
        tab.selected = Some(0);
        let picked = AppState::Files {
            tabs: vec![tab],
            active: 0,
        };
        let clipboard = crate::Clipboard::new(vec!["/work/notes.txt".into()], false);
        let live = app_content_with(
            &picked,
            &crate::AppEnv {
                theme: DesktopTheme::Windows,
                width: 900,
                height: 520,
                clock_us: 0,
                settings: &crate::SystemSettings::DEFAULT,
                clipboard: Some(&clipboard),
                share_to: None,
                editor: None,
                pointer: None,
                files: Default::default(),
            },
        );
        for action in [
            "files-cut",
            "files-copy",
            "files-paste",
            "files-rename",
            "files-delete",
        ] {
            assert!(actions(&live).contains(&action), "missing {action}");
        }
        // Share has no target model at all, in either shell.
        assert!(unavailable(&live).contains(&"Share"));
        let ios = app_content(&idle, DesktopTheme::Ios, 390, 700);
        assert!(actions(&ios).contains(&"files-search"));
        assert!(actions(&ios).contains(&"files-recents"));
        assert!(unavailable(&ios).contains(&"Shared"));
        for scene in [&explorer, &live, &ios] {
            for n in &scene.nodes {
                if n.semantic.as_ref().is_some_and(|s| s.disabled) {
                    assert!(n.interaction.is_none());
                }
            }
        }
    }

    /// Every file manager can move a thing to the trash, and the Trash — and only the
    /// Trash — can put it back. Each control is live exactly when it can act, and a
    /// greyed one carries no interaction at all.
    #[test]
    fn every_file_manager_offers_the_trash_and_only_the_trash_offers_restore() {
        const TRASH: &str = "/home/alice/.local/share/Trash/files";
        let scene = |path: &str, selected: Option<usize>, theme, w, h| {
            let mut tab = crate::FileTab::new(path);
            tab.set_rows(vec![
                crate::FileRow::named("notes.txt"),
                crate::FileRow {
                    entry: "budget.txt".into(),
                    original: Some("/home/alice/Documents/budget.txt".into()),
                    ..crate::FileRow::named("budget.txt")
                },
            ]);
            tab.selected = selected;
            let state = AppState::Files {
                tabs: vec![tab],
                active: 0,
            };
            app_content_with(
                &state,
                &crate::AppEnv {
                    theme,
                    width: w,
                    height: h,
                    clock_us: 0,
                    settings: &crate::SystemSettings::DEFAULT,
                    clipboard: None,
                    share_to: None,
                    editor: None,
                    pointer: None,
                    files: crate::FilesEnv {
                        home: "/home/alice",
                        folders: vec![],
                        trash: TRASH.to_owned(),
                        starred: &[],
                    },
                },
            )
        };
        // Explorer keeps Delete in its command bar; Finder and the phones carry an
        // honestly named Move to Trash of their own. Files keeps it in the context menu.
        for (theme, w, h, label) in [
            (DesktopTheme::Windows, 900, 520, "Delete"),
            (DesktopTheme::Macos, 900, 520, "Move to Trash"),
            (DesktopTheme::Ios, 390, 700, "Move to Trash"),
            (DesktopTheme::Android, 390, 700, "Move to Trash"),
        ] {
            let idle = scene("/home/alice", None, theme, w, h);
            assert!(
                unavailable(&idle).contains(&label),
                "{theme:?}: {label} looks live with nothing selected"
            );
            let picked = scene("/home/alice", Some(0), theme, w, h);
            let action = if theme == DesktopTheme::Windows {
                "files-delete"
            } else {
                "files-move-to-trash"
            };
            assert!(
                actions(&picked).contains(&action),
                "{theme:?} has no way to move a file to the trash"
            );
            // Nothing offers a Restore outside the Trash, greyed or otherwise.
            assert!(
                !actions(&picked).contains(&"files-restore"),
                "{theme:?} offered Restore in a folder"
            );
            // In the Trash it is the other way round.
            let put_back = if theme == DesktopTheme::Macos || theme == DesktopTheme::Ios {
                "Put Back"
            } else if theme == DesktopTheme::Windows {
                "Restore the selected items"
            } else {
                "Restore"
            };
            let trash_idle = scene(TRASH, None, theme, w, h);
            assert!(
                unavailable(&trash_idle).contains(&put_back),
                "{theme:?}: {put_back} looks live with nothing selected"
            );
            assert!(
                !actions(&trash_idle).contains(&action),
                "{theme:?} offered a move to the trash from inside it"
            );
            let trash_picked = scene(TRASH, Some(1), theme, w, h);
            assert!(
                actions(&trash_picked).contains(&"files-restore"),
                "{theme:?} cannot put back what it is showing"
            );
            // The row says where it came from, so a Restore is not a leap of faith.
            assert!(
                text_of(&trash_picked)
                    .iter()
                    .any(|t| t.contains("Documents")),
                "{theme:?} does not say where the trashed file came from"
            );
            // And a disabled control is never clickable in any of these scenes.
            for scene in [&idle, &picked, &trash_idle, &trash_picked] {
                for n in &scene.nodes {
                    if n.semantic.as_ref().is_some_and(|s| s.disabled) {
                        assert!(n.interaction.is_none(), "{theme:?}");
                    }
                }
            }
        }
        // Emptying the trash is offered only where there is a trash to empty, and the
        // desktops that offer it name it.
        let full = scene(TRASH, None, DesktopTheme::Ubuntu, 900, 520);
        assert!(actions(&full).contains(&"files-empty-trash"));
        // Files shows Restore and Empty only in the Trash; Finder greys Empty elsewhere.
        let folder = scene("/home/alice", None, DesktopTheme::Ubuntu, 900, 520);
        assert!(!actions(&folder).contains(&"files-empty-trash"));
        let folder = scene("/home/alice", None, DesktopTheme::Macos, 900, 520);
        assert!(unavailable(&folder).contains(&"Empty Trash"));
    }

    /// The Size and Date columns show the listing's own numbers, not a placeholder.
    #[test]
    fn the_metadata_columns_draw_what_the_listing_carried() {
        let mut tab = crate::FileTab::new("/work");
        tab.set_rows(vec![
            crate::FileRow {
                entry: "report.txt".into(),
                kind: crate::EntryKind::File,
                size: Some(2048),
                mode: Some(0o644),
                modified: Some(0),
                original: None,
            },
            crate::FileRow {
                entry: "shortcut".into(),
                kind: crate::EntryKind::Symlink,
                size: Some(7),
                mode: Some(0o777),
                modified: Some(90 * 60 * 1_000_000),
                original: None,
            },
            // A row the machine said nothing about: unknown stays unknown.
            crate::FileRow::named("mystery.txt"),
        ]);
        let state = AppState::Files {
            tabs: vec![tab],
            active: 0,
        };
        let scene = app_content(&state, DesktopTheme::Windows, 900, 520);
        let text = text_of(&scene);
        let has = |want: &str| text.contains(&want);
        assert!(has("2 KB"), "{text:?}");
        assert!(has("7 bytes"), "{text:?}");
        // The world clock's own epoch, and an hour and a half past it.
        assert!(has("2026-09-17 09:00"), "{text:?}");
        assert!(has("2026-09-17 10:30"), "{text:?}");
        // Kind comes from what the filesystem said, not from the name.
        assert!(has("Shortcut"), "{text:?}");
        assert!(has("Text Document"), "{text:?}");
        // Unknown is drawn as unknown, twice: no size and no date.
        assert_eq!(
            text.iter().filter(|t| **t == "\u{2014}").count(),
            2,
            "{text:?}"
        );
        // And the columns are sort controls.
        for action in ["files-sort:size", "files-sort:modified"] {
            assert!(actions(&scene).contains(&action), "missing {action}");
        }
        assert_eq!(size_label(1), "1 byte");
        assert_eq!(size_label(1023), "1023 bytes");
        assert_eq!(size_label(1536), "1.5 KB");
        assert_eq!(size_label(1024 * 1024 * 3), "3 MB");
    }

    /// Wide characters take two cells in the transcript's rows and under the caret,
    /// exactly as `Primitive::Text` draws them.
    #[test]
    fn terminal_rows_and_caret_count_cells_not_characters() {
        let output = "日本語のテキストが長く続くと折り返されます";
        let state = AppState::Terminal {
            input: "echo 中文".into(),
            prompt: "me@box:/$".into(),
            transcript: vec![crate::TerminalEntry::new("me@box:/$", "cat", output, "", 0)],
            history: vec![],
            cursor: "echo 中文".len(),
            scroll: 0,
        };
        let scene = app_content(&state, DesktopTheme::Ubuntu, 16 + 8 * 20, 400);
        let rows: Vec<&str> = scene
            .nodes
            .iter()
            .filter_map(|n| match &n.primitive {
                Primitive::Text { text, .. } if !text.starts_with("me@") => Some(text.as_str()),
                _ => None,
            })
            .filter(|t| t.chars().any(|c| c > '\u{3000}'))
            .collect();
        // Twenty cells hold ten ideographs.
        assert_eq!(rows[0], "日本語のテキストが長");
        assert!(rows
            .iter()
            .all(|r| cw_scene::text::terminal::columns(r) <= 20));
        // "echo 中文" is nine cells, so the caret sits nine cells past the prompt.
        assert_eq!(cw_scene::text::terminal::columns("echo 中文"), 9);
        assert_eq!(crate::caret_for_column("echo 中文", 7 * 8), "echo 中".len());
        assert_eq!(
            crate::caret_for_column("echo 中文", 9 * 8),
            "echo 中文".len()
        );
        // The editor soft-wraps and places its caret on the same cells.
        let text = "中文中文中文";
        assert_eq!(crate::editor_rows(text, 5), [(0, 6), (6, 12), (12, 18)]);
        assert_eq!(crate::editor_caret_cell(text, 9, 5), (1, 2));
        assert_eq!(crate::caret_for_point_wrapped(text, 0, 5, 2 * 8, 18), 9);
        assert_eq!(crate::editor_rows("e\u{301}xyz", 3), [(0, 5), (5, 6)]);
    }
    /// The scrollbar really scrolls, and the caret really sits where `cursor` says.
    #[test]
    fn terminal_scrollbar_pages_and_the_caret_follows_the_cursor() {
        let transcript = (0..60)
            .map(|i| crate::TerminalEntry::new("me@box:/$", "ls", &format!("line {i}"), "", 0))
            .collect::<Vec<_>>();
        let tail = AppState::Terminal {
            input: "ls".into(),
            prompt: "me@box:/$".into(),
            transcript: transcript.clone(),
            history: vec![],
            cursor: 2,
            scroll: 0,
        };
        let scene = app_content(&tail, DesktopTheme::Ubuntu, 600, 240);
        // Pinned to the tail: only "scroll back" is offered, and it names a real target.
        let back: Vec<_> = actions(&scene)
            .into_iter()
            .filter(|a| a.starts_with("terminal-scroll:"))
            .collect();
        assert_eq!(back.len(), 1);
        let lines: usize = back[0]
            .trim_start_matches("terminal-scroll:")
            .parse()
            .unwrap();
        assert!(lines > 0);
        let text = |scene: &Scene, want: &str| {
            scene
                .nodes
                .iter()
                .any(|n| matches!(&n.primitive, Primitive::Text { text, .. } if text == want))
        };
        assert!(text(&scene, "line 59"));
        let lifted = app_content(
            &AppState::Terminal {
                input: "ls".into(),
                prompt: "me@box:/$".into(),
                transcript,
                history: vec![],
                cursor: 2,
                scroll: lines,
            },
            DesktopTheme::Ubuntu,
            600,
            240,
        );
        // Scrolled back: earlier output is on screen and both directions are offered.
        assert!(!text(&lifted, "line 59"));
        assert_eq!(
            actions(&lifted)
                .into_iter()
                .filter(|a| a.starts_with("terminal-scroll:"))
                .count(),
            2
        );
        // The prompt line is its own target, and `click_at` reads a column off it.
        assert!(actions(&scene).contains(&"terminal-line"));
        let mut desktop = crate::DesktopState::default();
        let (id, _) = desktop.launch("terminal", "").unwrap();
        desktop.text("hello").unwrap();
        desktop.click_at("terminal-line", 8, 0).unwrap();
        desktop.text("X").unwrap();
        match &desktop.windows[&id].state {
            AppState::Terminal { input, cursor, .. } => {
                assert_eq!(input, "hXello");
                assert_eq!(*cursor, 2);
            }
            _ => panic!("not a terminal"),
        }
    }
}
