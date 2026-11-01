//! The platform's open dialog a browser shows for `<input type=file>`. A click on a
//! file input opens it over the page; it lists one folder of the machine at a time,
//! folders first, only the files the input's `accept` lets through. A click on a
//! folder goes into it, on a file selects it (with `multiple`, toggles it), Open
//! hands the selection to the page and Cancel closes it.
//!
//! The browser has no disk: the machine lists folders and reads the chosen files'
//! bytes for it. While the dialog waits for either, `BrowserState::chooser_wants_listing`
//! and `chooser_wants_files` say what it needs and `chooser_listed` and
//! `attach_files` answer.
use cw_scene::{Color, Node, Primitive, Rect, Scene, Semantic};
use serde::{Deserialize, Serialize};

/// Interaction ids the dialog paints, all under this prefix.
pub const PREFIX: &str = "filechooser:";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChooser {
    /// The interaction id of the input the files are for.
    pub input: String,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accept: String,
    /// The folder on show; empty until the machine names where the dialog starts
    /// (the person's home folder).
    pub folder: String,
    /// The folder's entries, folders first; `None` while the machine lists it.
    #[serde(default)]
    pub entries: Option<Vec<ChooserEntry>>,
    /// Names of the selected files in `folder`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected: Vec<String>,
    /// Open was pressed: the machine reads the selected files next.
    #[serde(default)]
    pub opening: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooserEntry {
    pub name: String,
    pub folder: bool,
}

impl FileChooser {
    pub fn new(input: &str, multiple: bool, accept: &str) -> FileChooser {
        FileChooser {
            input: input.to_owned(),
            multiple,
            accept: accept.to_owned(),
            folder: String::new(),
            entries: None,
            selected: vec![],
            opening: false,
        }
    }

    /// The machine listed `folder`: `names` as `list_tree` gives them (a folder
    /// ends in `/`). Hidden entries are left out, as the dialogs do by default.
    pub fn listed(&mut self, folder: &str, names: &[String]) {
        let mut folders = vec![];
        let mut files = vec![];
        for n in names {
            if n.starts_with('.') {
                continue;
            }
            match n.strip_suffix('/') {
                Some(f) => folders.push(ChooserEntry {
                    name: f.to_owned(),
                    folder: true,
                }),
                None if cw_web::upload::accepts(&self.accept, n) => files.push(ChooserEntry {
                    name: n.clone(),
                    folder: false,
                }),
                None => {}
            }
        }
        folders.sort_by(|a, b| a.name.cmp(&b.name));
        files.sort_by(|a, b| a.name.cmp(&b.name));
        folders.extend(files);
        self.folder = folder.to_owned();
        self.entries = Some(folders);
        self.selected.clear();
    }

    /// The full paths of the selected files.
    pub fn paths(&self) -> Vec<String> {
        self.selected
            .iter()
            .map(|n| format!("{}/{n}", self.folder.trim_end_matches('/')))
            .collect()
    }

    /// A click on one of the dialog's controls (`id` without the prefix). `Ok(true)`
    /// when the dialog should close without choosing (Cancel).
    pub fn click(&mut self, id: &str) -> Result<bool, String> {
        if id == "cancel" {
            return Ok(true);
        }
        if id == "open" {
            if self.selected.is_empty() {
                return Err("choose a file first".into());
            }
            self.opening = true;
            return Ok(false);
        }
        if id == "up" {
            let trimmed = self.folder.trim_end_matches('/');
            let parent = match trimmed.rsplit_once('/') {
                Some(("", _)) | None => "/".to_owned(),
                Some((p, _)) => p.to_owned(),
            };
            self.go(parent);
            return Ok(false);
        }
        let Some(name) = id.strip_prefix("entry:") else {
            return Err(format!("no control {id} in the file dialog"));
        };
        let entry = self
            .entries
            .as_ref()
            .and_then(|e| e.iter().find(|e| e.name == name))
            .cloned()
            .ok_or_else(|| format!("{name} is not in this folder"))?;
        if entry.folder {
            let path = format!("{}/{name}", self.folder.trim_end_matches('/'));
            self.go(path);
        } else if self.multiple {
            if let Some(i) = self.selected.iter().position(|s| *s == name) {
                self.selected.remove(i);
            } else {
                self.selected.push(name.to_owned());
            }
        } else {
            self.selected = vec![name.to_owned()];
        }
        Ok(false)
    }

    fn go(&mut self, folder: String) {
        self.folder = folder;
        self.entries = None;
        self.selected.clear();
    }

    /// Paints the dialog over a `scene` of the page.
    pub fn paint(&self, scene: &mut Scene) {
        let base = (1u64 << 62) + 0x100;
        let mut next = base;
        let mut push = |scene: &mut Scene,
                        rect: Rect,
                        primitive: Primitive,
                        interaction: Option<String>,
                        semantic: Option<Semantic>| {
            let mut n = Node::new(next, rect, primitive);
            next += 1;
            n.z = i32::MAX - 8;
            n.interaction = interaction;
            n.semantic = semantic;
            scene.nodes.push(n);
        };
        let sem = |role: &str, label: &str, value: Option<String>| {
            Some(Semantic {
                role: role.into(),
                label: label.into(),
                value,
                disabled: false,
                focusable: role == "button" || role == "option",
            })
        };
        let text = |t: &str, size: u16, color: Color| Primitive::UiText {
            text: t.to_owned(),
            size,
            fraction: 0,
            color,
            italic: false,
            lang: Default::default(),
            typeface: None,
            web: false,
        };
        let (sw, sh) = (scene.width, scene.height);
        // A scrim over the page, which takes no clicks while the dialog is up.
        push(
            scene,
            Rect::new(0, 0, sw, sh),
            Primitive::Box {
                fill: Color(0, 0, 0, 60),
                border: None,
                border_width: 0,
            },
            Some(format!("{PREFIX}scrim")),
            None,
        );
        let w = 520u32.min(sw.saturating_sub(16)).max(200);
        let rows = self.entries.as_ref().map_or(1, |e| e.len().clamp(1, 12)) as u32;
        let row_h = 26u32;
        let h = (44 + 32 + rows * row_h + 52)
            .min(sh.saturating_sub(16))
            .max(140);
        let x = (sw.saturating_sub(w) / 2) as i32;
        let y = (sh.saturating_sub(h) / 2) as i32;
        push(
            scene,
            Rect::new(x, y, w, h),
            Primitive::Box {
                fill: Color::rgb(250, 250, 250),
                border: Some(Color::rgb(160, 160, 160)),
                border_width: 1,
            },
            None,
            sem("dialog", "Open", None),
        );
        push(
            scene,
            Rect::new(x + 14, y + 12, w - 28, 20),
            text("Open", 15, Color::rgb(30, 30, 30)),
            None,
            sem("heading", "Open", None),
        );
        // The path bar: Up, and the folder on show.
        push(
            scene,
            Rect::new(x + 14, y + 44, 44, 24),
            Primitive::Box {
                fill: Color::rgb(235, 235, 235),
                border: Some(Color::rgb(190, 190, 190)),
                border_width: 1,
            },
            Some(format!("{PREFIX}up")),
            sem("button", "Up", None),
        );
        push(
            scene,
            Rect::new(x + 24, y + 48, 30, 16),
            text("Up", 12, Color::rgb(30, 30, 30)),
            None,
            None,
        );
        let shown = if self.folder.is_empty() {
            "…".to_owned()
        } else {
            self.folder.clone()
        };
        push(
            scene,
            Rect::new(x + 68, y + 48, w - 82, 16),
            text(&shown, 12, Color::rgb(60, 60, 60)),
            None,
            sem("text", "Folder", Some(shown.clone())),
        );
        let list_y = y + 76;
        match &self.entries {
            None => push(
                scene,
                Rect::new(x + 14, list_y + 4, w - 28, 18),
                text("Loading…", 12, Color::rgb(110, 110, 110)),
                None,
                None,
            ),
            Some(entries) if entries.is_empty() => push(
                scene,
                Rect::new(x + 14, list_y + 4, w - 28, 18),
                text("This folder is empty.", 12, Color::rgb(110, 110, 110)),
                None,
                None,
            ),
            Some(entries) => {
                for (i, e) in entries.iter().take(rows as usize).enumerate() {
                    let ry = list_y + (i as u32 * row_h) as i32;
                    let on = self.selected.contains(&e.name);
                    let label = if e.folder {
                        format!("{}/", e.name)
                    } else {
                        e.name.clone()
                    };
                    push(
                        scene,
                        Rect::new(x + 10, ry, w - 20, row_h - 2),
                        Primitive::Box {
                            fill: if on {
                                Color::rgb(204, 224, 255)
                            } else {
                                Color::rgb(250, 250, 250)
                            },
                            border: None,
                            border_width: 0,
                        },
                        Some(format!("{PREFIX}entry:{}", e.name)),
                        sem(
                            "option",
                            &label,
                            Some(if on { "selected" } else { "" }.to_owned()),
                        ),
                    );
                    push(
                        scene,
                        Rect::new(x + 18, ry + 5, w - 36, 16),
                        text(&label, 12, Color::rgb(30, 30, 30)),
                        None,
                        None,
                    );
                }
            }
        }
        // Cancel and Open, bottom right, Open the default.
        let by = y + h as i32 - 40;
        for (i, (id, label)) in [("cancel", "Cancel"), ("open", "Open")].iter().enumerate() {
            let bx = x + w as i32 - 14 - 84 * (2 - i as i32);
            let default = *id == "open";
            push(
                scene,
                Rect::new(bx, by, 76, 28),
                Primitive::Box {
                    fill: if default {
                        Color::rgb(26, 115, 232)
                    } else {
                        Color::rgb(235, 235, 235)
                    },
                    border: Some(Color::rgb(170, 170, 170)),
                    border_width: 1,
                },
                Some(format!("{PREFIX}{id}")),
                sem("button", label, None),
            );
            push(
                scene,
                Rect::new(bx + 14, by + 7, 56, 16),
                text(
                    label,
                    12,
                    if default {
                        Color::rgb(255, 255, 255)
                    } else {
                        Color::rgb(30, 30, 30)
                    },
                ),
                None,
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_walks_folders_filters_by_accept_and_selects() {
        let mut c = FileChooser::new("plan", false, ".txt");
        c.listed(
            "/home/alice",
            &[
                "Documents/".into(),
                ".cache/".into(),
                "a.txt".into(),
                "b.png".into(),
            ],
        );
        let names: Vec<&str> = c
            .entries
            .as_ref()
            .unwrap()
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["Documents", "a.txt"]);
        assert!(c.click("open").is_err(), "Open needs a file");
        c.click("entry:Documents").unwrap();
        assert_eq!(
            (c.folder.as_str(), c.entries.is_none()),
            ("/home/alice/Documents", true)
        );
        c.click("up").unwrap();
        assert_eq!(c.folder, "/home/alice");
        c.listed("/home/alice", &["a.txt".into()]);
        c.click("entry:a.txt").unwrap();
        assert_eq!(c.paths(), ["/home/alice/a.txt"]);
        assert_eq!(c.click("open"), Ok(false));
        assert!(c.opening);
        assert_eq!(c.click("cancel"), Ok(true));
    }
}
