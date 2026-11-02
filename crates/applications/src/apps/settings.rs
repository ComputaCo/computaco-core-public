//! Settings. Every switch and level here is the same state the quick settings, control
//! centre and shade read, so the two surfaces can never disagree.
use super::look::{look, screen, INK, LINE, MUTED};
use crate::desktop_scene::{shared::Align, DesktopTheme, Painter};
use crate::AppEffect;
use cw_scene::{Color, Rect};
use serde::{Deserialize, Serialize};

/// Sections, and the switches each one owns. Names match `SystemSettings::flag`.
pub const SECTIONS: [(&str, &[&str]); 4] = [
    (
        "Network",
        &["wifi", "bluetooth", "airplane_mode", "hotspot"],
    ),
    ("Display", &["dark_mode", "night_light", "rotation_lock"]),
    ("Sound & Focus", &["do_not_disturb", "flashlight"]),
    ("Battery", &["battery_saver"]),
];
pub const LEVELS: [(&str, &str); 2] = [("brightness", "Brightness"), ("volume", "Volume")];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Index into `SECTIONS`.
    pub section: usize,
    /// On GNOME, a sidebar panel the model's sections are not (Wi-Fi, Bluetooth,
    /// Appearance...), by its index in `GNOME_PANELS`; it shows in place of the section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<usize>,
    /// On GNOME, Sound's Volume Levels page is open.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub levels: bool,
}
impl Settings {
    pub const KIND: &'static str = "settings";
    pub fn launch(_argument: &str, _window: u64, _clock_us: u64) -> (Self, Vec<AppEffect>) {
        (Self::default(), vec![])
    }
    pub fn kind(&self) -> &'static str {
        Self::KIND
    }
    pub fn title(&self, theme: DesktopTheme) -> String {
        if theme == DesktopTheme::Macos {
            "System Settings".into()
        } else {
            "Settings".into()
        }
    }
    pub fn document(&self) -> String {
        String::new()
    }
    pub fn caption(&self) -> String {
        SECTIONS[self.section.min(SECTIONS.len() - 1)].0.into()
    }
    pub fn modified(&self) -> bool {
        false
    }
    /// The title GNOME Settings' header bar gives the page on screen.
    pub fn gnome_title(&self) -> &'static str {
        if self.levels {
            return "Volume Levels";
        }
        GNOME_PANELS[self.gnome_panel()].0
    }
    /// The sidebar row of the page on screen.
    fn gnome_panel(&self) -> usize {
        let section = self.section.min(SECTIONS.len() - 1);
        self.panel.unwrap_or_else(|| {
            GNOME_PANELS
                .iter()
                .position(|(_, s, _, _)| *s == Some(section))
                .unwrap_or(0)
        })
    }
    /// Facts the Ubuntu frame needs: the title of the panel its header bar shows.
    pub fn chrome(&self) -> Vec<(String, String)> {
        vec![("panel".into(), self.gnome_title().into())]
    }
    pub fn offline(&mut self, _tag: &str, _reason: &str) {}
    pub fn http(
        &mut self,
        _window: u64,
        _tag: &str,
        _status: u16,
        _body: &str,
    ) -> Result<Vec<AppEffect>, String> {
        Err("settings makes no requests".into())
    }
    pub fn text(&mut self, _text: &str) -> Result<(), String> {
        Err("settings has no text field".into())
    }
    pub fn key(
        &mut self,
        _window: u64,
        key: &str,
        _clock_us: u64,
    ) -> Result<Vec<AppEffect>, String> {
        Err(format!("unsupported settings key {key}"))
    }
    /// Only section changes belong to the app; switches and levels are shell actions
    /// (`shell:toggle:*`, `shell:set:*`) so one store backs every surface that shows them.
    /// On GNOME the sidebar's other panels open (`settings:panel:N`), Sound's Volume
    /// Levels row opens its page (`settings:levels`, left by `settings:back`), and a row
    /// that only reports a status (`settings:row:*`) does nothing when activated, as in
    /// GNOME.
    pub fn click(
        &mut self,
        _window: u64,
        target: &str,
        _clock_us: u64,
    ) -> Result<Vec<AppEffect>, String> {
        if let Some(index) = target.strip_prefix("settings:panel:") {
            let index: usize = index.parse().map_err(|_| "invalid panel")?;
            let (_, section, _, _) = GNOME_PANELS.get(index).ok_or("panel not found")?;
            match section {
                Some(section) => {
                    self.section = *section;
                    self.panel = None;
                }
                None => self.panel = Some(index),
            }
            self.levels = false;
            return Ok(vec![]);
        }
        match target {
            "settings:levels" if self.panel.is_none() && self.section == 2 && !self.levels => {
                self.levels = true;
                return Ok(vec![]);
            }
            "settings:back" if self.levels => {
                self.levels = false;
                return Ok(vec![]);
            }
            _ => {}
        }
        if let Some(row) = target.strip_prefix("settings:row:") {
            return if STATUS_ROWS.contains(&row) {
                Ok(vec![])
            } else {
                Err("row not found".into())
            };
        }
        let index = target
            .strip_prefix("settings:section:")
            .ok_or("interaction does not belong to settings")?;
        let index: usize = index.parse().map_err(|_| "invalid section")?;
        if index >= SECTIONS.len() {
            return Err("section not found".into());
        }
        self.section = index;
        self.panel = None;
        self.levels = false;
        Ok(vec![])
    }
    pub fn page(&self, page: &mut cw_protocol::Page) {
        use cw_protocol::PageElement as E;
        let act = |url: &str| cw_protocol::PageAction {
            method: "APP".into(),
            url: url.into(),
            fields: Default::default(),
        };
        for (index, (name, _)) in SECTIONS.iter().enumerate() {
            page.elements.push(E::Button {
                id: format!("settings:section:{index}"),
                text: (*name).into(),
                action: act(&format!("settings:section:{index}")),
                style: None,
            });
        }
        for switch in SECTIONS[self.section.min(SECTIONS.len() - 1)].1 {
            page.elements.push(E::Button {
                id: format!("shell:toggle:{switch}"),
                text: label_for(switch).into(),
                action: act(&format!("shell:toggle:{switch}")),
                style: None,
            });
        }
        if self.section == 1 {
            for (name, label) in LEVELS {
                page.elements.push(E::Button {
                    id: format!("shell:set:{name}:100"),
                    text: format!("{label} to 100%"),
                    action: act(&format!("shell:set:{name}:100")),
                    style: None,
                });
            }
        }
    }
    pub fn render(&self, p: &mut Painter, env: &crate::AppEnv<'_>) {
        let (theme, width, height) = (env.theme, env.width, env.height);
        if theme == DesktopTheme::Ubuntu {
            return self.render_gnome(p, env);
        }
        let l = look(theme);
        p.scene.background = l.surface;
        let screen = screen(p, theme, &l, width, height as i32, &self.title(theme));
        let top = screen.top;
        let section = self.section.min(SECTIONS.len() - 1);
        let list_w = if theme.mobile() || width < 520 {
            0
        } else {
            200
        };
        if list_w > 0 {
            p.box_(Rect::new(0, top, list_w, height), l.chrome, 0);
            p.vline(list_w as i32 - 1, top, height, LINE);
        }
        for (index, (name, _)) in SECTIONS.iter().enumerate() {
            let r = if list_w > 0 {
                Rect::new(8, top + 10 + index as i32 * 32, list_w - 16, 30)
            } else {
                let w = width / SECTIONS.len() as u32;
                Rect::new(index as i32 * w as i32, top + 4, w, 30)
            };
            let on = index == section;
            p.button(
                r,
                if on { l.selection } else { Color::TRANSPARENT },
                l.radius,
                &format!("settings:section:{index}"),
                name,
            );
            p.label(
                r.x + if list_w > 0 { 12 } else { 0 },
                r.y + 6,
                r.width.saturating_sub(20),
                name,
                12,
                if on { l.accent } else { INK },
                on,
                if list_w > 0 {
                    Align::Left
                } else {
                    Align::Center
                },
            );
        }
        let x = list_w as i32;
        let pane = width.saturating_sub(list_w);
        let start = top + if list_w > 0 { 0 } else { 38 };
        // Each section keeps its own place.
        let column = screen.column(
            p,
            &format!("section-{section}"),
            Rect::new(x, start, pane, (height as i32 - start).max(1) as u32),
        );
        let mut y = column.top + 12;
        for switch in SECTIONS[section].1 {
            let r = Rect::new(x + 16, y, pane.saturating_sub(32), 38);
            switch_row(p, &l, r, switch, env.switch(switch));
            y += 42;
        }
        if section == 1 {
            for (name, label) in LEVELS {
                let level = env.level(name);
                p.left(
                    x + 16,
                    y,
                    pane.saturating_sub(32),
                    &format!("{label}  {level}%"),
                    12,
                    MUTED,
                );
                slider(
                    p,
                    &l,
                    Rect::new(x + 16, y + 18, pane.saturating_sub(32), 24),
                    name,
                    level,
                );
                y += 54;
            }
        }
        column.end(p);
        screen.end(p);
    }
}
/// GNOME Settings 46's sidebar, as AT-SPI lists it on Ubuntu 24.04 (the coverage tour's
/// reference, window-relative): every panel a button 230 by 42 px named by the panel,
/// holding its icon and its label, in four groups split by separators. A panel the
/// model's sections are (Network, Displays, Sound, Power) opens that section; the
/// others open a page of their own, which holds the switches the machine has for it
/// (Wi-Fi and Airplane Mode, Bluetooth) or nothing. Each entry is the panel's name, the
/// section it is, its top in the window and its icon.
const GNOME_PANELS: [(&str, Option<usize>, i32, &str); 21] = [
    ("Wi-Fi", None, 58, "wifi"),
    ("Network", Some(0), 102, "globe"),
    ("Bluetooth", None, 146, "bluetooth"),
    ("Displays", Some(1), 203, "display"),
    ("Sound", Some(2), 247, "volume"),
    ("Power", Some(3), 291, "power"),
    ("Multitasking", None, 335, "grid"),
    ("Appearance", None, 379, "palette"),
    ("Ubuntu Desktop", None, 423, "ubuntu"),
    ("Apps", None, 480, "apps"),
    ("Notifications", None, 524, "bell"),
    ("Search", None, 568, "search"),
    ("Online Accounts", None, 612, "at"),
    ("Sharing", None, 656, "share"),
    ("Mouse & Touchpad", None, 713, "hand"),
    ("Keyboard", None, 757, "keyboard"),
    ("Color", None, 801, "contrast"),
    ("Printers", None, 845, "document"),
    ("Accessibility", None, 902, "accessibility"),
    ("Privacy & Security", None, 946, "shield"),
    ("System", None, 990, "gear"),
];
/// The separators between the sidebar's groups, window-relative.
const GNOME_SEPARATORS: [i32; 4] = [196, 473, 706, 895];
/// Rows that only report a status (VPN not set up, the display arrangement, a missing
/// device): activating one does nothing.
const STATUS_ROWS: [&str; 5] = ["vpn", "displays", "arrangement", "output", "input"];

