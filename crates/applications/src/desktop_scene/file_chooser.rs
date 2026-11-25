//! GTK's file chooser ([`crate::FileChooser`]) painted over the window that opened it, as a
//! surface of its own (a transient dialog the desktop places in the work area). Every
//! name, role and bound is the one GTK announces in the explorer's Ubuntu references
//! (store the Ubuntu reference store): GTK 3's from Image Viewer's Open Image and LibreOffice's Open, GTK 4's
//! from Maps' Open Shape Layer and Text Editor's Open File. Actions are `chooser:*`, which
//! `DesktopState::click` takes; a control with no model behind it is listed insensitive.
use super::shared::{Align, Painter};
use super::DesktopTheme;
use crate::chooser::{modified_label, size_label, type_label, ChooserColumn, FileChooser, Gtk};
use crate::{FileRow, FilesEnv};
use cw_scene::{Color, Rect, Scene};

/// The chooser's colours: GTK's light theme, or the dark one an application that prefers
/// it (Image Viewer) gives its dialogs, as the references' frames show it (ground 44, list
/// 39, buttons 55, the selection in Yaru's orange).
struct Palette {
    ink: Color,
    muted: Color,
    pane: Color,
    side: Color,
    list: Color,
    edge: Color,
    flat: Color,
    picked: Color,
}
const LIGHT: Palette = Palette {
    ink: Color::rgb(29, 29, 31),
    muted: Color::rgb(112, 114, 120),
    pane: Color::rgb(250, 250, 250),
    side: Color::rgb(235, 235, 235),
    list: Color::WHITE,
    edge: Color(0, 0, 0, 36),
    flat: Color(0, 0, 0, 20),
    picked: Color(53, 132, 228, 48),
};
const DARK: Palette = Palette {
    ink: Color::rgb(245, 245, 245),
    muted: Color::rgb(190, 190, 190),
    pane: Color::rgb(44, 44, 44),
    side: Color::rgb(44, 44, 44),
    list: Color::rgb(39, 39, 39),
    edge: Color(255, 255, 255, 26),
    flat: Color::rgb(55, 55, 55),
    picked: Color::rgb(233, 84, 32),
};
/// The home folders GTK's places list as bookmarks (xdg-user-dirs-gtk-update's, at an
/// Ubuntu desktop's first login), in its order.
const BOOKMARKS: [&str; 5] = ["Documents", "Music", "Pictures", "Videos", "Downloads"];

/// What the painter needs beyond the chooser: the machine's home folders, what was opened
/// lately (Recent), and the world clock the Modified column is read against.
pub struct ChooserEnv<'a> {
    pub files: &'a FilesEnv<'a>,
    pub recents: &'a [String],
    pub now: u64,
}

/// One place of the sidebar: its label, the name GTK 4 gives its row (GTK 3's rows are
/// unnamed), its symbol, the action, and whether it is the place shown.
struct Place {
    label: String,
    name: String,
    symbol: &'static str,
    action: String,
    on: bool,
    /// A rule above it: the bookmarks, and Other Locations.
    rule: bool,
}

