//! GIMP 2.10's own windows as GTK2 draws them: the file chooser of Open Image, Save Image and
//! Export Image, and the JPEG options of Export Image as JPEG. Each is a top-level window of
//! its own on a real desktop, so each is announced with a node of role `dialog` covering it
//! (which the fidelity tool lifts into a window of its own), laid out on the real dialog's
//! measurements (read from its accessibility tree on the reference machine) and drawn inside the editor's window,
//! compressed where the real dialog is taller than the window.
use super::view::{self, Skin};
use super::{Panel, Studio};
use crate::desktop_scene::{shared::Align, Painter};
use cw_scene::{Color, Rect};

/// Title-bar height the window manager gives a dialog (part of the real window's bounds).
const TITLE: i32 = 20;

/// A dialog window: where it is, and the factor its real vertical positions are squeezed by
/// when the window it is drawn in is shorter than the real dialog.
struct Win {
    r: Rect,
    sy: f32,
}
impl Win {
    fn x(&self, v: i32) -> i32 {
        self.r.x + v
    }
    fn y(&self, v: i32) -> i32 {
        self.r.y + (v as f32 * self.sy).round() as i32
    }
    fn rect(&self, x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect::new(self.x(x), self.y(y), w, h)
    }
}

/// A target that does nothing, of its own: the scene keys a control by its interaction id, so
/// two controls sharing one would be read as one.
fn noop(p: &Painter, st: &Studio) -> String {
    st.target(&format!("noop:{}", p.scene.nodes.len()))
}

/// A size as GTK's file chooser words it: `437 bytes`, `1.6 kB`, `2.2 MB`.
fn byte_size(n: u64) -> String {
    match n {
        0..=999 => format!("{n} bytes"),
        1_000..=999_999 => format!("{:.1} kB", n as f64 / 1e3),
        _ => format!("{:.1} MB", n as f64 / 1e6),
    }
}

/// The time of day of a world-clock instant (microseconds), as the chooser shows a file
/// written today.
fn clock_time(micros: u64) -> String {
    let secs = micros / 1_000_000 % 86_400;
    format!("{:02}:{:02}", secs / 3600, secs / 60 % 60)
}

/// A cell holding `text`, named by it (blank when there is none).
fn text_cell(p: &mut Painter, s: &Skin, st: &Studio, r: Rect, text: &str) {
    let id = noop(p, st);
    p.button(r, Color::TRANSPARENT, 0, &id, text);
    announce(p, "cell", Some(cw_scene::NodeState::default()));
    p.label(
        r.x + 4,
        r.y + 4,
        r.width.saturating_sub(8),
        text,
        12,
        s.text,
        false,
        Align::Left,
    );
    presentation(p);
}

/// A cell with no text of its own: the row of a list, or its icon.
fn blank_cell(p: &mut Painter, st: &Studio, r: Rect) {
    let id = noop(p, st);
    p.button(r, Color::TRANSPARENT, 0, &id, "");
    announce(p, "cell", Some(cw_scene::NodeState::default()));
}

/// Open the dialog's window: dim nothing, absorb the clicks of the window beneath (the dialog
/// is modal), announce the window, draw its frame and title. `real` is the real window's size.
fn open(
    p: &mut Painter,
    s: &Skin,
    st: &Studio,
    (width, height): (u32, u32),
    real: (u32, u32),
    title: &str,
) -> Win {
    let w = real.0.min(width.saturating_sub(16));
    // A GTK dialog is a window of its own and may stand taller than GIMP's window; drawn in
    // it, it takes the window's whole height (its buttons must stay reachable) and its rows
    // are squeezed to fit.
    let h = real.1.min(height);
    let r = Rect::new(
        (width as i32 - w as i32) / 2,
        (height as i32 - h as i32) / 2,
        w,
        h,
    );
    p.region(Rect::new(0, 0, width, height), &noop(p, st), "");
    presentation(p);
    p.drop_shadow(r, 0, 12, 70, 3);
    p.region(r, &noop(p, st), title);
    if let Some(n) = p.scene.nodes.last_mut() {
        if let Some(sem) = &mut n.semantic {
            sem.role = "dialog".into();
        }
    }
    p.border(r, s.panel, 0, s.line);
    p.box_(Rect::new(r.x + 1, r.y + 1, w - 2, TITLE as u32), s.bar, 0);
    p.label(r.x, r.y + 3, w, title, 11, s.muted, false, Align::Center);
    Win {
        r,
        sy: if real.1 > h {
            (h as f32 - TITLE as f32) / (real.1 as f32 - TITLE as f32)
        } else {
            1.0
        },
    }
}

