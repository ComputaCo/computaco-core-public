//! GTK's file chooser, one for every application that opens it: the dialog a GNOME
//! application shows on Ctrl+O (Image Viewer's Open Image, Maps' Open Shape Layer, Text
//! Editor's Open File, LibreOffice's Open), held per window in `DesktopState::file_choosers`
//! and painted over that window by `desktop_scene::file_chooser`.
//!
//! It browses the machine's real folders: a place, the path bar or a folder row asks the
//! environment for that folder's listing (`AppEffect::ListDirectory` on [`CHOOSER_TAB`]),
//! and the rows it shows are the listing that came back, with the size and the time the
//! filesystem reported. Choosing a file hands its path to the application that opened the
//! chooser; an application that cannot open what it chose keeps Open insensitive.
use crate::files::{EntryKind, FileRow};
use serde::{Deserialize, Serialize};

/// The `tab` a listing for a window's file chooser is asked and delivered on. A window's own
/// tabs count from zero; a chooser is never one of them.
pub const CHOOSER_TAB: usize = usize::MAX;
/// Rows a chooser keeps of one folder, in its sort order, the way a file manager's tab is
/// bounded: a large folder must not make a snapshot a function of its size.
const ROW_LIMIT: usize = 256;

/// Which GTK draws the chooser: GTK 3's (Image Viewer, LibreOffice, KiCad) fills the work
/// area and lists Trash among its places; GTK 4's (Text Editor, Maps) is a smaller dialog
/// whose places are named by what they do.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    cw_snapshot::Encode,
    cw_snapshot::Decode,
)]
#[serde(rename_all = "snake_case")]
pub enum Gtk {
    Three,
    Four,
}

/// The listing's column a header click sorts by.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    cw_snapshot::Encode,
    cw_snapshot::Decode,
)]
#[serde(rename_all = "snake_case")]
pub enum ChooserColumn {
    #[default]
    Name,
    Size,
    Type,
    Modified,
}
impl ChooserColumn {
    pub const ALL: [Self; 4] = [Self::Name, Self::Size, Self::Type, Self::Modified];
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Size => "Size",
            Self::Type => "Type",
            Self::Modified => "Modified",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.label() == s)
    }
}

/// What a click on a chooser came to.
#[derive(Debug, PartialEq, Eq)]
pub enum ChooserOutcome {
    /// It stays open, having asked for these listings.
    Stay(Vec<crate::AppEffect>),
    /// Cancel: it closes, and nothing is opened.
    Cancel,
    /// This file was chosen: it closes, and the application opens the file.
    Open(String),
}

#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, cw_snapshot::Encode, cw_snapshot::Decode,
)]
pub struct FileChooser {
    pub gtk: Gtk,
    /// The dialog's title, as the application names it.
    pub title: String,
    /// The filter's name, and the extensions it lets through (none: every file).
    pub filter: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// The folder shown; `None` is Recent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Rows of the folder shown, as the machine listed it, in the order shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<FileRow>,
    /// The row chosen, by its entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
    #[serde(default)]
    pub sort: ChooserColumn,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub descending: bool,
    /// GTK 4's Grid View, toggled by its button.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub grid: bool,
    /// The application can open what is chosen; when not, Open stays insensitive.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub opens: bool,
    /// The application prefers GTK's dark theme, which its dialogs take too (Image Viewer).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dark: bool,
    /// Text Editor's Character Encoding row under the listing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub encoding: bool,
    /// The filter's width where the application sets one (LibreOffice's 364 px); 0 is
    /// GTK's own, as wide as the filter's name.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub filter_width: u32,
}
fn is_zero(v: &u32) -> bool {
    *v == 0
}

