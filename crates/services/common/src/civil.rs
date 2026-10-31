//! Calendar dates on the world clock, by integer arithmetic only.
//!
//! The world clock counts microseconds from 2026-09-17 09:00, the epoch the calendar
//! service and the desktop clock read. A publish time typed as `2026-09-18 14:00` and a
//! payout run on the 21st of a month both go through here, so they agree on every machine.
pub const MINUTE_US: u64 = 60_000_000;
pub const HOUR_US: u64 = 60 * MINUTE_US;
pub const DAY_US: u64 = 24 * HOUR_US;
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// Minutes into the epoch day the clock starts at.
const EPOCH_MINUTE: u64 = 9 * 60;

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
fn epoch_day() -> i64 {
    days_from_civil(2026, 9, 17)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: u64,
    pub minute: u64,
}
impl Civil {
    /// `Sep 18, 2026`.
    pub fn date(&self) -> String {
        format!(
            "{} {}, {}",
            MONTHS[(self.month - 1) as usize],
            self.day,
            self.year
        )
    }
    /// `Sep 18, 2026, 14:00`.
    pub fn stamp(&self) -> String {
        format!("{}, {:02}:{:02}", self.date(), self.hour, self.minute)
    }
    /// `2026-09`: the key earnings are kept under.
    pub fn month_key(&self) -> String {
        format!("{:04}-{:02}", self.year, self.month)
    }
    /// `2026-09-18 14:00`: what a form shows and accepts.
    pub fn input(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute
        )
    }
}

pub fn civil(us: u64) -> Civil {
    let minutes = us / MINUTE_US + EPOCH_MINUTE;
    let (y, m, d) = civil_from_days(epoch_day() + (minutes / 1440) as i64);
    Civil {
        year: y,
        month: m,
        day: d,
        hour: (minutes % 1440) / 60,
        minute: minutes % 60,
    }
}

/// The tick a calendar moment falls on; `None` before the epoch or for a date that is not one.
pub fn tick(year: i64, month: i64, day: i64, hour: u64, minute: u64) -> Option<u64> {
    if !(1..=12).contains(&month) || day < 1 || hour > 23 || minute > 59 {
        return None;
    }
    if civil_from_days(days_from_civil(year, month, day)) != (year, month, day) {
        return None;
    }
    let days = days_from_civil(year, month, day) - epoch_day();
    let minutes = days * 1440 + (hour * 60 + minute) as i64 - EPOCH_MINUTE as i64;
    u64::try_from(minutes).ok().map(|m| m * MINUTE_US)
}

/// `2026-09-18 14:00` or `2026-09-18T14:00` (a bare date is midnight).
pub fn parse(text: &str) -> Option<u64> {
    let t = text.trim().replace('T', " ");
    let (date, time) = t.split_once(' ').unwrap_or((&t, "00:00"));
    let mut d = date.split('-').map(|p| p.trim().parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    if d.next().is_some() {
        return None;
    }
    let (h, min) = time.trim().split_once(':')?;
    tick(y, m, day, h.parse().ok()?, min.parse().ok()?)
}

/// The first tick of the month after the one `us` falls in.
pub fn next_month(us: u64) -> u64 {
    let c = civil(us);
    let (y, m) = if c.month == 12 {
        (c.year + 1, 1)
    } else {
        (c.year, c.month + 1)
    };
    tick(y, m, 1, 0, 0).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_epoch_and_round_trips() {
        assert_eq!(civil(0).stamp(), "Sep 17, 2026, 09:00");
        assert_eq!(civil(DAY_US * 15).date(), "Oct 2, 2026");
        assert_eq!(parse("2026-09-17 09:00"), Some(0));
        assert_eq!(parse("2026-09-17 08:59"), None);
        let t = parse("2026-12-31T23:30").unwrap();
        assert_eq!(civil(t).input(), "2026-12-31 23:30");
        assert_eq!(civil(next_month(t)).input(), "2027-01-01 00:00");
        assert_eq!(parse("2026-02-30 10:00"), None);
        assert_eq!(civil(t).month_key(), "2026-12");
    }
}