/// The node just painted is layout, not a control.
fn presentation(p: &mut Painter) {
    if let Some(n) = p.scene.nodes.last_mut() {
        n.semantic = Some(cw_scene::Semantic {
            role: "presentation".into(),
            label: String::new(),
            value: None,
            disabled: false,
            focusable: false,
        });
    }
}

/// The role (and state) a toolkit gives the control just painted.
fn announce(p: &mut Painter, role: &str, state: Option<cw_scene::NodeState>) {
    if let Some(n) = p.scene.nodes.last_mut() {
        if let Some(sem) = &mut n.semantic {
            sem.role = role.into();
        }
        if state.is_some() {
            n.state = state;
        }
    }
}

fn label(p: &mut Painter, s: &Skin, x: i32, y: i32, w: u32, text: &str) {
    p.label(x, y + 4, w, text, 12, s.text, false, Align::Left);
    announce_label(p, text);
}

/// A check box with its caption: a node of role `checkbox`.
#[allow(clippy::too_many_arguments)]
fn checkbox(p: &mut Painter, s: &Skin, r: Rect, caption: &str, target: &str, on: bool, live: bool) {
    let box_ = Rect::new(r.x, r.y + 1, 17, 17);
    p.button(r, Color::TRANSPARENT, 0, target, caption);
    announce(
        p,
        "checkbox",
        Some(cw_scene::NodeState {
            checked: Some(on),
            ..Default::default()
        }),
    );
    if !live {
        p.disabled(caption);
        announce(
            p,
            "checkbox",
            Some(cw_scene::NodeState {
                checked: Some(on),
                ..Default::default()
            }),
        );
    }
    p.border(box_, s.bg, 2, s.line);
    if on {
        p.symbol("check", box_.x + 1, box_.y + 1, 15, s.text);
    }
    p.label(
        r.x + 24,
        r.y + 1,
        r.width.saturating_sub(24),
        caption,
        12,
        if live { s.text } else { s.muted },
        false,
        Align::Left,
    );
    presentation(p);
}

/// An entry: a node of role `textbox` whose value is its text.
fn textbox(p: &mut Painter, s: &Skin, r: Rect, text: &str, target: &str) {
    p.border(r, s.bg, 2, s.line);
    p.button(r, Color::TRANSPARENT, 2, target, "");
    if let Some(n) = p.scene.nodes.last_mut() {
        if let Some(sem) = &mut n.semantic {
            sem.role = "textbox".into();
            sem.value = Some(text.to_owned());
        }
    }
    p.label(
        r.x + 6,
        r.y + 4,
        r.width.saturating_sub(12),
        text,
        12,
        s.text,
        false,
        Align::Left,
    );
}

/// A button of GIMP's dialogs: 85x29, a node of role `button`.
fn push(p: &mut Painter, s: &Skin, r: Rect, caption: &str, target: &str, primary: bool) {
    view::button(p, s, r, caption, target, primary);
}

/// A button of the dialog that does nothing here (Help, Create Folder, the defaults): GTK lists
/// it as an enabled button, so it is one, with a target of its own.
fn idle(p: &mut Painter, s: &Skin, st: &Studio, r: Rect, caption: &str) {
    let id = noop(p, st);
    push(p, s, r, caption, &id, false);
}

/// An inert control the dialog shows as GTK does: listed, insensitive.
fn inert(p: &mut Painter, s: &Skin, r: Rect, caption: &str, why: &str) {
    view::inert(p, s, r, caption, why);
    // Listed under its own name; the reason is not what a toolkit calls it.
    if let Some(n) = p
        .scene
        .nodes
        .iter_mut()
        .rev()
        .find(|n| n.semantic.is_some())
    {
        if let Some(sem) = &mut n.semantic {
            sem.label = caption.to_owned();
        }
    }
}

