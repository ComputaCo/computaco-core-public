//! Calendar backed by the `calendar` service. Events are real service records; the grid
//! is derived from simulation time only. Nothing here invents an appointment.
use super::look::{action, look, notice, screen, FAINT, INK, LINE, MUTED};
use super::{push_bounded, Status};
use crate::desktop_scene::shared::{Align, CalendarDate, MONTHS, WEEKDAYS};
use crate::desktop_scene::{DesktopTheme, Painter};
use crate::AppEffect;
use cw_scene::{Color, Rect};
use serde::{Deserialize, Serialize};

/// One simulated day, in the microseconds the world clock counts.
pub const DAY_US: u64 = 24 * 60 * 60 * 1_000_000;
const HOUR_US: u64 = 60 * 60 * 1_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Month,
    Day,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub owner: String,
    pub title: String,
    pub start: u64,
    pub end: u64,
}

/// Composer for a new or edited event. `editing` holds the service id when editing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub title: String,
    pub day: u64,
    pub hour: u64,
    pub length_hours: u64,
    pub editing: Option<String>,
    /// The title is selected (Ctrl+A): what is typed next replaces it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub selected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    pub base: String,
    pub view: View,
    /// Day index the grid is anchored on, counted from the world epoch.
    pub cursor_day: u64,
    pub events: Vec<Event>,
    pub selected: Option<String>,
    pub draft: Option<Draft>,
    pub status: Status,
}
impl Calendar {
    pub const KIND: &'static str = "calendar";
    pub fn launch(argument: &str, window: u64, clock_us: u64) -> (Self, Vec<AppEffect>) {
        // `<service>#<date>` opens on a day; a bare `<date>` is a day on the default service.
        let (base, place) = match argument.split_once('#') {
            Some((base, place)) => (base, place),
            None if day_of(argument).is_some() => ("", argument),
            None => (argument, ""),
        };
        let base = if base.is_empty() {
            "http://calendar.internal/".into()
        } else {
            base.to_owned()
        };
        let today = day_of(place).unwrap_or(clock_us / DAY_US);
        let opened_on_a_day = !place.is_empty() && day_of(place).is_some();
        let app = Self {
            base,
            // Opening on a particular day means looking at that day.
            view: if opened_on_a_day {
                View::Day
            } else {
                View::Month
            },
            cursor_day: today,
            events: vec![],
            selected: None,
            draft: None,
            status: Status::Loading,
        };
        let effects = app.fetch(window);
        (app, effects)
    }
    pub fn kind(&self) -> &'static str {
        Self::KIND
    }
    pub fn title(&self, _theme: DesktopTheme) -> String {
        "Calendar".into()
    }
    pub fn document(&self) -> String {
        String::new()
    }
    pub fn caption(&self) -> String {
        let date = self.cursor_date();
        format!("{} {}", date.month_name(), date.year)
    }
    pub fn modified(&self) -> bool {
        self.draft.is_some()
    }
    /// Facts the Ubuntu frame needs: the view, whose tab its header bar shows selected,
    /// and the day on screen, against which it greys Today.
    pub fn chrome(&self) -> Vec<(String, String)> {
        vec![
            (
                "view".into(),
                match self.view {
                    View::Month => "month",
                    View::Day => "day",
                }
                .into(),
            ),
            ("cursor".into(), self.cursor_day.to_string()),
        ]
    }
    fn url(&self, suffix: &str) -> String {
        format!("{}{suffix}", self.base.trim_end_matches('/'))
    }
    /// Ask for the whole month around the cursor, so paging a month is one request.
    fn fetch(&self, window: u64) -> Vec<AppEffect> {
        let first = self.cursor_day.saturating_sub(45);
        vec![AppEffect::Http {
            window,
            tag: "events".into(),
            method: "GET".into(),
            url: self.url(&format!(
                "/api/events?start={}&end={}",
                first * DAY_US,
                (first + 120) * DAY_US
            )),
            body: String::new(),
        }]
    }
    pub fn cursor_date(&self) -> CalendarDate {
        CalendarDate::from_clock(self.cursor_day * DAY_US)
    }
    /// Events that overlap `day`, earliest first.
    pub fn events_on(&self, day: u64) -> Vec<&Event> {
        let (start, end) = (day * DAY_US, (day + 1) * DAY_US);
        let mut found: Vec<_> = self
            .events
            .iter()
            .filter(|e| e.start < end && e.end > start)
            .collect();
        found.sort_by_key(|e| (e.start, e.id.clone()));
        found
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
            "events" => {
                self.events = serde_json::from_str(body).unwrap_or_default();
                self.events
                    .sort_by(|a, b| (a.start, &a.id).cmp(&(b.start, &b.id)));
                Ok(vec![])
            }
            // A mutation refetches, so the screen only ever shows committed state.
            "create" | "update" | "delete" | "rsvp" => {
                self.draft = None;
                self.status = Status::Loading;
                Ok(self.fetch(window))
            }
            other => Err(format!("unexpected calendar reply {other}")),
        }
    }
    pub fn text(&mut self, text: &str) -> Result<(), String> {
        let draft = self.draft.as_mut().ok_or("no event is being edited")?;
        if std::mem::take(&mut draft.selected) {
            draft.title.clear();
        }
        push_bounded(&mut draft.title, text, 160);
        Ok(())
    }
    pub fn key(&mut self, window: u64, key: &str, clock_us: u64) -> Result<Vec<AppEffect>, String> {
        // GNOME Calendar's event title is a GTK entry: Ctrl+A selects its text. The
        // calendar has no Ctrl+S; its event is created with Enter or the Save button.
        match super::chord(key).as_str() {
            "Ctrl+a" | "Meta+a" => {
                let draft = self.draft.as_mut().ok_or("no event is being edited")?;
                draft.selected = !draft.title.is_empty();
                Ok(vec![])
            }
            "Backspace" => {
                let draft = self.draft.as_mut().ok_or("no event is being edited")?;
                if std::mem::take(&mut draft.selected) {
                    draft.title.clear();
                } else {
                    draft.title.pop();
                }
                Ok(vec![])
            }
            "Enter" if self.draft.is_some() => self.click(window, "cal:save", clock_us),
            "Escape" => {
                self.draft = None;
                Ok(vec![])
            }
            other => Err(format!("unsupported calendar key {other}")),
        }
    }
    pub fn click(
        &mut self,
        window: u64,
        target: &str,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String> {
        let command = target
            .strip_prefix("cal:")
            .ok_or("interaction does not belong to the calendar")?;
        match command {
            "prev" | "next" => {
                let date = self.cursor_date();
                let step = if self.view == View::Day {
                    1
                } else if command == "prev" {
                    date.day.max(1)
                } else {
                    date.days_in_month - date.day + 1
                };
                self.cursor_day = if command == "prev" {
                    self.cursor_day.saturating_sub(step)
                } else {
                    self.cursor_day + step
                };
                self.status = Status::Loading;
                Ok(self.fetch(window))
            }
            "today" => {
                self.cursor_day = clock_us / DAY_US;
                self.status = Status::Loading;
                Ok(self.fetch(window))
            }
            "month" | "day" => {
                self.view = if command == "month" {
                    View::Month
                } else {
                    View::Day
                };
                Ok(vec![])
            }
            "reload" => {
                self.status = Status::Loading;
                Ok(self.fetch(window))
            }
            "new" => {
                self.draft = Some(Draft {
                    title: String::new(),
                    day: self.cursor_day,
                    hour: 9,
                    length_hours: 1,
                    editing: None,
                    selected: false,
                });
                Ok(vec![])
            }
            "cancel" => {
                self.draft = None;
                Ok(vec![])
            }
            // The title entry: a click puts the caret in it, where typing already goes.
            "title" => {
                let draft = self.draft.as_mut().ok_or("no event is being edited")?;
                draft.selected = false;
                Ok(vec![])
            }
            "earlier" | "later" => {
                let draft = self.draft.as_mut().ok_or("no event is being edited")?;
                draft.hour = if command == "earlier" {
                    draft.hour.saturating_sub(1)
                } else {
                    (draft.hour + 1).min(23)
                };
                Ok(vec![])
            }
            "shorter" | "longer" => {
                let draft = self.draft.as_mut().ok_or("no event is being edited")?;
                draft.length_hours = if command == "shorter" {
                    draft.length_hours.saturating_sub(1).max(1)
                } else {
                    (draft.length_hours + 1).min(12)
                };
                Ok(vec![])
            }
            "save" => {
                let draft = self.draft.clone().ok_or("no event is being edited")?;
                if draft.title.trim().is_empty() {
                    return Err("an event needs a title".into());
                }
                let start = draft.day * DAY_US + draft.hour * HOUR_US;
                let end = start + draft.length_hours.max(1) * HOUR_US;
                let body = serde_json::json!({
                    "title": draft.title,
                    "start": start,
                    "end": end,
                })
                .to_string();
                self.status = Status::Loading;
                Ok(vec![AppEffect::Http {
                    window,
                    tag: if draft.editing.is_some() {
                        "update".into()
                    } else {
                        "create".into()
                    },
                    method: "POST".into(),
                    url: match &draft.editing {
                        Some(id) => self.url(&format!("/api/events/{id}")),
                        None => self.url("/api/events"),
                    },
                    body,
                }])
            }
            "delete" => {
                let id = self.selected.clone().ok_or("no event is selected")?;
                self.selected = None;
                self.status = Status::Loading;
                Ok(vec![AppEffect::Http {
                    window,
                    tag: "delete".into(),
                    method: "DELETE".into(),
                    url: self.url(&format!("/api/events/{id}")),
                    body: String::new(),
                }])
            }
            rest => {
                if let Some(day) = rest.strip_prefix("day:") {
                    self.cursor_day = day.parse().map_err(|_| "invalid day")?;
                    self.view = View::Day;
                    return Ok(vec![]);
                }
                if let Some(id) = rest.strip_prefix("event:") {
                    let event = self
                        .events
                        .iter()
                        .find(|e| e.id == id)
                        .ok_or("event not found")?;
                    self.selected = Some(event.id.clone());
                    self.cursor_day = event.start / DAY_US;
                    return Ok(vec![]);
                }
                if let Some(id) = rest.strip_prefix("edit:") {
                    let event = self
                        .events
                        .iter()
                        .find(|e| e.id == id)
                        .ok_or("event not found")?;
                    self.draft = Some(Draft {
                        title: event.title.clone(),
                        day: event.start / DAY_US,
                        hour: (event.start % DAY_US) / HOUR_US,
                        length_hours: ((event.end.saturating_sub(event.start)) / HOUR_US).max(1),
                        editing: Some(event.id.clone()),
                        selected: false,
                    });
                    return Ok(vec![]);
                }
                if let Some(rest) = rest.strip_prefix("rsvp:") {
                    let (id, response) = rest.split_once(':').ok_or("invalid RSVP")?;
                    self.status = Status::Loading;
                    return Ok(vec![AppEffect::Http {
                        window,
                        tag: "rsvp".into(),
                        method: "POST".into(),
                        url: self.url(&format!("/api/events/{id}/rsvp")),
                        body: serde_json::json!({ "response": response }).to_string(),
                    }]);
                }
                Err(format!("unknown calendar command {command}"))
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
        let date = self.cursor_date();
        page.elements.push(E::Heading {
            id: "cal-title".into(),
            text: format!("{} {}", date.month_name(), date.year),
            level: 2,
        });
        if let Some(text) = self.status.notice() {
            page.elements.push(E::Text {
                id: "cal-status".into(),
                text: text.into(),
            });
        }
        for (id, label) in [
            ("cal:prev", "Previous"),
            ("cal:next", "Next"),
            ("cal:today", "Today"),
            ("cal:month", "Month"),
            ("cal:day", "Day"),
            ("cal:new", "New event"),
            ("cal:reload", "Reload"),
        ] {
            page.elements.push(E::Button {
                id: id.into(),
                text: label.into(),
                action: act(id),
                style: None,
            });
        }
        for event in self.events_on(self.cursor_day) {
            page.elements.push(E::Button {
                id: format!("cal:event:{}", event.id),
                text: format!("{} — {}", clock_label(event.start), event.title),
                action: act(&format!("cal:event:{}", event.id)),
                style: None,
            });
            page.elements.push(E::Button {
                id: format!("cal:edit:{}", event.id),
                text: format!("Edit {}", event.title),
                action: act(&format!("cal:edit:{}", event.id)),
                style: None,
            });
        }
        if let Some(draft) = &self.draft {
            page.elements.push(E::Input {
                id: "cal-draft-title".into(),
                label: "Title".into(),
                value: draft.title.clone(),
                placeholder: "New event".into(),
            });
            for (id, label) in [
                ("cal:earlier", "Earlier"),
                ("cal:later", "Later"),
                ("cal:shorter", "Shorter"),
                ("cal:longer", "Longer"),
                ("cal:save", "Save"),
                ("cal:cancel", "Cancel"),
            ] {
                page.elements.push(E::Button {
                    id: id.into(),
                    text: label.into(),
                    action: act(id),
                    style: None,
                });
            }
        }
        if self.selected.is_some() {
            page.elements.push(E::Button {
                id: "cal:delete".into(),
                text: "Delete".into(),
                action: act("cal:delete"),
                style: None,
            });
        }
    }
    pub fn render(&self, p: &mut Painter, env: &crate::AppEnv<'_>) {
        let (theme, width, height) = (env.theme, env.width, env.height);
        if theme == DesktopTheme::Ubuntu {
            return self.render_gnome(p, width, height, env.clock_us / DAY_US);
        }
        let l = look(theme);
        p.scene.background = l.surface;
        let date = self.cursor_date();
        let title = format!("{} {}", date.month_name(), date.year);
        let screen = screen(p, theme, &l, width, height as i32, &title);
        let mut top = screen.top;
        // Toolbar: every control here moves real state.
        let bar = 38;
        p.box_(Rect::new(0, top, width, bar), l.chrome, 0);
        p.hline(0, top + bar as i32, width, LINE);
        // The New event button owns the right end; chips stop before it rather than
        // overlapping it on a narrow screen.
        // A phone has no room for the word "Reload": it wears the refresh symbol at the
        // right of the toolbar instead, as the phone calendars do, and the chips that
        // flow from the left stop before it.
        // A phone adds an event with a "+", as its calendar does; a desktop names it.
        let mut limit = width as i32 - if theme.mobile() { 50 } else { 116 };
        if theme.mobile() {
            let r = Rect::new(limit - 34, top + 6, 30, 26);
            p.button(r, Color::TRANSPARENT, l.radius, "cal:reload", "Reload");
            self.busy(p);
            p.symbol("reload", r.x + 7, r.y + 5, 16, INK);
            limit -= 38;
        }
        let mut x = 8;
        for (target, label) in [
            ("cal:prev", "‹"),
            ("cal:today", "Today"),
            ("cal:next", "›"),
            ("cal:month", "Month"),
            ("cal:day", "Day"),
            ("cal:reload", "Reload"),
        ] {
            if theme.mobile() && target == "cal:reload" {
                continue;
            }
            let w = p.measure(label, 12, false) + 22;
            if x + w as i32 > limit {
                break;
            }
            let r = Rect::new(x, top + 6, w, 26);
            let on = (target == "cal:month" && self.view == View::Month)
                || (target == "cal:day" && self.view == View::Day);
            p.button(
                r,
                if on { l.selection } else { Color::TRANSPARENT },
                l.radius,
                target,
                label,
            );
            if target == "cal:reload" {
                self.busy(p);
            }
            p.label(
                r.x,
                r.y + 4,
                r.width,
                label,
                12,
                if on { l.accent } else { INK },
                on,
                Align::Center,
            );
            x += w as i32 + 4;
        }
        if theme.mobile() {
            let new = Rect::new(width as i32 - 42, top + 4, 32, 30);
            p.button(new, Color::TRANSPARENT, l.radius, "cal:new", "New event");
            p.symbol("plus", new.x + 7, new.y + 7, 18, l.accent);
        } else {
            let new = Rect::new(width as i32 - 106, top + 6, 96, 26);
            action(p, &l, new, "New event", "cal:new", true);
        }
        top += bar as i32 + 1;
        if let Some(text) = self.status.notice() {
            notice(p, width, top + 10, text);
            top += 30;
        }
        match self.view {
            View::Month => {
                // Sized to the window as it would be unscrolled, so the grid keeps its
                // size while a phone scrolls it.
                let rest = height.saturating_sub((top + screen.offset()).max(0) as u32);
                self.month(p, theme, &l, width, rest, top, date)
            }
            View::Day => {
                let list = screen.column(
                    p,
                    "day",
                    Rect::new(0, top, width, (height as i32 - top).max(1) as u32),
                );
                self.day(p, &l, width, list.top);
                list.end(p);
            }
        }
        screen.end(p);
        if let Some(draft) = &self.draft {
            self.composer(p, &l, width, height, draft);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn month(
        &self,
        p: &mut Painter,
        theme: DesktopTheme,
        l: &super::look::Look,
        width: u32,
        room: u32,
        top: i32,
        date: CalendarDate,
    ) {
        let columns = 7;
        let cell_w = width / columns;
        let head = 22;
        for (index, name) in WEEKDAYS.iter().enumerate() {
            let short = if theme.mobile() {
                &name[..1]
            } else {
                &name[..3]
            };
            p.label(
                index as i32 * cell_w as i32,
                top + 4,
                cell_w,
                short,
                11,
                MUTED,
                false,
                Align::Center,
            );
        }
        p.hline(0, top + head, width, LINE);
        // The grid always starts on the Sunday of the first week of this month. Day
        // indices count from the world's first day, so a cell earlier than that has no
        // index at all and is drawn inert rather than mislabelled.
        let first_day = self.cursor_day as i64 - (date.day as i64 - 1);
        let lead = date.first_weekday;
        let rows = (lead + date.days_in_month).div_ceil(7).max(1);
        let body = room.saturating_sub(head as u32 + 2);
        let cell_h = (body / rows as u32).max(28);
        let today = self.cursor_day;
        for slot in 0..(rows * 7) {
            let column = slot % 7;
            let row = slot / 7;
            let r = Rect::new(
                column as i32 * cell_w as i32,
                top + head + 1 + row as i32 * cell_h as i32,
                cell_w,
                cell_h,
            );
            if slot < lead || slot >= lead + date.days_in_month {
                p.box_(r, Color(0, 0, 0, 8), 0);
                continue;
            }
            let day_number = slot - lead + 1;
            let index = first_day + (day_number as i64 - 1);
            let Ok(day) = u64::try_from(index) else {
                // Before the world began: label the date, offer nothing to click.
                p.vline(r.x, r.y, cell_h, LINE);
                p.hline(r.x, r.y, cell_w, LINE);
                p.label(
                    r.x + 4,
                    r.y + 3,
                    cell_w.saturating_sub(8),
                    &day_number.to_string(),
                    11,
                    FAINT,
                    false,
                    Align::Left,
                );
                continue;
            };
            p.button(
                r,
                if day == today {
                    l.selection
                } else {
                    Color::TRANSPARENT
                },
                0,
                &format!("cal:day:{day}"),
                &format!("{} {day_number}", date.month_name()),
            );
            p.announce("listitem", None);
            p.vline(r.x, r.y, cell_h, LINE);
            p.hline(r.x, r.y, cell_w, LINE);
            p.label(
                r.x + 4,
                r.y + 3,
                cell_w.saturating_sub(8),
                &day_number.to_string(),
                11,
                if day == today { l.accent } else { INK },
                day == today,
                Align::Left,
            );
            for (index, event) in self.events_on(day).iter().take(3).enumerate() {
                let chip = Rect::new(
                    r.x + 3,
                    r.y + 18 + index as i32 * 15,
                    cell_w.saturating_sub(6),
                    13,
                );
                if chip.y + 13 > r.y + cell_h as i32 {
                    break;
                }
                p.button(
                    chip,
                    l.selection,
                    3,
                    &format!("cal:event:{}", event.id),
                    &event.title,
                );
                p.select_last(self.selected.as_deref() == Some(event.id.as_str()));
                p.left(
                    chip.x + 4,
                    chip.y,
                    chip.width.saturating_sub(8),
                    &event.title,
                    10,
                    INK,
                );
            }
        }
    }
    fn day(&self, p: &mut Painter, l: &super::look::Look, width: u32, top: i32) {
        let date = self.cursor_date();
        p.strong(
            14,
            top + 6,
            width.saturating_sub(28),
            &format!("{} {} {}", date.weekday_name(), date.day, date.month_name()),
            14,
            INK,
        );
        let mut y = top + 32;
        let events = self.events_on(self.cursor_day);
        if events.is_empty() {
            notice(p, width, y + 20, "No events");
            return;
        }
        for event in events {
            let r = Rect::new(8, y, width.saturating_sub(16), l.row);
            let selected = self.selected.as_deref() == Some(event.id.as_str());
            p.button(
                r,
                if selected {
                    l.selection
                } else {
                    Color(0, 0, 0, 8)
                },
                l.radius,
                &format!("cal:event:{}", event.id),
                &event.title,
            );
            p.select_last(self.selected.as_deref() == Some(event.id.as_str()));
            p.box_(Rect::new(r.x, r.y, 3, r.height), l.accent, 0);
            p.left(r.x + 12, r.y + 5, 110, &clock_label(event.start), 12, MUTED);
            p.left(
                r.x + 120,
                r.y + 5,
                r.width.saturating_sub(220),
                &event.title,
                13,
                INK,
            );
            let edit = Rect::new(r.x + r.width as i32 - 96, r.y + 4, 44, r.height - 8);
            action(p, l, edit, "Edit", &format!("cal:edit:{}", event.id), false);
            if selected {
                let del = Rect::new(r.x + r.width as i32 - 48, r.y + 4, 44, r.height - 8);
                action(p, l, del, "Delete", "cal:delete", false);
            }
            y += l.row as i32 + 6;
        }
    }
    /// While a request is in flight, Reload is greyed, as a calendar's is while it syncs: the screen shows
    /// a load in flight as well as the words "Loading…" do.
    fn busy(&self, p: &mut Painter) {
        if self.status == Status::Loading {
            if let Some(s) = p.scene.nodes.last_mut().and_then(|n| n.semantic.as_mut()) {
                s.disabled = true;
            }
        }
    }
    fn composer(
        &self,
        p: &mut Painter,
        l: &super::look::Look,
        width: u32,
        height: u32,
        draft: &Draft,
    ) {
        let h = 176.min(height.saturating_sub(20));
        let r = Rect::new(
            12,
            height as i32 - h as i32 - 10,
            width.saturating_sub(24),
            h,
        );
        p.drop_shadow(r, l.radius, 18, 60, 6);
        p.border(r, l.surface, l.radius, LINE);
        p.strong(
            r.x + 14,
            r.y + 12,
            r.width.saturating_sub(28),
            if draft.editing.is_some() {
                "Edit event"
            } else {
                "New event"
            },
            14,
            INK,
        );
        let field = Rect::new(r.x + 14, r.y + 38, r.width.saturating_sub(28), 30);
        p.border(field, Color::WHITE, 5, LINE);
        p.region(field, "cal:title", "Event title");
        // A GTK entry: announced as a text box whose value is what it holds, so what was
        // typed can be read back; the painted text is the value, not a second label.
        p.announce("textbox", Some(&draft.title));
        if draft.selected {
            let chars = draft.title.chars().count() as u32;
            p.box_(
                Rect::new(
                    field.x + 7,
                    field.y + 5,
                    (chars * 8).min(field.width.saturating_sub(16)),
                    20,
                ),
                Color(53, 132, 228, 77),
                0,
            );
        }
        p.left(
            field.x + 8,
            field.y + 7,
            field.width.saturating_sub(16),
            if draft.title.is_empty() {
                "Title"
            } else {
                &draft.title
            },
            13,
            if draft.title.is_empty() { FAINT } else { INK },
        );
        p.presentation();
        let date = CalendarDate::from_clock(draft.day * DAY_US);
        p.left(
            r.x + 14,
            r.y + 78,
            r.width.saturating_sub(28),
            &format!(
                "{} {} {} · {} for {} hour{}",
                date.weekday_name(),
                date.day,
                MONTHS[(date.month - 1) as usize],
                clock_label(draft.hour * HOUR_US),
                draft.length_hours,
                if draft.length_hours == 1 { "" } else { "s" }
            ),
            12,
            MUTED,
        );
        let mut x = r.x + 14;
        for (target, label) in [
            ("cal:earlier", "Earlier"),
            ("cal:later", "Later"),
            ("cal:shorter", "Shorter"),
            ("cal:longer", "Longer"),
        ] {
            let w = p.measure(label, 12, false) + 20;
            action(p, l, Rect::new(x, r.y + 102, w, 28), label, target, false);
            x += w as i32 + 6;
        }
        action(
            p,
            l,
            Rect::new(r.x + 14, r.y + 138, 92, 28),
            "Save",
            "cal:save",
            true,
        );
        // Saving an edit writes the event it edits, and a new draft writes a new one: the screen says which,
        // as the form's heading does in words.
        if let Some(id) = &draft.editing {
            p.tag(
                "cal:save",
                None,
                Some(cw_scene::ObjRef::new("event", id.as_str())),
            );
        }
        action(
            p,
            l,
            Rect::new(r.x + 114, r.y + 138, 92, 28),
            "Cancel",
            "cal:cancel",
            false,
        );
    }
}
/// GNOME Calendar's colours on Ubuntu 24.04: sidebar, cells, lines, the accent and the
/// greyed days of another month.
const G_SIDEBAR: Color = Color::rgb(235, 235, 235);
const G_CELL: Color = Color::rgb(253, 253, 253);
const G_LINE: Color = Color::rgb(238, 238, 238);
const G_BLUE: Color = Color::rgb(53, 132, 228);
const G_TITLE: Color = Color::rgb(28, 113, 216);
const G_INK: Color = Color::rgb(46, 46, 46);
const G_DIM: Color = Color::rgb(190, 190, 190);
/// The month view's columns (window-relative left edges, less the content's one-pixel
/// inset) and the tops of its seven week rows, the first behind the weekday header.
const G_COLUMNS: [(i32, u32); 7] = [
    (303, 129),
    (433, 128),
    (562, 128),
    (691, 127),
    (819, 128),
    (948, 128),
    (1077, 128),
];
const G_ROWS: [i32; 7] = [-47, 81, 209, 336, 464, 591, 719];
/// The weekday header's labels, as GTK allocates them.
const G_WEEKDAYS: [(&str, i32, u32); 7] = [
    ("SUN", 305, 124),
    ("MON", 435, 129),
    ("TUE", 570, 121),
    ("WED", 697, 127),
    ("THU", 830, 123),
    ("FRI", 959, 116),
    ("SAT", 1081, 120),
];
/// Days from 1970-01-01 to the world's first day, Thursday 17 September 2026.
const WORLD_EPOCH: i64 = 20_713;
/// `(year, month, day)` and weekday (0 is Sunday) of a day counted from the world's
/// first day; days before it are negative (Howard Hinnant's civil_from_days).
fn civil(day: i64) -> (i64, u32, u32, u32) {
    let z = day + WORLD_EPOCH + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    let weekday = (day + WORLD_EPOCH + 4).rem_euclid(7) as u32;
    (y, m, d, weekday)
}
/// ISO 8601 week number of the week holding `day`: its Thursday's week of the year.
fn iso_week(day: i64) -> u32 {
    let monday_based = i64::from((civil(day).3 + 6) % 7);
    let thursday = day - monday_based + 3;
    let (_, month, mday, _) = civil(thursday);
    // Day of the year of that Thursday, counted back a month at a time.
    let mut ordinal = i64::from(mday);
    let mut cursor = thursday - i64::from(mday);
    for _ in 1..month {
        let (_, _, days, _) = civil(cursor);
        ordinal += i64::from(days);
        cursor -= i64::from(days);
    }
    ((ordinal - 1) / 7 + 1) as u32
}
impl Calendar {
    /// GNOME Calendar 46's month view as Ubuntu 24.04 shows it at 1212x768, below the
    /// header bar (desktop_scene/ubuntu.rs): a 303 px sidebar with the date chooser
    /// (month, arrows, weekday letters, ISO week numbers, a six-week grid of day buttons)
    /// over the day's agenda, and the month view's title, weekday header and seven week
    /// rows from the week before the one on screen, each cell's day number, its image
    /// and its overflow button (measured on the real application, fidelity/tours). Days
    /// before the world began have no index, so their buttons are announced
    /// insensitive; so are the controls the model has no function for.
    fn render_gnome(&self, p: &mut Painter, width: u32, height: u32, today: u64) {
        p.scene.background = G_CELL;
        p.box_(Rect::new(0, 0, 303, height), G_SIDEBAR, 0);
        p.vline(303, 0, height, Color::rgb(218, 218, 218));
        let cursor = self.cursor_day as i64;
        let today = today as i64;
        // The date chooser: the cursor's month in six weeks from the Sunday on or before
        // its first day.
        let (_, month, mday, _) = civil(cursor);
        for (x, action, name) in [(11, "cal:prev", ""), (253, "cal:next", "")] {
            let r = Rect::new(x, 6, 34, 34);
            p.region(r, action, name);
            p.symbol(
                if action == "cal:prev" {
                    "chevron-left"
                } else {
                    "chevron-right"
                },
                r.x + 9,
                r.y + 9,
                16,
                G_INK,
            );
        }
        let month_name = MONTHS[month as usize - 1];
        p.label(51, 14, 196, month_name, 14, G_INK, true, Align::Center);
        p.presentation();
        p.insensitive(Rect::new(51, 6, 196, 34), "cal:pick-month", month_name);
        for (i, letter) in ["S", "M", "T", "W", "T", "F", "S"].iter().enumerate() {
            let x = 41 + i as i32 * 36;
            p.label(
                x + 1,
                57,
                36,
                letter,
                11,
                Color::rgb(120, 120, 120),
                false,
                Align::Center,
            );
            p.presentation();
            p.container(Rect::new(x, 46, 36, 36), "label", letter);
        }
        let first = cursor - i64::from(mday) + 1;
        let start = first - i64::from(civil(first).3);
        const MINI_X: [i32; 7] = [44, 81, 118, 154, 190, 226, 262];
        const MINI_Y: [i32; 6] = [85, 122, 159, 195, 231, 267];
        for (row, y) in MINI_Y.iter().enumerate() {
            let sunday = start + row as i64 * 7;
            let week = iso_week(sunday + 1).to_string();
            if (sunday..sunday + 7).contains(&cursor) {
                p.box_(Rect::new(7, y - 4, 288, 36), Color::rgb(222, 222, 222), 8);
            }
            // Week numbers run 36 px apart from 88 px down, beside rows that do not.
            let wy = 88 + row as i32 * 36;
            p.box_(Rect::new(11, wy, 24, 24), Color::rgb(217, 217, 217), 4);
            p.label(
                11,
                wy + 5,
                24,
                &week,
                10,
                Color::rgb(120, 120, 120),
                false,
                Align::Center,
            );
            p.presentation();
            p.container(Rect::new(11, wy, 24, 24), "label", &week);
            for (column, x) in MINI_X.iter().enumerate() {
                let day = sunday + column as i64;
                let (_, m, d, _) = civil(day);
                let r = Rect::new(*x, *y, 28, 28);
                let name = d.to_string();
                let ink = if day == today {
                    p.box_(r, G_BLUE, 14);
                    Color::WHITE
                } else if m != month {
                    G_DIM
                } else {
                    G_INK
                };
                // GTK paints the day's number a pixel right of and 5 px above the middle
                // of its button.
                p.label(
                    r.x + 1,
                    r.y + 4,
                    28,
                    &name,
                    13,
                    ink,
                    day == today,
                    Align::Center,
                );
                p.presentation();
                // `cal:day:-3` names no day, so the model refuses it.
                if day >= 0 {
                    p.region(r, &format!("cal:day:{day}"), &name);
                } else {
                    p.insensitive(r, &format!("cal:day:{day}"), &name);
                }
                // GTK 4's date chooser announces each day a button (GNOME Calendar 46, AT-SPI).
                p.announce("button", None);
            }
        }
        // The agenda: the events of the day on screen, or a placeholder row.
        let events = self.events_on(self.cursor_day);
        if events.is_empty() {
            p.label(
                10,
                334,
                280,
                "No events",
                14,
                Color::rgb(120, 120, 120),
                false,
                Align::Left,
            );
            p.presentation();
            p.insensitive(Rect::new(11, 334, 300, 22), "cal:agenda", "No events");
            p.announce("option", None);
        }
        for (i, event) in events.iter().enumerate() {
            let r = Rect::new(11, 334 + i as i32 * 26, 280, 22);
            p.label(
                r.x + 5,
                r.y + 2,
                270,
                &event.title,
                13,
                G_INK,
                false,
                Align::Left,
            );
            p.presentation();
            p.region(r, &format!("cal:event:{}", event.id), &event.title);
            p.announce("option", None);
            p.select_last(self.selected.as_deref() == Some(event.id.as_str()));
        }
        if let Some(text) = self.status.notice() {
            p.label(
                16,
                400,
                280,
                text,
                12,
                Color::rgb(120, 120, 120),
                false,
                Align::Left,
            );
        }
        match self.view {
            View::Month => self.gnome_month(p, width, today),
            View::Day => {
                let l = look(DesktopTheme::Ubuntu);
                p.box_(
                    Rect::new(304, 0, width.saturating_sub(304), height),
                    G_CELL,
                    0,
                );
                let mark = p.scene.nodes.len();
                self.day(p, &l, width.saturating_sub(304), 8);
                for n in &mut p.scene.nodes[mark..] {
                    n.bounds.x += 304;
                    n.transform.tx += 304;
                }
            }
        }
        if let Some(draft) = &self.draft {
            self.composer(p, &look(DesktopTheme::Ubuntu), width, height, draft);
        }
    }
    fn gnome_month(&self, p: &mut Painter, width: u32, today: i64) {
        let cursor = self.cursor_day as i64;
        let (_, month, _, weekday) = civil(cursor);
        // The view opens on the cursor's week, with the week before it under the header.
        let top_sunday = cursor - i64::from(weekday) - 7;
        let shown = (top_sunday + 7, top_sunday + 7 * 6 - 1);
        let (y0, m0, _, _) = civil(shown.0);
        let (y1, m1, _, _) = civil(shown.1);
        let title = if m0 == m1 {
            MONTHS[m0 as usize - 1].to_owned()
        } else {
            format!("{}–{}", MONTHS[m0 as usize - 1], MONTHS[m1 as usize - 1])
        };
        let year = if y0 == y1 {
            y0.to_string()
        } else {
            format!("{y0}–{y1}")
        };
        for (r, y) in G_ROWS.iter().copied().enumerate() {
            for (column, (x, w)) in G_COLUMNS.iter().enumerate() {
                let day = top_sunday + r as i64 * 7 + column as i64;
                let cell = Rect::new(*x, y - 2, *w, 128);
                if day == today {
                    p.box_(cell, Color::rgb(225, 237, 251), 0);
                }
                p.vline(*x, y - 2, 128, G_LINE);
                p.hline(*x, y - 2, *w, G_LINE);
            }
        }
        // The title band and weekday header sit over the first row.
        p.box_(
            Rect::new(304, 0, width.saturating_sub(304), 79),
            Color::WHITE,
            0,
        );
        p.hline(304, 78, width.saturating_sub(304), G_LINE);
        // The title and year are painted 4 px left of and 5 px above their labels' bounds.
        p.label(307, 11, 529, &title, 22, G_TITLE, true, Align::Left);
        p.presentation();
        p.container(Rect::new(311, 12, 529, 51), "label", &title);
        p.label(
            836,
            11,
            365,
            &year,
            22,
            Color::rgb(143, 143, 143),
            true,
            Align::Right,
        );
        p.presentation();
        p.container(Rect::new(840, 12, 373, 51), "label", &year);
        for (name, x, w) in G_WEEKDAYS {
            p.label(
                x + 1,
                52,
                w,
                name,
                12,
                Color::rgb(120, 120, 120),
                true,
                Align::Left,
            );
            p.presentation();
            p.container(Rect::new(x, 51, w, 17), "label", name);
        }
        for (r, y) in G_ROWS.iter().enumerate() {
            for (column, (x, w)) in G_COLUMNS.iter().enumerate() {
                let day = top_sunday + r as i64 * 7 + column as i64;
                // Each cell's overflow button, insensitive while its events fit, lies
                // along the bottom of the cell above.
                p.insensitive(
                    Rect::new(*x, y - 31, *w, 24),
                    &format!("cal:more:{day}"),
                    "",
                );
                let (_, m, d, _) = civil(day);
                let other = m != month;
                let number = d.to_string();
                let digits = number.len() as i32;
                let mut img = x + 20 + 8 * digits;
                if day == today {
                    p.box_(Rect::new(x + 1, *y, 28, 17), G_BLUE, 8);
                    p.label(
                        x + 1,
                        *y,
                        28,
                        &number,
                        12,
                        Color::WHITE,
                        true,
                        Align::Center,
                    );
                    p.presentation();
                    p.container(Rect::new(x + 7, *y, 28, 17), "label", &number);
                    img = x + 47;
                } else {
                    let ink = if other { G_DIM } else { G_INK };
                    // GTK paints the number a pixel right of and below its label's bounds.
                    p.label(
                        x + 3,
                        y + 1,
                        8 * digits as u32 + 4,
                        &number,
                        12,
                        ink,
                        false,
                        Align::Left,
                    );
                    p.presentation();
                    p.container(
                        Rect::new(x + 2, *y, 8 * digits as u32, 17),
                        "label",
                        &number,
                    );
                }
                if d == 1 {
                    let short = &MONTHS[m as usize - 1][..3];
                    p.label(
                        x + 17,
                        y + 1,
                        30,
                        short,
                        12,
                        if other { G_DIM } else { G_INK },
                        false,
                        Align::Left,
                    );
                    p.presentation();
                    p.container(Rect::new(x + 16, *y, 21, 17), "label", short);
                    img = x + 55;
                }
                p.container(Rect::new(img, *y, 16, 17), "img", "");
                let events = if day >= 0 {
                    self.events_on(day as u64)
                } else {
                    vec![]
                };
                for (index, event) in events.iter().take(3).enumerate() {
                    let chip = Rect::new(x + 3, y + 22 + index as i32 * 22, w - 6, 20);
                    p.button(
                        chip,
                        Color::rgb(225, 237, 251),
                        4,
                        &format!("cal:event:{}", event.id),
                        &event.title,
                    );
                    p.select_last(self.selected.as_deref() == Some(event.id.as_str()));
                    p.label(
                        chip.x + 6,
                        chip.y + 2,
                        chip.width.saturating_sub(12),
                        &event.title,
                        12,
                        G_INK,
                        false,
                        Align::Left,
                    );
                    p.presentation();
                }
            }
        }
    }
}
/// Day index, counted from the world's first day, of a `YYYY-MM-DD` date or a bare index.
/// `None` for anything else, and for a date before the world began, which has no index.
pub fn day_of(text: &str) -> Option<u64> {
    if let Ok(index) = text.parse::<u64>() {
        return Some(index);
    }
    let mut parts = text.split('-');
    let (year, month, day): (u64, u64, u64) = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    if parts.next().is_some() || !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    // Walk month by month from the world's start; bounded, so a far date is refused.
    let mut index = 0u64;
    for step in 0..1200u64 {
        let here = CalendarDate::from_clock(index * DAY_US);
        if (here.year, here.month) == (year, month) {
            if day > here.days_in_month {
                return None;
            }
            // `here` is the first representable day of this month.
            let offset = day.checked_sub(here.day)?;
            return Some(index + offset);
        }
        if (here.year, here.month) > (year, month) {
            return None;
        }
        index += here.days_in_month - here.day + 1;
        let _ = step;
    }
    None
}
/// 24-hour clock label for a world timestamp.
fn clock_label(us: u64) -> String {
    let minutes = (us % DAY_US) / 60_000_000;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// What the calendar declares for the rules (`super::declare`): its commands' prefix, what each control
/// stands for and where it is drawn, and which parts of its state its commands branch on. The events,
/// the day on screen and the draft's words are data.
pub const DECLARATION: super::declare::Declaration = {
    use super::declare::{command, field, As};
    const TOOLBAR: &[(&str, &str)] = &[("toolbar", "calendar")];
    const GRID: &[(&str, &str)] = &[("list", "days")];
    const FORM: &[(&str, &str)] = &[("form", "event")];
    const DETAIL: &[(&str, &str)] = &[("toolbar", "event")];
    super::declare::Declaration {
        prefix: "cal",
        structure: &[
            field("view", "/view", As::Value),
            field("rows", "/events", As::Filled),
            field("open", "/selected", As::Present),
            field("draft", "/draft", As::Present),
            field("titled", "/draft/title", As::Filled),
            field("editing", "/draft/editing", As::Present),
            field("selected", "/draft/selected", As::Filled),
            field("status", "/status", As::Variant),
        ],
        commands: &[
            command("cal:prev").region(TOOLBAR),
            command("cal:next").region(TOOLBAR),
            command("cal:today").region(TOOLBAR),
            command("cal:month").role("tab").region(TOOLBAR),
            command("cal:day").role("tab").region(TOOLBAR),
            command("cal:new").region(TOOLBAR),
            command("cal:reload").region(TOOLBAR),
            command("cal:day:*")
                .role("listitem")
                .as_painted()
                .object("day")
                .region(GRID),
            command("cal:event:*")
                .role("listitem")
                .object("event")
                .region(GRID),
            command("cal:edit:*").object("event").region(DETAIL),
            command("cal:delete").region(DETAIL),
            command("cal:title")
                .role("textbox")
                .object("draft.title")
                .region(FORM),
            command("cal:earlier").region(FORM),
            command("cal:later").region(FORM),
            command("cal:shorter").region(FORM),
            command("cal:longer").region(FORM),
            command("cal:save").object("draft").region(FORM),
            // Cancel keeps nothing of the draft, so it stands for none of it: Save is the draft's.
            command("cal:cancel").region(FORM),
        ],
        keys: &[],
    }
};

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> Calendar {
        let (mut app, effects) = Calendar::launch("http://calendar.internal/", 1, 0);
        assert!(matches!(effects.as_slice(), [AppEffect::Http { tag, .. }] if tag == "events"));
        app.http(
            1,
            "events",
            200,
            r#"[{"id":"event-1","owner":"alice","title":"Standup","start":32400000000,"end":34200000000}]"#,
        )
        .unwrap();
        app
    }
    #[test]
    fn the_event_title_is_a_text_box_that_reads_back_what_was_typed_and_takes_select_all() {
        // the engine's design notes 42: a click in the title answered invalid, Ctrl+A was refused, and
        // the field announced no value, so a harness could not read what it had typed.
        let mut app = app();
        app.click(1, "cal:new", 0).unwrap();
        app.click(1, "cal:title", 0).unwrap();
        app.text("Dentst").unwrap();
        app.key(1, "Ctrl+A", 0).unwrap();
        app.text("Dentist").unwrap();
        assert_eq!(app.draft.as_ref().unwrap().title, "Dentist");
        app.key(1, "Control+a", 0).unwrap();
        app.key(1, "Backspace", 0).unwrap();
        app.text("Dentist").unwrap();
        assert_eq!(app.draft.as_ref().unwrap().title, "Dentist");
        // GNOME Calendar has no Ctrl+S: Enter or Save creates the event.
        assert!(app.key(1, "Ctrl+s", 0).is_err());
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
        let field = scene
            .scene
            .nodes
            .iter()
            .find(|n| n.interaction.as_deref() == Some("cal:title"))
            .and_then(|n| n.semantic.clone())
            .expect("the title field");
        assert_eq!(
            (field.role.as_str(), field.value.as_deref()),
            ("textbox", Some("Dentist"))
        );
    }
    #[test]
    fn a_day_is_a_button_where_gtk_announces_one_and_a_list_item_elsewhere() {
        let app = app();
        let role_of = |theme: DesktopTheme| {
            let mut scene = Painter::themed(theme, 1210, 721, 0);
            app.render(
                &mut scene,
                &crate::AppEnv {
                    theme,
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
            let d = crate::apps::declared("calendar").expect("Calendar declares its commands");
            crate::apps::declare::annotate(d, &mut scene.scene.nodes);
            scene
                .scene
                .nodes
                .iter()
                .find(|n| {
                    n.interaction
                        .as_deref()
                        .is_some_and(|i| i.starts_with("cal:day:"))
                })
                .and_then(|n| n.semantic.as_ref())
                .map(|s| s.role.clone())
                .expect("a day")
        };
        // GNOME Calendar 46's date chooser: AT-SPI says button.
        assert_eq!(role_of(DesktopTheme::Ubuntu), "button");
        // Everywhere else the declaration's list item stands, as before.
        assert_eq!(role_of(DesktopTheme::Macos), "listitem");
    }
    #[test]
    fn events_come_from_the_service_and_land_on_the_right_day() {
        let app = app();
        assert_eq!(app.status, Status::Idle);
        assert_eq!(app.events_on(0).len(), 1);
        assert_eq!(app.events_on(1).len(), 0);
        assert_eq!(clock_label(32_400_000_000), "09:00");
    }
    #[test]
    fn creating_an_event_posts_real_fields_and_refetches() {
        let mut app = app();
        app.click(1, "cal:new", 0).unwrap();
        app.text("Design review").unwrap();
        app.click(1, "cal:later", 0).unwrap();
        app.click(1, "cal:longer", 0).unwrap();
        let effects = app.click(1, "cal:save", 0).unwrap();
        let AppEffect::Http {
            method,
            url,
            body,
            tag,
            ..
        } = &effects[0]
        else {
            panic!("expected a request");
        };
        assert_eq!((method.as_str(), tag.as_str()), ("POST", "create"));
        assert_eq!(url, "http://calendar.internal/api/events");
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["title"], "Design review");
        assert_eq!(sent["start"], 10 * HOUR_US);
        assert_eq!(sent["end"], 12 * HOUR_US);
        // The reply clears the draft and refetches rather than guessing the new state.
        let more = app.http(1, "create", 200, "{}").unwrap();
        assert!(app.draft.is_none());
        assert!(matches!(more.as_slice(), [AppEffect::Http { tag, .. }] if tag == "events"));
    }
    #[test]
    fn an_empty_title_is_refused_and_transport_failure_is_state() {
        let mut app = app();
        app.click(1, "cal:new", 0).unwrap();
        assert!(app.click(1, "cal:save", 0).is_err());
        app.offline("events", "network unreachable");
        assert_eq!(app.status, Status::Offline("network unreachable".into()));
        app.http(1, "events", 403, r#"{"error":"calendar unavailable"}"#)
            .unwrap();
        assert_eq!(app.status, Status::Denied("calendar unavailable".into()));
    }
    #[test]
    fn every_painted_control_is_one_the_model_accepts() {
        let app = app();
        let mut scene = Painter::themed(DesktopTheme::Macos, 900, 640, 0);
        app.render(
            &mut scene,
            &crate::AppEnv {
                theme: DesktopTheme::Macos,
                width: 900,
                height: 640,
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
        assert!(!targets.is_empty());
        for target in targets {
            let mut app = app.clone();
            if target == "cal:title" {
                continue; // The composer's field is only live while a draft exists.
            }
            assert!(
                app.click(1, &target, 0).is_ok(),
                "unhandled control {target}"
            );
        }
    }
    #[test]
    fn ubuntu_paints_gnome_calendars_month_view_and_routes_only_what_the_model_accepts() {
        let app = app();
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
        for n in &scene.scene.nodes {
            let Some(id) = n.interaction.as_deref() else {
                continue;
            };
            let refused = app.clone().click(1, id, 0).is_err();
            let insensitive = n.semantic.as_ref().is_some_and(|s| s.disabled);
            assert_eq!(refused, insensitive, "{id}");
        }
        let label = |text: &str| {
            scene.scene.nodes.iter().any(|n| {
                n.semantic
                    .as_ref()
                    .is_some_and(|s| s.role == "label" && s.label == text)
            })
        };
        // 17 September 2026 is in ISO week 38; the view spans September into October.
        assert!(label("38") && label("36") && label("41"));
        assert!(label("September–October") && label("2026"));
        assert_eq!(civil(0), (2026, 9, 17, 4));
        assert_eq!(civil(-17), (2026, 8, 31, 1));
        assert_eq!(iso_week(-17), 36);
        // Friday 1 January 2027 is in the 53rd week of 2026.
        assert_eq!(iso_week(106), 53);
    }
    #[test]
    fn a_date_names_the_right_day_and_one_before_the_world_began_names_none() {
        // The world starts on Thursday 17 September 2026, which is day 0.
        assert_eq!(day_of("2026-09-17"), Some(0));
        assert_eq!(day_of("2026-09-18"), Some(1));
        assert_eq!(day_of("2026-09-30"), Some(13));
        assert_eq!(day_of("2026-10-01"), Some(14));
        assert_eq!(day_of("2027-01-01"), Some(106));
        // Leap day is real in 2028 and not in 2027.
        assert!(day_of("2028-02-29").is_some());
        assert_eq!(day_of("2027-02-29"), None);
        // Before the world began there is no day to open.
        assert_eq!(day_of("2026-09-16"), None);
        assert_eq!(day_of("2026-09-01"), None);
        for bad in [
            "",
            "2026-13-01",
            "2026-09-00",
            "2026-09-31",
            "yesterday",
            "2026-09",
        ] {
            assert_eq!(day_of(bad), None, "{bad}");
        }
        assert_eq!(day_of("40"), Some(40));
    }
    #[test]
    fn opening_on_a_date_keeps_the_service_and_shows_that_day() {
        let (app, effects) = Calendar::launch("http://calendar.google.com/#2026-10-01", 1, 0);
        assert_eq!(app.base, "http://calendar.google.com/");
        assert_eq!(app.cursor_day, 14);
        assert_eq!(app.view, View::Day);
        let AppEffect::Http { url, .. } = &effects[0] else {
            panic!("expected a fetch");
        };
        assert!(url.starts_with("http://calendar.google.com/"), "{url}");
        // A plain launch still opens on today's month.
        let (plain, _) = Calendar::launch("http://calendar.google.com/", 1, 0);
        assert_eq!((plain.cursor_day, plain.view), (0, View::Month));
    }
    #[test]
    fn month_paging_moves_whole_months_and_today_returns() {
        let mut app = app();
        assert_eq!(app.cursor_date().month, 9);
        app.click(1, "cal:next", 0).unwrap();
        assert_eq!(app.cursor_date().month, 10);
        app.click(1, "cal:prev", 0).unwrap();
        assert_eq!(app.cursor_date().month, 9);
        app.click(1, "cal:day:40", 0).unwrap();
        assert_eq!(app.view, View::Day);
        app.click(1, "cal:today", 0).unwrap();
        assert_eq!(app.cursor_day, 0);
    }
}
