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
    /// iOS's navigation stack: the pages pushed over the root list (`General`, `About`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<String>,
    /// The Android Settings page open (`ANDROID_PAGES`), empty for the main list.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub page: String,
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
        } else if let Some(page) =
            android_page(&self.page).filter(|_| theme == DesktopTheme::Android)
        {
            page.title.into()
        } else {
            "Settings".into()
        }
    }
    /// Where Back goes on a phone: a page returns to the one it was opened from.
    pub fn phone_back(&self) -> Option<&'static str> {
        (!self.page.is_empty()).then_some("settings:back")
    }
    /// The app bar's leading control on a phone: an up arrow on a page.
    pub fn phone_nav(&self) -> Option<(&'static str, String, String)> {
        (!self.page.is_empty()).then(|| ("back", "settings:back".into(), String::new()))
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
    pub fn click(
        &mut self,
        _window: u64,
        target: &str,
        _clock_us: u64,
    ) -> Result<Vec<AppEffect>, String> {
        if target == "settings:back" {
            // iOS's stack first, then Android's page tree.
            if self.pages.pop().is_some() {
                return Ok(vec![]);
            }
            if self.page.is_empty() {
                return Err("settings is at its root".into());
            }
            self.page = android_page(&self.page)
                .map(|p| p.parent.to_owned())
                .unwrap_or_default();
            return Ok(vec![]);
        }
        if let Some(page) = target.strip_prefix("settings:open:") {
            if !ios::PAGES.contains(&page) || self.pages.len() >= 8 {
                return Err(format!("no settings page {page}"));
            }
            self.pages.push(page.to_owned());
            return Ok(vec![]);
        }
        if let Some(page) = target.strip_prefix("settings:page:") {
            if android_page(page).is_none() {
                return Err("page not found".into());
            }
            self.page = page.to_owned();
            return Ok(vec![]);
        }
        let index = target
            .strip_prefix("settings:section:")
            .ok_or("interaction does not belong to settings")?;
        let index: usize = index.parse().map_err(|_| "invalid section")?;
        if index >= SECTIONS.len() {
            return Err("section not found".into());
        }
        self.section = index;
        Ok(vec![])
    }
    /// What iOS's navigation bar shows over a pushed page: its title and the way back.
    pub fn chrome(&self) -> Vec<(String, String)> {
        let Some(page) = self.pages.last() else {
            return vec![];
        };
        let back = self
            .pages
            .len()
            .checked_sub(2)
            .map_or("Settings", |i| self.pages[i].as_str());
        vec![
            ("title".into(), page.clone()),
            ("nav".into(), format!("back\tsettings:back\t{back}")),
        ]
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
        if theme == DesktopTheme::Ios {
            return ios::render(self, p, env);
        }
        if theme == DesktopTheme::Android {
            return self.render_android(p, env);
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
/// GNOME Settings' panels, in its sidebar's order and places (Ubuntu 24.04, measured on the
/// real application in the coverage tours, laid out under the shell's 46 px title bar): the
/// panel each of the model's sections is (Network, Displays, Sound, Power), the rest listed
/// and insensitive. A panel holds the switches and levels a desktop's has; a phone's
/// (hotspot, flashlight, rotation lock) are not on a desktop's Settings, and Do Not Disturb
/// and the dark style are the shell's (its quick settings and notification list).
const GNOME_PANELS: [(&str, Option<usize>, i32); 21] = [
    ("Wi-Fi", None, 12),
    ("Network", Some(0), 56),
    ("Bluetooth", None, 100),
    ("Displays", Some(1), 157),
    ("Sound", Some(2), 201),
    ("Power", Some(3), 245),
    ("Multitasking", None, 289),
    ("Appearance", None, 333),
    ("Ubuntu Desktop", None, 377),
    ("Apps", None, 434),
    ("Notifications", None, 478),
    ("Search", None, 522),
    ("Online Accounts", None, 566),
    ("Sharing", None, 610),
    ("Mouse & Touchpad", None, 667),
    ("Keyboard", None, 711),
    ("Color", None, 755),
    ("Printers", None, 799),
    ("Accessibility", None, 856),
    ("Privacy & Security", None, 900),
    ("System", None, 944),
];
impl Settings {
    fn render_gnome(&self, p: &mut Painter, env: &crate::AppEnv<'_>) {
        let (width, height) = (env.width, env.height);
        let l = look(DesktopTheme::Ubuntu);
        const SIDE: Color = Color::rgb(235, 235, 235);
        const CARD: Color = Color::WHITE;
        const EDGE: Color = Color(0, 0, 0, 24);
        p.scene.background = Color::rgb(250, 250, 250);
        let section = self.section.min(SECTIONS.len() - 1);
        p.box_(Rect::new(0, 0, 242, height), SIDE, 0);
        p.vline(241, 0, height, LINE);
        for (name, index, y) in GNOME_PANELS {
            if y > height as i32 {
                break;
            }
            let r = Rect::new(14, y, 214, 42);
            let on = index == Some(section);
            if on {
                p.box_(r, Color(0, 0, 0, 22), 6);
            }
            match index {
                Some(i) => p.button(
                    r,
                    Color::TRANSPARENT,
                    6,
                    &format!("settings:section:{i}"),
                    name,
                ),
                None => p.insensitive(r, &format!("settings:panel:{name}"), name),
            }
            p.label(48, y + 12, 174, name, 13, INK, on, Align::Left);
        }
        let pane = width.saturating_sub(242);
        let card_w = pane.saturating_sub(40).min(558);
        let x = 242 + (pane.saturating_sub(card_w) / 2) as i32;
        let row = |p: &mut Painter, y: i32, label: &str, value: Option<&str>| {
            p.left(x + 12, y + 16, card_w.saturating_sub(160), label, 13, INK);
            if let Some(v) = value {
                p.right(x, y + 16, card_w.saturating_sub(12), v, 13, MUTED);
            }
        };
        let card = |p: &mut Painter, y: i32, rows: u32| {
            p.border(Rect::new(x, y, card_w, rows * 54), CARD, 12, EDGE);
            for i in 1..rows {
                p.hline(x, y + (i * 54) as i32, card_w, EDGE);
            }
        };
        let switch = |p: &mut Painter, y: i32, name: &str, label: &str| {
            let r = Rect::new(x, y, card_w, 54);
            let on = env.switch(name);
            p.button(
                r,
                Color::TRANSPARENT,
                12,
                &format!("shell:toggle:{name}"),
                label,
            );
            p.left(x + 12, y + 16, card_w.saturating_sub(100), label, 13, INK);
            let track = Rect::new(x + card_w as i32 - 60, y + 15, 46, 24);
            p.box_(track, if on { l.accent } else { Color(0, 0, 0, 40) }, 12);
            p.circle(
                if on { track.x + 34 } else { track.x + 12 },
                track.y + 12,
                10,
                Color::WHITE,
            );
        };
        match section {
            0 => {
                p.strong(x, 32, card_w, "Wireless", 13, INK);
                card(p, 62, 3);
                for (i, (name, label)) in [
                    ("wifi", "Wi-Fi"),
                    ("bluetooth", "Bluetooth"),
                    ("airplane_mode", "Airplane Mode"),
                ]
                .into_iter()
                .enumerate()
                {
                    switch(p, 62 + i as i32 * 54, name, label);
                }
            }
            1 => {
                card(p, 284, 1);
                row(p, 284, "Primary Display", Some("Built-in display"));
                card(p, 362, 1);
                switch(p, 362, "night_light", "Night Light");
            }
            2 => {
                p.strong(x, 32, card_w, "Output", 13, INK);
                card(p, 64, 2);
                row(p, 64, "Output Device", Some("Speakers"));
                let volume = env.level("volume");
                p.left(x + 12, 134, 140, "Output Volume", 13, INK);
                slider(
                    p,
                    &l,
                    Rect::new(x + 160, 130, card_w.saturating_sub(176), 24),
                    "volume",
                    volume,
                );
                p.strong(x, 268, card_w, "Sounds", 13, INK);
                card(p, 300, 2);
                row(p, 300, "Volume Levels", None);
                row(p, 354, "Alert Sound", Some("Default"));
            }
            _ => {
                p.strong(x, 32, card_w, "Power Mode", 13, INK);
                card(p, 62, 1);
                switch(p, 62, "battery_saver", "Power Saver");
            }
        }
    }
}
/// A row of an Android Settings page.
#[derive(Clone, Copy, Debug)]
enum Row {
    /// A category heading over the rows after it.
    Header(&'static str),
    /// A preference that opens a page of its own: title, summary, the page's id.
    Link(&'static str, &'static str, &'static str),
    /// A preference the simulated phone has no page behind: shown, listed, insensitive.
    Item(&'static str, &'static str),
    /// A preference with a switch on the system store (`SystemSettings::flag`).
    Switch(&'static str, &'static str, &'static str),
    /// A level on the system store, shown as a slider (`SystemSettings::level`).
    Level(&'static str, &'static str),
    /// Explanatory text at the foot of a page.
    Footer(&'static str),
}
use Row::{Footer, Header, Item, Level, Link, Switch};

/// An Android Settings page: its id, its title, the page Back returns to, its rows in groups
/// (each group one card on the main page), and the symbol of each main-page row.
struct Page {
    id: &'static str,
    title: &'static str,
    parent: &'static str,
    groups: &'static [&'static [Row]],
}

/// The main list's rows' symbols, by title.
fn row_symbol(title: &str) -> &'static str {
    match title {
        "Network & internet" => "wifi",
        "Connected devices" => "bluetooth",
        "Apps" => "apps",
        "Notifications" => "bell",
        "Sound & vibration" => "volume",
        "Modes" => "dnd",
        "Display" => "sun",
        "Wallpaper" => "palette",
        "Storage" => "drive",
        "Battery" => "battery-vertical",
        "Security & privacy" => "shield",
        "Location" => "location",
        "Accessibility" => "accessibility",
        "System" => "info",
        _ => "info",
    }
}

/// Android Settings as AOSP's Settings (Android 17, captured on Cuttlefish) and Pixel's share
/// it: the main list in its cards, and the pages a capture opened, row for row (titles and
/// summaries as the reference shows them, fidelity/android/scripts/and-settings-*). Values
/// that are the capture host's (its carrier, storage, model) are the simulated phone's.
/// Pixel's main list adds Google's own entries (Google, Digital Wellbeing, Tips & support),
/// which the AOSP reference cannot show; this list keeps to what both have
/// (fidelity/coverage.md, "AOSP or Pixel").
const ANDROID_PAGES: &[Page] = &[
    Page {
        id: "",
        title: "Settings",
        parent: "",
        groups: &[
            &[
                Link("Network & internet", "Mobile, Wi\u{2011}Fi, hotspot", "network-internet"),
                Link("Connected devices", "Bluetooth, pairing", "connected-devices"),
            ],
            &[
                Link("Apps", "Recent apps, default apps", "apps"),
                Link("Notifications", "Notification history, conversations", "notifications"),
                Link("Sound & vibration", "Volume and vibration", "sound-vibration"),
                Item("Modes", "Do Not Disturb, Sleeping"),
                Link("Display", "Dark theme, font size, brightness", "display"),
                Item("Wallpaper", "Home, lock screen"),
            ],
            &[
                Link("Storage", "49% used - 8.17 GB free", "storage"),
                Link("Battery", "85% - Charging slowly", "battery-saver"),
            ],
            &[
                Link("Security & privacy", "App security, device lock, permissions", "security-privacy"),
                Link("Location", "On / 1 app has access to location", "location"),
                Link("Accessibility", "Display, interaction, audio", "accessibility"),
            ],
            &[
                Link("System", "Languages, gestures, time", "system"),
                Item("About phone", "Pixel 9"),
            ],
        ],
    },
    Page {
        id: "network-internet",
        title: "Network & internet",
        parent: "",
        groups: &[&[
            Link("Internet", "Networks available", "internet"),
            Item("SIMs", "Mobile"),
            Switch("Airplane mode", "", "airplane_mode"),
            Switch("Hotspot & tethering", "Off", "hotspot"),
            Item("Data Saver", "Off"),
            Item("VPN", ""),
            Item("Private DNS", "Automatic"),
            Item("Mobile network security", "Network type, encryption, notification controls"),
        ]],
    },
    Page {
        id: "internet",
        title: "Internet",
        parent: "network-internet",
        groups: &[&[
            Header("Mobile data"),
            Item("Mobile", "5G"),
            Header("Wi\u{2011}Fi"),
            Switch("Use Wi\u{2011}Fi", "", "wifi"),
            Item("Network preferences", "Wi\u{2011}Fi doesn't turn back on automatically"),
            Item("Non-carrier data usage", "0 byte used"),
        ]],
    },
    Page {
        id: "connected-devices",
        title: "Connected devices",
        parent: "",
        groups: &[&[
            Item("Pair new device", ""),
            Header("Saved devices"),
            Item("See all", ""),
            Link("Connection preferences", "Bluetooth, NFC", "bluetooth"),
            Footer("Visible as \u{201c}Pixel 9\u{201d} to other devices"),
        ]],
    },
    Page {
        id: "bluetooth",
        title: "Bluetooth",
        parent: "connected-devices",
        groups: &[&[
            Switch("Use Bluetooth", "", "bluetooth"),
            Item("Pair new device", ""),
            Footer("When Bluetooth is turned on, your device can communicate with other nearby Bluetooth devices."),
        ]],
    },
    Page {
        id: "apps",
        title: "Apps",
        parent: "",
        groups: &[&[
            Header("Recently opened apps"),
            Item("See all apps", ""),
            Header("General"),
            Item("Default apps", "Chrome, Phone, and Messages"),
            Item("Contacts storage", "Device only"),
            Item("Unused apps", "0 unused apps"),
            Item("Special app access", ""),
        ]],
    },
    Page {
        id: "notifications",
        title: "Notifications",
        parent: "",
        groups: &[&[
            Header("Manage"),
            Item("App notifications", "Control notifications from individual apps"),
            Item("Notification history", "Show recent and snoozed notifications"),
            Item("Notification summaries", "Get quick recaps of longer messages"),
            Item("Notification organizer", "Bundle and silence lower priority notifications"),
            Header("Conversation"),
            Item("Conversations", "No priority conversations"),
            Item("Bubbles", "On / Conversations can appear as floating icons"),
            Header("Privacy"),
            Item("Notification read, reply & control", "Control which apps and devices can read notifications"),
        ]],
    },
    Page {
        id: "sound-vibration",
        title: "Sound & vibration",
        parent: "",
        groups: &[&[
            Item("Audio will play on", "This phone"),
            Level("Media volume", "volume"),
            Item("Call volume", ""),
            Item("Ring volume", ""),
            Item("Notification volume", ""),
            Item("Alarm volume", ""),
            Item("Assistant volume", ""),
            Item("Vibration & haptics", "On / Control vibration levels for calls, notifications, and more"),
        ]],
    },
    Page {
        id: "display",
        title: "Display",
        parent: "",
        groups: &[&[
            Header("Brightness"),
            Level("Brightness level", "brightness"),
            Item("Adaptive brightness", ""),
            Header("Lock display"),
            Item("Lock screen", "Show all notification content"),
            Item("Screen timeout", "After 30 minutes of inactivity"),
            Header("Appearance"),
            Switch("Dark theme", "Will never turn on automatically", "dark_mode"),
            Item("Display size & text", "Font size, display size, bold text, outline text"),
            Header("Color"),
            Switch("Night Light", "Will never turn on automatically", "night_light"),
            Item("Colors", ""),
            Switch("Auto-rotate screen", "", "rotation_lock"),
        ]],
    },
    Page {
        id: "storage",
        title: "Storage",
        parent: "",
        groups: &[&[
            Item("This device", ""),
            Item("7.8 GB used", "16 GB total"),
            Item("Storage manager", ""),
            Item("Free up space", "Go to Files app to manage and free up space"),
            Item("Apps", "31 MB"),
            Item("Documents", "14 B"),
            Item("Images", "0 B"),
            Item("Videos", "0 B"),
            Item("Audio", "0 B"),
            Item("Games", "0 B"),
        ]],
    },
    Page {
        id: "battery-saver",
        title: "Battery Saver",
        parent: "",
        groups: &[&[
            Switch("Use Battery Saver", "", "battery_saver"),
            Item("Set a schedule", "No schedule"),
            Item("Turn off at 90%", "Battery Saver turns off when battery charges to 90%"),
            Footer("Battery Saver turns on dark theme and limits or turns off background activity, some visual effects, certain features, and some network connections."),
        ]],
    },
    Page {
        id: "security-privacy",
        title: "Security & privacy",
        parent: "",
        groups: &[&[
            Item("Device unlock", "Set a screen lock"),
            Header("Security"),
            Item("Device unlock", "Swipe"),
            Header("Privacy"),
            Item("Privacy controls", "Permissions, controls"),
            Item("Privacy dashboard", "App & AI assistant activity"),
            Item("Private space", "Keep private apps locked and hidden"),
        ]],
    },
    Page {
        id: "location",
        title: "Location",
        parent: "",
        groups: &[&[
            Item("Use location", ""),
            Header("Recent access"),
            Item("No apps recently accessed location", ""),
            Item("See all", ""),
            Item("App location permissions", "1 of 4 apps has access to location"),
            Item("Location services", ""),
            Footer("Learn more about Location settings"),
        ]],
    },
    Page {
        id: "accessibility",
        title: "Accessibility",
        parent: "",
        groups: &[&[
            Header("Display"),
            Item("Display size & text", "Font size, display size, bold text, outline text"),
            Item("Color & motion", "Dark theme, color correction, remove animations"),
            Item("Magnification", "Off / Zoom in to make content larger on the screen"),
            Header("Interaction controls"),
            Item("Accessibility Menu", "Off / Control volume, brightness, and more"),
            Item("Timing controls", "Touch & hold delay, time to take action"),
            Item("System controls", "Navigation mode, screen rotation"),
            Item("Vibration & haptics", "On / Control vibration levels for calls, notifications, and more"),
            Item("Physical keyboard accessibility", "Sticky keys, Bounce keys, Mouse keys"),
        ]],
    },
    Page {
        id: "system",
        title: "System",
        parent: "",
        groups: &[&[
            Link("Languages", "System languages, app languages, speech", "languages"),
            Item("Keyboard", "On-screen keyboard, tools"),
            Item("Gestures", ""),
            Link("Date & time", "GMT", "date-time"),
            Item("Backup", "Off"),
            Item("Reset options", ""),
        ]],
    },
    Page {
        id: "languages",
        title: "Languages",
        parent: "system",
        groups: &[&[
            Footer("Your device, apps, and websites use the first supported language from your preferred language list."),
            Header("Preferred Languages"),
            Item("English (United States)", "System language"),
            Item("Add a language", ""),
            Header("More language settings"),
            Item("App languages", "Choose the language for each app"),
            Item("Speech", "Control speech recognition and output"),
            Header("Regional preferences"),
            Item("Region", "United States"),
            Item("Temperature", "Use default"),
            Item("Measurement system", "Use default"),
        ]],
    },
    Page {
        id: "date-time",
        title: "Date & time",
        parent: "system",
        groups: &[&[
            Item("Automatic date and time", "Set automatically using your network and wireless signals"),
            Item("Date", ""),
            Item("Time", ""),
            Header("Time zone"),
            Item("Automatic time zone", "Set automatically based on mobile networks near you"),
            Item("Time zone", "GMT+00:00 Coordinated Universal Time"),
            Header("12-hour / 24-hour format"),
            Item("Use 24-hour format", "1:00 PM"),
        ]],
    },
];

fn android_page(id: &str) -> Option<&'static Page> {
    ANDROID_PAGES.iter().find(|p| p.id == id)
}

impl Settings {
    /// Android Settings (see `ANDROID_PAGES`): the main list as cards of rows with a symbol, a
    /// title and a summary; a page as its rows under category headings, a switch at the end
    /// of a switch's row. The shell's app bar carries the page's title and its up arrow.
    fn render_android(&self, p: &mut Painter, env: &crate::AppEnv<'_>) {
        let (width, height) = (env.width, env.height);
        let l = look(DesktopTheme::Android);
        p.scene.background = l.surface;
        let page = android_page(&self.page).unwrap_or(&ANDROID_PAGES[0]);
        let screen = screen(
            p,
            DesktopTheme::Android,
            &l,
            width,
            height as i32,
            page.title,
        );
        let main = page.id.is_empty();
        let mut y = screen.top;
        let (x, w) = (16, width.saturating_sub(32));
        if main {
            // Search Settings: a pill across the top of the list.
            let bar = Rect::new(x, y + 8, w, 56);
            p.box_(bar, l.chrome, 28);
            p.symbol("search", bar.x + 20, bar.y + 16, 24, INK);
            p.left(
                bar.x + 56,
                bar.y + 18,
                w.saturating_sub(72),
                "Search Settings",
                16,
                MUTED,
            );
            y += 80;
        }
        let d = crate::desktop_scene::shared::CalendarDate::from_clock(env.clock_us);
        let minutes = 9 * 60 + env.clock_us / 60_000_000;
        let (h, m) = ((minutes / 60) % 24, minutes % 60);
        let when = (
            format!("{} {}, {}", d.month_name(), d.day, d.year),
            format!(
                "{}:{m:02} {}",
                if h % 12 == 0 { 12 } else { h % 12 },
                if h < 12 { "AM" } else { "PM" }
            ),
        );
        for group in page.groups {
            if main {
                let h: u32 = group.len() as u32 * 75;
                p.box_(Rect::new(x, y, w, h), l.chrome, 24);
            }
            for row in group.iter() {
                y = android_row(p, &l, env, *row, Rect::new(x, y, w, 0), main, &when);
            }
            y += if main { 12 } else { 0 };
        }
        screen.end(p);
    }
}

/// Paint one row of an Android Settings page at `at` (its x, y and width); returns the y the
/// next row starts at.
fn android_row(
    p: &mut Painter,
    l: &super::look::Look,
    env: &crate::AppEnv<'_>,
    row: Row,
    at: Rect,
    main: bool,
    when: &(String, String),
) -> i32 {
    let (x, y, w) = (at.x, at.y, at.width);
    let text_x = if main { x + 68 } else { x + 8 };
    let text_w = w.saturating_sub(if main { 84 } else { 96 });
    let two_lines = |p: &mut Painter, title: &str, summary: &str, h: i32| {
        let ty = if summary.is_empty() {
            y + h / 2 - 11
        } else {
            y + h / 2 - 21
        };
        p.left(text_x, ty, text_w, title, 16, INK);
        if !summary.is_empty() {
            p.left(text_x, ty + 23, text_w, summary, 14, MUTED);
        }
    };
    match row {
        Header(title) => {
            p.left(x + 8, y + 22, w.saturating_sub(16), title, 14, l.accent);
            y + 53
        }
        Footer(text) => {
            p.left(x + 8, y + 16, w.saturating_sub(16), text, 14, MUTED);
            y + 64
        }
        Link(title, summary, page) => {
            let h = 75;
            p.button(
                Rect::new(x, y, w, h as u32),
                Color::TRANSPARENT,
                if main { 24 } else { 0 },
                &format!("settings:page:{page}"),
                title,
            );
            if main {
                p.circle(x + 36, y + h / 2, 20, l.selection);
                p.symbol(row_symbol(title), x + 24, y + h / 2 - 12, 24, l.accent);
            }
            two_lines(p, title, summary, h);
            y + h
        }
        Item(title, summary) => {
            let h = if summary.is_empty() && !main { 56 } else { 75 };
            p.insensitive(
                Rect::new(x, y, w, h as u32),
                &format!("settings:item:{title}"),
                title,
            );
            if main {
                p.circle(x + 36, y + h / 2, 20, l.selection);
                p.symbol(row_symbol(title), x + 24, y + h / 2 - 12, 24, l.accent);
            }
            // The date and time rows tell the world's.
            let summary = match title {
                "Date" => when.0.as_str(),
                "Time" => when.1.as_str(),
                _ => summary,
            };
            two_lines(p, title, summary, h);
            y + h
        }
        Switch(title, summary, flag) => {
            let h = if summary.is_empty() { 64 } else { 75 };
            let on = env.switch(flag);
            p.button(
                Rect::new(x, y, w, h as u32),
                Color::TRANSPARENT,
                0,
                &format!("shell:toggle:{flag}"),
                title,
            );
            two_lines(p, title, summary, h);
            let track = Rect::new(x + w as i32 - 60, y + h / 2 - 16, 52, 32);
            p.box_(track, if on { l.accent } else { Color(0, 0, 0, 34) }, 16);
            p.circle(
                if on { track.x + 36 } else { track.x + 16 },
                track.y + 16,
                if on { 12 } else { 8 },
                if on { Color::WHITE } else { MUTED },
            );
            y + h
        }
        Level(title, name) => {
            let level = env.level(name);
            p.left(text_x, y + 12, text_w, title, 16, INK);
            p.left(text_x, y + 35, text_w, &format!("{level}%"), 14, MUTED);
            slider(
                p,
                l,
                Rect::new(x + 8, y + 62, w.saturating_sub(16), 20),
                name,
                level,
            );
            y + 94
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

/// Apple's Settings on iOS 18: inset grouped lists, a large title and search field over the
/// root, an Apple Account card, rows with a tinted 29 pt icon, a value and a chevron, and
/// each row a page pushed with the navigation bar's way back. The order and wording are
/// iOS 18's; the geometry (44 pt rows, 35 pt between groups, 20 pt insets) is measured on
/// Apple's iOS 14 Settings in fidelity/ios, which draws the same grouped list. Switches
/// and levels are the shell's own (`shell:toggle:*`, `shell:set:*`), as on every platform;
/// rows the model has nothing behind open a page with nothing on it, and switches it has no
/// state for are drawn as a new iPhone has them.
mod ios {
    use super::{Settings, INK};
    use crate::desktop_scene::{shared::Align, Painter};
    use cw_scene::{Color, Rect};

    const BG: Color = Color::rgb(242, 242, 247);
    const GREY: Color = Color::rgb(142, 142, 147);
    const RULE: Color = Color(60, 60, 67, 74);
    const BLUE: Color = Color::rgb(0, 122, 255);
    const GREEN: Color = Color::rgb(52, 199, 89);
    const ORANGE: Color = Color::rgb(255, 149, 0);
    const RED: Color = Color::rgb(255, 59, 48);
    const PINK: Color = Color::rgb(255, 45, 85);
    const INDIGO: Color = Color::rgb(88, 86, 214);
    const CYAN: Color = Color::rgb(50, 173, 230);
    const ICON_GREY: Color = Color::rgb(142, 142, 147);
    const BLACK: Color = Color::rgb(28, 28, 30);
    const INSET: i32 = 20;
    const ROW: i32 = 44;
    const GAP: i32 = 35;

    /// Every page a row can push.
    pub const PAGES: &[&str] = &[
        "Wi-Fi",
        "Bluetooth",
        "Cellular",
        "Personal Hotspot",
        "Battery",
        "General",
        "Accessibility",
        "Camera",
        "Control Center",
        "Display & Brightness",
        "Home Screen & App Library",
        "Search",
        "Siri",
        "StandBy",
        "Wallpaper",
        "Notifications",
        "Sounds & Haptics",
        "Focus",
        "Screen Time",
        "Face ID & Passcode",
        "Emergency SOS",
        "Privacy & Security",
        "Wallet & Apple Pay",
        "Apps",
        "Apple Account",
        "About",
        "Software Update",
        "AirDrop",
        "AirPlay & Continuity",
        "Picture in Picture",
        "CarPlay",
        "iPhone Storage",
        "Background App Refresh",
        "Date & Time",
        "Keyboard",
        "Fonts",
        "Language & Region",
        "Dictionary",
        "VPN & Device Management",
        "Legal & Regulatory",
        "Transfer or Reset iPhone",
        "Keyboards",
        "Text Replacement",
        "One-Handed Keyboard",
        "Night Shift",
        "Auto-Lock",
        "Text Size",
        "Ringtone",
        "Text Tone",
        "Do Not Disturb",
        "Personal",
        "Sleep",
        "Work",
        "Display As",
        "Scheduled Summary",
        "Show Previews",
    ];

    enum Kind {
        /// A page; the value beside its chevron.
        Nav(String),
        /// A system switch the shell owns.
        Switch(&'static str),
        /// A switch the model keeps no state for, drawn as a new iPhone has it.
        Fixed(bool),
        /// A value with no page behind it.
        Value(String),
        /// A level the shell owns, as a slider.
        Level(&'static str),
        /// A button in the tint (or red): the shell target it performs.
        Button(&'static str, Color),
        /// Display & Brightness's Light and Dark choice.
        Appearance,
    }
    struct Row {
        label: &'static str,
        icon: Option<(&'static str, Color)>,
        kind: Kind,
    }
    struct Group {
        header: Option<&'static str>,
        rows: Vec<Row>,
        footer: Option<&'static str>,
    }
    fn nav(label: &'static str, icon: Option<(&'static str, Color)>, value: &str) -> Row {
        Row {
            label,
            icon,
            kind: Kind::Nav(value.to_owned()),
        }
    }
    fn page(label: &'static str) -> Row {
        nav(label, None, "")
    }
    fn fixed(label: &'static str, on: bool) -> Row {
        Row {
            label,
            icon: None,
            kind: Kind::Fixed(on),
        }
    }
    fn value(label: &'static str, v: &str) -> Row {
        Row {
            label,
            icon: None,
            kind: Kind::Value(v.to_owned()),
        }
    }
    fn group(rows: Vec<Row>) -> Group {
        Group {
            header: None,
            rows,
            footer: None,
        }
    }
    fn headed(header: &'static str, rows: Vec<Row>) -> Group {
        Group {
            header: Some(header),
            rows,
            footer: None,
        }
    }

    fn groups(name: Option<&str>, env: &crate::AppEnv<'_>) -> Vec<Group> {
        let on = |f: &str| env.switch(f);
        let onoff = |f: &str| if env.switch(f) { "On" } else { "Off" };
        let switch = |label, icon, flag| Row {
            label,
            icon,
            kind: Kind::Switch(flag),
        };
        match name {
            None => vec![
                group(vec![
                    switch("Airplane Mode", Some(("airplane", ORANGE)), "airplane_mode"),
                    nav("Wi-Fi", Some(("wifi", BLUE)), if on("wifi") { "Not Connected" } else { "Off" }),
                    nav("Bluetooth", Some(("bluetooth", BLUE)), onoff("bluetooth")),
                    nav("Cellular", Some(("signal", GREEN)), ""),
                    nav("Personal Hotspot", Some(("hotspot", GREEN)), onoff("hotspot")),
                    nav("Battery", Some(("battery-vertical", GREEN)), ""),
                ]),
                group(vec![
                    nav("General", Some(("gear", ICON_GREY)), ""),
                    nav("Accessibility", Some(("accessibility", BLUE)), ""),
                    nav("Camera", Some(("camera", ICON_GREY)), ""),
                    nav("Control Center", Some(("control-center", ICON_GREY)), ""),
                    nav("Display & Brightness", Some(("sun", BLUE)), ""),
                    nav("Home Screen & App Library", Some(("grid", INDIGO)), ""),
                    nav("Search", Some(("search", ICON_GREY)), ""),
                    nav("Siri", Some(("mic", BLACK)), ""),
                    nav("StandBy", Some(("clock", BLACK)), ""),
                    nav("Wallpaper", Some(("image", CYAN)), ""),
                ]),
                group(vec![
                    nav("Notifications", Some(("bell", RED)), ""),
                    nav("Sounds & Haptics", Some(("volume", PINK)), ""),
                    nav("Focus", Some(("moon", INDIGO)), ""),
                    nav("Screen Time", Some(("clock", INDIGO)), ""),
                ]),
                group(vec![
                    nav("Face ID & Passcode", Some(("lock", GREEN)), ""),
                    nav("Emergency SOS", Some(("bell", RED)), ""),
                    nav("Privacy & Security", Some(("shield", BLUE)), ""),
                ]),
                group(vec![nav("Wallet & Apple Pay", Some(("wallet", BLACK)), "")]),
                group(vec![nav("Apps", Some(("apps", INDIGO)), "")]),
            ],
            Some("Wi-Fi") => vec![
                group(vec![switch("Wi-Fi", None, "wifi")]),
                Group { header: None, rows: vec![], footer: Some("Apps and services will use Wi-Fi for location services when it is on.") },
            ],
            Some("Bluetooth") => vec![Group {
                header: None,
                rows: vec![switch("Bluetooth", None, "bluetooth")],
                footer: Some("This iPhone is discoverable as \u{201c}iPhone\u{201d} while Bluetooth Settings is open."),
            }],
            Some("Personal Hotspot") => vec![group(vec![switch("Allow Others to Join", None, "hotspot")])],
            Some("Battery") => vec![group(vec![
                fixed("Battery Percentage", true),
                switch("Low Power Mode", None, "battery_saver"),
            ])],
            Some("General") => vec![
                group(vec![page("About"), page("Software Update")]),
                group(vec![page("AirDrop"), page("AirPlay & Continuity"), page("Picture in Picture"), page("CarPlay")]),
                group(vec![page("iPhone Storage"), page("Background App Refresh")]),
                group(vec![page("Date & Time"), page("Keyboard"), page("Fonts"), page("Language & Region"), page("Dictionary")]),
                group(vec![page("VPN & Device Management")]),
                group(vec![page("Legal & Regulatory")]),
                group(vec![
                    page("Transfer or Reset iPhone"),
                    Row { label: "Shut Down", icon: None, kind: Kind::Button("shell:power:off", BLUE) },
                ]),
            ],
            Some("About") => vec![
                group(vec![
                    nav("Name", None, "iPhone"),
                    nav("iOS Version", None, "18.0"),
                    value("Model Name", "iPhone 11"),
                    value("Model Number", "MWLT2LL/A"),
                    value("Serial Number", "F2LZK1Q8N73D"),
                ]),
                group(vec![
                    value("Songs", "0"),
                    value("Videos", "0"),
                    value("Photos", "0"),
                    value("Applications", "14"),
                    value("Capacity", "64 GB"),
                    value("Available", "49.2 GB"),
                ]),
                group(vec![value("Wi-Fi Address", "N/A"), value("Bluetooth", "N/A")]),
            ],
            Some("Keyboard") => vec![
                group(vec![nav("Keyboards", None, "2")]),
                group(vec![page("Text Replacement"), nav("One-Handed Keyboard", None, "Off")]),
                headed("ALL KEYBOARDS", vec![
                    fixed("Auto-Capitalization", true),
                    fixed("Auto-Correction", true),
                    fixed("Check Spelling", true),
                    fixed("Enable Caps Lock", true),
                    fixed("Predictive Text", true),
                    fixed("Smart Punctuation", true),
                    fixed("Slide to Type", true),
                    fixed("Delete Slide-to-Type by Word", true),
                    fixed("Character Preview", true),
                    fixed("\u{201c}.\u{201d} Shortcut", true),
                ]),
            ],
            Some("Display & Brightness") => vec![
                headed("APPEARANCE", vec![
                    Row { label: "", icon: None, kind: Kind::Appearance },
                    fixed("Automatic", false),
                ]),
                headed("BRIGHTNESS", vec![
                    Row { label: "Brightness", icon: None, kind: Kind::Level("brightness") },
                    fixed("True Tone", true),
                ]),
                group(vec![nav("Night Shift", None, onoff("night_light"))]),
                group(vec![nav("Auto-Lock", None, "30 seconds"), fixed("Raise to Wake", true)]),
                group(vec![page("Text Size"), fixed("Bold Text", false)]),
            ],
            Some("Night Shift") => vec![group(vec![
                fixed("Scheduled", false),
                switch("Manually Enable Until Tomorrow", None, "night_light"),
            ])],
            Some("Control Center") => vec![Group {
                header: None,
                rows: vec![fixed("Access Within Apps", true), fixed("Show Home Controls", true)],
                footer: Some("Allow access to Control Center within apps. When disabled, you can still access Control Center from the Home Screen."),
            }],
            Some("Sounds & Haptics") => vec![
                group(vec![fixed("Silent Mode", false)]),
                headed("RINGTONE AND ALERT VOLUME", vec![
                    Row { label: "Volume", icon: None, kind: Kind::Level("volume") },
                    fixed("Change with Buttons", true),
                ]),
                group(vec![nav("Ringtone", None, "Reflection"), nav("Text Tone", None, "Note")]),
            ],
            Some("Notifications") => vec![
                group(vec![nav("Display As", None, "Stack")]),
                group(vec![nav("Scheduled Summary", None, "Off"), nav("Show Previews", None, "Always")]),
            ],
            Some("Focus") => vec![group(vec![
                switch("Do Not Disturb", Some(("moon", INDIGO)), "do_not_disturb"),
                nav("Personal", Some(("person", Color::rgb(175, 82, 222))), ""),
                nav("Sleep", Some(("moon", Color::rgb(255, 149, 0))), ""),
                nav("Work", Some(("document", Color::rgb(48, 176, 199))), ""),
            ])],
            Some("Do Not Disturb") => vec![group(vec![switch("Do Not Disturb", None, "do_not_disturb")])],
            _ => vec![],
        }
    }

    pub fn render(app: &Settings, p: &mut Painter, env: &crate::AppEnv<'_>) {
        let (width, height) = (env.width, env.height);
        let w = width as i32;
        p.scene.background = BG;
        p.box_(Rect::new(0, 0, width, height), BG, 0);
        let current = app.pages.last().map(String::as_str);
        let l = super::look(env.theme);
        let (screen, mut y) = match current {
            None => {
                let s = super::screen(p, env.theme, &l, width, height as i32, "Settings");
                let top = s.top;
                // The search field under the large title.
                let field = Rect::new(16, top - 2, width.saturating_sub(32), 36);
                p.box_(field, Color(118, 118, 128, 31), 10);
                p.symbol("search", field.x + 8, field.y + 10, 16, GREY);
                p.left(field.x + 30, field.y + 8, 200, "Search", 17, GREY);
                p.symbol(
                    "mic",
                    field.x + field.width as i32 - 26,
                    field.y + 9,
                    18,
                    GREY,
                );
                // The Apple Account card, signed out.
                let card = Rect::new(INSET, top + 54, (w - 2 * INSET) as u32, 70);
                p.button(
                    card,
                    Color::WHITE,
                    10,
                    "settings:open:Apple Account",
                    "Apple Account",
                );
                p.circle(card.x + 36, card.y + 35, 22, Color::rgb(190, 190, 196));
                p.symbol("person", card.x + 22, card.y + 21, 28, Color::WHITE);
                p.strong(
                    card.x + 70,
                    card.y + 14,
                    card.width.saturating_sub(100),
                    "Apple Account",
                    17,
                    BLUE,
                );
                p.paragraph(
                    card.x + 70,
                    card.y + 36,
                    card.width.saturating_sub(100),
                    "Sign in to access your iCloud data, the App Store, Apple services, and more.",
                    12,
                    INK,
                );
                p.symbol(
                    "chevron-right",
                    card.x + card.width as i32 - 24,
                    card.y + 27,
                    14,
                    Color::rgb(196, 196, 199),
                );
                (Some(s), top + 54 + 70 + GAP)
            }
            Some(_) => (None, 35),
        };
        let pane = match &screen {
            Some(_) => None,
            None => Some(p.pane(
                &format!(
                    "page{}",
                    PAGES.iter().position(|x| Some(*x) == current).unwrap_or(0)
                ),
                Rect::new(0, 0, width, height),
            )),
        };
        if let Some(pane) = &pane {
            y += pane.top();
        }
        let cell_w = (w - 2 * INSET) as u32;
        for g in groups(current, env) {
            if let Some(h) = g.header {
                p.left(INSET + 16, y - 26, 300, h, 13, GREY);
            }
            let rows = g.rows.len() as i32;
            let tall = |r: &Row| match r.kind {
                Kind::Appearance => 196,
                Kind::Level(_) => 52,
                _ => ROW,
            };
            let total: i32 = g.rows.iter().map(tall).sum();
            if rows > 0 {
                p.box_(Rect::new(INSET, y, cell_w, total as u32), Color::WHITE, 10);
            }
            let mut ry = y;
            for (i, r) in g.rows.iter().enumerate() {
                let h = tall(r);
                let mut x = INSET + 16;
                if let Some((icon, tint)) = r.icon {
                    p.box_(Rect::new(x, ry + (h - 29) / 2, 29, 29), tint, 7);
                    p.symbol(icon, x + 5, ry + (h - 29) / 2 + 5, 19, Color::WHITE);
                    x += 29 + 16;
                }
                let right = INSET + cell_w as i32 - 16;
                match &r.kind {
                    Kind::Nav(v) => {
                        p.button(
                            Rect::new(INSET, ry, cell_w, h as u32),
                            Color::TRANSPARENT,
                            0,
                            &format!("settings:open:{}", r.label),
                            r.label,
                        );
                        p.left(
                            x,
                            ry + 11,
                            (right - x - 120).max(40) as u32,
                            r.label,
                            17,
                            INK,
                        );
                        p.symbol(
                            "chevron-right",
                            right - 10,
                            ry + 15,
                            14,
                            Color::rgb(196, 196, 199),
                        );
                        if !v.is_empty() {
                            p.label(right - 170, ry + 11, 152, v, 17, GREY, false, Align::Right);
                        }
                    }
                    Kind::Value(v) => {
                        p.left(x, ry + 11, 200, r.label, 17, INK);
                        p.label(right - 200, ry + 11, 200, v, 17, GREY, false, Align::Right);
                    }
                    Kind::Switch(flag) => {
                        p.button(
                            Rect::new(INSET, ry, cell_w, h as u32),
                            Color::TRANSPARENT,
                            0,
                            &format!("shell:toggle:{flag}"),
                            r.label,
                        );
                        p.left(
                            x,
                            ry + 11,
                            (right - x - 70).max(40) as u32,
                            r.label,
                            17,
                            INK,
                        );
                        knob(p, right - 51, ry + 6, env.switch(flag));
                    }
                    Kind::Fixed(on) => {
                        p.left(
                            x,
                            ry + 11,
                            (right - x - 70).max(40) as u32,
                            r.label,
                            17,
                            INK,
                        );
                        knob(p, right - 51, ry + 6, *on);
                    }
                    Kind::Button(target, tint) => {
                        p.button(
                            Rect::new(INSET, ry, cell_w, h as u32),
                            Color::TRANSPARENT,
                            0,
                            target,
                            r.label,
                        );
                        p.left(x, ry + 11, 300, r.label, 17, *tint);
                    }
                    Kind::Level(name) => {
                        let level = env.level(name);
                        p.symbol("sun", x, ry + 17, 16, GREY);
                        let track = Rect::new(x + 30, ry + 24, (right - x - 64).max(40) as u32, 4);
                        p.box_(track, Color(120, 120, 128, 64), 2);
                        let filled = track.width * u32::from(level) / 100;
                        p.box_(Rect::new(track.x, track.y, filled.max(1), 4), BLUE, 2);
                        p.circle(track.x + filled as i32, track.y + 2, 14, Color::WHITE);
                        p.ring(
                            track.x + filled as i32,
                            track.y + 2,
                            14,
                            1,
                            Color(0, 0, 0, 30),
                        );
                        p.symbol("sun", right - 22, ry + 14, 22, GREY);
                        // Twenty steps along the track, each one exact percentage.
                        for step in 1..=20u32 {
                            let at = track.x + (track.width * (step - 1) / 20) as i32;
                            p.region(
                                Rect::new(at, ry, track.width / 20, h as u32),
                                &format!("shell:set:{name}:{}", step * 5),
                                &format!("{} {}%", r.label, step * 5),
                            );
                        }
                    }
                    Kind::Appearance => {
                        let dark = env.switch("dark_mode");
                        for (k, (label, is_dark)) in
                            [("Light", false), ("Dark", true)].iter().enumerate()
                        {
                            let cx = INSET + cell_w as i32 * (1 + 2 * k as i32) / 4;
                            let phone = Rect::new(cx - 38, ry + 18, 76, 110);
                            // Each preview is the wallpaper as that appearance shows it, so
                            // its colours are its own in either appearance.
                            let (top, bottom) = if *is_dark {
                                (Color::rgb(64, 22, 58), Color::rgb(24, 18, 64))
                            } else {
                                (Color::rgb(255, 176, 92), Color::rgb(236, 92, 120))
                            };
                            p.box_(phone, Color::rgb(30, 30, 46), 12);
                            p.gradient(
                                Rect::new(phone.x + 3, phone.y + 3, 70, 104),
                                top,
                                bottom,
                                8,
                            );
                            p.label(cx - 50, ry + 136, 100, label, 15, INK, false, Align::Center);
                            let chosen = dark == *is_dark;
                            p.circle(
                                cx,
                                ry + 172,
                                11,
                                if chosen { BLUE } else { Color(60, 60, 67, 74) },
                            );
                            if chosen {
                                p.symbol("check", cx - 7, ry + 165, 14, Color::WHITE);
                            } else {
                                p.circle(cx, ry + 172, 10, Color::WHITE);
                                // Choosing the other appearance is the shell's switch.
                                p.region(
                                    Rect::new(cx - 50, ry + 10, 100, 180),
                                    "shell:toggle:dark_mode",
                                    label,
                                );
                            }
                        }
                    }
                }
                if i + 1 < g.rows.len() {
                    p.hline(x, ry + h - 1, (right + 16 - x) as u32, RULE);
                }
                ry += h;
            }
            y += total;
            if let Some(f) = g.footer {
                y += p.paragraph(INSET + 16, y + 8, cell_w.saturating_sub(32), f, 13, GREY) as i32
                    + 8;
            }
            y += GAP;
        }
        if let Some(pane) = pane {
            p.end_pane(pane, None);
        }
        if let Some(s) = screen {
            s.end(p);
        }
    }
    /// The iOS switch: a green track when on, a white knob.
    fn knob(p: &mut Painter, x: i32, y: i32, on: bool) {
        let track = Rect::new(x, y, 51, 31);
        p.box_(track, if on { GREEN } else { Color(120, 120, 128, 40) }, 16);
        p.circle(if on { x + 35 } else { x + 16 }, y + 15, 13, Color::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ios_pages_push_and_pop_with_their_title_and_way_back() {
        let (mut app, _) = Settings::launch("", 1, 0);
        assert!(app.chrome().is_empty());
        assert!(app.click(1, "settings:back", 0).is_err());
        app.click(1, "settings:open:General", 0).unwrap();
        app.click(1, "settings:open:About", 0).unwrap();
        assert_eq!(app.chrome()[0], ("title".into(), "About".into()));
        assert_eq!(app.chrome()[1].1, "back\tsettings:back\tGeneral");
        app.click(1, "settings:back", 0).unwrap();
        assert_eq!(app.pages, vec!["General".to_owned()]);
        assert!(app.click(1, "settings:open:Nowhere", 0).is_err());
    }
    #[test]
    fn every_control_ios_settings_paints_is_one_the_model_or_the_shell_takes() {
        for pages in [
            vec![],
            vec!["General"],
            vec!["Display & Brightness"],
            vec!["Wi-Fi"],
            vec!["General", "Keyboard"],
        ] {
            let (mut app, _) = Settings::launch("", 1, 0);
            for p in &pages {
                app.click(1, &format!("settings:open:{p}"), 0).unwrap();
            }
            let mut scene = Painter::themed(DesktopTheme::Ios, 414, 768, 0);
            app.render(
                &mut scene,
                &crate::AppEnv {
                    theme: DesktopTheme::Ios,
                    width: 414,
                    height: 768,
                    clock_us: 0,
                    settings: &crate::SystemSettings::DEFAULT,
                    clipboard: None,
                    share_to: None,
                    editor: None,
                    pointer: None,
                    files: Default::default(),
                },
            );
            let targets: Vec<String> = scene
                .scene
                .nodes
                .iter()
                .filter_map(|n| n.interaction.clone())
                .collect();
            assert!(!targets.is_empty(), "{pages:?}");
            for t in targets {
                if t.starts_with("shell:") {
                    continue;
                }
                assert!(app.clone().click(1, &t, 0).is_ok(), "{t} on {pages:?}");
            }
        }
    }
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
        // Wi-Fi in the sidebar is a panel the model does not have: insensitive.
        let wifi_panel = p.scene.nodes.iter().find(|n| {
            n.semantic
                .as_ref()
                .is_some_and(|s| s.label == "Wi-Fi" && s.disabled)
        });
        assert!(wifi_panel.is_some());
        app.click(1, "settings:section:2", 0).unwrap();
        let p = render(&app);
        assert!(label(&p, "Sound & Focus").is_none());
        assert!(label(&p, "Flashlight").is_none());
        assert!(p
            .scene
            .nodes
            .iter()
            .any(|n| n.interaction.as_deref() == Some("shell:set:volume:50")));
    }
    #[test]
    fn android_settings_lists_its_pages_and_back_returns_to_the_parent() {
        let (mut app, _) = Settings::launch("", 1, 0);
        let render = |app: &Settings| {
            let mut p = Painter::themed(DesktopTheme::Android, 411, 770, 0);
            app.render(
                &mut p,
                &crate::AppEnv {
                    theme: DesktopTheme::Android,
                    width: 411,
                    height: 770,
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
        let has = |p: &Painter, target: &str| {
            p.scene
                .nodes
                .iter()
                .any(|n| n.interaction.as_deref() == Some(target))
        };
        let p = render(&app);
        assert!(has(&p, "settings:page:network-internet"));
        assert!(has(&p, "settings:page:display"));
        assert_eq!(app.phone_back(), None);
        app.click(1, "settings:page:network-internet", 0).unwrap();
        app.click(1, "settings:page:internet", 0).unwrap();
        assert_eq!(app.title(DesktopTheme::Android), "Internet");
        assert!(has(&render(&app), "shell:toggle:wifi"));
        assert_eq!(app.phone_back(), Some("settings:back"));
        app.click(1, "settings:back", 0).unwrap();
        assert_eq!(app.page, "network-internet");
        app.click(1, "settings:back", 0).unwrap();
        assert_eq!(app.page, "");
        assert!(app.click(1, "settings:page:nowhere", 0).is_err());
        // Every switch a page shows is one the system store has, and every link a page.
        for page in ANDROID_PAGES {
            for row in page.groups.iter().flat_map(|g| g.iter()) {
                match row {
                    Row::Switch(_, _, flag) => {
                        assert!(crate::SystemSettings::DEFAULT.flag(flag).is_ok(), "{flag}")
                    }
                    Row::Link(_, _, to) => assert!(android_page(to).is_some(), "{to}"),
                    Row::Level(_, name) => {
                        assert!(crate::SystemSettings::DEFAULT.level(name).is_ok())
                    }
                    _ => {}
                }
            }
        }
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