/// The file chooser of Open Image, Save Image and Export Image.
pub fn chooser(
    p: &mut Painter,
    s: &Skin,
    st: &Studio,
    panel: &Panel,
    home_name: &str,
    width: u32,
    height: u32,
) {
    let (title, folder, entries, name, loading, export) = match panel {
        Panel::Open {
            folder,
            entries,
            loading,
        } => ("Open Image", folder, entries, None, *loading, false),
        Panel::Save {
            folder,
            entries,
            name,
            export,
        } => (
            if *export {
                "Export Image"
            } else {
                "Save Image"
            },
            folder,
            entries,
            Some(name),
            false,
            *export,
        ),
        _ => return,
    };
    let z = p.z;
    p.z += 400;
    let win = open(p, s, st, (width, height), (950, 668), title);
    let wr = win.r;
    // The name, for a save: a text field to the right of its label.
    let mut top = 30;
    if let Some(name) = name {
        label(p, s, win.x(11), win.y(top), 102, "Name:");
        textbox(p, s, win.rect(125, top, 814, 25), name, &noop(p, st));
        top = 67;
    } else {
        top = 30;
    }
    // The folder: Up Path, then a button for each folder from the home on down.
    let row = win.y(top);
    label(p, s, win.x(11), row + 4, 102, "Save in folder:");
    let up_ok = !(super::folder_of(folder.trim_end_matches('/')).is_empty() || folder == "/");
    if up_ok {
        p.button(
            win.rect(125, top, 23, 33),
            Color::TRANSPARENT,
            0,
            &st.target("folder-up"),
            "Up Path",
        );
    } else {
        p.box_(win.rect(125, top, 23, 33), Color::TRANSPARENT, 0);
        p.disabled("Up Path");
    }
    p.symbol("chevron-left", win.x(127), row + 8, 16, s.text);
    // Home, the path's folders: the home button shows the home folder's name.
    let home = if home_name.is_empty() {
        "home"
    } else {
        home_name
    };
    let mut bx = 148;
    // A folder is named from the home (`Pictures/Supplier`) or from the root.
    let absolute = folder.starts_with('/');
    let mut crumbs: Vec<(String, String)> = if absolute {
        Vec::new()
    } else {
        vec![(home.to_owned(), String::new())]
    };
    let mut acc = if absolute {
        "/".to_owned()
    } else {
        String::new()
    };
    for part in folder.split('/').filter(|x| !x.is_empty()) {
        acc = match acc.as_str() {
            "" | "/" => format!("{acc}{part}"),
            _ => format!("{acc}/{part}"),
        };
        crumbs.push((part.to_owned(), acc.clone()));
    }
    for (i, (shown, path)) in crumbs.iter().enumerate() {
        let w = (p.measure(shown, 12, false) + 24).max(44);
        let target = if i + 1 == crumbs.len() || path.is_empty() {
            noop(p, st)
        } else {
            st.target(&format!("folder-to:{path}"))
        };
        p.button(
            win.rect(bx, top, w, 33),
            if i + 1 == crumbs.len() {
                s.selected
            } else {
                Color::TRANSPARENT
            },
            0,
            &target,
            shown,
        );
        p.label(
            win.x(bx),
            win.y(top) + 9,
            w,
            shown,
            12,
            s.text,
            i + 1 == crumbs.len(),
            Align::Center,
        );
        bx += w as i32;
    }
    if name.is_some() {
        idle(p, s, st, win.rect(833, top, 106, 33), "Create Folder");
    }
    // Places beside the file list, and the preview beside that.
    let list_top = top + 58;
    let list_h = (572 - 30 - 58 - 58 + top.min(37) - (top - 30)).max(120) as u32;
    let places = win.rect(12, list_top, 146, list_h);
    p.region(places, &noop(p, st), "Places");
    announce(p, "table", None);
    p.border(places, s.bg, 0, s.line);
    p.button(
        win.rect(12, list_top, 146, 27),
        s.bar,
        0,
        &noop(p, st),
        "Places",
    );
    announce(p, "columnheader", None);
    p.label(
        win.x(20),
        win.y(list_top) + 6,
        120,
        "Places",
        12,
        s.text,
        false,
        Align::Left,
    );
    let mut py = list_top + 28;
    for (i, place) in [
        "Search",
        "Recently Used",
        "Pictures",
        home,
        "File System",
        "OS",
    ]
    .into_iter()
    .enumerate()
    {
        if i == 2 {
            // The separator between the searches and the folders is a row of its own.
            blank_cell(p, st, win.rect(14, py, 142, 15));
            blank_cell(p, st, win.rect(14, py, 18, 15));
            blank_cell(p, st, win.rect(32, py, 126, 15));
            py += 4;
        }
        // GTK2's list rows are cells holding cells: the row, its icon, then its text.
        blank_cell(p, st, win.rect(14, py, 142, 23));
        blank_cell(p, st, win.rect(14, py, 18, 23));
        // A place is a folder to go to; only Pictures is one this GIMP lists.
        let target = if place == "Pictures" {
            st.target("folder-to:./Pictures")
        } else {
            noop(p, st)
        };
        p.button(
            win.rect(32, py, 126, 23),
            Color::TRANSPARENT,
            0,
            &target,
            place,
        );
        announce(p, "cell", Some(cw_scene::NodeState::default()));
        p.label(
            win.x(36),
            win.y(py) + 4,
            120,
            place,
            12,
            s.text,
            false,
            Align::Left,
        );
        py += 25;
    }
    // Add and remove a bookmark: a tool bar of two buttons under the places.
    let bar = win.rect(11, list_top + list_h as i32 + 1, 148, 26);
    p.region(bar, &noop(p, st), "");
    announce(p, "toolbar", None);
    for (i, what) in ["add", "remove"].iter().enumerate() {
        let id = st.target(&format!("noop:bookmark:{what}"));
        p.button(
            win.rect(11 + 26 * i as i32, list_top + list_h as i32 + 1, 26, 26),
            Color::TRANSPARENT,
            0,
            &id,
            "",
        );
    }
    let files = win.rect(162, list_top, 593, list_h);
    p.region(files, &noop(p, st), "Files");
    announce(p, "table", None);
    p.border(files, s.bg, 0, s.line);
    for (caption, x, w) in [("Name", 162, 445), ("Size", 607, 77), ("Modified", 684, 71)] {
        p.button(
            win.rect(x, list_top, w, 27),
            s.bar,
            0,
            &noop(p, st),
            caption,
        );
        announce(p, "columnheader", None);
        p.label(
            win.x(x + 6),
            win.y(list_top) + 6,
            w - 8,
            caption,
            12,
            s.text,
            false,
            Align::Left,
        );
    }
    let mut fy = list_top + 28;
    if loading {
        p.label(
            win.x(170),
            win.y(fy) + 4,
            400,
            "Loading…",
            12,
            s.muted,
            false,
            Align::Left,
        );
    }
    for entry in entries {
        if win.y(fy + 23) > win.y(list_top) + list_h as i32 {
            break;
        }
        let folder_entry = entry.ends_with('/');
        let clean = entry.trim_end_matches('/');
        let command = if folder_entry {
            Some(format!("folder:{clean}"))
        } else if name.is_none() {
            Some(format!("open:{clean}"))
        } else {
            None
        };
        blank_cell(p, st, win.rect(162, fy, 445, 23));
        blank_cell(p, st, win.rect(162, fy, 18, 23));
        let (size, when) = match st.meta.get(entry) {
            Some((bytes, modified)) => (byte_size(*bytes), clock_time(*modified)),
            None => (String::new(), String::new()),
        };
        text_cell(p, s, st, win.rect(609, fy, 73, 23), &size);
        text_cell(p, s, st, win.rect(686, fy, 67, 23), &when);
        let target = match &command {
            Some(c) => st.target(c),
            None => noop(p, st),
        };
        p.button(
            win.rect(182, fy, 425, 23),
            Color::TRANSPARENT,
            0,
            &target,
            clean,
        );
        announce(p, "cell", Some(cw_scene::NodeState::default()));
        p.symbol(
            if folder_entry { "folder" } else { "image" },
            win.x(164),
            win.y(fy) + 4,
            16,
            s.accent,
        );
        p.label(
            win.x(186),
            win.y(fy) + 4,
            410,
            clean,
            12,
            s.text,
            false,
            Align::Left,
        );
        fy += 25;
    }
    // The preview pane.
    p.button(
        win.rect(784, list_top, 154, 27),
        s.bar,
        0,
        &noop(p, st),
        "Preview",
    );
    p.label(
        win.x(790),
        win.y(list_top) + 6,
        140,
        "Preview",
        12,
        s.text,
        false,
        Align::Left,
    );
    p.symbol("help", win.x(842), win.y(list_top) + 130, 40, s.muted);
    p.label(
        win.x(786),
        win.y(list_top) + 163,
        150,
        "No selection",
        12,
        s.muted,
        false,
        Align::Center,
    );
    // Below the list.
    let below = list_top + list_h as i32 + 12;
    checkbox(
        p,
        s,
        win.rect(11, below, 928, 19),
        "Show All Files",
        &noop(p, st),
        false,
        true,
    );
    p.button(
        win.rect(11, below + 24, 928, 21),
        Color::TRANSPARENT,
        0,
        &noop(p, st),
        if name.is_some() {
            "Select File Type (By Extension)"
        } else {
            "Select File Type (Automatically Detected)"
        },
    );
    announce(
        p,
        "button",
        Some(cw_scene::NodeState {
            expanded: Some(false),
            ..Default::default()
        }),
    );
    p.symbol(
        "chevron-right",
        win.x(14),
        win.y(below + 24) + 3,
        14,
        s.text,
    );
    p.label(
        win.x(34),
        win.y(below + 24) + 2,
        600,
        if name.is_some() {
            "Select File Type (By Extension)"
        } else {
            "Select File Type (Automatically Detected)"
        },
        12,
        s.text,
        false,
        Align::Left,
    );
    // Help, Cancel and the action, anchored to the window's foot.
    let foot = wr.y + wr.height as i32 - 13 - 29;
    let by = foot - win.r.y;
    let at = |x: i32| Rect::new(win.x(x), win.r.y + by, 85, 29);
    idle(p, s, st, at(18), "Help");
    push(p, s, at(756), "Cancel", &st.target("cancel"), false);
    if name.is_some() {
        push(
            p,
            s,
            at(847),
            if export { "Export" } else { "Save" },
            &st.target("save-confirm"),
            true,
        );
    } else {
        inert(p, s, at(847), "Open", "choose a file in the list");
    }
    p.z = z;
}