fn places(c: &FileChooser, files: &FilesEnv<'_>) -> Vec<Place> {
    let home = files.home.trim_end_matches('/');
    let shown = c.folder.as_deref();
    let go = |path: &str| format!("chooser:go:{path}");
    let mut out = vec![
        Place {
            label: "Recent".into(),
            name: "Recent files".into(),
            symbol: "clock",
            action: "chooser:place:recent".into(),
            on: shown.is_none(),
            rule: false,
        },
        Place {
            label: "Home".into(),
            name: "Open your personal folder".into(),
            symbol: "home",
            action: go(home),
            on: shown == Some(home),
            rule: false,
        },
        Place {
            label: "Desktop".into(),
            name: "Open the contents of your desktop in a folder".into(),
            symbol: "desktop",
            action: go(&files.folder("Desktop")),
            on: shown == Some(files.folder("Desktop").as_str()),
            rule: false,
        },
    ];
    if c.gtk == Gtk::Three && !files.trash.is_empty() {
        out.push(Place {
            label: "Trash".into(),
            name: String::new(),
            symbol: "trash",
            action: go(&files.trash),
            on: shown == Some(files.trash.trim_end_matches('/')),
            rule: false,
        });
    }
    let mut first = true;
    for name in BOOKMARKS.into_iter().filter(|n| files.has(n)) {
        let path = files.folder(name);
        out.push(Place {
            label: name.into(),
            name: path.clone(),
            symbol: "folder",
            on: shown == Some(path.as_str()),
            action: go(&path),
            rule: std::mem::take(&mut first),
        });
    }
    out.push(Place {
        label: "Other Locations".into(),
        name: "Show other locations".into(),
        symbol: "plus",
        // Other Locations browses the machine's mounts, which the chooser does not model.
        action: String::new(),
        on: false,
        rule: true,
    });
    out
}

/// The path bar's crumbs from home (or from `/` outside it) to the folder shown: each its
/// label and the folder it opens.
fn crumbs(folder: &str, home: &str) -> Vec<(String, String)> {
    let home = home.trim_end_matches('/');
    let (mut path, rest) =
        if !home.is_empty() && (folder == home || folder.starts_with(&format!("{home}/"))) {
            (home.to_owned(), &folder[home.len()..])
        } else {
            (String::new(), folder)
        };
    let mut out = vec![if path.is_empty() {
        ("/".to_owned(), "/".to_owned())
    } else {
        (
            path.rsplit('/').next().unwrap_or("").to_owned(),
            path.clone(),
        )
    }];
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        path = format!("{}/{part}", path.trim_end_matches('/'));
        out.push((part.to_owned(), path.clone()));
    }
    out
}

/// The rows shown: the folder's listing, or Recent's paths with what the desktop knows.
fn rows(c: &FileChooser, env: &ChooserEnv<'_>) -> Vec<FileRow> {
    match &c.folder {
        Some(_) => c.rows.clone(),
        None => env
            .recents
            .iter()
            .map(FileRow::named)
            .filter(|r| c.admits(r))
            .collect(),
    }
}

/// Paint `c` over a window's `content`.
pub fn file_chooser(
    content: &mut Scene,
    theme: DesktopTheme,
    c: &FileChooser,
    env: &ChooserEnv<'_>,
) {
    let (w, h) = (content.width as i32, content.height as i32);
    let (dw, dh) = match c.gtk {
        Gtk::Three => (1205, 768),
        Gtk::Four => (819, 384),
    };
    // GTK 3's fills the work area (the desktop moves it there); GTK 4's centres on the
    // window under its 46 px header bar, 7 px low.
    let o = match c.gtk {
        Gtk::Three => Rect::new((w - dw as i32) / 2, (h - dh as i32) / 2, dw, dh),
        Gtk::Four => Rect::new(
            (w - dw as i32) / 2,
            (h + 46 - dh as i32) / 2 - 46 + 7,
            dw,
            dh,
        ),
    };
    let mut p = Painter::themed(theme, content.width, content.height, 5_u64 << 52);
    p.z = content.nodes.iter().map(|n| n.z).max().unwrap_or(0) + 10;
    p.scene.surfaces.push(cw_scene::Surface { rect: o, z: p.z });
    let pal = if c.dark { &DARK } else { &LIGHT };
    p.box_(o, pal.pane, 0);
    p.region(o, "chooser:frame", &c.title);
    p.announce("dialog", None);
    match c.gtk {
        Gtk::Three => gtk3(&mut p, pal, o, c, env),
        Gtk::Four => gtk4(&mut p, pal, o, c, env),
    }
    let painted = p.scene;
    content.nodes.extend(painted.nodes);
    content.surfaces.extend(painted.surfaces);
}

