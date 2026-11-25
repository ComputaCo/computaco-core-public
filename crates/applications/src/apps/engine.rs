//! What the engine's own applications declare for the rules (`declare`): the file manager, the text editor
//! and the terminal. They are drawn by the compositor (`desktop_scene::app_content`), not by a `NativeApp`,
//! and their commands are named as the compositor names them (`files-up`, `open:<row>`, `editor-text`,
//! `terminal-input`), so each command table names the commands outright: what a table names is the
//! application's own (`Declaration::also`). Their structure is read from the window's state as serialized
//! (`AppState`, tagged by `type`).
use super::declare::{command, field, As, Declaration};

/// The file manager (Finder, Explorer, GNOME Files, the phones' Files).
pub const FILES: Declaration = {
    const NAV: &[(&str, &str)] = &[("toolbar", "navigation")];
    const ROWS: &[(&str, &str)] = &[("list", "files")];
    const PLACES: &[(&str, &str)] = &[("list", "places")];
    const TABS: &[(&str, &str)] = &[("tablist", "tabs")];
    Declaration {
        prefix: "files",
        structure: &[
            field("scope", "/tabs/0/scope", As::Variant),
            field("view", "/tabs/0/view", As::Variant),
            field("sort", "/tabs/0/sort", As::Variant),
            field("descending", "/tabs/0/descending", As::Value),
            field("rows", "/tabs/0/entries", As::Filled),
            field("selected", "/tabs/0/selected", As::Present),
            field("back", "/tabs/0/position", As::Filled),
            field("searching", "/tabs/0/searching", As::Value),
            field("query", "/tabs/0/query", As::Filled),
            field("renaming", "/tabs/0/rename", As::Present),
            field("hidden", "/tabs/0/show_hidden", As::Value),
            field("tabs", "/tabs/1", As::Present),
        ],
        commands: &[
            // A row stands for the file it names; its command is its place in the listing.
            command("open:*").object("file").named().region(ROWS),
            command("files-back").region(NAV),
            command("files-forward").region(NAV),
            command("files-up").region(NAV),
            command("files-reload").region(NAV),
            command("files-search").object("search.query").region(NAV),
            command("files-search-clear").region(NAV),
            command("files-location:*").object("place").region(PLACES),
            command("files-home").region(PLACES),
            command("files-recents").region(PLACES),
            command("files-starred").region(PLACES),
            command("files-trash").region(PLACES),
            command("files-tab:*").object("tab").region(TABS),
            command("files-closetab:*").object("tab").region(TABS),
            command("files-newtab").region(TABS),
            // Everything else the file manager draws: its view, sort and file commands.
            command("files-*"),
        ],
        keys: &["Enter", "Escape", "Backspace", "Delete", "Ctrl+h"],
    }
};

/// The text editor (TextEdit, Notepad, GNOME Text Editor, the phones' editors).
pub const EDITOR: Declaration = {
    const DIALOG: &[(&str, &str)] = &[("dialog", "editor")];
    Declaration {
        prefix: "editor",
        structure: &[
            field("text", "/text", As::Filled),
            field("dirty", "/dirty", As::Value),
            field("selected", "/selected", As::Value),
            field("named", "/path", As::Filled),
        ],
        commands: &[
            command("editor-text*").object("document.text"),
            command("editor:dialog:field")
                .object("dialog.name")
                .region(DIALOG),
            command("editor:dialog:*").region(DIALOG),
            command("editor-save"),
            command("editor-*"),
        ],
        keys: &["Ctrl+s", "Ctrl+a", "Enter", "Backspace", "Escape"],
    }
};

/// The terminal (Terminal, Windows Terminal, GNOME Terminal).
pub const TERMINAL: Declaration = Declaration {
    prefix: "terminal",
    structure: &[
        field("input", "/input", As::Filled),
        field("output", "/transcript", As::Filled),
        field("scrolled", "/scroll", As::Filled),
    ],
    commands: &[
        command("terminal-input").object("command.line"),
        command("terminal-*"),
    ],
    keys: &["Enter", "Backspace", "Up", "Ctrl+c"],
};