impl FileChooser {
    /// The chooser `kind` opens on Ctrl+O on Ubuntu, with the title, filter and first place
    /// the real application gives it (the explorer's references, store the Ubuntu reference store): `None` for an
    /// application that opens none.
    pub fn for_kind(kind: &str, home: &str) -> Option<Self> {
        let home = home.trim_end_matches('/');
        let (gtk, title, filter, extensions, folder, opens): (_, _, _, &[&str], _, _) = match kind {
            // Image Viewer (Eye of GNOME 45, GTK 3) opens on the home folder.
            "photos" => (
                Gtk::Three,
                "Open Image",
                "Supported image files",
                &["png", "jpg", "jpeg", "gif", "bmp", "webp"],
                Some(home.to_owned()),
                true,
            ),
            // LibreOffice Writer's own GTK 3 dialog opens on Documents. The simulated Writer
            // is a service's document, so a local file cannot be opened in it.
            "docs" => (
                Gtk::Three,
                "Open",
                "All files",
                &[],
                Some(format!("{home}/Documents")),
                false,
            ),
            // Maps 46 (GTK 4) opens a shape layer; the simulated map draws no layers.
            "maps" => (
                Gtk::Four,
                "Open Shape Layer",
                "All Layer Files",
                &["geojson", "kml", "kmz", "gpx", "shp"],
                None,
                false,
            ),
            "editor" => (Gtk::Four, "Open File", "Text Files", &[], None, true),
            _ => return None,
        };
        Some(Self {
            gtk,
            title: title.into(),
            filter: filter.into(),
            extensions: extensions.iter().map(|e| (*e).to_owned()).collect(),
            folder,
            rows: vec![],
            selected: None,
            sort: ChooserColumn::Name,
            descending: false,
            grid: false,
            opens,
            dark: kind == "photos",
            encoding: kind == "editor",
            filter_width: if kind == "docs" { 364 } else { 0 },
        })
    }
    /// The listing to ask for, when a folder is shown.
    pub fn listing(&self, window: u64) -> Vec<crate::AppEffect> {
        self.folder
            .iter()
            .map(|path| crate::AppEffect::ListDirectory {
                window,
                tab: CHOOSER_TAB,
                path: path.clone(),
            })
            .collect()
    }
    /// Show `folder` (`None`: Recent); its rows arrive with its listing.
    pub fn go(&mut self, window: u64, folder: Option<String>) -> Vec<crate::AppEffect> {
        self.folder = folder.map(|f| {
            let f = f.trim_end_matches('/');
            if f.is_empty() {
                "/".into()
            } else {
                f.to_owned()
            }
        });
        self.rows.clear();
        self.selected = None;
        self.listing(window)
    }
    /// The folder above the one shown (the path bar's Up Path), if any.
    pub fn up(&self) -> Option<String> {
        let folder = self.folder.as_deref()?;
        if folder == "/" {
            return None;
        }
        let (parent, _) = folder.rsplit_once('/')?;
        Some(if parent.is_empty() { "/" } else { parent }.to_owned())
    }
    /// Whether the filter lets a file through. Folders always show.
    pub fn admits(&self, row: &FileRow) -> bool {
        if row.kind == EntryKind::Directory || self.extensions.is_empty() {
            return true;
        }
        row.name()
            .rsplit_once('.')
            .is_some_and(|(_, ext)| self.extensions.contains(&ext.to_ascii_lowercase()))
    }
    /// A listing came back: hidden files out, the filter applied, folders first, sorted.
    pub fn listed(&mut self, rows: Vec<FileRow>) {
        let mut rows: Vec<FileRow> = rows
            .into_iter()
            .filter(|r| !r.name().starts_with('.') && self.admits(r))
            .collect();
        self.order(&mut rows);
        rows.truncate(ROW_LIMIT);
        // GTK 3 selects the first row of a folder it opens on; GTK 4 selects none.
        self.selected = match self.gtk {
            Gtk::Three => rows.first().map(|r| r.entry.clone()),
            Gtk::Four => None,
        };
        self.rows = rows;
    }
    fn order(&self, rows: &mut [FileRow]) {
        let key = |r: &FileRow| type_label(r);
        rows.sort_by(|a, b| {
            let dirs = b.kind.is_dir().cmp(&a.kind.is_dir());
            let by = match self.sort {
                ChooserColumn::Name => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
                ChooserColumn::Size => a.size.cmp(&b.size),
                ChooserColumn::Type => key(a).cmp(key(b)),
                ChooserColumn::Modified => a.modified.cmp(&b.modified),
            };
            let by = if self.descending { by.reverse() } else { by };
            dirs.then(by).then_with(|| a.entry.cmp(&b.entry))
        });
    }
    /// A column header clicked: sort by it, or turn its order round.
    pub fn sort_by(&mut self, column: ChooserColumn) {
        if self.sort == column {
            self.descending = !self.descending;
        } else {
            self.sort = column;
            self.descending = false;
        }
        let mut rows = std::mem::take(&mut self.rows);
        self.order(&mut rows);
        self.rows = rows;
    }
    /// The chosen row's path, and whether it is a folder.
    pub fn chosen(&self) -> Option<(String, bool)> {
        let entry = self.selected.as_deref()?;
        let row = self.rows.iter().find(|r| r.entry == entry)?;
        let folder = self.folder.as_deref()?;
        Some((
            format!("{}/{}", folder.trim_end_matches('/'), row.name()),
            row.kind.is_dir(),
        ))
    }
    /// A click on one of the chooser's controls (`action` after the painter's prefix;
    /// `activate`: a double click, which opens the row it lands on). Every application that
    /// shows a chooser answers its clicks through this one grammar.
    pub fn act(
        &mut self,
        window: u64,
        action: &str,
        activate: bool,
    ) -> Result<ChooserOutcome, String> {
        let effects = match action {
            // The dialog's own ground, and GTK 3's chooser widget, take a click and change
            // nothing.
            "frame" | "widget" => vec![],
            "cancel" => return Ok(ChooserOutcome::Cancel),
            "open" => return self.open(window),
            "place:recent" => self.go(window, None),
            "grid" => {
                self.grid = !self.grid;
                vec![]
            }
            "up" => match self.up() {
                Some(up) => self.go(window, Some(up)),
                None => return Err("the folder shown has no parent".into()),
            },
            other => {
                if let Some(path) = other
                    .strip_prefix("go:")
                    .or_else(|| other.strip_prefix("crumb:"))
                {
                    self.go(window, Some(path.to_owned()))
                } else if let Some(column) = other.strip_prefix("sort:") {
                    self.sort_by(ChooserColumn::parse(column).ok_or("unknown column")?);
                    vec![]
                } else if let Some(entry) = other.strip_prefix("row:") {
                    if self.folder.is_some() && !self.rows.iter().any(|r| r.entry == entry) {
                        return Err("that row is not listed".into());
                    }
                    self.selected = Some(entry.to_owned());
                    if activate {
                        return self.open(window);
                    }
                    vec![]
                } else if let Some(control) = other.strip_prefix("off:") {
                    return Err(format!("{control} is not available"));
                } else {
                    return Err("unknown file chooser action".into());
                }
            }
        };
        Ok(ChooserOutcome::Stay(effects))
    }
    /// Open what is chosen: a folder is entered; a file is the application's to open.
    fn open(&mut self, window: u64) -> Result<ChooserOutcome, String> {
        let (path, folder) = self.chosen().ok_or("nothing is chosen")?;
        if folder {
            return Ok(ChooserOutcome::Stay(self.go(window, Some(path))));
        }
        if !self.opens {
            return Err("this application cannot open that file".into());
        }
        Ok(ChooserOutcome::Open(path))
    }
    /// Whether Open would act: a row is chosen, and it is a folder or a file the
    /// application can open.
    pub fn can_open(&self) -> bool {
        self.chosen().is_some_and(|(_, dir)| dir || self.opens)
    }
}