fn button(p: &mut Painter, pal: &Palette, r: Rect, action: Option<&str>, label: &str) {
    p.box_(r, pal.flat, 6);
    match action {
        Some(a) => p.region(r, a, label),
        None => p.insensitive(r, &format!("chooser:off:{label}"), label),
    }
    p.announce("button", None);
    let ink = if action.is_some() { pal.ink } else { pal.muted };
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
    p.presentation();
}

/// A place row: a control (named as GTK 4 names it; GTK 3's are unnamed), its image and
/// its label, each announced at GTK's bounds. A `hidden` row is one GTK 4's sidebar has
/// scrolled under the dialog's bottom bar: listed, not drawn, and a click at it lands on
/// the bar and does nothing.
fn place(p: &mut Painter, pal: &Palette, r: Rect, pl: &Place, gtk: Gtk, hidden: bool) {
    // GTK 3's rows are unnamed and named by the label inside them, as normalization reads
    // the reference; GTK 4 names them by what they do.
    let name = match gtk {
        Gtk::Three => pl.label.as_str(),
        Gtk::Four => pl.name.as_str(),
    };
    if pl.on && !hidden {
        p.box_(r, pal.picked, 6);
    }
    if hidden {
        p.region(r, &format!("chooser:hidden:{}", pl.label), name);
    } else if pl.action.is_empty() {
        p.insensitive(r, "chooser:off:other", name);
    } else {
        p.region(r, &pl.action, name);
    }
    p.announce("option", None);
    p.select_last(pl.on);
    let (ix, lx, lw) = match gtk {
        Gtk::Three => (14, 38, 108),
        Gtk::Four => (14, 38, 76),
    };
    if !hidden {
        p.symbol(pl.symbol, r.x + ix + 4, r.y + 10, 16, pal.ink);
    }
    let image = match (gtk, pl.label.as_str()) {
        (Gtk::Three, "Other Locations") => "Add",
        (Gtk::Three, _) => "",
        (Gtk::Four, l) => l,
    };
    p.container(Rect::new(r.x + ix, r.y, 24, 36), "img", image);
    if !hidden {
        p.label(
            r.x + lx,
            r.y + 8,
            lw,
            &pl.label,
            13,
            pal.ink,
            false,
            Align::Left,
        );
        p.presentation();
    }
    p.container(Rect::new(r.x + lx, r.y, lw, 36), "label", &pl.label);
}

