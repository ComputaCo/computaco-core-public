//! Documents over the `docs` service: real documents, real revisions, real edits.
use super::look::{action, look, notice, notice_in, screen, INK, LINE, MUTED};
use super::{push_bounded, Status};
use crate::desktop_scene::{DesktopTheme, Painter};
use crate::AppEffect;
use cw_scene::{ObjRef, Rect};
use serde::{Deserialize, Serialize};
/// The document a control acts on or a row stands for.
fn document_ref(document: &Document) -> ObjRef {
    ObjRef::new("document", &document.id)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Docs {
    pub base: String,
    pub documents: Vec<Document>,
    pub open: Option<Document>,
    pub draft: String,
    pub dirty: bool,
    pub status: Status,
    /// The body has been tapped, so on a phone it has the keyboard. A desktop focuses
    /// the body whenever a document is open.
    #[serde(default)]
    pub editing: bool,
    /// Writer's File menu is open (Ubuntu), listing the service's documents.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub menu: bool,
    /// Writer's other menu that is open (Ubuntu): Edit, View … Help, by its label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_menu: Option<String>,
    /// Bold is on (Ctrl+B): what is typed next is bold.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    /// The draft's bold runs, as byte ranges of `draft`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bold_runs: Vec<(usize, usize)>,
    /// Word's Save As over a new document (Windows: its first Ctrl+S asks where), holding
    /// the File name field; `save_as_selected` while its text is selected, so typing
    /// replaces it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_as: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub save_as_selected: bool,
}
impl Docs {
    pub const KIND: &'static str = "docs";
    pub fn launch(argument: &str, window: u64, _clock_us: u64) -> (Self, Vec<AppEffect>) {
        let app = Self {
            base: if argument.is_empty() {
                "http://docs.internal/".into()
            } else {
                argument.to_owned()
            },
            documents: vec![],
            open: None,
            draft: String::new(),
            dirty: false,
            status: Status::Loading,
            editing: false,
            menu: false,
            open_menu: None,
            bold: false,
            bold_runs: vec![],
            save_as: None,
            save_as_selected: false,
        };
        let effects = vec![app.request(window, "list", "GET", "/api/documents", String::new())];
        (app, effects)
    }
    pub fn kind(&self) -> &'static str {
        Self::KIND
    }
    pub fn title(&self, theme: DesktopTheme) -> String {
        match theme {
            DesktopTheme::Windows => "Word",
            DesktopTheme::Ubuntu => "Writer",
            DesktopTheme::Android => "Docs",
            _ => "Pages",
        }
        .into()
    }
    pub fn document(&self) -> String {
        self.open
            .as_ref()
            .map(|d| d.title.clone())
            .unwrap_or_default()
    }
    pub fn caption(&self) -> String {
        self.open
            .as_ref()
            .map(|d| format!("Revision {}", d.revision))
            .unwrap_or_default()
    }
    pub fn modified(&self) -> bool {
        self.dirty
    }
    /// Structure, not data (`NativeApp::abstract_state`): whether the list has rows,
    /// whether a document is open and whether the service has it yet (a new one's save
    /// creates it, and Word asks for its name first), unsaved edits (Close refuses them),
    /// the phone's editing mode, Writer's File menu, Bold, and Word's Save As with whether
    /// its name is selected and empty. The documents, the text and the names are data.
    pub fn abstract_state(&self) -> serde_json::Value {
        let mut state = serde_json::json!({
            "rows": !self.documents.is_empty(),
            "open": self.open.as_ref().map(|d| if d.id.is_empty() { "new" } else { "saved" }),
            "dirty": self.dirty,
            "editing": self.editing,
            "menu": self.menu,
            "bold": self.bold,
            "save_as": self.save_as.as_ref().map(|name| serde_json::json!({
                "selected": self.save_as_selected,
                "name": !name.trim().is_empty(),
            })),
            "status": self.status.kind(),
        });
        // Writer's other menus, only while one is open, so a state without one reads as it did.
        if let Some(menu) = &self.open_menu {
            state["open_menu"] = serde_json::json!(menu);
        }
        state
    }
    fn request(
        &self,
        window: u64,
        tag: &str,
        method: &str,
        suffix: &str,
        body: String,
    ) -> AppEffect {
        AppEffect::Http {
            window,
            tag: tag.into(),
            method: method.into(),
            url: format!("{}{suffix}", self.base.trim_end_matches('/')),
            body,
        }
    }
    pub fn offline(&mut self, _tag: &str, reason: &str) {
        self.status = Status::offline(reason);
    }
    pub fn http(
        &mut self,
        window: u64,
        tag: &str,
        status: u16,
        body: &str,
    ) -> Result<Vec<AppEffect>, String> {
        self.status = Status::from_status(status, body);
        if self.status != Status::Idle {
            return Ok(vec![]);
        }
        match tag {
            "list" => {
                self.documents = serde_json::from_str(body).unwrap_or_default();
                Ok(vec![])
            }
            "open" => {
                let document: Document = serde_json::from_str(body).unwrap_or_default();
                self.bold_runs.clear();
                self.bold = false;
                self.draft = document.body.clone();
                self.dirty = false;
                self.open = Some(document);
                Ok(vec![])
            }
            // A save returns the committed document, which carries the new revision; an
            // editor that kept its own guess would silently diverge from the service.
            "save" => {
                let document: Document = serde_json::from_str(body).unwrap_or_default();
                // The service keeps the text; the runs stand while the text is what was sent.
                if document.body != self.draft {
                    self.bold_runs.clear();
                }
                self.draft = document.body.clone();
                self.dirty = false;
                self.open = Some(document);
                self.status = Status::Loading;
                Ok(vec![self.request(
                    window,
                    "list",
                    "GET",
                    "/api/documents",
                    String::new(),
                )])
            }
            other => Err(format!("unexpected documents reply {other}")),
        }
    }
    /// Typing with no document open starts a new, untitled one, as a word processor opened
    /// on its blank document takes typing (Word's Document1, WordPad's Document); its first
    /// save creates it in the service.
    fn start_blank(&mut self) {
        if self.open.is_none() {
            self.open = Some(Document {
                title: "Document1".into(),
                ..Document::default()
            });
            self.draft.clear();
            self.bold_runs.clear();
            self.editing = true;
        }
    }
    pub fn text(&mut self, text: &str) -> Result<(), String> {
        if let Some(field) = &mut self.save_as {
            if self.save_as_selected {
                field.clear();
                self.save_as_selected = false;
            }
            push_bounded(field, text, 260);
            return Ok(());
        }
        self.start_blank();
        let start = self.draft.len();
        push_bounded(&mut self.draft, text, 64 * 1024);
        self.mark_bold(start);
        self.dirty = true;
        Ok(())
    }
    /// What was just added from `start` is bold when Bold is on: it extends the run it
    /// follows, or starts one.
    fn mark_bold(&mut self, start: usize) {
        let end = self.draft.len();
        if !self.bold || end == start {
            return;
        }
        match self.bold_runs.last_mut() {
            Some(run) if run.1 == start => run.1 = end,
            _ => self.bold_runs.push((start, end)),
        }
    }
    /// Word's Save As for a new document: its File name field holds the document's name,
    /// selected. Windows' desktop opens it for the first Ctrl+S (`DesktopState::key`).
    pub fn begin_save_as(&mut self) -> bool {
        let new = self.open.as_ref().is_some_and(|d| d.id.is_empty());
        if new && self.save_as.is_none() {
            self.save_as = Some(self.document());
            self.save_as_selected = true;
        }
        new
    }
    fn save_as_key(
        &mut self,
        window: u64,
        key: &str,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String> {
        let field = self.save_as.as_mut().ok_or("Save As is not open")?;
        match key {
            "Escape" => {
                self.save_as = None;
                self.save_as_selected = false;
            }
            "Ctrl+a" | "Ctrl+A" => self.save_as_selected = true,
            "Backspace" => {
                if self.save_as_selected {
                    field.clear();
                    self.save_as_selected = false;
                } else {
                    field.pop();
                }
            }
            "Enter" => {
                // The document is named after the file: `C:\Users\omar\Documents\thanks.rtf`
                // is `thanks.rtf`.
                let name = field
                    .trim()
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or("")
                    .to_owned();
                if name.is_empty() {
                    return Err("enter a file name".into());
                }
                self.save_as = None;
                self.save_as_selected = false;
                if let Some(d) = &mut self.open {
                    d.title = name;
                }
                return self.click(window, "docs:save", clock_us);
            }
            other => return Err(format!("the Save As dialog takes no {other}")),
        }
        Ok(vec![])
    }
    pub fn key(&mut self, window: u64, key: &str, clock_us: u64) -> Result<Vec<AppEffect>, String> {
        if self.save_as.is_some() {
            return self.save_as_key(window, key, clock_us);
        }
        if key == "Escape" && (self.open_menu.is_some() || self.menu) {
            self.open_menu = None;
            self.menu = false;
            return Ok(vec![]);
        }
        match key {
            // Bold on or off for what is typed next, as Word's and Writer's Ctrl+B is with
            // nothing selected.
            "Ctrl+b" | "Ctrl+B" | "Meta+b" => {
                self.start_blank();
                self.bold = !self.bold;
                Ok(vec![])
            }
            "Backspace" => {
                if self.open.is_none() {
                    return Err("no document is open".into());
                }
                self.draft.pop();
                let end = self.draft.len();
                self.bold_runs.retain_mut(|run| {
                    run.1 = run.1.min(end);
                    run.0 < run.1
                });
                self.dirty = true;
                Ok(vec![])
            }
            "Enter" => {
                self.start_blank();
                let start = self.draft.len();
                self.draft.push('\n');
                self.mark_bold(start);
                self.dirty = true;
                Ok(vec![])
            }
            "Ctrl+s" | "Meta+s" => self.click(window, "docs:save", clock_us),
            // The caret to the end of the document (Word's, Writer's and Google Docs' Ctrl+End, Pages' Command+Down)
            // or of its line (End). This editor types at the end, so the caret is there already: the key is taken
            // and nothing moves. Refused, it read as a key the application does not have: model agents pressed
            // these 106 times in the harness study's document runs (concern 65).
            "Ctrl+End" | "Meta+ArrowDown" | "End" => {
                if self.open.is_none() {
                    return Err("no document is open".into());
                }
                self.editing = true;
                Ok(vec![])
            }
            // A modifier pressed alone types nothing.
            "Shift" | "Control" | "Ctrl" | "Alt" | "Meta" | "Super" => Ok(vec![]),
            other => Err(format!("unsupported documents key {other}")),
        }
    }
    pub fn click(
        &mut self,
        window: u64,
        target: &str,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String> {
        let command = target
            .strip_prefix("docs:")
            .ok_or("interaction does not belong to documents")?;
        // Writer's other menus open and close from their headers, and one at a time.
        if let Some(label) = command
            .strip_prefix("menu:")
            .filter(|l| writer::MENU_ROWS.iter().any(|(m, _, _)| m == l))
        {
            self.menu = false;
            self.open_menu = if self.open_menu.as_deref() == Some(label) {
                None
            } else {
                Some(label.to_owned())
            };
            return Ok(vec![]);
        }
        if command == "menu" || command.starts_with("menu:") {
            self.open_menu = None;
            self.menu = !self.menu;
            return Ok(vec![]);
        }
        // An open menu holds the pointer, as GTK's does: a click on one of its rows is the
        // row's (each insensitive: nothing happens), and a click anywhere else closes it
        // and does nothing more.
        if let Some(open) = self.open_menu.clone() {
            let row = format!("off:menu-{}-", open.to_lowercase());
            if command.starts_with(&row) {
                return Err("this command is not available".into());
            }
            self.open_menu = None;
            return Ok(vec![]);
        }
        // Writer's menu bar, tool bars and sidebar decks hold their controls; a click on
        // one between its controls does nothing.
        if command.starts_with("noop:") {
            return Ok(vec![]);
        }
        // Writer's other menus and tool bars have no model; their actions are refused.
        if command.starts_with("off:") {
            return Err("this command is not available".into());
        }
        self.menu = false;
        self.open_menu = None;
        match command {
            "bold" => {
                self.start_blank();
                self.bold = !self.bold;
                Ok(vec![])
            }
            "dialog" | "dialog:field" if self.save_as.is_some() => Ok(vec![]),
            "dialog:save" if self.save_as.is_some() => self.save_as_key(window, "Enter", clock_us),
            "dialog:cancel" if self.save_as.is_some() => {
                self.save_as_key(window, "Escape", clock_us)
            }
            "reload" => {
                self.status = Status::Loading;
                Ok(vec![self.request(
                    window,
                    "list",
                    "GET",
                    "/api/documents",
                    String::new(),
                )])
            }
            "body" => {
                if self.open.is_none() {
                    return Err("no document is open".into());
                }
                self.editing = true;
                Ok(vec![])
            }
            // A phone's back button returns to the list. An edit not yet saved would be
            // lost, so it is refused rather than dropped: Save first.
            "close" => {
                if self.open.is_none() {
                    return Err("no document is open".into());
                }
                if self.dirty {
                    return Err("save the document before closing it".into());
                }
                self.open = None;
                self.editing = false;
                self.draft.clear();
                Ok(vec![])
            }
            "save" => {
                let document = self.open.clone().ok_or("no document is open")?;
                // A saved document with no change since is not written again: Pages and Word make no new version of
                // an unchanged document, and Google Docs has nothing to save. Its Save showed it saved ("Save", no
                // "Edited"); a second press making a revision is what failed 30 of the harness study's model runs.
                if !self.dirty && !document.id.is_empty() {
                    return Ok(vec![]);
                }
                self.status = Status::Loading;
                if document.id.is_empty() {
                    return Ok(vec![self.request(
                        window,
                        "save",
                        "POST",
                        "/api/documents",
                        serde_json::json!({
                            "title": document.title,
                            "type": "doc",
                            "body": self.draft,
                        })
                        .to_string(),
                    )]);
                }
                Ok(vec![self.request(
                    window,
                    "save",
                    "POST",
                    &format!("/api/documents/{}", document.id),
                    serde_json::json!({
                        "revision": document.revision,
                        "body": self.draft,
                    })
                    .to_string(),
                )])
            }
            rest => {
                let id = rest
                    .strip_prefix("open:")
                    .ok_or_else(|| format!("unknown documents command {command}"))?;
                if !self.documents.iter().any(|d| d.id == id) {
                    return Err("document not found".into());
                }
                self.editing = false;
                self.status = Status::Loading;
                Ok(vec![self.request(
                    window,
                    "open",
                    "GET",
                    &format!("/api/documents/{id}"),
                    String::new(),
                )])
            }
        }
    }
    pub fn page(&self, page: &mut cw_protocol::Page) {
        use cw_protocol::PageElement as E;
        let act = |url: &str| cw_protocol::PageAction {
            method: "APP".into(),
            url: url.into(),
            fields: Default::default(),
        };
        page.elements.push(E::Heading {
            id: "docs-title".into(),
            text: self.document(),
            level: 2,
        });
        if let Some(text) = self.status.notice() {
            page.elements.push(E::Text {
                id: "docs-status".into(),
                text: text.into(),
            });
        }
        page.elements.push(E::Button {
            id: "docs:reload".into(),
            text: "Reload".into(),
            action: act("docs:reload"),
            style: None,
        });
        for document in &self.documents {
            page.elements.push(E::Button {
                id: format!("docs:open:{}", document.id),
                text: document.title.clone(),
                action: act(&format!("docs:open:{}", document.id)),
                style: None,
            });
        }
        if self.open.is_some() {
            page.elements.push(E::Input {
                id: "docs-body".into(),
                label: "Content".into(),
                value: self.draft.clone(),
                placeholder: String::new(),
            });
            page.elements.push(E::Button {
                id: "docs:save".into(),
                text: "Save".into(),
                action: act("docs:save"),
                style: None,
            });
        }
    }
    pub fn render(&self, p: &mut Painter, env: &crate::AppEnv<'_>) {
        let (theme, width, height) = (env.theme, env.width, env.height);
        if theme == DesktopTheme::Ubuntu && width >= 520 {
            return self.render_writer(p, width, height);
        }
        let l = look(theme);
        p.scene.background = l.surface;
        let narrow = theme.mobile() || width < 520;
        // A phone shows the open document, or the list: one thing at a time.
        if narrow {
            if let Some(document) = &self.open {
                self.document_view(p, &l, width, height, document, 0, 0);
                return;
            }
        }
        let screen = screen(p, theme, &l, width, height as i32, &self.title(theme));
        let top = screen.top;
        let list_w = if narrow { width } else { 216 };
        if !screen.scrolls() {
            p.box_(Rect::new(0, top, list_w, height), l.chrome, 0);
        }
        // Google's Docs on Android refreshes on its own: its list starts under the app bar,
        // with no Reload row (Material 3 lists, as LibreOffice Viewer's start screen has).
        let android = theme == DesktopTheme::Android;
        if !android {
            let tools = p.begin_region("toolbar", "documents");
            action(
                p,
                &l,
                Rect::new(8, top + 8, list_w.saturating_sub(16), 26),
                "Reload",
                "docs:reload",
                false,
            );
            p.end_region(tools);
        }
        let head = if android { 8 } else { 40 };
        let status = p.begin_region("status", "status");
        if let Some(text) = self.status.notice() {
            notice(p, list_w, top + head + 4, text);
        } else if self.documents.is_empty() {
            notice(p, list_w, top + head + 4, "No documents");
        }
        p.end_region(status);
        let rows = p.begin_region("list", "documents");
        let list = screen.column(
            p,
            "list",
            Rect::new(
                0,
                top + head,
                list_w,
                (height as i32 - top - head).max(1) as u32,
            ),
        );
        for (index, document) in self.documents.iter().enumerate() {
            let r = Rect::new(
                4,
                list.top + 2 + index as i32 * 32,
                list_w.saturating_sub(8),
                30,
            );
            let on = self.open.as_ref().is_some_and(|d| d.id == document.id);
            let item = p.begin_region_of("item", "document", Some(document_ref(document)));
            let open = format!("docs:open:{}", document.id);
            p.button(
                r,
                if on {
                    l.selection
                } else {
                    cw_scene::Color::TRANSPARENT
                },
                l.radius,
                &open,
                &document.title,
            );
            p.tag(&open, Some("listitem"), Some(document_ref(document)));
            p.select_last(on);
            p.left(
                r.x + 10,
                r.y + 3,
                r.width.saturating_sub(18),
                &document.title,
                13,
                if on { l.accent } else { INK },
            );
            p.left(
                r.x + 10,
                r.y + 17,
                r.width.saturating_sub(18),
                &format!("{} · rev {}", document.owner, document.revision),
                10,
                MUTED,
            );
            p.end_region(item);
        }
        list.end(p);
        p.end_region(rows);
        screen.end(p);
        if narrow {
            return;
        }
        let x = list_w as i32;
        p.vline(x, top, height, LINE);
        match &self.open {
            Some(document) => {
                self.document_view(p, &l, width, height, document, x, top);
                if theme == DesktopTheme::Windows {
                    // Word's Bold sits in the document's tool bar, beside Save.
                    let detail =
                        p.begin_region_of("detail", "document", Some(document_ref(document)));
                    let tools = p.begin_region("toolbar", "document");
                    self.bold_button(p, &l, width, top);
                    p.end_region(tools);
                    p.end_region(detail);
                }
            }
            None => {
                let status = p.begin_region("status", "status");
                notice_in(
                    p,
                    x,
                    width.saturating_sub(list_w),
                    top + 40,
                    "Select a document",
                );
                p.end_region(status);
            }
        }
        self.save_as_dialog(p, width, height);
    }
    /// The open document from `x` rightwards and `top` down; its body scrolls. On a
    /// phone it fills the screen and a back button returns to the list.
    #[allow(clippy::too_many_arguments)]
    fn document_view(
        &self,
        p: &mut Painter,
        l: &super::look::Look,
        width: u32,
        height: u32,
        document: &Document,
        x: i32,
        top: i32,
    ) {
        let detail = p.begin_region_of("detail", "document", Some(document_ref(document)));
        let mut left = x + 20;
        if x == 0 {
            let back = Rect::new(4, top + 6, 40, 30);
            let nav = p.begin_region("navigation", "back");
            p.button(
                back,
                cw_scene::Color::TRANSPARENT,
                l.radius,
                "docs:close",
                "Back to documents",
            );
            if self.dirty {
                p.disabled("Save the document before going back to the list");
            }
            p.end_region(nav);
            p.symbol("chevron-left", back.x + 8, back.y + 5, 20, l.accent);
            left = back.x + back.width as i32 + 4;
        }
        p.strong(
            left,
            top + 12,
            (width as i32 - left - 100).max(0) as u32,
            &document.title,
            16,
            INK,
        );
        let tools = p.begin_region("toolbar", "document");
        action(
            p,
            l,
            Rect::new(width as i32 - 92, top + 10, 82, 26),
            if self.dirty { "Save •" } else { "Save" },
            "docs:save",
            self.dirty,
        );
        p.tag("docs:save", None, Some(document_ref(document)));
        p.end_region(tools);
        let body = Rect::new(
            x + 20,
            top + 46,
            width.saturating_sub(x as u32 + 40),
            (height as i32 - top - 58).max(1) as u32,
        );
        p.region(body, "docs:body", "Document body");
        p.tag(
            "docs:body",
            Some("document"),
            Some(ObjRef::new("document.body", &document.id)),
        );
        let pane = p.pane("body", body);
        if self.bold_runs.is_empty() {
            p.paragraph(
                body.x,
                pane.top(),
                body.width.saturating_sub(14),
                &self.draft,
                13,
                INK,
            );
        } else {
            // Line by line, a line typed in bold drawn bold.
            let mut y = pane.top();
            let mut start = 0;
            for line in self.draft.split('\n') {
                let end = start + line.len();
                let bold =
                    !line.is_empty() && self.bold_runs.iter().any(|&(a, b)| a <= start && end <= b);
                let h = if bold {
                    p.strong(body.x, y, body.width.saturating_sub(14), line, 13, INK)
                } else {
                    p.paragraph(body.x, y, body.width.saturating_sub(14), line, 13, INK)
                };
                y += h.max(17) as i32;
                start = end + 1;
            }
        }
        p.end_pane(pane, None);
        p.end_region(detail);
    }
    /// Word's Bold, beside Save: pressed while what is typed is bold (Ctrl+B).
    fn bold_button(&self, p: &mut Painter, l: &super::look::Look, width: u32, top: i32) {
        let r = Rect::new(width as i32 - 130, top + 10, 30, 26);
        p.button(
            r,
            if self.bold {
                l.selection
            } else {
                cw_scene::Color::TRANSPARENT
            },
            l.radius,
            "docs:bold",
            "Bold",
        );
        if let Some(n) = p.scene.nodes.last_mut() {
            n.state.get_or_insert_with(Default::default).checked = Some(self.bold);
        }
        p.symbol("bold", r.x + 7, r.y + 5, 16, INK);
    }
    /// Word's Save As over the window: the File name field, the type, Save and Cancel.
    fn save_as_dialog(&self, p: &mut Painter, width: u32, height: u32) {
        let Some(field) = &self.save_as else {
            return;
        };
        use cw_scene::Color;
        let (dw, dh) = (560.min(width.saturating_sub(24)), 220);
        let o = Rect::new(
            (width as i32 - dw as i32) / 2,
            (height as i32 - dh as i32) / 2,
            dw,
            dh,
        );
        let dialog = p.begin_region_of("dialog", "save-as", self.open.as_ref().map(document_ref));
        p.border(o, Color::rgb(249, 249, 249), 8, Color(0, 0, 0, 40));
        p.region(o, "docs:dialog", "Save As");
        if let Some(s) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
            s.role = "dialog".into();
        }
        p.left(o.x + 16, o.y + 14, 200, "Save As", 12, INK);
        p.presentation();
        p.left(o.x + 24, o.y + 62, 110, "File name:", 12, INK);
        let name = Rect::new(o.x + 130, o.y + 56, dw.saturating_sub(154), 28);
        p.border(name, Color::WHITE, 4, Color(0, 0, 0, 40));
        p.region(name, "docs:dialog:field", "File name:");
        p.announce("combo box", Some(field));
        p.tag(
            "docs:dialog:field",
            None,
            Some(ObjRef::new("document.name", "")),
        );
        p.left(
            name.x + 8,
            name.y + 6,
            name.width.saturating_sub(16),
            field,
            12,
            INK,
        );
        p.left(o.x + 24, o.y + 104, 110, "Save as type:", 12, INK);
        let kind = Rect::new(o.x + 130, o.y + 98, dw.saturating_sub(154), 28);
        p.border(kind, Color::WHITE, 4, Color(0, 0, 0, 40));
        p.insensitive(kind, "docs:off:type", "Save as type:");
        p.left(
            kind.x + 8,
            kind.y + 6,
            300,
            "Word Document (*.docx)",
            12,
            INK,
        );
        for (i, (action, label)) in [
            ("docs:dialog:save", "Save"),
            ("docs:dialog:cancel", "Cancel"),
        ]
        .into_iter()
        .enumerate()
        {
            let r = Rect::new(
                o.x + dw as i32 - 196 + i as i32 * 96,
                o.y + dh as i32 - 44,
                88,
                28,
            );
            p.border(r, Color::WHITE, 4, Color(0, 0, 0, 40));
            p.region(r, action, label);
            p.center(r.x, r.y + 6, r.width, label, 12, INK);
        }
        p.end_region(dialog);
    }
}

/// LibreOffice Writer 24.2 on Ubuntu, laid out from the real window's client area at
/// 1212x749 (fidelity/tours, tour-docs): the menu bar, the Standard and Formatting tool
/// bars, the document with its ruler and page, the sidebar's tab bar and the status bar.
/// Each control is announced with the role, name and bounds LibreOffice's AT-SPI gives it,
/// which for the two tool bars lie 27 px above the buttons drawn (the reference's own).
/// File opens the service's documents and Save saves the open one; the document takes
/// typing once one is open. Everything else Writer has no model for is insensitive.
mod writer {
    /// Standard tool bar: name, check box, x, width, drawn insensitive, glyph.
    pub const STANDARD: [(&str, bool, i32, u32, bool, &str); 30] = [
        ("New", false, 4, 44, false, "document"),
        ("Open", false, 48, 44, false, "folder"),
        ("Save", false, 92, 44, false, "download"),
        ("Export Directly as PDF", false, 144, 33, false, "file"),
        ("Print", false, 177, 33, false, "document"),
        ("Print Preview", true, 210, 33, false, "search"),
        ("Cut", false, 251, 33, true, "scissors"),
        ("Copy", false, 284, 33, true, "copy"),
        ("Paste", false, 317, 44, true, "paste"),
        ("Clone", true, 369, 33, false, "brush"),
        ("Undo", false, 410, 44, true, "undo"),
        ("Redo", false, 454, 44, true, "redo"),
        ("Find and Replace", true, 506, 33, false, "search"),
        ("Spelling", false, 539, 33, false, "check"),
        ("Formatting Marks", true, 572, 33, false, "¶"),
        ("Table", false, 613, 44, false, "grid"),
        ("Image", false, 657, 33, false, "image"),
        ("Chart", false, 690, 33, false, "sort"),
        ("Text Box", false, 723, 33, false, "text-tool"),
        ("Page Break", false, 764, 33, false, "split"),
        ("Field", true, 797, 44, false, "hash"),
        ("Symbol", false, 841, 44, false, "Ω"),
        ("Hyperlink", true, 893, 33, false, "link"),
        ("Footnote", false, 926, 33, false, "document"),
        ("Endnote", false, 959, 33, false, "document"),
        ("Bookmark", false, 992, 33, false, "star-filled"),
        ("Cross-reference", false, 1025, 33, false, "arrow-right"),
        ("Comment", false, 1066, 33, false, "comment"),
        ("Track Changes Functions", true, 1099, 33, false, "pencil"),
        ("Line", true, 1140, 33, false, "line-tool"),
    ];
    /// Formatting tool bar: name, check box, x, width, glyph.
    pub const FORMATTING: [(&str, bool, i32, u32, &str); 16] = [
        ("Update", false, 206, 33, "reload"),
        ("New", false, 239, 33, "plus"),
        ("Bold", true, 618, 33, "bold"),
        ("Italic", true, 651, 33, "italic"),
        ("Underline", true, 684, 44, "U"),
        ("Strikethrough", true, 728, 33, "S"),
        ("Superscript", true, 769, 33, "A²"),
        ("Subscript", true, 802, 33, "A₂"),
        ("Clear", false, 843, 33, "eraser"),
        ("Font Color", false, 884, 44, "A"),
        (
            "Character Highlighting Color",
            false,
            928,
            44,
            "highlighter",
        ),
        ("Left", true, 980, 33, "list-view"),
        ("Center", true, 1013, 33, "list-view"),
        ("Right", true, 1046, 33, "list-view"),
        ("Justified", true, 1079, 33, "list-view"),
        ("Unordered List", true, 1120, 44, "list-view"),
    ];
    /// The menu bar: name, x, width.
    pub const MENUS: [(&str, i32, u32); 11] = [
        ("File", 0, 40),
        ("Edit", 40, 43),
        ("View", 83, 49),
        ("Insert", 132, 54),
        ("Format", 186, 66),
        ("Styles", 252, 55),
        ("Table", 307, 53),
        ("Form", 360, 52),
        ("Tools", 412, 51),
        ("Window", 463, 71),
        ("Help", 534, 47),
    ];
    /// A menu's name, its width and its rows (role, name, top).
    pub type MenuRows = (
        &'static str,
        u32,
        &'static [(&'static str, &'static str, i32)],
    );
    /// Writer's other menus as LibreOffice 24.2 opens them on Ubuntu (store the Ubuntu reference store, read from the
    /// AT-SPI tree of an opened menu: each row's role, name and top, 51 px below the window's
    /// client top for the menu bar's height, the first row 26 px down): menu, width, rows. A
    /// separator is a row of role `separator`, 1 px tall. The model has none of these commands, so
    /// every row is listed insensitive.
    pub const MENU_ROWS: [MenuRows; 10] = [
        (
            "Edit",
            287,
            &[
                ("menuitem", "Undo", 26),
                ("menuitem", "Redo", 52),
                ("menuitem", "Repeat", 78),
                ("separator", "", 104),
                ("menuitem", "Cut", 105),
                ("menuitem", "Copy", 131),
                ("menuitem", "Paste", 157),
                ("menu", "Paste Special", 183),
                ("separator", "", 209),
                ("menuitem", "Select All", 210),
                ("menu", "Selection Mode", 236),
                ("menuitem", "Select Text", 262),
                ("separator", "", 288),
                ("menuitem", "Find...", 289),
                ("menuitem", "Find and Replace...", 315),
                ("menuitem", "Go to Page...", 341),
                ("separator", "", 367),
                ("menu", "Track Changes", 368),
                ("menu", "Comment", 394),
                ("separator", "", 420),
                ("menuitem", "Hyperlink", 421),
                ("menu", "Reference", 447),
                ("menuitem", "Fields...", 473),
                ("menuitem", "External Links...", 499),
                ("menu", "OLE Object", 525),
                ("menuitem", "Exchange Database...", 551),
                ("separator", "", 577),
                ("menuitem", "Direct Cursor Mode", 578),
                ("menuitem", "Edit Mode", 604),
            ],
        ),
        (
            "View",
            313,
            &[
                ("menuitem", "Normal", 26),
                ("menuitem", "Web", 52),
                ("separator", "", 78),
                ("menuitem", "User Interface...", 79),
                ("menu", "Toolbars", 105),
                ("menuitem", "Status Bar", 131),
                ("separator", "", 157),
                ("menu", "Rulers", 158),
                ("menu", "Scrollbars", 184),
                ("menu", "Grid and Helplines", 210),
                ("separator", "", 236),
                ("menuitem", "Formatting Marks", 237),
                ("menuitem", "Text Boundaries", 263),
                ("menuitem", "Table Boundaries", 289),
                ("menuitem", "Section Boundaries", 315),
                ("menuitem", "Images and Charts", 341),
                ("menuitem", "Whitespace", 367),
                ("separator", "", 393),
                ("menuitem", "Show Tracked Changes", 394),
                ("menuitem", "Comments", 420),
                ("menuitem", "Resolved Comments", 446),
                ("separator", "", 472),
                ("menuitem", "Field Shadings", 473),
                ("menuitem", "Field Names", 499),
                ("menuitem", "Field Hidden Paragraphs", 525),
                ("separator", "", 551),
                ("menuitem", "Sidebar", 552),
                ("menuitem", "Styles", 578),
                ("menuitem", "Gallery", 604),
                ("menuitem", "Navigator", 630),
                ("menuitem", "Data Sources", 656),
                ("separator", "", 682),
                ("menuitem", "Full Screen", 683),
                ("menu", "Zoom", 709),
            ],
        ),
        (
            "Insert",
            350,
            &[
                ("menuitem", "Page Break", 26),
                ("menu", "More Breaks", 52),
                ("separator", "", 78),
                ("menuitem", "Image...", 79),
                ("menuitem", "Chart...", 105),
                ("menu", "Media", 131),
                ("menu", "OLE Object", 157),
                ("menu", "Shape", 183),
                ("separator", "", 209),
                ("menuitem", "Section...", 210),
                ("menuitem", "Text from File...", 236),
                ("separator", "", 262),
                ("menuitem", "Text Box", 263),
                ("menuitem", "Comment", 289),
                ("menu", "Frame", 315),
                ("menuitem", "Fontwork...", 341),
                ("menuitem", "Caption...", 367),
                ("separator", "", 393),
                ("menuitem", "Hyperlink...", 394),
                ("menuitem", "Bookmark...", 420),
                ("menuitem", "Cross-reference...", 446),
                ("separator", "", 472),
                ("menuitem", "Special Character...", 473),
                ("menu", "Formatting Mark", 499),
                ("menuitem", "Horizontal Line", 525),
                ("separator", "", 551),
                ("menu", "Footnote and Endnote", 552),
                ("menu", "Table of Contents and Index", 578),
                ("separator", "", 604),
                ("menuitem", "Page Number...", 605),
                ("menu", "Field", 631),
                ("menu", "Header and Footer", 657),
                ("separator", "", 683),
                ("menuitem", "Envelope...", 684),
                ("menuitem", "Signature Line...", 710),
            ],
        ),
        (
            "Format",
            300,
            &[
                ("menu", "Spacing", 26),
                ("menu", "Align Text", 52),
                ("separator", "", 78),
                ("menuitem", "Clone Formatting", 79),
                ("menuitem", "Clear Direct Formatting", 105),
                ("menu", "Spotlight", 131),
                ("separator", "", 157),
                ("menuitem", "Character...", 158),
                ("menuitem", "Paragraph...", 184),
                ("menu", "Lists", 210),
                ("menuitem", "Bullets and Numbering...", 236),
                ("menuitem", "Theme...", 262),
                ("separator", "", 288),
                ("menuitem", "Page Style...", 289),
                ("menuitem", "Title Page...", 315),
                ("menuitem", "Comments...", 341),
                ("menuitem", "Asian Phonetic Guide...", 367),
                ("menuitem", "Columns...", 393),
                ("menuitem", "Watermark...", 419),
                ("menuitem", "Sections...", 445),
                ("separator", "", 471),
                ("menu", "Image", 472),
                ("menu", "Text Box and Shape", 498),
                ("menu", "Frame and Object", 524),
                ("menuitem", "Name...", 550),
                ("menuitem", "Alt Text...", 576),
                ("separator", "", 602),
                ("menu", "Anchor", 603),
                ("menu", "Wrap", 629),
                ("menu", "Arrange", 655),
                ("menu", "Rotate or Flip", 681),
                ("menu", "Group", 707),
            ],
        ),
        (
            "Styles",
            336,
            &[
                ("menuitem", "Body Text", 26),
                ("menuitem", "Title", 52),
                ("menuitem", "Subtitle", 78),
                ("menuitem", "Heading 1", 104),
                ("menuitem", "Heading 2", 130),
                ("menuitem", "Heading 3", 156),
                ("menuitem", "Block Quotation", 182),
                ("menuitem", "Preformatted Text", 208),
                ("separator", "", 234),
                ("menuitem", "No Character Style", 235),
                ("menuitem", "Emphasis", 261),
                ("menuitem", "Strong Emphasis", 287),
                ("menuitem", "Quotation", 313),
                ("menuitem", "Source Text", 339),
                ("separator", "", 365),
                ("menuitem", "No List", 366),
                ("menuitem", "Bullet • List Style", 392),
                ("menuitem", "Numbering 123 List Style", 418),
                ("menuitem", "Numbering ABC List Style", 444),
                ("menuitem", "Numbering abc List Style", 470),
                ("menuitem", "Numbering IVX List Style", 496),
                ("menuitem", "Numbering ivx List Style", 522),
                ("separator", "", 548),
                ("menuitem", "Edit Style...", 549),
                ("menuitem", "Update Selected Style", 575),
                ("menuitem", "New Style from Selection", 601),
                ("menuitem", "Load Styles from Template", 627),
                ("separator", "", 653),
                ("menuitem", "Manage Styles", 654),
            ],
        ),
        (
            "Table",
            345,
            &[
                ("menuitem", "Insert Table...", 26),
                ("menu", "Insert", 52),
                ("menu", "Delete", 78),
                ("menu", "Select", 104),
                ("menu", "Size", 130),
                ("separator", "", 156),
                ("menuitem", "Merge Cells", 157),
                ("menuitem", "Split Cells...", 183),
                ("menuitem", "Merge Table", 209),
                ("menuitem", "Split Table...", 235),
                ("separator", "", 261),
                ("menuitem", "Protect Cells", 262),
                ("menuitem", "Unprotect Cells", 288),
                ("separator", "", 314),
                ("menuitem", "AutoFormat Styles...", 315),
                ("menuitem", "Number Format...", 341),
                ("menuitem", "Number Recognition", 367),
                ("separator", "", 393),
                ("menuitem", "Header Rows Repeat Across Pages", 394),
                ("menuitem", "Row to Break Across Pages", 420),
                ("separator", "", 446),
                ("menu", "Convert", 447),
                ("menuitem", "Edit Formula", 473),
                ("menuitem", "Sort...", 499),
                ("separator", "", 525),
                ("menuitem", "Properties...", 526),
            ],
        ),
        (
            "Form",
            224,
            &[
                ("menuitem", "Design Mode", 26),
                ("menuitem", "Control Wizards", 52),
                ("separator", "", 78),
                ("menuitem", "Label", 79),
                ("menuitem", "Text Box", 105),
                ("menuitem", "Check Box", 131),
                ("menuitem", "Option Button", 157),
                ("menuitem", "List Box", 183),
                ("menuitem", "Combo Box", 209),
                ("separator", "", 235),
                ("menuitem", "Push Button", 236),
                ("menuitem", "Image Button", 262),
                ("separator", "", 288),
                ("menuitem", "Formatted Field", 289),
                ("menu", "More Fields", 315),
                ("separator", "", 341),
                ("menuitem", "Group Box", 342),
                ("separator", "", 368),
                ("menuitem", "Image Control", 369),
                ("menuitem", "File Selection", 395),
                ("separator", "", 421),
                ("menuitem", "Table Control", 422),
                ("menuitem", "Navigation Bar", 448),
                ("separator", "", 474),
                ("menuitem", "Control Properties...", 475),
                ("menuitem", "Form Properties...", 501),
                ("separator", "", 527),
                ("menuitem", "Form Navigator...", 528),
                ("menuitem", "Activation Order...", 554),
                ("separator", "", 580),
                ("menuitem", "Open in Design Mode", 581),
                ("menuitem", "Automatic Control Focus", 607),
                ("separator", "", 633),
                ("menu", "Content Controls", 634),
            ],
        ),
        (
            "Tools",
            348,
            &[
                ("menuitem", "Automatic Spell Checking", 26),
                ("menuitem", "Thesaurus...", 52),
                ("menu", "Language", 78),
                ("menuitem", "Word Count...", 104),
                ("menuitem", "Accessibility Check...", 130),
                ("menuitem", "Automatic Accessibility Checking", 156),
                ("separator", "", 182),
                ("menu", "AutoCorrect", 183),
                ("menuitem", "AutoText...", 209),
                ("menuitem", "ImageMap", 235),
                ("separator", "", 261),
                ("menuitem", "Redact", 262),
                ("menuitem", "Auto-Redact", 288),
                ("separator", "", 314),
                ("menuitem", "Heading Numbering...", 315),
                ("menuitem", "Line Numbering...", 341),
                ("menuitem", "Footnote/Endnote Settings...", 367),
                ("separator", "", 393),
                ("menuitem", "Mail Merge Wizard...", 394),
                ("menuitem", "Bibliography Database", 420),
                ("menuitem", "Address Book Source...", 446),
                ("separator", "", 472),
                ("menu", "Update", 473),
                ("menu", "Protect Document", 499),
                ("menuitem", "Calculate", 525),
                ("menuitem", "Sort...", 551),
                ("separator", "", 577),
                ("menu", "Macros", 578),
                ("menuitem", "Development Tools", 604),
                ("separator", "", 630),
                ("menuitem", "XML Filter Settings...", 631),
                ("menuitem", "Extensions...", 657),
                ("menuitem", "Customize...", 683),
                ("menuitem", "Options...", 709),
            ],
        ),
        (
            "Window",
            316,
            &[
                ("menuitem", "New Window", 26),
                ("menuitem", "Close Window", 52),
                ("separator", "", 78),
                ("menuitem", "{window}", 79),
            ],
        ),
        (
            "Help",
            302,
            &[
                ("menuitem", "LibreOffice Help", 26),
                ("menuitem", "What's This?", 52),
                ("menuitem", "User Guides", 78),
                ("menuitem", "Show Tip of the Day", 104),
                ("menuitem", "Search Commands", 130),
                ("separator", "", 156),
                ("menuitem", "Get Help Online", 157),
                ("menuitem", "Send Feedback", 183),
                ("separator", "", 209),
                ("menuitem", "Restart in Safe Mode...", 210),
                ("separator", "", 236),
                ("menuitem", "Get Involved", 237),
                ("menuitem", "Donate to LibreOffice", 263),
                ("separator", "", 289),
                ("menuitem", "License Information", 290),
                ("menuitem", "About LibreOffice", 316),
            ],
        ),
    ];
    /// The sidebar's decks, each a one-button tool bar.
    pub const DECKS: [(&str, &str); 8] = [
        ("Properties", "sliders"),
        ("Styles", "text-tool"),
        ("Gallery", "image"),
        ("Navigator", "compass"),
        ("Page", "file"),
        ("Style Inspector", "eye"),
        ("Manage Changes", "pencil"),
        ("Accessibility Check", "accessibility"),
    ];
}

impl Docs {
    fn render_writer(&self, p: &mut Painter, width: u32, height: u32) {
        use super::announce::{bar, check_last, control, glyph, quiet};
        use crate::desktop_scene::shared::Align;
        use cw_scene::Color;
        const BG: Color = Color::rgb(250, 250, 250);
        const TEXT: Color = Color::rgb(46, 46, 46);
        const FAINT: Color = Color::rgb(186, 186, 186);
        const SEP: Color = Color::rgb(214, 214, 214);
        const EDGE: Color = Color::rgb(156, 156, 156);
        let (w, h) = (width as i32, height as i32);
        p.scene.background = BG;
        p.box_(Rect::new(0, 0, width, height), BG, 0);
        let off = |name: &str| format!("docs:off:{}", name.to_lowercase().replace(' ', "-"));
        // The document: the ruler and the page on the application background.
        let doc_h = (h - 154).max(40) as u32;
        let doc = Rect::new(0, 104, (w - 71).max(100) as u32, doc_h);
        let page = Rect::new(172, 147, 796, (h - 147 - 25).max(20) as u32);
        p.border(page, Color::WHITE, 0, EDGE);
        let ruler = Rect::new(172, 107, 796, 19);
        p.border(ruler, Color::WHITE, 0, FAINT);
        p.box_(Rect::new(172, 108, 77, 17), BG, 0);
        p.box_(Rect::new(903, 108, 64, 17), BG, 0);
        for i in 0..34 {
            let x = 249 + i * 19;
            if x > 903 {
                break;
            }
            let tall = i % 4 == 0;
            p.vline(
                x,
                if tall { 117 } else { 114 },
                if tall { 6 } else { 3 },
                EDGE,
            );
        }
        p.vline(10, 113, 4, TEXT);
        p.hline(10, 117, 4, TEXT);
        // The text area's corner marks and, in a document that takes typing, its text.
        for (x, dir) in [(249, -1), (891, 1)] {
            p.hline(if dir < 0 { x - 12 } else { x }, 224, 12, FAINT);
            p.vline(x, 212, 12, FAINT);
        }
        let name = self
            .open
            .as_ref()
            .map_or("Untitled 1", |d| d.title.as_str())
            .to_owned();
        let detail = p.begin_region_of("detail", "document", self.open.as_ref().map(document_ref));
        match &self.open {
            Some(document) => {
                let text_area = Rect::new(249, 224, 642, page.height.saturating_sub(100));
                p.paragraph(
                    text_area.x,
                    text_area.y,
                    text_area.width,
                    &self.draft,
                    16,
                    Color::BLACK,
                );
                p.presentation();
                p.region(doc, "docs:body", &format!("{name} - LibreOffice Document"));
                p.announce("document", None);
                p.tag(
                    "docs:body",
                    None,
                    Some(ObjRef::new("document.body", &document.id)),
                );
            }
            None => {
                if let Some(text) = self.status.notice() {
                    quiet(p, 249, 240, 642, text, 14, EDGE, false, Align::Left);
                }
                // With no document of the service's open there is nothing to type into.
                p.insensitive(
                    doc,
                    "docs:off:document",
                    &format!("{name} - LibreOffice Document"),
                );
                p.announce("document", None);
            }
        }
        p.end_region(detail);
        p.vline(249, 225, 17, TEXT);
        // The vertical scroll bar and the sidebar's tab bar.
        p.box_(
            Rect::new(1144, 107, 8, 330.min(doc_h)),
            Color::rgb(137, 137, 137),
            4,
        );
        p.vline(1153, 104, (h - 127).max(1) as u32, FAINT);
        p.box_(Rect::new(1154, 378, 7, 72), Color::rgb(156, 156, 156), 0);
        let sidebar = p.begin_region("toolbar", "sidebar");
        control(
            p,
            Rect::new(1166, 108, 36, 34),
            "button",
            "Menu",
            None,
            Err("docs:off:sidebar-menu"),
        );
        glyph(p, "menu", 1175, 116, 18, TEXT);
        for (i, (deck, symbol)) in writer::DECKS.iter().enumerate() {
            let y = 147 + 42 * i as i32;
            bar(
                p,
                Rect::new(1162, y, 44, 42),
                &format!("docs:noop:deck-{i}"),
                "toolbar",
                deck,
            );
            control(
                p,
                Rect::new(1166, y + 4, 37, 35),
                "button",
                "",
                None,
                Err(&off(deck)),
            );
            glyph(p, symbol, 1173, y + 10, 22, Color::rgb(52, 101, 164));
        }
        p.end_region(sidebar);
        // The status bar.
        let status = p.begin_region("status", "status");
        let sy = h - 23;
        p.hline(0, sy, width, Color::rgb(230, 230, 230));
        glyph(p, "check", 9, sy + 4, 14, EDGE);
        for (x, text) in [
            (39, "Page 1 of 1"),
            (
                166,
                &*format!(
                    "{} words, {} characters",
                    words(&self.draft_text()),
                    self.draft_text().chars().count()
                ),
            ),
            (401, "Default Page Style"),
            (603, "English (USA)"),
        ] {
            quiet(p, x, sy + 4, 220, text, 12, TEXT, false, Align::Left);
        }
        for x in [157, 347, 539, 777, 851, 1022] {
            p.vline(x, sy + 2, 19, Color::rgb(222, 222, 222));
        }
        p.hline(1048, sy + 11, 104, EDGE);
        p.circle(1093, sy + 11, 5, Color::rgb(220, 220, 220));
        quiet(p, w - 42, sy + 4, 40, "100%", 12, TEXT, false, Align::Left);
        p.end_region(status);
        // The tool bars: announced where LibreOffice reports them, drawn below.
        let bars: [(&str, i32, i32); 2] = [("Standard", 0, 27), ("Formatting", 39, 26)];
        for (name_bar, y, drop) in bars {
            let tools = p.begin_region("toolbar", &name_bar.to_lowercase());
            bar(
                p,
                Rect::new(0, y, width, 39),
                &format!("docs:noop:{}", name_bar.to_lowercase()),
                "toolbar",
                name_bar,
            );
            let items: Vec<(&str, bool, i32, u32, bool, &str)> = if name_bar == "Standard" {
                writer::STANDARD.to_vec()
            } else {
                writer::FORMATTING
                    .iter()
                    .map(|&(n, c, x, bw, g)| (n, c, x, bw, false, g))
                    .collect()
            };
            let mut last_end = 0;
            for (name, check, x, bw, faint, symbol) in items {
                let r = Rect::new(x, y + 2, bw, 35);
                if last_end > 0 && x - last_end >= 6 {
                    p.vline((x + last_end) / 2, y + drop + 6, 23, SEP);
                }
                last_end = x + bw as i32;
                let on = name_bar == "Formatting" && name == "Left";
                let action = if name_bar == "Standard" && name == "Save" && self.open.is_some() {
                    Ok("docs:save")
                } else if name_bar == "Standard" && name == "Open" {
                    Ok("docs:menu:open")
                } else {
                    Err("")
                };
                let off_action = off(&format!("{name_bar}-{name}"));
                control(
                    p,
                    r,
                    if check { "checkbox" } else { "button" },
                    name,
                    Some(if on { "1" } else { "0" }),
                    action.map_err(|_| off_action.as_str()),
                );
                if let (Ok("docs:save"), Some(document)) = (action, &self.open) {
                    p.tag("docs:save", None, Some(document_ref(document)));
                }
                if check {
                    check_last(p, on);
                }
                if faint {
                    super::announce::disable_last(p);
                }
                let drawn = Rect::new(x, y + 2 + drop, bw, 35);
                if on {
                    p.box_(
                        Rect::new(x + 1, drawn.y + 1, bw - 2, 33),
                        Color::rgb(218, 218, 218),
                        4,
                    );
                }
                let ink = if faint { FAINT } else { icon_colour(symbol) };
                if symbol.chars().count() <= 2 {
                    quiet(
                        p,
                        x + 4,
                        drawn.y + 7,
                        25,
                        symbol,
                        18,
                        ink,
                        true,
                        Align::Center,
                    );
                } else {
                    glyph(p, symbol, x + 5, drawn.y + 6, 22, ink);
                }
                if bw > 40 {
                    glyph(
                        p,
                        "chevron-down",
                        x + 31,
                        drawn.y + 13,
                        9,
                        if faint { FAINT } else { TEXT },
                    );
                }
            }
            p.end_region(tools);
        }
        // The Formatting bar's three boxes, which LibreOffice does not expose.
        for (x, bw, divider, text) in [
            (4, 202, 170, "Default Paragraph Style"),
            (280, 220, 465, "Liberation Serif"),
            (509, 100, 574, "12 pt"),
        ] {
            let r = Rect::new(x, 67, bw, 34);
            p.border(r, Color::WHITE, 5, Color::rgb(200, 200, 200));
            p.vline(divider, 68, 32, Color::rgb(220, 220, 220));
            quiet(
                p,
                x + 9,
                75,
                (divider - x - 10) as u32,
                text,
                14,
                TEXT,
                false,
                Align::Left,
            );
            glyph(p, "chevron-down", divider + 12, 79, 12, TEXT);
        }
        for y in [46, 84] {
            quiet(p, w - 14, y - 9, 12, "»", 14, TEXT, false, Align::Left);
        }
        // The menu bar.
        // LibreOffice reports the menu bar 26 px tall and the Standard tool bar's buttons
        // over its lower part; it is announced 18 px tall so that the buttons, whose centres
        // lie 19 px down, fall in their tool bar rather than in it.
        let menubar = p.begin_region("menu", "menubar");
        bar(
            p,
            Rect::new(0, 0, (w - 24).max(1) as u32, 18),
            "docs:noop:menubar",
            "menubar",
            "",
        );
        for (label, x, mw) in writer::MENUS {
            let r = Rect::new(x, 0, mw, 26);
            if label == "File" {
                if self.menu {
                    p.box_(r, Color::rgb(222, 222, 222), 0);
                }
                control(p, r, "menu", label, None, Ok("docs:menu"));
            } else {
                if self.open_menu.as_deref() == Some(label) {
                    p.box_(r, Color::rgb(222, 222, 222), 0);
                }
                control(p, r, "menu", label, None, Ok(&format!("docs:menu:{label}")));
            }
            quiet(p, x + 8, 4, mw, label, 14, TEXT, false, Align::Left);
        }
        // The document's own close button, at the menu bar's end.
        let close = Rect::new(w - 24, 5, 16, 16);
        match &self.open {
            Some(_) if !self.dirty => control(p, close, "button", "", None, Ok("docs:close")),
            _ => control(p, close, "button", "", None, Err("docs:off:close")),
        }
        glyph(p, "close", close.x + 2, close.y + 2, 12, TEXT);
        p.end_region(menubar);
        // File: the service's documents, and Save.
        if self.menu {
            let file = p.begin_region("menu", "file");
            let rows: Vec<(String, Result<String, String>)> =
                std::iter::once(("Reload".to_string(), Ok("docs:reload".to_string())))
                    .chain(
                        self.documents
                            .iter()
                            .map(|d| (d.title.clone(), Ok(format!("docs:open:{}", d.id)))),
                    )
                    .chain(std::iter::once((
                        "Save".to_string(),
                        if self.open.is_some() {
                            Ok("docs:save".to_string())
                        } else {
                            Err("docs:off:save".to_string())
                        },
                    )))
                    .collect();
            let mw = rows
                .iter()
                .map(|(l, _)| p.measure(l, 14, false))
                .max()
                .unwrap_or(100)
                .max(180)
                + 40;
            let menu = Rect::new(0, 26, mw, rows.len() as u32 * 28 + 8);
            p.drop_shadow(menu, 2, 8, 50, 2);
            p.border(menu, Color::WHITE, 2, Color::rgb(200, 200, 200));
            for (i, (label, target)) in rows.iter().enumerate() {
                let r = Rect::new(4, 30 + i as i32 * 28, mw - 8, 28);
                let live = target.is_ok();
                match target {
                    Ok(t) => control(p, r, "menuitem", label, None, Ok(t)),
                    Err(t) => control(p, r, "menuitem", label, None, Err(t)),
                }
                if let Ok(t) = target {
                    let object = match t.strip_prefix("docs:open:") {
                        Some(id) => Some(ObjRef::new("document", id)),
                        None if t == "docs:save" => self.open.as_ref().map(document_ref),
                        None => None,
                    };
                    p.tag(t, None, object);
                }
                quiet(
                    p,
                    r.x + 28,
                    r.y + 6,
                    r.width - 36,
                    label,
                    14,
                    if live { TEXT } else { FAINT },
                    false,
                    Align::Left,
                );
            }
            p.end_region(file);
        }
        // Another menu: its rows as LibreOffice lists them, every one insensitive (the
        // model has none of their commands), under its header.
        if let Some((label, mw, rows)) = self
            .open_menu
            .as_deref()
            .and_then(|o| writer::MENU_ROWS.iter().find(|(m, _, _)| *m == o))
        {
            let x = writer::MENUS
                .iter()
                .find(|(m, _, _)| m == label)
                .map_or(0, |(_, x, _)| *x);
            let bottom = rows.last().map_or(26, |(_, _, y)| y + 26);
            let menu = p.begin_region("menu", &label.to_lowercase());
            let frame = Rect::new(x, 26, *mw, (bottom - 26 + 4) as u32);
            p.drop_shadow(frame, 2, 8, 50, 2);
            p.border(frame, Color::WHITE, 2, Color::rgb(200, 200, 200));
            // The Window menu lists the window by its title, as the shell names it.
            let title = match self.document() {
                d if d.is_empty() => "Untitled 1 — LibreOffice Writer".to_owned(),
                d => format!("{d} — LibreOffice Writer"),
            };
            for (i, (role, name, y)) in rows.iter().enumerate() {
                if *role == "separator" {
                    p.hline(x + 4, *y, mw - 8, Color::rgb(220, 220, 220));
                    continue;
                }
                let name = if *name == "{window}" {
                    title.as_str()
                } else {
                    name
                };
                let r = Rect::new(x, *y, *mw, 26);
                control(
                    p,
                    r,
                    role,
                    name,
                    None,
                    Err(&off(&format!("menu-{}-{i}", label.to_lowercase()))),
                );
                quiet(
                    p,
                    x + 28,
                    y + 5,
                    mw - 36,
                    name,
                    14,
                    FAINT,
                    false,
                    Align::Left,
                );
                if *role == "menu" {
                    glyph(p, "chevron-right", x + *mw as i32 - 20, y + 8, 10, FAINT);
                }
            }
            p.end_region(menu);
        }
    }
    fn draft_text(&self) -> String {
        if self.open.is_some() {
            self.draft.clone()
        } else {
            String::new()
        }
    }
}
fn words(text: &str) -> usize {
    text.split_whitespace().count()
}
/// Colibre's tints, roughly: blue documents, tan folders, red PDF, green checks.
fn icon_colour(symbol: &str) -> cw_scene::Color {
    use cw_scene::Color;
    match symbol {
        "document" | "reload" | "link" | "image" | "sliders" | "accessibility" => {
            Color::rgb(52, 101, 164)
        }
        "folder" => Color::rgb(201, 160, 92),
        "file" | "A" => Color::rgb(204, 40, 40),
        "check" | "plus" | "grid" => Color::rgb(78, 154, 6),
        "brush" | "pencil" | "eraser" => Color::rgb(233, 120, 40),
        "star-filled" | "comment" | "highlighter" => Color::rgb(237, 196, 30),
        "line-tool" => Color::rgb(160, 70, 200),
        _ => Color::rgb(70, 70, 70),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Docs {
        let (mut app, _) = Docs::launch("http://docs.internal/", 1, 0);
        app.http(
            1,
            "list",
            200,
            r#"[{"id":"launch","title":"Atlas launch checklist","owner":"carol","body":"","revision":1}]"#,
        )
        .unwrap();
        app
    }
    #[test]
    fn bold_marks_what_is_typed_and_save_as_names_a_new_document() {
        let mut d = app();
        d.text("Dear Riley,").unwrap();
        d.key(1, "Enter", 0).unwrap();
        d.key(1, "Ctrl+b", 0).unwrap();
        d.text("Thank you.").unwrap();
        d.key(1, "Ctrl+b", 0).unwrap();
        d.key(1, "Enter", 0).unwrap();
        d.text("Omar").unwrap();
        assert_eq!(d.draft, "Dear Riley,\nThank you.\nOmar");
        assert_eq!(d.bold_runs, vec![(12, 22)]);
        // Save As opens with the document's name selected; typing replaces it.
        assert!(d.begin_save_as());
        assert_eq!(d.save_as.as_deref(), Some("Document1"));
        d.key(1, "Ctrl+a", 0).unwrap();
        d.text("C:\\Users\\omar\\Documents\\thanks.rtf").unwrap();
        let effects = d.key(1, "Enter", 0).unwrap();
        assert!(d.save_as.is_none());
        assert!(matches!(
            effects.as_slice(),
            [AppEffect::Http { body, method, .. }] if method == "POST" && body.contains("\"title\":\"thanks.rtf\"")
        ));
        // An open, saved document is not asked about again.
        let mut saved = app();
        saved.text("x").unwrap();
        saved.open.as_mut().unwrap().id = "d1".into();
        assert!(!saved.begin_save_as());
    }
    #[test]
    fn saving_sends_the_revision_the_service_last_committed() {
        let mut app = app();
        app.click(1, "docs:open:launch", 0).unwrap();
        app.http(
            1,
            "open",
            200,
            r#"{"id":"launch","title":"Atlas launch checklist","owner":"carol","body":"Status: pending","revision":1}"#,
        )
        .unwrap();
        assert_eq!(app.draft, "Status: pending");
        app.text(" done").unwrap();
        assert!(app.dirty);
        let effects = app.click(1, "docs:save", 0).unwrap();
        let AppEffect::Http { url, body, .. } = &effects[0] else {
            panic!("expected a request");
        };
        assert_eq!(url, "http://docs.internal/api/documents/launch");
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["revision"], 1);
        assert_eq!(sent["body"], "Status: pending done");
        // The committed reply, not the local guess, becomes the new state.
        app.http(
            1,
            "save",
            200,
            r#"{"id":"launch","title":"Atlas launch checklist","owner":"carol","body":"Status: pending done","revision":2}"#,
        )
        .unwrap();
        assert_eq!(app.open.as_ref().unwrap().revision, 2);
        assert!(!app.dirty);
        // Saved and unchanged since: Save again writes nothing and makes no revision, as Pages and Word do.
        assert!(app.click(1, "docs:save", 0).unwrap().is_empty());
        assert!(app.key(1, "Ctrl+s", 0).unwrap().is_empty());
        // The caret to the end is where this editor types already: taken, and the next line goes at the end.
        for key in ["Ctrl+End", "Meta+ArrowDown", "End"] {
            assert!(app.key(1, key, 0).unwrap().is_empty(), "{key}");
        }
        app.key(1, "Enter", 0).unwrap();
        app.text("Checked").unwrap();
        assert_eq!(app.draft, "Status: pending done\nChecked");
        assert_eq!(
            app.click(1, "docs:save", 0).unwrap().len(),
            1,
            "a changed document is saved again"
        );
    }
    #[test]
    fn a_conflict_is_shown_and_never_silently_overwrites() {
        let mut app = app();
        app.click(1, "docs:open:launch", 0).unwrap();
        app.http(
            1,
            "open",
            200,
            r#"{"id":"launch","title":"T","revision":1,"body":"a"}"#,
        )
        .unwrap();
        app.text("b").unwrap();
        app.click(1, "docs:save", 0).unwrap();
        app.http(1, "save", 409, r#"{"error":"revision conflict"}"#)
            .unwrap();
        assert_eq!(app.status, Status::Offline("revision conflict".into()));
        assert!(app.dirty, "the unsaved edit survives a rejected save");
    }
    #[test]
    fn every_painted_control_is_one_the_model_accepts() {
        let mut base = app();
        base.click(1, "docs:open:launch", 0).unwrap();
        base.http(
            1,
            "open",
            200,
            r#"{"id":"launch","title":"T","revision":1,"body":"a"}"#,
        )
        .unwrap();
        let mut scene = Painter::themed(DesktopTheme::Windows, 900, 600, 0);
        base.render(
            &mut scene,
            &crate::AppEnv {
                theme: DesktopTheme::Windows,
                width: 900,
                height: 600,
                clock_us: 0,
                settings: &crate::SystemSettings::DEFAULT,
                clipboard: None,
                share_to: None,
                editor: None,
                pointer: None,
                files: Default::default(),
            },
        );
        for target in scene
            .scene
            .nodes
            .iter()
            .filter_map(|n| n.interaction.clone())
        {
            let mut app = base.clone();
            assert!(
                app.click(1, &target, 0).is_ok(),
                "unhandled control {target}"
            );
        }
    }
    #[test]
    fn writer_on_ubuntu_routes_only_what_the_model_accepts() {
        let mut open = app();
        open.click(1, "docs:open:launch", 0).unwrap();
        open.http(
            1,
            "open",
            200,
            r#"{"id":"launch","title":"T","revision":1,"body":"a"}"#,
        )
        .unwrap();
        let mut menu = app();
        menu.click(1, "docs:menu", 0).unwrap();
        for base in [app(), open, menu] {
            let mut scene = Painter::themed(DesktopTheme::Ubuntu, 1210, 721, 0);
            base.render(
                &mut scene,
                &crate::AppEnv {
                    theme: DesktopTheme::Ubuntu,
                    width: 1210,
                    height: 721,
                    clock_us: 0,
                    settings: &crate::SystemSettings::DEFAULT,
                    clipboard: None,
                    share_to: None,
                    editor: None,
                    pointer: None,
                    files: Default::default(),
                },
            );
            let mut targets = std::collections::BTreeSet::new();
            for n in &scene.scene.nodes {
                let Some(target) = n.interaction.clone() else {
                    continue;
                };
                // Each control has an action of its own, as AT-SPI lists each.
                assert!(targets.insert(target.clone()), "{target} is painted twice");
                let disabled = n.semantic.as_ref().is_some_and(|s| s.disabled);
                let mut app = base.clone();
                assert_eq!(
                    app.click(1, &target, 0).is_ok(),
                    !disabled,
                    "{target} disagrees with its announced state"
                );
            }
            assert!(targets.contains("docs:menu"));
        }
    }
    #[test]
    fn writers_other_menus_open_as_libreoffice_lists_them_and_do_nothing_else() {
        let mut app = app();
        app.click(1, "docs:menu:Edit", 0).unwrap();
        assert_eq!(app.open_menu.as_deref(), Some("Edit"));
        let mut scene = Painter::themed(DesktopTheme::Ubuntu, 1210, 721, 0);
        app.render(
            &mut scene,
            &crate::AppEnv {
                theme: DesktopTheme::Ubuntu,
                width: 1210,
                height: 721,
                clock_us: 0,
                settings: &crate::SystemSettings::DEFAULT,
                clipboard: None,
                share_to: None,
                editor: None,
                pointer: None,
                files: Default::default(),
            },
        );
        // Undo is the first row, 26 px under the menu bar, and like every row insensitive.
        let undo = scene
            .scene
            .nodes
            .iter()
            .find(|n| {
                n.semantic
                    .as_ref()
                    .is_some_and(|s| s.label == "Undo" && s.role == "menuitem")
            })
            .expect("Undo listed");
        assert!(undo.semantic.as_ref().unwrap().disabled);
        assert_eq!(undo.bounds.y, 26);
        assert!(app.click(1, "docs:off:menu-edit-0", 0).is_err());
        assert_eq!(app.open_menu.as_deref(), Some("Edit"));
        // A click outside the menu closes it and does nothing more, even on a control that
        // would otherwise act.
        assert!(app.click(1, "docs:bold", 0).is_ok());
        assert_eq!(app.open_menu, None);
        assert!(!app.bold);
        app.click(1, "docs:menu:Edit", 0).unwrap();
        // Another header switches menus; the same header, or Escape, closes it.
        app.click(1, "docs:menu:View", 0).unwrap();
        assert_eq!(app.open_menu.as_deref(), Some("View"));
        app.click(1, "docs:menu:View", 0).unwrap();
        assert_eq!(app.open_menu, None);
        app.click(1, "docs:menu:Help", 0).unwrap();
        app.key(1, "Escape", 0).unwrap();
        assert_eq!(app.open_menu, None);
        // File is the service's documents, as before, and closes the other menus.
        app.click(1, "docs:menu:Table", 0).unwrap();
        app.click(1, "docs:menu", 0).unwrap();
        assert!(app.menu);
        assert_eq!(app.open_menu, None);
    }
}
