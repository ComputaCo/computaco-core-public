//! Clock, stopwatch and timer over simulation time. There is no host clock anywhere in
//! this file; every reading is derived from the `clock_us` the shell threads in.
use super::look::{action, look, screen, INK, LINE, MUTED};
use crate::desktop_scene::shared::{arc_points, CalendarDate};
use crate::desktop_scene::{shared::Align, DesktopTheme, Painter};
use crate::AppEffect;
use cw_scene::{Color, Rect};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    #[default]
    World,
    Stopwatch,
    Timer,
}
/// A stopwatch is `elapsed` plus, while running, the time since `started`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stopwatch {
    pub elapsed_us: u64,
    pub started_us: Option<u64>,
}
impl Stopwatch {
    pub fn reading(&self, now_us: u64) -> u64 {
        self.elapsed_us + self.started_us.map_or(0, |s| now_us.saturating_sub(s))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    pub face: Face,
    pub stopwatch: Stopwatch,
    pub laps: Vec<u64>,
    /// Countdown length in whole minutes, and when it was started.
    pub timer_minutes: u64,
    pub timer_started_us: Option<u64>,
}
impl Clock {
    pub const KIND: &'static str = "clock";
    pub fn launch(_argument: &str, _window: u64, _clock_us: u64) -> (Self, Vec<AppEffect>) {
        (
            Self {
                timer_minutes: 5,
                ..Default::default()
            },
            vec![],
        )
    }
    pub fn kind(&self) -> &'static str {
        Self::KIND
    }
    pub fn title(&self, theme: DesktopTheme) -> String {
        if theme == DesktopTheme::Ubuntu {
            "Clocks".into()
        } else {
            "Clock".into()
        }
    }
    pub fn document(&self) -> String {
        String::new()
    }
    pub fn caption(&self) -> String {
        String::new()
    }
    pub fn modified(&self) -> bool {
        false
    }
    /// Facts the Ubuntu frame needs: the face, whose tab its header bar shows selected.
    pub fn chrome(&self) -> Vec<(String, String)> {
        let face = match self.face {
            Face::World => "world",
            Face::Stopwatch => "stopwatch",
            Face::Timer => "timer",
        };
        vec![("face".into(), face.into())]
    }
    pub fn offline(&mut self, _tag: &str, _reason: &str) {}
    pub fn http(
        &mut self,
        _window: u64,
        _tag: &str,
        _status: u16,
        _body: &str,
    ) -> Result<Vec<AppEffect>, String> {
        Err("clock makes no requests".into())
    }
    pub fn text(&mut self, _text: &str) -> Result<(), String> {
        Err("clock has no text field".into())
    }
    pub fn key(&mut self, window: u64, key: &str, clock_us: u64) -> Result<Vec<AppEffect>, String> {
        match key {
            "Enter" => self.click(window, "clock:start", clock_us),
            other => Err(format!("unsupported clock key {other}")),
        }
    }
    /// Microseconds left on the countdown, or `None` when no timer is running.
    pub fn remaining(&self, now_us: u64) -> Option<u64> {
        let started = self.timer_started_us?;
        let total = self.timer_minutes * 60 * 1_000_000;
        Some(total.saturating_sub(now_us.saturating_sub(started)))
    }
    pub fn click(
        &mut self,
        _window: u64,
        target: &str,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String> {
        let command = target
            .strip_prefix("clock:")
            .ok_or("interaction does not belong to the clock")?;
        match command {
            "world" => self.face = Face::World,
            "stopwatch" => self.face = Face::Stopwatch,
            "timer" => self.face = Face::Timer,
            "start" => match self.face {
                Face::Stopwatch => {
                    if self.stopwatch.started_us.is_none() {
                        self.stopwatch.started_us = Some(clock_us);
                    }
                }
                Face::Timer => self.timer_started_us = Some(clock_us),
                Face::World => return Err("the world clock cannot be started".into()),
            },
            "stop" => match self.face {
                Face::Stopwatch => {
                    self.stopwatch.elapsed_us = self.stopwatch.reading(clock_us);
                    self.stopwatch.started_us = None;
                }
                Face::Timer => self.timer_started_us = None,
                Face::World => return Err("the world clock cannot be stopped".into()),
            },
            "reset" => match self.face {
                Face::Stopwatch => {
                    self.stopwatch = Stopwatch::default();
                    self.laps.clear();
                }
                Face::Timer => self.timer_started_us = None,
                Face::World => return Err("the world clock cannot be reset".into()),
            },
            "lap" => {
                if self.stopwatch.started_us.is_some() && self.laps.len() < 64 {
                    self.laps.push(self.stopwatch.reading(clock_us));
                }
            }
            "shorter" => self.timer_minutes = self.timer_minutes.saturating_sub(1).max(1),
            // A quick-start length, in minutes.
            preset if preset.starts_with("preset:") => {
                let minutes: u64 = preset["preset:".len()..]
                    .parse()
                    .map_err(|_| format!("unknown timer preset {preset}"))?;
                if !(1..=180).contains(&minutes) {
                    return Err(format!("unknown timer preset {preset}"));
                }
                self.timer_minutes = minutes;
            }
            "longer" => self.timer_minutes = (self.timer_minutes + 1).min(180),
            other => return Err(format!("unknown clock command {other}")),
        }
        Ok(vec![])
    }
    pub fn page(&self, page: &mut cw_protocol::Page) {
        use cw_protocol::PageElement as E;
        let act = |url: &str| cw_protocol::PageAction {
            method: "APP".into(),
            url: url.into(),
            fields: Default::default(),
        };
        page.elements.push(E::Heading {
            id: "clock-face".into(),
            text: match self.face {
                Face::World => "World clock",
                Face::Stopwatch => "Stopwatch",
                Face::Timer => "Timer",
            }
            .into(),
            level: 2,
        });
        for (id, label) in [
            ("clock:world", "World clock"),
            ("clock:stopwatch", "Stopwatch"),
            ("clock:timer", "Timer"),
        ] {
            page.elements.push(E::Button {
                id: id.into(),
                text: label.into(),
                action: act(id),
                style: None,
            });
        }
        let controls: &[(&str, &str)] = match self.face {
            Face::World => &[],
            Face::Stopwatch => &[
                ("clock:start", "Start"),
                ("clock:stop", "Stop"),
                ("clock:lap", "Lap"),
                ("clock:reset", "Reset"),
            ],
            Face::Timer => &[
                ("clock:shorter", "Shorter"),
                ("clock:longer", "Longer"),
                ("clock:start", "Start"),
                ("clock:stop", "Stop"),
            ],
        };
        for (id, label) in controls {
            page.elements.push(E::Button {
                id: (*id).into(),
                text: (*label).into(),
                action: act(id),
                style: None,
            });
        }
        for (index, lap) in self.laps.iter().enumerate() {
            page.elements.push(E::Text {
                id: format!("clock-lap-{index}"),
                text: format!("Lap {}: {}", index + 1, duration(*lap)),
            });
        }
    }
    pub fn render(&self, p: &mut Painter, env: &crate::AppEnv<'_>) {
        let (theme, width, height) = (env.theme, env.width, env.height);
        if theme == DesktopTheme::Ubuntu {
            return self.render_gnome(p);
        }
        let l = look(theme);
        p.scene.background = l.surface;
        let screen = screen(p, theme, &l, width, height as i32, &self.title(theme));
        let mut top = screen.top;
        let mut x = 8;
        for (target, label) in [
            ("clock:world", "World"),
            ("clock:stopwatch", "Stopwatch"),
            ("clock:timer", "Timer"),
        ] {
            let on = matches!(
                (self.face, target),
                (Face::World, "clock:world")
                    | (Face::Stopwatch, "clock:stopwatch")
                    | (Face::Timer, "clock:timer")
            );
            let w = p.measure(label, 12, true) + 24;
            let r = Rect::new(x, top + 6, w, 26);
            p.button(
                r,
                if on { l.selection } else { Color::TRANSPARENT },
                l.radius,
                target,
                label,
            );
            p.label(
                r.x,
                r.y + 4,
                r.width,
                label,
                12,
                if on { l.accent } else { MUTED },
                on,
                Align::Center,
            );
            x += w as i32 + 6;
        }
        top += 40;
        p.hline(0, top, width, LINE);
        // The rendered reading is always the state at the last action, never "now":
        // a scene must be reproducible from the snapshot alone.
        let (big, controls): (String, &[(&str, &str)]) = match self.face {
            Face::World => (String::new(), &[]),
            Face::Stopwatch => (
                duration(
                    self.stopwatch
                        .reading(self.stopwatch.started_us.unwrap_or(0)),
                ),
                if self.stopwatch.started_us.is_some() {
                    &[("clock:lap", "Lap"), ("clock:stop", "Stop")]
                } else {
                    &[("clock:start", "Start"), ("clock:reset", "Reset")]
                },
            ),
            Face::Timer => (
                duration(self.timer_minutes * 60 * 1_000_000),
                if self.timer_started_us.is_some() {
                    &[("clock:stop", "Cancel")]
                } else {
                    &[
                        ("clock:shorter", "−"),
                        ("clock:longer", "+"),
                        ("clock:start", "Start"),
                    ]
                },
            ),
        };
        if self.face == Face::World {
            self.world_face(p, &l, width, height, top);
        } else {
            p.label(
                0,
                top + (height as i32 - top) / 3,
                width,
                &big,
                40,
                INK,
                false,
                Align::Center,
            );
        }
        let mut x = 14;
        for (target, label) in controls {
            let w = p.measure(label, 13, true) + 30;
            action(
                p,
                &l,
                Rect::new(x, height as i32 - 46, w, 32),
                label,
                target,
                *target == "clock:start",
            );
            x += w as i32 + 8;
        }
        // Every lap, newest first, in a list that scrolls once it outgrows its column.
        if !self.laps.is_empty() {
            let laps = p.pane(
                "laps",
                Rect::new(
                    width as i32 - 156,
                    top + 8,
                    150,
                    (height as i32 - top - 64).max(18) as u32,
                ),
            );
            for (index, lap) in self.laps.iter().rev().enumerate() {
                p.left(
                    width as i32 - 150,
                    laps.top() + 4 + index as i32 * 18,
                    140,
                    &format!("Lap {}  {}", self.laps.len() - index, duration(*lap)),
                    11,
                    MUTED,
                );
            }
            p.end_pane(laps, None);
        }
        screen.end(p);
    }
    /// An analogue face drawn from the world clock the shell already carries.
    fn world_face(
        &self,
        p: &mut Painter,
        l: &super::look::Look,
        width: u32,
        height: u32,
        top: i32,
    ) {
        let size = (width.min(height.saturating_sub(top as u32 + 60)) * 3 / 5).clamp(80, 260);
        let (cx, cy) = (width as i32 / 2, top + (height as i32 - top) / 2 - 16);
        let radius = size as i32 / 2;
        p.ring(cx, cy, radius as u32, 2, LINE);
        for tick in 0..12 {
            let outer = arc_points(cx, cy, radius - 4, tick * 30, tick * 30, 30);
            let inner = arc_points(cx, cy, radius - 12, tick * 30, tick * 30, 30);
            p.line(vec![outer[0], inner[0]], MUTED, 2);
        }
        // Hands are omitted deliberately: the world clock advances with simulation time,
        // and a scene must be reproducible from state alone, so the digital readout is
        // the honest presentation of the current tick.
        let date = CalendarDate::from_clock(0);
        p.label(
            0,
            cy - 12,
            width,
            &format!("{} {}", date.weekday_name(), date.day),
            15,
            INK,
            true,
            Align::Center,
        );
        p.label(
            0,
            cy + 10,
            width,
            date.month_name(),
            12,
            MUTED,
            false,
            Align::Center,
        );
        let _ = l;
    }
}
/// GNOME Clocks' window background, and the grey of its flat buttons.
pub const GNOME_WINDOW: Color = Color::rgb(250, 250, 250);
const GNOME_BUTTON: Color = Color::rgb(230, 230, 230);
const GNOME_BLUE: Color = Color::rgb(53, 132, 228);
const GNOME_DIM: Color = Color::rgb(145, 145, 145);
const GNOME_INK: Color = Color::rgb(46, 46, 46);
impl Clock {
    /// GNOME Clocks 46 as Ubuntu 24.04 shows it at 870x690, below the header bar that
    /// holds its view switcher (desktop_scene/ubuntu.rs): the stopwatch's split reading
    /// over Start and Lap, the timer's quick-start lengths over its three spin buttons,
    /// and the world view's empty page (measured on the real application,
    /// fidelity/tours). What the model lacks is announced insensitive: world clocks,
    /// and the timer's hours and seconds (it counts whole minutes).
    fn render_gnome(&self, p: &mut Painter) {
        p.scene.background = GNOME_WINDOW;
        match self.face {
            Face::World => {
                p.symbol("globe", 366, 214, 128, GNOME_DIM);
                let r = Rect::new(365, 387, 191, 44);
                p.box_(r, GNOME_BLUE, 22);
                p.insensitive(r, "clock:add-world-clock", "Add World Clock");
                p.label(
                    r.x,
                    r.y + 12,
                    r.width,
                    "Add World Clock…",
                    15,
                    Color::WHITE,
                    true,
                    Align::Center,
                );
                p.presentation();
            }
            Face::Stopwatch => self.gnome_stopwatch(p),
            Face::Timer => self.gnome_timer(p),
        }
    }
    fn gnome_stopwatch(&self, p: &mut Painter) {
        let running = self.stopwatch.started_us.is_some();
        let us = self
            .stopwatch
            .reading(self.stopwatch.started_us.unwrap_or(0));
        let ds = us / 100_000;
        let (h, m, sec, tenth) = (ds / 36_000, (ds / 600) % 60, (ds / 10) % 60, ds % 10);
        // Leading parts still at zero are greyed, as Clocks greys them.
        let lit = |part: u8| match part {
            0 => h > 0,
            1 => h > 0 || m > 0,
            _ => true,
        };
        let parts = [
            (272, 78, format!("{h:02}"), 0),
            (350, 18, "∶".to_owned(), 0),
            (368, 78, format!("{m:02}"), 1),
            (446, 18, "∶".to_owned(), 1),
            (464, 78, format!("{sec:02}"), 2),
            (542, 16, ".".to_owned(), 2),
        ];
        for (x, w, text, part) in parts {
            let color = if lit(part) { GNOME_INK } else { GNOME_DIM };
            // The ratio sign Clocks sets between the parts is painted as a colon, always in
            // ink, 6 px left of and 4 px above its label's bounds.
            if text == "∶" {
                p.label(x - 6, 225, w, ":", 64, GNOME_INK, false, Align::Center);
            } else {
                p.label(x, 229, w, &text, 64, color, false, Align::Center);
            }
            p.presentation();
            p.container(Rect::new(x, 229, w, 85), "label", &text);
        }
        let tenth = tenth.to_string();
        p.label(558, 256, 28, &tenth, 40, GNOME_INK, false, Align::Center);
        p.presentation();
        p.container(Rect::new(558, 250, 28, 60), "label", &tenth);
        let (first, second): ((&str, &str), (&str, &str)) = if running {
            (("clock:stop", "Pause"), ("clock:lap", "Lap"))
        } else if us > 0 {
            (("clock:start", "Resume"), ("clock:reset", "Reset"))
        } else {
            (("clock:start", "Start"), ("", "Lap"))
        };
        for (x, (action, label)) in [(263, first), (487, second)] {
            let r = Rect::new(x, 338, 120, 36);
            // GTK paints the two buttons 6 px left of and above the bounds it announces.
            let painted = Rect::new(x - 6, 332, 120, 36);
            let blue = action == "clock:start";
            let fill = if blue {
                GNOME_BLUE
            } else {
                Color::rgb(240, 240, 240)
            };
            p.box_(painted, fill, 18);
            if action.is_empty() {
                // Nothing to lap before the stopwatch runs.
                p.insensitive(r, "clock:lap:idle", label);
            } else {
                p.region(r, action, label);
            }
            let ink = match (blue, action.is_empty()) {
                (true, _) => Color::WHITE,
                (false, true) => GNOME_DIM,
                (false, false) => GNOME_INK,
            };
            p.label(
                painted.x,
                painted.y + 9,
                120,
                label,
                14,
                ink,
                true,
                Align::Center,
            );
            p.presentation();
        }
    }
    fn gnome_timer(&self, p: &mut Painter) {
        let running = self.timer_started_us.is_some();
        for (y, text) in [(112, "Quick Start"), (262, "Set Timer")] {
            p.label(229, y, 400, text, 16, GNOME_INK, true, Align::Center);
            p.presentation();
            p.container(Rect::new(229, y, 400, 22), "label", text);
        }
        const PRESETS: [(u64, &str); 8] = [
            (1, "1 m"),
            (2, "2 m"),
            (3, "3 m"),
            (5, "5 m"),
            (15, "15 m"),
            (30, "30 m"),
            (45, "45 m"),
            (60, "1 h"),
        ];
        for (i, (minutes, label)) in PRESETS.iter().enumerate() {
            let r = Rect::new(296 + (i as i32 % 4) * 78, 157 + (i as i32 / 4) * 46, 66, 34);
            // GTK announces each quick-start button 17 px right of and 5 px below where
            // it paints it.
            let painted = Rect::new(r.x - 17, r.y - 5, 66, 34);
            p.box_(painted, GNOME_BUTTON, 8);
            p.region(r, &format!("clock:preset:{minutes}"), label);
            p.label(
                painted.x,
                painted.y + 8,
                r.width,
                label,
                14,
                GNOME_INK,
                true,
                Align::Center,
            );
            p.presentation();
        }
        // Hours, minutes and seconds; the model counts whole minutes up to three hours.
        let fields = [
            (283, self.timer_minutes / 60, None),
            (
                390,
                self.timer_minutes % 60,
                Some(("clock:longer", "clock:shorter")),
            ),
            (497, 0, None),
        ];
        for (i, (x, value, actions)) in fields.into_iter().enumerate() {
            let r = Rect::new(x, 302, 78, 138);
            p.box_(r, GNOME_BUTTON, 8);
            p.hline(x, 336, 78, Color(0, 0, 0, 24));
            p.hline(x, 405, 78, Color(0, 0, 0, 24));
            p.label(x, 307, 78, "+", 16, GNOME_INK, false, Align::Center);
            p.presentation();
            p.label(x, 412, 78, "−", 16, GNOME_INK, false, Align::Center);
            p.presentation();
            p.label(
                x,
                341,
                78,
                &format!("{value:02}"),
                44,
                GNOME_INK,
                false,
                Align::Center,
            );
            p.presentation();
            let value = value.to_string();
            match actions {
                Some((up, down)) if !running => {
                    // The spin button is announced once; its minus half takes its own click.
                    p.region(r, up, "");
                    p.announce("spinbutton", Some(&value));
                    p.region_above(Rect::new(x, 410, 78, 32), down, "−");
                    p.announce("presentation", None);
                }
                _ => {
                    let field = ["hours", "minutes", "seconds"][i];
                    p.insensitive(r, &format!("clock:spin:{field}"), "");
                    p.announce("spinbutton", Some(&value));
                }
            }
            if i < 2 {
                p.label(x + 84, 337, 17, ":", 44, GNOME_INK, false, Align::Center);
                p.presentation();
                p.container(Rect::new(x + 84, 302, 17, 138), "label", "∶");
            }
        }
        let r = Rect::new(412, 488, 98, 44);
        // GTK announces Start 32 px right of and 10 px below where it paints it.
        let painted = Rect::new(380, 478, 98, 44);
        let (action, label) = if running {
            ("clock:stop", "Cancel")
        } else {
            ("clock:start", "Start")
        };
        p.box_(painted, GNOME_BLUE, 22);
        p.region(r, action, label);
        p.label(
            painted.x,
            painted.y + 13,
            r.width,
            label,
            14,
            Color::WHITE,
            true,
            Align::Center,
        );
        p.presentation();
    }
}
/// `h:mm:ss.cc` for a stopwatch, `mm:ss` for anything under an hour.
fn duration(us: u64) -> String {
    let total_cs = us / 10_000;
    let (h, m, s, cs) = (
        total_cs / 360_000,
        (total_cs / 6_000) % 60,
        (total_cs / 100) % 60,
        total_cs % 100,
    );
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{cs:02}")
    } else {
        format!("{m:02}:{s:02}.{cs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopwatch_accumulates_simulation_time_only() {
        let (mut c, _) = Clock::launch("", 1, 0);
        c.click(1, "clock:stopwatch", 0).unwrap();
        c.click(1, "clock:start", 1_000_000).unwrap();
        assert_eq!(c.stopwatch.reading(3_500_000), 2_500_000);
        c.click(1, "clock:lap", 3_500_000).unwrap();
        c.click(1, "clock:stop", 4_000_000).unwrap();
        assert_eq!(c.stopwatch.reading(9_000_000), 3_000_000);
        assert_eq!(c.laps, vec![2_500_000]);
        c.click(1, "clock:reset", 9_000_000).unwrap();
        assert_eq!(c.stopwatch, Stopwatch::default());
        assert!(c.laps.is_empty());
    }
    #[test]
    fn timer_counts_down_from_a_real_length() {
        let (mut c, _) = Clock::launch("", 1, 0);
        c.click(1, "clock:timer", 0).unwrap();
        assert_eq!(c.timer_minutes, 5);
        c.click(1, "clock:longer", 0).unwrap();
        c.click(1, "clock:start", 60_000_000).unwrap();
        assert_eq!(c.remaining(120_000_000), Some(5 * 60 * 1_000_000));
        c.click(1, "clock:stop", 0).unwrap();
        assert_eq!(c.remaining(0), None);
    }
    #[test]
    fn faces_refuse_controls_that_do_not_belong_to_them() {
        let (mut c, _) = Clock::launch("", 1, 0);
        assert!(c.click(1, "clock:start", 0).is_err());
        assert!(c.click(1, "clock:bogus", 0).is_err());
        assert!(c.click(1, "not-mine", 0).is_err());
    }
    #[test]
    fn ubuntu_paints_gnome_clocks_and_routes_only_what_the_model_accepts() {
        for face in ["clock:world", "clock:stopwatch", "clock:timer"] {
            let (mut c, _) = Clock::launch("", 1, 0);
            c.click(1, face, 0).unwrap();
            let mut p = Painter::themed(DesktopTheme::Ubuntu, 868, 643, 0);
            c.render(
                &mut p,
                &crate::AppEnv {
                    theme: DesktopTheme::Ubuntu,
                    width: 868,
                    height: 643,
                    clock_us: 0,
                    settings: &crate::SystemSettings::DEFAULT,
                    clipboard: None,
                    share_to: None,
                    editor: None,
                    pointer: None,
                    files: Default::default(),
                },
            );
            for n in &p.scene.nodes {
                let Some(id) = n.interaction.as_deref() else {
                    continue;
                };
                let refused = c.clone().click(1, id, 0).is_err();
                let insensitive = n.semantic.as_ref().is_some_and(|s| s.disabled);
                assert_eq!(refused, insensitive, "{face}: {id}");
            }
        }
        let (mut c, _) = Clock::launch("", 1, 0);
        c.click(1, "clock:preset:45", 0).unwrap();
        assert_eq!(c.timer_minutes, 45);
        assert!(c.click(1, "clock:preset:0", 0).is_err());
    }
    #[test]
    fn durations_format_without_floating_point() {
        assert_eq!(duration(0), "00:00.00");
        assert_eq!(duration(3_661_230_000), "1:01:01.23");
    }
}