fn gtk3(p: &mut Painter, pal: &Palette, o: Rect, c: &FileChooser, env: &ChooserEnv<'_>) {
    let at = |x: i32, y: i32, w: u32, h: u32| Rect::new(o.x + x, o.y + y, w, h);
    let dw = o.width;
    // The title bar the window manager draws, then the file chooser widget under it.
    p.box_(at(0, 0, dw, 20), pal.side, 0);
    p.label(o.x, o.y + 1, dw, &c.title, 12, pal.ink, true, Align::Center);
    p.presentation();
    // GTK 3 announces the chooser widget as a dialog of its own inside the dialog.
    p.region(
        at(1, 20, dw - 2, 697),
        "chooser:widget",
        "File Chooser Widget",
    );
    p.announce("dialog", None);
    p.box_(at(1, 20, 160, 650), pal.side, 0);
    p.container(at(1, 20, 160, 650), "panel", "Places");
    let mut y = 24;
    for pl in places(c, env.files) {
        if pl.rule {
            y += 9;
            p.hline(o.x + 1, o.y + y - 5, 160, pal.edge);
        }
        place(p, pal, at(1, y, 160, 36), &pl, Gtk::Three, false);
        y += 38;
    }
    // The path bar: Up Path, a crumb per folder from home, Down Path (insensitive at the
    // path's end, which is where the chooser always is).
    if let Some(folder) = c.folder.as_deref() {
        match c.up() {
            Some(_) => p.region(at(168, 26, 26, 34), "chooser:up", "Up Path"),
            None => p.insensitive(at(168, 26, 26, 34), "chooser:off:up", "Up Path"),
        }
        p.announce("button", None);
        p.symbol("chevron-left", o.x + 173, o.y + 35, 16, pal.ink);
        let mut x = 194;
        let trail = crumbs(folder, env.files.home);
        let last = trail.len() - 1;
        for (i, (label, path)) in trail.iter().enumerate() {
            let home = i == 0 && path != "/";
            let bw = p.measure(label, 13, i == last) + if home { 42 } else { 30 };
            let r = at(x, 26, bw, 34);
            if i == last {
                p.box_(r, pal.flat, 0);
            }
            p.region(r, &format!("chooser:crumb:{path}"), label);
            p.announce("button", None);
            if home {
                p.symbol("home", r.x + 8, r.y + 9, 16, pal.ink);
            }
            p.label(
                r.x + if home { 30 } else { 12 },
                r.y + 8,
                bw,
                label,
                13,
                pal.ink,
                i == last,
                Align::Left,
            );
            p.presentation();
            x += bw as i32;
        }
        p.insensitive(at(x, 26, 26, 34), "chooser:off:down", "Down Path");
        p.announce("button", None);
        p.symbol("chevron-right", o.x + x + 5, o.y + 35, 16, pal.muted);
    }
    // The listing: a table of column headers and a row per entry, each row's cells as GTK
    // 3 announces them (the row, its icon, its name, size, type, the Modified cell and its
    // text).
    let tw = dw - 163;
    p.box_(at(162, 67, tw, 603), pal.list, 0);
    p.container(at(162, 67, tw, 603), "table", "Files");
    let cols = [
        (ChooserColumn::Size, 70),
        (ChooserColumn::Type, 75),
        (ChooserColumn::Modified, 90),
    ];
    let name_w = tw - cols.iter().map(|(_, w)| *w).sum::<u32>();
    let mut x = 162 + name_w as i32;
    let mut col_x = vec![(ChooserColumn::Name, 162, name_w)];
    for (col, cw) in cols {
        col_x.push((col, x, cw));
        x += cw as i32;
    }
    for (col, x, cw) in &col_x {
        p.region(
            at(*x, 67, *cw, 25),
            &format!("chooser:sort:{}", col.label()),
            col.label(),
        );
        p.announce("columnheader", None);
        p.label(
            o.x + x + 6,
            o.y + 70,
            *cw,
            col.label(),
            12,
            pal.muted,
            true,
            Align::Left,
        );
        p.presentation();
    }
    p.hline(o.x + 162, o.y + 92, tw, pal.edge);
    for (k, row) in rows(c, env).iter().enumerate() {
        let y = 93 + 24 * k as i32;
        if y + 22 > 67 + 603 {
            break;
        }
        let on = c.selected.as_deref() == Some(row.entry.as_str());
        if on {
            p.box_(at(164, y, tw - 4, 22), pal.picked, 0);
        }
        let act = format!("chooser:row:{}", row.entry);
        let cell = |p: &mut Painter, r: Rect, name: &str| {
            p.region(r, &act, name);
            p.announce("cell", None);
            p.select_last(on);
        };
        cell(p, at(164, y, name_w - 4, 22), "");
        cell(p, at(164, y, 28, 22), "");
        p.symbol(
            if row.kind.is_dir() {
                "folder"
            } else {
                "document"
            },
            o.x + 170,
            o.y + y + 3,
            16,
            pal.ink,
        );
        cell(p, at(192, y, name_w - 28, 22), row.name());
        p.label(
            o.x + 194,
            o.y + y + 2,
            name_w - 32,
            row.name(),
            13,
            pal.ink,
            false,
            Align::Left,
        );
        p.presentation();
        let size = size_label(row);
        let modified = match c.folder {
            Some(_) => modified_label(row.modified, env.now),
            None => String::new(),
        };
        for (i, text) in [size.as_str(), type_label(row), "", modified.as_str()]
            .into_iter()
            .enumerate()
        {
            let (_, cx, cw) = col_x[(i + 1).min(3)];
            let r = at(cx + 2, y, cw.saturating_sub(4), 22);
            cell(p, r, text);
            if !text.is_empty() {
                p.label(r.x + 2, r.y + 2, cw, text, 13, pal.ink, false, Align::Left);
                p.presentation();
            }
        }
    }
    // The filter, then Cancel and Open.
    let fw = if c.filter_width > 0 {
        c.filter_width
    } else {
        186
    };
    let fr = at(dw as i32 - 7 - fw as i32, 677, fw, 34);
    p.box_(fr, pal.flat, 6);
    p.insensitive(fr, "chooser:off:filter", &c.filter);
    p.announce("combobox", None);
    p.label(
        fr.x + 10,
        fr.y + 8,
        fw - 30,
        &c.filter,
        13,
        pal.ink,
        false,
        Align::Left,
    );
    p.presentation();
    p.symbol(
        "chevron-down",
        fr.x + fw as i32 - 20,
        fr.y + 12,
        10,
        pal.ink,
    );
    button(
        p,
        pal,
        at(dw as i32 - 183, 723, 86, 34),
        Some("chooser:cancel"),
        "Cancel",
    );
    button(
        p,
        pal,
        at(dw as i32 - 93, 723, 86, 34),
        c.can_open().then_some("chooser:open"),
        "Open",
    );
}