/// The Type column's word for a row, as GTK's chooser writes it (the references of store
/// the Ubuntu reference store: `expenses.csv` and `handbook.txt` Text, `report.md` Document). A folder has none.
pub fn type_label(row: &FileRow) -> &'static str {
    if row.kind.is_dir() {
        return "";
    }
    let ext = row
        .name()
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "txt" | "csv" | "log" => "Text",
        "md" | "odt" | "doc" | "docx" | "pdf" | "rtf" => "Document",
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" => "Image",
        _ => "Unknown",
    }
}

/// The Size column: GLib's `g_format_size`, decimal units, a folder none.
pub fn size_label(row: &FileRow) -> String {
    let Some(bytes) = row.size.filter(|_| !row.kind.is_dir()) else {
        return String::new();
    };
    if bytes < 1000 {
        return format!("{bytes} {}", if bytes == 1 { "byte" } else { "bytes" });
    }
    let units = ["kB", "MB", "GB", "TB"];
    let (mut scale, mut unit) = (1000u64, 0);
    while bytes / scale >= 1000 && unit + 1 < units.len() {
        scale *= 1000;
        unit += 1;
    }
    let tenths = bytes * 10 / scale;
    format!("{}.{} {}", tenths / 10, tenths % 10, units[unit])
}

/// The Modified column, against the world clock `now`: the time of day for today
/// (`09‎∶00`, GTK's ratio sign after a left-to-right mark), `Yesterday`, else the day and
/// the month (`2 Nov`). Both are world-clock microseconds, the day starting at 09:00.
pub fn modified_label(modified: Option<u64>, now: u64) -> String {
    let Some(at) = modified else {
        return String::new();
    };
    const DAY: u64 = 24 * 60;
    let minutes = |us: u64| us / 60_000_000 + 9 * 60;
    let (m, n) = (minutes(at), minutes(now));
    if m / DAY == n / DAY {
        let t = m % DAY;
        return format!("{:02}\u{200e}\u{2236}{:02}", t / 60, t % 60);
    }
    if m / DAY + 1 == n / DAY {
        return "Yesterday".into();
    }
    let date = crate::desktop_scene::shared::CalendarDate::from_clock(at);
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(date.month as usize).clamp(1, 12) - 1];
    format!("{} {month}", date.day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(entry: &str, size: Option<u64>, modified: u64) -> FileRow {
        FileRow {
            size,
            modified: Some(modified),
            ..FileRow::named(entry)
        }
    }

    #[test]
    fn a_listing_shows_folders_first_and_only_what_the_filter_admits() {
        let mut c = FileChooser::for_kind("photos", "/home/ada").unwrap();
        assert_eq!(c.folder.as_deref(), Some("/home/ada"));
        c.listed(vec![
            row("b.png", Some(10), 0),
            row("notes.txt", Some(3), 0),
            row("Pictures/", None, 0),
            row(".hidden/", None, 0),
            row("A.JPG", Some(5), 0),
        ]);
        let names: Vec<&str> = c.rows.iter().map(|r| r.name()).collect();
        assert_eq!(names, ["Pictures", "A.JPG", "b.png"]);
        // GTK 3 selects the first row; Open on a folder enters it.
        assert_eq!(c.chosen(), Some(("/home/ada/Pictures".into(), true)));
        c.sort_by(ChooserColumn::Size);
        let names: Vec<&str> = c.rows.iter().map(|r| r.name()).collect();
        assert_eq!(names, ["Pictures", "A.JPG", "b.png"]);
        c.sort_by(ChooserColumn::Size);
        let names: Vec<&str> = c.rows.iter().map(|r| r.name()).collect();
        assert_eq!(names, ["Pictures", "b.png", "A.JPG"]);
        assert_eq!(c.up().as_deref(), Some("/home"));
    }

    #[test]
    fn the_columns_read_as_gtk_writes_them() {
        assert_eq!(size_label(&row("a.csv", Some(142), 0)), "142 bytes");
        assert_eq!(size_label(&row("a.csv", Some(1500), 0)), "1.5 kB");
        assert_eq!(size_label(&row("Music/", None, 0)), "");
        assert_eq!(type_label(&row("report.md", None, 0)), "Document");
        assert_eq!(type_label(&row("expenses.csv", None, 0)), "Text");
        let hour = 3_600_000_000;
        assert_eq!(modified_label(Some(0), hour), "09\u{200e}\u{2236}00");
        assert_eq!(modified_label(Some(0), 24 * hour), "Yesterday");
        assert_eq!(modified_label(None, 0), "");
    }
}