/// The subsampling choices of Export Image as JPEG, in GIMP's order.
pub const SUBSAMPLINGS: [&str; 4] = [
    "4:4:4 (best quality)",
    "4:2:2 horizontal (chroma halved)",
    "4:2:2 vertical (chroma halved)",
    "4:2:0 (chroma quartered)",
];

/// Export Image as JPEG: the quality, what to save, and, under Advanced Options, how.
pub fn jpeg(p: &mut Painter, s: &Skin, st: &Studio, width: u32, height: u32) {
    let Some(Panel::Dialog { id, values }) = &st.panel else {
        return;
    };
    if id != "jpeg" {
        return;
    }
    let g = |k: &str| values.get(k).copied().unwrap_or(0);
    let advanced = g("advanced") == 1;
    let quality = g("quality");
    let z = p.z;
    p.z += 450;
    let real_h = if advanced { 630 } else { 498 };
    let win = open(
        p,
        s,
        st,
        (width, height),
        (680, real_h),
        "Export Image as JPEG",
    );
    // Quality: a slider and its spin button.
    label(p, s, win.x(13), win.y(32), 54, "Quality:");
    view::slider(
        p,
        s,
        win.rect(73, 28, 536, 34),
        st,
        "Quality",
        "quality",
        false,
    );
    p.button(win.rect(615, 32, 52, 25), s.bg, 2, &noop(p, st), "");
    if let Some(n) = p.scene.nodes.last_mut() {
        if let Some(sem) = &mut n.semantic {
            sem.role = "spinbutton".into();
            sem.value = Some(quality.to_string());
        }
    }
    p.border(win.rect(615, 32, 52, 25), Color::TRANSPARENT, 2, s.line);
    p.label(
        win.x(621),
        win.y(32) + 4,
        40,
        &quality.to_string(),
        12,
        s.text,
        false,
        Align::Left,
    );
    checkbox(
        p,
        s,
        win.rect(13, 63, 654, 19),
        "Use quality settings from original image",
        &noop(p, st),
        false,
        false,
    );
    label(p, s, win.x(13), win.y(94), 654, "File size: unknown");
    checkbox(
        p,
        s,
        win.rect(13, 117, 654, 19),
        "Show preview in image window",
        &noop(p, st),
        false,
        true,
    );
    for (i, (caption, on)) in [
        ("Save Exif data", true),
        ("Save XMP data", true),
        ("Save IPTC data", false),
        ("Save thumbnail", false),
        ("Save color profile", true),
    ]
    .iter()
    .enumerate()
    {
        checkbox(
            p,
            s,
            win.rect(13, 148 + 25 * i as i32, 654, 19),
            caption,
            &noop(p, st),
            *on,
            true,
        );
    }
    // Comment: a framed text box under its caption.
    p.region(win.rect(13, 279, 654, 69), &noop(p, st), "Comment");
    announce(p, "panel", None);
    p.label(
        win.x(13),
        win.y(279),
        654,
        "Comment",
        12,
        s.text,
        true,
        Align::Left,
    );
    textbox(p, s, win.rect(30, 299, 636, 48), "", &noop(p, st));
    // Advanced Options: an expander.
    let expander = win.rect(13, 360, 654, if advanced { 153 } else { 21 });
    p.button(
        Rect::new(expander.x, expander.y, expander.width, 21),
        Color::TRANSPARENT,
        0,
        &st.target(&format!("set:advanced:{}", 1 - g("advanced"))),
        "Advanced Options",
    );
    announce(
        p,
        "button",
        Some(cw_scene::NodeState {
            expanded: Some(advanced),
            ..Default::default()
        }),
    );
    p.symbol(
        if advanced {
            "chevron-down"
        } else {
            "chevron-right"
        },
        expander.x + 2,
        expander.y + 3,
        14,
        s.text,
    );
    p.label(
        expander.x + 22,
        expander.y + 2,
        400,
        "Advanced Options",
        12,
        s.text,
        true,
        Align::Left,
    );
    if advanced {
        checkbox(
            p,
            s,
            win.rect(29, 386, 176, 19),
            "Optimize",
            &noop(p, st),
            true,
            true,
        );
        checkbox(
            p,
            s,
            win.rect(29, 417, 176, 19),
            "Use arithmetic coding",
            &noop(p, st),
            false,
            true,
        );
        checkbox(
            p,
            s,
            win.rect(29, 451, 176, 19),
            "Progressive",
            &noop(p, st),
            true,
            true,
        );
        label(p, s, win.x(223), win.y(383), 82, "Smoothing:");
        p.region(win.rect(311, 383, 280, 25), &noop(p, st), "");
        announce(p, "slider", None);
        p.hline(win.x(311), win.y(395), 280, s.line);
        p.button(win.rect(597, 383, 58, 25), s.bg, 2, &noop(p, st), "");
        if let Some(n) = p.scene.nodes.last_mut() {
            if let Some(sem) = &mut n.semantic {
                sem.role = "spinbutton".into();
                sem.value = Some("0".into());
            }
        }
        p.border(win.rect(597, 383, 58, 25), Color::TRANSPARENT, 2, s.line);
        p.label(
            win.x(603),
            win.y(383) + 4,
            40,
            "0",
            12,
            s.text,
            false,
            Align::Left,
        );
        checkbox(
            p,
            s,
            win.rect(223, 417, 160, 19),
            "Use restart markers",
            &noop(p, st),
            false,
            true,
        );
        label(p, s, win.x(389), win.y(414), 208, "Interval (MCU rows):");
        label(p, s, win.x(223), win.y(445), 125, "Subsampling:");
        label(p, s, win.x(223), win.y(482), 125, "DCT method:");
        let sub = SUBSAMPLINGS[g("subsampling").clamp(0, 3) as usize];
        let combo = win.rect(354, 445, 301, 31);
        p.button(
            combo,
            s.bg,
            2,
            &st.target(&format!("set:subpopup:{}", 1 - g("subpopup"))),
            sub,
        );
        announce(p, "combobox", None);
        p.border(combo, Color::TRANSPARENT, 2, s.line);
        p.label(
            combo.x + 8,
            combo.y + 7,
            260,
            sub,
            12,
            s.text,
            false,
            Align::Left,
        );
        p.symbol("chevron-down", combo.x + 280, combo.y + 8, 14, s.text);
        let dct = win.rect(354, 482, 301, 31);
        p.button(dct, s.bg, 2, &noop(p, st), "Integer");
        announce(p, "combobox", None);
        p.border(dct, Color::TRANSPARENT, 2, s.line);
        p.label(
            dct.x + 8,
            dct.y + 7,
            260,
            "Integer",
            12,
            s.text,
            false,
            Align::Left,
        );
        p.symbol("chevron-down", dct.x + 280, dct.y + 8, 14, s.text);
        if g("subpopup") == 1 {
            // The open list covers the combo box and what is under it.
            p.z += 5;
            let list = win.rect(354, 445, 301, 4 * 29 + 2);
            p.drop_shadow(list, 0, 8, 60, 2);
            p.border(list, s.panel, 0, s.line);
            for (i, name) in SUBSAMPLINGS.iter().enumerate() {
                let row = win.rect(356, 447 + 29 * i as i32, 297, 29);
                let live = *name == SUBSAMPLINGS[0] || *name == SUBSAMPLINGS[3];
                if live {
                    p.button(
                        row,
                        if i as i32 == g("subsampling") {
                            s.selected
                        } else {
                            Color::TRANSPARENT
                        },
                        0,
                        &st.target(&format!("set:subsampling:{i}")),
                        name,
                    );
                } else {
                    p.box_(row, Color::TRANSPARENT, 0);
                    p.disabled("This GIMP writes 4:4:4 or 4:2:0");
                    if let Some(n) = p.scene.nodes.last_mut() {
                        if let Some(sem) = &mut n.semantic {
                            sem.label = (*name).to_owned();
                        }
                    }
                }
                announce(p, "menuitem", None);
                p.label(
                    row.x + 8,
                    row.y + 7,
                    280,
                    name,
                    12,
                    if live { s.text } else { s.muted },
                    false,
                    Align::Left,
                );
            }
            p.z -= 5;
        }
    }
    let defaults_y = if advanced { 537 } else { 405 };
    idle(p, s, st, win.rect(13, defaults_y, 108, 27), "Load Defaults");
    idle(
        p,
        s,
        st,
        win.rect(127, defaults_y, 108, 27),
        "Save Defaults",
    );
    let foot = win.r.y + win.r.height as i32 - 13 - 29;
    let at = |x: i32| Rect::new(win.x(x), foot, 85, 29);
    idle(p, s, st, at(13), "Help");
    push(p, s, at(491), "Cancel", &st.target("cancel"), false);
    push(p, s, at(582), "Export", &st.target("apply"), true);
    p.z = z;
}