fn gtk4(p: &mut Painter, pal: &Palette, o: Rect, c: &FileChooser, env: &ChooserEnv<'_>) {
    let at = |x: i32, y: i32, w: u32, h: u32| Rect::new(o.x + x, o.y + y, w, h);
    let dw = o.width;
    p.box_(at(0, 0, 140, 286), pal.side, 0);
    p.container(at(0, 6, 140, 380), "list", "");
    let mut y = 6;
    for pl in places(c, env.files) {
        if pl.rule {
            y += 13;
            p.hline(o.x + 6, o.y + y - 7, 128, pal.edge);
        }
        // The sidebar ends at the bottom bar (286 px down).
        place(p, pal, at(6, y, 128, 36), &pl, Gtk::Four, y + 36 > 286);
        y += 38;
    }
    // The path bar: Recent's one button, or a crumb per folder from home.
    match c.folder.as_deref() {
        None => {
            p.box_(at(162, 17, 89, 34), pal.flat, 6);
            p.region(at(162, 17, 89, 34), "chooser:place:recent", "");
            p.announce("button", None);
            p.symbol("clock", o.x + 162, o.y + 26, 16, pal.ink);
            p.container(at(162, 17, 16, 24), "img", "");
            p.label(
                o.x + 184,
                o.y + 23,
                56,
                "Recent",
                13,
                pal.ink,
                true,
                Align::Left,
            );
            p.presentation();
            p.container(at(184, 17, 51, 24), "label", "Recent");
        }
        Some(folder) => {
            let mut x = 163;
            let trail = crumbs(folder, env.files.home);
            let last = trail.len() - 1;
            for (i, (label, path)) in trail.iter().enumerate() {
                let home = i == 0 && path != "/";
                let lw = p.measure(label, 13, i == last);
                let bw = lw + if home { 38 } else { 26 };
                let r = at(x, 17, bw, 34);
                if i == last {
                    p.box_(r, pal.flat, 6);
                }
                p.region(r, &format!("chooser:crumb:{path}"), "");
                p.announce("button", None);
                let tx = if home {
                    p.symbol("home", r.x, r.y + 9, 16, pal.ink);
                    p.container(Rect::new(r.x, r.y, 16, 24), "img", "");
                    r.x + 22
                } else {
                    r.x + 12
                };
                p.label(
                    tx,
                    r.y + 6,
                    lw + 4,
                    label,
                    13,
                    pal.ink,
                    i == last,
                    Align::Left,
                );
                p.presentation();
                p.container(Rect::new(tx, r.y, lw, 24), "label", label);
                x += bw as i32;
            }
        }
    }
    let view = if c.grid { "List View" } else { "Grid View" };
    p.region(at(790, 17, 34, 34), "chooser:grid", view);
    p.announce("button", None);
    p.symbol(
        if c.grid { "list-view" } else { "grid-view" },
        o.x + 799,
        o.y + 26,
        16,
        pal.ink,
    );
    let shown = rows(c, env);
    if c.grid {
        p.container(at(157, 61, 678, 234), "table", "");
        for (k, row) in shown.iter().enumerate() {
            let (cx, cy) = (160 + 162 * (k as i32 % 4), 64 + 138 * (k as i32 / 4));
            if cy > 61 + 234 {
                break;
            }
            let on = c.selected.as_deref() == Some(row.entry.as_str());
            let r = at(cx, cy, 156, 132);
            if on {
                p.box_(r, pal.picked, 6);
            }
            p.region(r, &format!("chooser:row:{}", row.entry), row.name());
            p.announce("cell", None);
            p.select_last(on);
            p.symbol(
                if row.kind.is_dir() {
                    "folder"
                } else {
                    "document"
                },
                r.x + 54,
                r.y + 30,
                48,
                pal.ink,
            );
            p.container(Rect::new(r.x + 18, r.y + 6, 120, 96), "img", "");
            p.label(
                r.x + 12,
                r.y + 108,
                132,
                row.name(),
                12,
                pal.ink,
                false,
                Align::Center,
            );
            p.presentation();
            p.container(Rect::new(r.x + 12, r.y + 108, 132, 18), "label", row.name());
        }
    } else {
        p.container(at(141, 46, 678, 30), "row", "Name Size Type Modified");
        let cols = [
            (ChooserColumn::Name, 147, 541),
            (ChooserColumn::Size, 688, 36),
            (ChooserColumn::Type, 724, 39),
            (ChooserColumn::Modified, 763, 56),
        ];
        for (col, x, cw) in cols {
            p.region(
                at(x, 49, cw, 30),
                &format!("chooser:sort:{}", col.label()),
                col.label(),
            );
            p.announce("panel", None);
            p.label(
                o.x + x,
                o.y + 52,
                cw + 20,
                col.label(),
                11,
                pal.muted,
                false,
                Align::Left,
            );
            p.presentation();
            p.container(
                at(x, 49, p.measure(col.label(), 11, false), 24),
                "label",
                col.label(),
            );
        }
        p.hline(o.x + 141, o.y + 76, 678, pal.edge);
        p.container(at(141, 76, 678, 204), "list", "");
        for (k, row) in shown.iter().enumerate() {
            let y = 76 + 34 * k as i32;
            if y + 34 > 280 {
                break;
            }
            let on = c.selected.as_deref() == Some(row.entry.as_str());
            if on {
                p.box_(at(141, y, 678, 34), pal.picked, 0);
            }
            let size = size_label(row);
            let modified = match c.folder {
                Some(_) => modified_label(row.modified, env.now),
                None => String::new(),
            };
            for (i, (text, (_, x, cw))) in [
                row.name(),
                size.as_str(),
                type_label(row),
                modified.as_str(),
            ]
            .into_iter()
            .zip(cols)
            .enumerate()
            {
                let r = at(x, y, cw, 34);
                p.region(r, &format!("chooser:row:{}", row.entry), text);
                p.announce("cell", None);
                p.select_last(on);
                if i == 0 {
                    p.symbol(
                        if row.kind.is_dir() {
                            "folder"
                        } else {
                            "document"
                        },
                        r.x,
                        r.y + 9,
                        16,
                        pal.ink,
                    );
                }
                p.label(
                    r.x + if i == 0 { 24 } else { 0 },
                    r.y + 8,
                    cw + 20,
                    text,
                    13,
                    pal.ink,
                    false,
                    Align::Left,
                );
                p.presentation();
            }
        }
    }
    p.hline(o.x, o.y + 286, dw, pal.edge);
    if c.encoding {
        p.label(
            o.x + 10,
            o.y + 295,
            134,
            "Character Encoding:",
            13,
            pal.ink,
            false,
            Align::Left,
        );
        p.presentation();
        p.container(at(6, 286, 134, 34), "label", "Character Encoding");
        p.box_(at(146, 291, 204, 34), pal.flat, 6);
        p.insensitive(
            at(146, 286, 204, 34),
            "chooser:off:encoding",
            "Automatically Detected",
        );
        p.announce("combobox", None);
        p.strong(
            o.x + 156,
            o.y + 300,
            170,
            "Automatically Detected",
            13,
            pal.ink,
        );
        p.presentation();
        p.symbol("chevron-down", o.x + 330, o.y + 302, 10, pal.ink);
    }
    let fw = if c.filter_width > 0 {
        c.filter_width
    } else {
        p.measure(&c.filter, 13, true) + 42
    };
    let fr = at(814 - fw as i32, 286, fw, 34);
    p.box_(at(fr.x - o.x, 291, fw, 34), pal.flat, 6);
    p.insensitive(fr, "chooser:off:filter", &c.filter);
    p.announce("combobox", None);
    p.strong(fr.x + 10, o.y + 300, fw - 30, &c.filter, 13, pal.ink);
    p.presentation();
    p.symbol(
        "chevron-down",
        fr.x + fw as i32 - 20,
        o.y + 302,
        10,
        pal.ink,
    );
    p.hline(o.x, o.y + 335, dw, pal.edge);
    button(
        p,
        pal,
        at(667, 343, 79, 34),
        Some("chooser:cancel"),
        "Cancel",
    );
    button(
        p,
        pal,
        at(752, 343, 72, 34),
        c.can_open().then_some("chooser:open"),
        "Open",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk4s_places_under_its_bottom_bar_are_listed_and_take_no_click() {
        let files = FilesEnv {
            home: "/home/ada",
            folders: [
                "Desktop",
                "Documents",
                "Music",
                "Pictures",
                "Videos",
                "Downloads",
            ]
            .map(String::from)
            .to_vec(),
            trash: String::new(),
            starred: &[],
        };
        let mut c = FileChooser::for_kind("editor", "/home/ada").unwrap();
        let mut scene = Scene::new(700, 474);
        let env = ChooserEnv {
            files: &files,
            recents: &[],
            now: 0,
        };
        file_chooser(&mut scene, DesktopTheme::Ubuntu, &c, &env);
        let action = |name: &str| {
            scene
                .nodes
                .iter()
                .find(|n| n.semantic.as_ref().is_some_and(|s| s.label == name))
                .and_then(|n| n.interaction.clone())
                .unwrap()
        };
        // Videos ends 283 px down, above the bar; Downloads and Other Locations lie under it.
        assert_eq!(action("/home/ada/Videos"), "chooser:go:/home/ada/Videos");
        assert!(action("/home/ada/Downloads").starts_with("chooser:hidden:"));
        assert!(action("Show other locations").starts_with("chooser:hidden:"));
        let hidden = action("/home/ada/Downloads");
        assert_eq!(
            c.act(1, hidden.strip_prefix("chooser:").unwrap(), false),
            Ok(crate::ChooserOutcome::Stay(vec![]))
        );
        assert!(c.folder.is_none());
    }

    #[test]
    fn the_path_bar_runs_from_home_or_from_the_root() {
        assert_eq!(
            crumbs("/home/ada/Documents", "/home/ada"),
            vec![
                ("ada".to_owned(), "/home/ada".to_owned()),
                ("Documents".to_owned(), "/home/ada/Documents".to_owned())
            ]
        );
        assert_eq!(
            crumbs("/etc", "/home/ada"),
            vec![
                ("/".to_owned(), "/".to_owned()),
                ("etc".to_owned(), "/etc".to_owned())
            ]
        );
    }
}