/// A node announced as `role` with the bounds GTK gives it, apart from what is painted.
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

impl Settings {
    /// GNOME Settings 46 on the Ubuntu desktop, laid out from the real window's AT-SPI
    /// bounds (window-relative; the content starts 1 px in and 46 px down, under the
    /// header bar the frame draws) and announced with GTK's roles: libadwaita's boxed
    /// lists hold options (status rows), buttons (rows that open something) and labels.
    /// The panel's lists are 558 px wide, centred in a 728 px pane.
    fn render_gnome(&self, p: &mut Painter, env: &crate::AppEnv<'_>) {
        let (width, height) = (env.width, env.height);
        let l = look(DesktopTheme::Ubuntu);
        const SIDE: Color = Color::rgb(235, 235, 235);
        const EDGE: Color = Color(0, 0, 0, 24);
        p.scene.background = Color::rgb(250, 250, 250);
        let current = self.gnome_panel();
        // The sidebar.
        p.box_(Rect::new(0, 0, 242, height), SIDE, 0);
        p.vline(241, 0, height, LINE);
        let side = |y: i32| y - 46;
        for y in GNOME_SEPARATORS {
            p.hline(5, side(y), 230, EDGE);
        }
        for (index, (name, section, y, icon)) in GNOME_PANELS.iter().enumerate() {
            let y = side(*y);
            if y > height as i32 {
                break;
            }
            let r = Rect::new(13, y, 230, 42);
            let on = index == current;
            if on {
                p.box_(Rect::new(13, y, 223, 42), Color(0, 0, 0, 22), 6);
            }
            let action = match section {
                Some(section) => format!("settings:section:{section}"),
                None => format!("settings:panel:{index}"),
            };
            p.button(r, Color::TRANSPARENT, 6, &action, name);
            p.select_last(on);
            if on {
                if let Some(n) = p.scene.nodes.last_mut() {
                    n.state.get_or_insert_with(Default::default).focused = true;
                }
            }
            p.symbol(icon, 19, y + 13, 16, INK);
            announced(p, Rect::new(19, y + 12, 16, 18), "img", "");
            p.label(47, y + 12, 174, name, 13, INK, false, Align::Left);
        }
        // The panel: window-relative x of the reference's 728 px pane mapped onto this
        // one, the left half from the lists' left edge and the right half from their right.
        let pane = width.saturating_sub(250);
        let card_w = pane.saturating_sub(24).min(558);
        let left = 241 + (pane.saturating_sub(card_w) / 2) as i32;
        let right = left + card_w as i32;
        let x = move |rx: i32| {
            if rx < 606 {
                left + rx - 327
            } else {
                right + rx - 885
            }
        };
        let wide = move |w: u32| (w + card_w).saturating_sub(558);
        let at = move |rx: i32, ry: i32, w: u32, h: u32| Rect::new(x(rx), ry - 46, w, h);
        // A boxed list's card, with a rule between its rows.
        let card = |p: &mut Painter, ry: i32, rows: &[u32]| {
            let h: u32 = rows.iter().sum();
            p.border(at(327, ry, card_w, h + 2), Color::WHITE, 12, EDGE);
            let mut y = ry + 1;
            for row in &rows[..rows.len() - 1] {
                y += *row as i32;
                p.hline(left, y - 46, card_w, EDGE);
            }
        };
        // A group's heading, announced as GTK's label.
        let heading = |p: &mut Painter, ry: i32, w: u32, text: &str| {
            p.strong(x(327), ry - 46, w, text, 13, INK);
            p.presentation();
            announced(p, at(327, ry, w, 18), "label", text);
        };
        // Text a row's own name already announces.
        let quiet = |p: &mut Painter, ry: i32, w: u32, text: &str| {
            p.left(x(341), ry - 46, w, text, 13, INK);
            p.presentation();
        };
        // A label on a row's right, announced with its own bounds.
        let value = |p: &mut Painter, r: Rect, text: &str| {
            p.right(r.x, r.y, r.width, text, 13, MUTED);
            p.presentation();
            announced(p, r, "label", text);
        };
        let chevron = |p: &mut Painter, ry: i32| {
            p.symbol("chevron-right", x(855), ry - 46, 16, INK);
            announced(p, at(855, ry, 16, 16), "img", "");
        };
        // A status row: an option with nothing to activate.
        let status = |p: &mut Painter, ry: i32, key: &str, name: &str| {
            p.region(
                at(329, ry, card_w, 54),
                &format!("settings:row:{key}"),
                name,
            );
            p.announce("option", None);
            p.select_last(false);
        };
        let insensitive = |p: &mut Painter, r: Rect, key: &str, role: &str, name: &str| {
            p.insensitive(r, &format!("settings:off:{key}"), name);
            p.announce(role, None);
        };
        // A row holding one of the machine's switches, flipping the shared store.
        let switch = |p: &mut Painter, ry: i32, name: &str, label: &str| {
            let on = env.switch(name);
            p.button(
                at(329, ry, card_w, 54),
                Color::TRANSPARENT,
                12,
                &format!("shell:toggle:{name}"),
                label,
            );
            if let Some(n) = p.scene.nodes.last_mut() {
                n.state.get_or_insert_with(Default::default).checked = Some(on);
            }
            quiet(p, ry + 16, wide(438), label);
            let track = at(831, ry + 15, 46, 24);
            p.box_(track, if on { l.accent } else { Color(0, 0, 0, 40) }, 12);
            p.circle(
                if on { track.x + 34 } else { track.x + 12 },
                track.y + 12,
                10,
                Color::WHITE,
            );
        };
        let header_button = |p: &mut Painter, ry: i32, key: &str, name: &str| {
            p.symbol("plus", x(865), ry - 37, 16, MUTED);
            insensitive(p, at(856, ry, 34, 34), key, "button", name);
        };
        if self.levels {
            // Volume Levels: the machine's one output level, in the shared store.
            let back = at(327, 70, 34, 34);
            p.symbol("chevron-left", back.x + 9, back.y + 9, 16, INK);
            p.region(back, "settings:back", "Back");
            heading(p, 122, card_w, "Output");
            card(p, 154, &[54]);
            quiet(p, 172, 140, "Volume");
            let volume = env.level("volume");
            slider(
                p,
                &l,
                Rect::new(x(329) + 160, 168 - 46 + 4, card_w.saturating_sub(176), 24),
                "volume",
                volume,
            );
            return;
        }
        match (self.panel, self.section.min(SECTIONS.len() - 1)) {
            // Wi-Fi: its switch and Airplane Mode.
            (Some(0), _) => {
                card(p, 110, &[54, 54]);
                switch(p, 112, "wifi", "Wi-Fi");
                switch(p, 166, "airplane_mode", "Airplane Mode");
            }
            (Some(2), _) => {
                card(p, 110, &[54]);
                switch(p, 112, "bluetooth", "Bluetooth");
            }
            // A panel the machine has no settings for: its title alone.
            (Some(_), _) => {}
            // Network, on a machine whose network is Wi-Fi: VPN and Proxy.
            (None, 0) => {
                heading(p, 78, wide(524), "VPN");
                header_button(p, 75, "vpn-add", "Add VPN");
                card(p, 110, &[54]);
                status(p, 112, "vpn", "Not set up");
                quiet(p, 128, wide(530), "Not set up");
                heading(p, 196, card_w, "Proxy");
                card(p, 228, &[54]);
                insensitive(p, at(329, 230, card_w, 54), "proxy", "button", "Proxy");
                p.symbol("globe", x(341), 247 - 46, 16, INK);
                announced(p, at(341, 247, 16, 16), "img", "");
                p.left(x(369), 246 - 46, wide(452), "Proxy", 13, INK);
                p.presentation();
                value(p, at(827, 246, 22, 18), "Off");
                chevron(p, 247);
            }
            // Displays: the arrangement, the primary display and Night Light.
            (None, 1) => {
                card(p, 70, &[54]);
                status(p, 72, "displays", "Multiple Displays");
                quiet(p, 88, wide(383), "Multiple Displays");
                // Join (pressed) and Mirror, a linked pair; one display, so neither acts.
                p.box_(at(747, 85, 63, 34), Color(0, 0, 0, 40), 6);
                p.box_(at(810, 85, 75, 34), Color(0, 0, 0, 18), 6);
                p.center(x(747), 93 - 46, 63, "Join", 13, INK);
                p.presentation();
                p.center(x(810), 93 - 46, 75, "Mirror", 13, INK);
                p.presentation();
                insensitive(p, at(747, 85, 64, 34), "join", "button", "Join");
                insensitive(p, at(810, 85, 78, 34), "mirror", "button", "Mirror");
                card(p, 148, &[180, 54]);
                // The arrangement, which the reference leaves blank.
                status(p, 150, "arrangement", "");
                insensitive(
                    p,
                    at(329, 330, card_w, 54),
                    "primary",
                    "combobox",
                    "Primary Display",
                );
                p.select_last(false);
                // One display to choose from: the row is insensitive, its text greyed.
                p.left(x(341), 336 - 46, wide(524), "Primary Display", 13, MUTED);
                p.presentation();
                announced(p, at(341, 336, wide(524), 18), "label", "Primary Display");
                let caption = "Contains top bar and Activities";
                p.left(x(341), 357 - 46, wide(524), caption, 11, MUTED);
                p.presentation();
                announced(p, at(341, 357, wide(524), 16), "label", caption);
                card(p, 406, &[54]);
                let on = env.switch("night_light");
                p.button(
                    at(329, 408, card_w, 54),
                    Color::TRANSPARENT,
                    12,
                    "shell:toggle:night_light",
                    "Night Light",
                );
                p.select_last(false);
                quiet(p, 424, wide(480), "Night Light");
                let state = if on { "On" } else { "Off" };
                let w = p.measure(state, 13, false).max(1);
                value(p, at(849 - w as i32, 424, w, 18), state);
                chevron(p, 425);
            }
            // Sound, with no output or input device to choose, as the reference
            // machine has none; the output level is on the Volume Levels page.
            (None, 2) => {
                heading(p, 78, card_w, "Output");
                card(p, 110, &[54]);
                status(p, 112, "output", "Output Device");
                quiet(p, 128, wide(401), "Output Device");
                p.right(x(748), 128 - 46, 123, "No Output Devices", 13, MUTED);
                p.presentation();
                announced(p, at(748, 112, 123, 50), "label", "No Output Devices");
                heading(p, 196, card_w, "Input");
                card(p, 228, &[54]);
                status(p, 230, "input", "Input Device");
                quiet(p, 246, wide(414), "Input Device");
                p.right(x(761), 246 - 46, 110, "No Input Devices", 13, MUTED);
                p.presentation();
                announced(p, at(761, 230, 110, 50), "label", "No Input Devices");
                heading(p, 314, card_w, "Sounds");
                card(p, 346, &[55, 54]);
                p.button(
                    at(329, 348, card_w, 55),
                    Color::TRANSPARENT,
                    12,
                    "settings:levels",
                    "Volume Levels",
                );
                p.select_last(false);
                quiet(p, 364, wide(502), "Volume Levels");
                chevron(p, 365);
                insensitive(
                    p,
                    at(329, 403, card_w, 54),
                    "alert",
                    "button",
                    "Alert Sound",
                );
                p.select_last(false);
                quiet(p, 419, wide(452), "Alert Sound");
                value(p, at(799, 419, 50, 18), "Default");
                chevron(p, 420);
            }
            // Power: its mode, where Power Saver is the machine's switch.
            _ => {
                heading(p, 78, card_w, "Power Mode");
                card(p, 110, &[54]);
                switch(p, 112, "battery_saver", "Power Saver");
            }
        }
    }
}
/// One switch row, drawn in its true position and clicking to flip the shared store.
fn switch_row(p: &mut Painter, l: &super::look::Look, r: Rect, switch: &str, on: bool) {
    p.button(
        r,
        Color::TRANSPARENT,
        l.radius,
        &format!("shell:toggle:{switch}"),
        label_for(switch),
    );
    p.hline(r.x, r.y + r.height as i32, r.width, LINE);
    p.left(
        r.x + 4,
        r.y + 10,
        r.width.saturating_sub(80),
        label_for(switch),
        13,
        INK,
    );
    let track = Rect::new(r.x + r.width as i32 - 52, r.y + 8, 44, 22);
    p.box_(track, if on { l.accent } else { Color(0, 0, 0, 34) }, 11);
    p.circle(
        if on { track.x + 33 } else { track.x + 11 },
        track.y + 11,
        9,
        Color::WHITE,
    );
}
/// Discrete steps, so a click lands on an exact percentage.
fn slider(p: &mut Painter, l: &super::look::Look, r: Rect, name: &str, filled: u8) {
    const STEPS: u32 = 20;
    let step_w = r.width / STEPS;
    for index in 0..STEPS {
        let percent = (index + 1) * 100 / STEPS;
        let cell = Rect::new(
            r.x + (index * step_w) as i32,
            r.y,
            step_w.saturating_sub(1),
            r.height,
        );
        p.button(
            cell,
            if percent <= u32::from(filled) {
                l.accent
            } else {
                Color(0, 0, 0, 18)
            },
            0,
            &format!("shell:set:{name}:{percent}"),
            &format!("{name} {percent}%"),
        );
    }
}
fn label_for(switch: &str) -> &str {
    match switch {
        "wifi" => "Wi-Fi",
        "bluetooth" => "Bluetooth",
        "airplane_mode" => "Airplane mode",
        "hotspot" => "Personal hotspot",
        "dark_mode" => "Dark appearance",
        "night_light" => "Night light",
        "rotation_lock" => "Rotation lock",
        "do_not_disturb" => "Do not disturb",
        "flashlight" => "Flashlight",
        "battery_saver" => "Battery saver",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sections_switch_and_unknown_ones_are_refused() {
        let (mut app, _) = Settings::launch("", 1, 0);
        assert_eq!(app.section, 0);
        app.click(1, "settings:section:2", 0).unwrap();
        assert_eq!(app.section, 2);
        assert!(app.click(1, "settings:section:9", 0).is_err());
        assert!(app.click(1, "shell:toggle:wifi", 0).is_err());
    }
    #[test]
    fn switches_and_levels_are_painted_as_shell_actions_not_app_state() {
        let (app, _) = Settings::launch("", 1, 0);
        let mut scene = Painter::themed(DesktopTheme::Macos, 820, 560, 0);
        app.render(
            &mut scene,
            &crate::AppEnv {
                theme: DesktopTheme::Macos,
                width: 820,
                height: 560,
                clock_us: 0,
                settings: &crate::SystemSettings::DEFAULT,
                clipboard: None,
                share_to: None,
                editor: None,
                pointer: None,
                files: Default::default(),
            },
        );
        let targets: Vec<_> = scene
            .scene
            .nodes
            .iter()
            .filter_map(|n| n.interaction.clone())
            .collect();
        assert!(targets.iter().any(|t| t == "shell:toggle:wifi"));
        assert!(targets
            .iter()
            .all(|t| t.starts_with("settings:section:") || t.starts_with("shell:")));
    }
    #[test]
    fn ubuntu_lists_gnome_settings_panels_and_only_a_desktops_switches() {
        let (mut app, _) = Settings::launch("", 1, 0);
        let render = |app: &Settings| {
            let mut p = Painter::themed(DesktopTheme::Ubuntu, 980, 594, 0);
            app.render(
                &mut p,
                &crate::AppEnv {
                    theme: DesktopTheme::Ubuntu,
                    width: 980,
                    height: 594,
                    clock_us: 0,
                    settings: &crate::SystemSettings::DEFAULT,
                    clipboard: None,
                    share_to: None,
                    editor: None,
                    pointer: None,
                    files: Default::default(),
                },
            );
            p
        };
        let p = render(&app);
        let label = |p: &Painter, name: &str| {
            p.scene
                .nodes
                .iter()
                .find(|n| n.semantic.as_ref().is_some_and(|s| s.label == name))
                .map(|n| (n.interaction.clone(), n.semantic.clone().unwrap().disabled))
        };
        assert_eq!(
            label(&p, "Displays").unwrap().0.as_deref(),
            Some("settings:section:1")
        );
        assert!(label(&p, "Personal hotspot").is_none());
        // Network is selected and focused in the sidebar, as GTK reports the open panel.
        let network = p
            .scene
            .nodes
            .iter()
            .find(|n| n.interaction.as_deref() == Some("settings:section:0"))
            .unwrap();
        let state = network.state.unwrap();
        assert_eq!(state.selected, Some(true));
        assert!(state.focused);
        // Every panel in the sidebar acts: Wi-Fi opens a page holding its switch.
        assert_eq!(
            label(&p, "Wi-Fi").unwrap(),
            (Some("settings:panel:0".to_owned()), false)
        );
        app.click(1, "settings:panel:0", 0).unwrap();
        assert_eq!(app.gnome_title(), "Wi-Fi");
        let p = render(&app);
        assert!(p
            .scene
            .nodes
            .iter()
            .any(|n| n.interaction.as_deref() == Some("shell:toggle:wifi")));
        // Displays' Night Light row flips the shared switch.
        app.click(1, "settings:section:1", 0).unwrap();
        assert_eq!(app.panel, None);
        let p = render(&app);
        assert_eq!(
            label(&p, "Night Light").unwrap().0.as_deref(),
            Some("shell:toggle:night_light")
        );
        app.click(1, "settings:section:2", 0).unwrap();
        let p = render(&app);
        assert!(label(&p, "Sound & Focus").is_none());
        assert!(label(&p, "Flashlight").is_none());
        // The output level is on the Volume Levels page, which Back leaves.
        app.click(1, "settings:levels", 0).unwrap();
        assert_eq!(app.gnome_title(), "Volume Levels");
        let p = render(&app);
        assert!(p
            .scene
            .nodes
            .iter()
            .any(|n| n.interaction.as_deref() == Some("shell:set:volume:50")));
        app.click(1, "settings:back", 0).unwrap();
        assert!(app.click(1, "settings:back", 0).is_err());
        assert!(app.click(1, "settings:row:output", 0).unwrap().is_empty());
        assert!(app.click(1, "settings:row:nothing", 0).is_err());
        // The model's state serializes as before while no GNOME-only page is open.
        let (fresh, _) = Settings::launch("", 1, 0);
        assert_eq!(serde_json::to_string(&fresh).unwrap(), r#"{"section":0}"#);
    }
    #[test]
    fn every_switch_name_is_one_the_system_store_knows() {
        for (_, switches) in SECTIONS {
            for switch in switches {
                assert!(
                    crate::SystemSettings::DEFAULT.flag(switch).is_ok(),
                    "{switch} is not a real system switch"
                );
            }
        }
        for (name, _) in LEVELS {
            assert!(crate::SystemSettings::DEFAULT.level(name).is_ok());
        }
    }
}