/// Scale Image: the width and height fields with their chain and units, the resolution, and
/// the interpolation. The simulated fields take no typing: a click on the width field sets the
/// width to half of what it is (`set:width:<half>`, the id the procedure's scale step uses), a
/// click on the height field likewise.
pub fn scale(p: &mut Painter, s: &Skin, st: &Studio, width: u32, height: u32) {
    let Some(Panel::Dialog { id, values }) = &st.panel else {
        return;
    };
    if id != "resize" {
        return;
    }
    let g = |k: &str| values.get(k).copied().unwrap_or(0);
    let (w_px, h_px) = (g("width"), g("height"));
    let z = p.z;
    p.z += 450;
    let win = open(p, s, st, (width, height), (384, 351), "Scale Image");
    p.label(
        win.x(34),
        win.y(23),
        302,
        "Scale Image",
        14,
        s.text,
        true,
        Align::Left,
    );
    announce_label(p, "Scale Image");
    let name = st.document_name();
    p.label(
        win.x(34),
        win.y(43),
        302,
        &name,
        11,
        s.muted,
        false,
        Align::Left,
    );
    announce_label(p, &name);
    // Image Size.
    p.region(win.rect(13, 72, 358, 151), &noop(p, st), "Image Size");
    announce(p, "panel", None);
    p.label(
        win.x(13),
        win.y(72),
        358,
        "Image Size",
        12,
        s.text,
        true,
        Align::Left,
    );
    announce_label(p, "Image Size");
    label(p, s, win.x(29), win.y(91), 96, "Width:");
    label(p, s, win.x(29), win.y(116), 96, "Height:");
    spin(
        p,
        s,
        win.rect(129, 91, 94, 25),
        &w_px.to_string(),
        &st.target(&format!("set:width:{}", (w_px / 2).max(1))),
    );
    spin(
        p,
        s,
        win.rect(129, 116, 94, 31),
        &h_px.to_string(),
        &st.target(&format!("set:height:{}", (h_px / 2).max(1))),
    );
    combo(p, s, st, win.rect(257, 116, 59, 31), "pixels");
    label(
        p,
        s,
        win.x(129),
        win.y(147),
        124,
        &format!("{w_px} × {h_px} pixels"),
    );
    announce_label(p, &format!("{w_px} × {h_px} pixels"));
    let chain = |p: &mut Painter, r: Rect, locked: bool| {
        let id = noop(p, st);
        p.button(r, Color::TRANSPARENT, 2, &id, "");
        p.symbol(
            if locked { "check" } else { "close" },
            r.x + 7,
            r.y + 7,
            16,
            s.text,
        );
    };
    chain(p, win.rect(223, 104, 30, 30), g("ratio") == 1);
    label(p, s, win.x(29), win.y(167), 96, "X resolution:");
    label(p, s, win.x(29), win.y(192), 96, "Y resolution:");
    spin(p, s, win.rect(129, 167, 94, 25), "72", &noop(p, st));
    spin(p, s, win.rect(129, 192, 94, 31), "72", &noop(p, st));
    combo(p, s, st, win.rect(257, 192, 105, 31), "inches");
    chain(p, win.rect(223, 180, 30, 30), true);
    // Quality.
    p.region(win.rect(13, 235, 358, 50), &noop(p, st), "Quality");
    announce(p, "panel", None);
    p.label(
        win.x(13),
        win.y(235),
        358,
        "Quality",
        12,
        s.text,
        true,
        Align::Left,
    );
    announce_label(p, "Quality");
    label(p, s, win.x(29), win.y(254), 96, "Interpolation:");
    let resample = ["Nearest", "Bilinear", "Cubic"][g("resample").clamp(0, 2) as usize];
    combo(p, s, st, win.rect(131, 254, 240, 31), resample);
    let foot = win.r.y + win.r.height as i32 - 13 - 29;
    let at = |x: i32| Rect::new(win.x(x), foot, 85, 29);
    idle(p, s, st, at(13), "Help");
    push(p, s, at(104), "Reset", &st.target("reset"), false);
    push(p, s, at(195), "Cancel", &st.target("cancel"), false);
    push(p, s, at(286), "Scale", &st.target("apply"), true);
    p.z = z;
}

/// The painted text just laid is a label of its own, as GTK announces its captions.
fn announce_label(p: &mut Painter, text: &str) {
    if let Some(n) = p.scene.nodes.last_mut() {
        n.semantic = Some(cw_scene::Semantic {
            role: "label".into(),
            label: text.to_owned(),
            value: None,
            disabled: false,
            focusable: false,
        });
    }
}

/// A spin button: a node of role `spinbutton` whose value is its text, a click running `target`.
fn spin(p: &mut Painter, s: &Skin, r: Rect, value: &str, target: &str) {
    p.border(r, s.bg, 2, s.line);
    p.button(r, Color::TRANSPARENT, 2, target, "");
    if let Some(n) = p.scene.nodes.last_mut() {
        if let Some(sem) = &mut n.semantic {
            sem.role = "spinbutton".into();
            sem.value = Some(value.to_owned());
        }
    }
    p.label(
        r.x + 6,
        r.y + 5,
        r.width.saturating_sub(12),
        value,
        12,
        s.text,
        false,
        Align::Left,
    );
}

/// A combo box showing `value`, named by it.
fn combo(p: &mut Painter, s: &Skin, st: &Studio, r: Rect, value: &str) {
    let id = noop(p, st);
    p.border(r, s.bg, 2, s.line);
    p.button(r, Color::TRANSPARENT, 2, &id, value);
    announce(p, "combobox", None);
    p.label(
        r.x + 8,
        r.y + 7,
        r.width.saturating_sub(30),
        value,
        12,
        s.text,
        false,
        Align::Left,
    );
    p.symbol(
        "chevron-down",
        r.x + r.width as i32 - 22,
        r.y + 8,
        14,
        s.text,
    );
}
