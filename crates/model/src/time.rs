//! Civil (time-zone-free) time at minute resolution (ARCHITECTURE.md, Data model).
//!
//! XER files have no time zones and hold times to the minute, so the model uses
//! its own small types instead of a general date-time library: a [`Date`] is a
//! day number, a [`DateTime`] is a minute number, and [`Minutes`] is a signed
//! span. None of them can represent seconds, so sub-minute values cannot leak
//! into scheduling math.
//!
//! Supported years are 1 through 9999, the range the four-digit XER date
//! format can express. Constructors return `None` outside it.

use std::fmt;
use std::num::NonZeroI64;
use std::ops::{Add, Neg, Sub};
use std::str::FromStr;

/// Minutes in one hour.
pub const MINUTES_PER_HOUR: i64 = 60;
/// Minutes in one calendar day.
pub const MINUTES_PER_DAY: i64 = 24 * MINUTES_PER_HOUR;

const MIN_YEAR: i32 = 1;
const MAX_YEAR: i32 = 9999;
/// Day number of 0001-01-01.
const MIN_DAY: i32 = days_from_civil(MIN_YEAR, 1, 1);
/// Day number of 9999-12-31.
const MAX_DAY: i32 = days_from_civil(MAX_YEAR, 12, 31);
/// Minute number of 0001-01-01 00:00.
const MIN_MINUTE: i64 = MIN_DAY as i64 * MINUTES_PER_DAY;
/// Minute number of 9999-12-31 23:59.
const MAX_MINUTE: i64 = MAX_DAY as i64 * MINUTES_PER_DAY + MINUTES_PER_DAY - 1;

/// A signed span of time in whole minutes.
///
/// Used for durations, lags, float and calendar work time. XER stores these as
/// decimal hours (`*_hr_cnt` fields); the codec converts to minutes and reports
/// any value that is not a whole minute.
///
/// The `+`, `-` and unary `-` operators saturate at the `i64` limits instead of
/// panicking or wrapping, so they are safe on untrusted input. Use the
/// `checked_*` methods where overflow must be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Minutes(pub i64);

impl Minutes {
    /// Zero minutes.
    pub const ZERO: Self = Self(0);

    /// A span of `minutes` minutes.
    #[must_use]
    #[inline]
    pub const fn new(minutes: i64) -> Self {
        Self(minutes)
    }

    /// A span of whole `hours`, or `None` on overflow.
    #[must_use]
    pub const fn from_hours(hours: i64) -> Option<Self> {
        match hours.checked_mul(MINUTES_PER_HOUR) {
            Some(m) => Some(Self(m)),
            None => None,
        }
    }

    /// The number of minutes.
    #[must_use]
    #[inline]
    pub const fn get(self) -> i64 {
        self.0
    }

    /// `self + rhs`, or `None` on overflow.
    #[must_use]
    #[inline]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(m) => Some(Self(m)),
            None => None,
        }
    }

    /// `self - rhs`, or `None` on overflow.
    #[must_use]
    #[inline]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(m) => Some(Self(m)),
            None => None,
        }
    }

    /// `-self`, or `None` on overflow (only for `i64::MIN`).
    #[must_use]
    #[inline]
    pub const fn checked_neg(self) -> Option<Self> {
        match self.0.checked_neg() {
            Some(m) => Some(Self(m)),
            None => None,
        }
    }

    /// `self * factor`, or `None` on overflow.
    #[must_use]
    pub const fn checked_mul(self, factor: i64) -> Option<Self> {
        match self.0.checked_mul(factor) {
            Some(m) => Some(Self(m)),
            None => None,
        }
    }

    /// The absolute value, saturating at `i64::MAX`.
    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0.saturating_abs())
    }

    /// True if the span is negative (a lead, or negative float).
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }
}

impl Add for Minutes {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Minutes {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Neg for Minutes {
    type Output = Self;
    fn neg(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

impl fmt::Display for Minutes {
    /// Formats as `<n>m`, e.g. `480m`. Display units (hours, days) are an
    /// interface concern and depend on calendar hours per period.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}m", self.0)
    }
}

/// Day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
    /// Sunday. P6 calendars list the week starting on Sunday.
    Sunday,
    /// Monday.
    Monday,
    /// Tuesday.
    Tuesday,
    /// Wednesday.
    Wednesday,
    /// Thursday.
    Thursday,
    /// Friday.
    Friday,
    /// Saturday.
    Saturday,
}

impl Weekday {
    /// All days, Sunday first.
    pub const ALL: [Self; 7] = [
        Self::Sunday,
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
    ];

    /// Index with Sunday = 0 through Saturday = 6.
    #[must_use]
    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A time of day in minutes after midnight, from `00:00` to `24:00` inclusive.
///
/// `24:00` is allowed so a work interval can run to the end of the day, as XER
/// calendars do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TimeOfDay(u16);

impl TimeOfDay {
    /// Midnight at the start of the day.
    pub const MIDNIGHT: Self = Self(0);
    /// Midnight at the end of the day (`24:00`).
    pub const END_OF_DAY: Self = Self(1440);

    /// `hour:minute`, or `None` unless it is between `00:00` and `24:00`.
    #[must_use]
    pub const fn from_hm(hour: u8, minute: u8) -> Option<Self> {
        if minute >= 60 || hour > 24 || (hour == 24 && minute != 0) {
            return None;
        }
        Some(Self(hour as u16 * 60 + minute as u16))
    }

    /// `minutes` after midnight, or `None` if over `24:00`.
    #[must_use]
    pub const fn from_minutes(minutes: u16) -> Option<Self> {
        if minutes > 1440 {
            None
        } else {
            Some(Self(minutes))
        }
    }

    /// Minutes after midnight (0 to 1440).
    #[must_use]
    #[inline]
    pub const fn minutes(self) -> u16 {
        self.0
    }

    /// The hour (0 to 24).
    #[must_use]
    pub const fn hour(self) -> u8 {
        // At most 24, so the cast is lossless.
        #[allow(clippy::cast_possible_truncation)]
        let h = (self.0 / 60) as u8;
        h
    }

    /// The minute within the hour (0 to 59).
    #[must_use]
    pub const fn minute(self) -> u8 {
        #[allow(clippy::cast_possible_truncation)]
        let m = (self.0 % 60) as u8;
        m
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour(), self.minute())
    }
}

/// A civil calendar date (no time zone).
///
/// Stored as a day number relative to 1970-01-01, so dates order and subtract
/// as integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i32);

impl Date {
    /// The date `year-month-day`, or `None` if it doesn't exist or the year is
    /// outside 1–9999.
    #[must_use]
    pub const fn from_ymd(year: i32, month: u8, day: u8) -> Option<Self> {
        if year < MIN_YEAR || year > MAX_YEAR || month < 1 || month > 12 || day < 1 {
            return None;
        }
        if day > days_in_month(year, month) {
            return None;
        }
        Some(Self(days_from_civil(year, month, day)))
    }

    /// `(year, month, day)`.
    #[must_use]
    pub const fn ymd(self) -> (i32, u8, u8) {
        civil_from_days(self.0)
    }

    /// The day of the week.
    #[must_use]
    #[inline]
    pub const fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday (index 4 with Sunday = 0).
        match (self.0 as i64 + 4).rem_euclid(7) {
            0 => Weekday::Sunday,
            1 => Weekday::Monday,
            2 => Weekday::Tuesday,
            3 => Weekday::Wednesday,
            4 => Weekday::Thursday,
            5 => Weekday::Friday,
            _ => Weekday::Saturday,
        }
    }

    /// The date `days` later (or earlier, if negative), or `None` outside the
    /// supported range.
    #[must_use]
    #[inline]
    pub fn checked_add_days(self, days: i32) -> Option<Self> {
        let n = self.0.checked_add(days)?;
        (MIN_DAY..=MAX_DAY).contains(&n).then_some(Self(n))
    }

    /// Whole days from `earlier` to `self` (negative if `earlier` is later).
    #[must_use]
    #[inline]
    pub const fn days_since(self, earlier: Self) -> i32 {
        self.0 - earlier.0
    }

    /// This date at `time`. `24:00` gives midnight at the start of the next
    /// day, so this is `None` only for 9999-12-31 at `24:00`.
    #[must_use]
    #[inline]
    pub const fn at(self, time: TimeOfDay) -> Option<DateTime> {
        let n = self.0 as i64 * MINUTES_PER_DAY + time.0 as i64;
        if n > MAX_MINUTE {
            None
        } else {
            Some(DateTime::new(n))
        }
    }
}

impl fmt::Display for Date {
    /// Formats as `YYYY-MM-DD`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

impl FromStr for Date {
    type Err = ParseTimeError;

    /// Parses `YYYY-MM-DD`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return Err(ParseTimeError::new(s));
        }
        let year = digits(&b[0..4]).ok_or_else(|| ParseTimeError::new(s))?;
        let month = digits(&b[5..7]).ok_or_else(|| ParseTimeError::new(s))?;
        let day = digits(&b[8..10]).ok_or_else(|| ParseTimeError::new(s))?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        Self::from_ymd(year as i32, month as u8, day as u8).ok_or_else(|| ParseTimeError::new(s))
    }
}

/// A civil date and time at minute resolution (no time zone, no seconds).
///
/// Stored as minutes relative to 1970-01-01 00:00. The XER text form is
/// `YYYY-MM-DD HH:MM`, which [`Display`](fmt::Display) and [`FromStr`] use.
///
/// Inside, that count is shifted to stay above zero. An activity holds about 20
/// optional dates, and this way `Option<DateTime>` is no larger than a date.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime(NonZeroI64);

impl fmt::Debug for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DateTime").field(&self.minutes()).finish()
    }
}

impl DateTime {
    /// Shifts minutes from the epoch to the stored value. Every caller checks
    /// the range, which keeps the value above zero.
    #[inline]
    const fn new(minutes: i64) -> Self {
        match NonZeroI64::new(minutes - MIN_MINUTE + 1) {
            Some(value) => Self(value),
            None => Self(NonZeroI64::MIN),
        }
    }

    #[inline]
    const fn minutes(self) -> i64 {
        self.0.get() + MIN_MINUTE - 1
    }

    /// `year-month-day hour:minute`, or `None` if invalid. `24:00` is not
    /// accepted here; use [`Date::at`] with [`TimeOfDay::END_OF_DAY`].
    #[must_use]
    pub const fn from_ymd_hm(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> Option<Self> {
        let Some(date) = Date::from_ymd(year, month, day) else {
            return None;
        };
        if hour >= 24 || minute >= 60 {
            return None;
        }
        // Hours below 24 always stay within the date, so this is in range.
        date.at(TimeOfDay(hour as u16 * 60 + minute as u16))
    }

    /// The calendar date.
    #[must_use]
    #[inline]
    pub const fn date(self) -> Date {
        // Within the supported year range the day number fits in i32.
        #[allow(clippy::cast_possible_truncation)]
        let d = self.minutes().div_euclid(MINUTES_PER_DAY) as i32;
        Date(d)
    }

    /// The time of day (`00:00` to `23:59`).
    #[must_use]
    #[inline]
    pub const fn time(self) -> TimeOfDay {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let m = self.minutes().rem_euclid(MINUTES_PER_DAY) as u16;
        TimeOfDay(m)
    }

    /// The day of the week.
    #[must_use]
    #[inline]
    pub const fn weekday(self) -> Weekday {
        self.date().weekday()
    }

    /// The instant `span` later (or earlier, if negative) in elapsed time, or
    /// `None` outside the supported range. Work-time arithmetic lives in the
    /// calendar crate; this is plain clock arithmetic.
    #[must_use]
    #[inline]
    pub fn checked_add(self, span: Minutes) -> Option<Self> {
        let n = self.minutes().checked_add(span.0)?;
        (MIN_MINUTE..=MAX_MINUTE).contains(&n).then(|| Self::new(n))
    }

    /// Elapsed minutes from `earlier` to `self` (negative if `earlier` is later).
    #[must_use]
    #[inline]
    pub const fn minutes_since(self, earlier: Self) -> Minutes {
        Minutes(self.minutes() - earlier.minutes())
    }
}

impl fmt::Display for DateTime {
    /// Formats as `YYYY-MM-DD HH:MM`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day) = self.date().ymd();
        let Ok(year @ 0..=9999) = u16::try_from(year) else {
            return write!(f, "{} {}", self.date(), self.time());
        };
        let time = self.time();
        // Writing a file formats every date in it, so this skips `write!`.
        let pair = |value: u16| {
            let value = u8::try_from(value % 100).unwrap_or(0);
            [b'0' + value / 10, b'0' + value % 10]
        };
        let mut text = *b"0000-00-00 00:00";
        for (at, value) in [
            (0, year / 100),
            (2, year),
            (5, month.into()),
            (8, day.into()),
            (11, time.hour().into()),
            (14, time.minute().into()),
        ] {
            text[at..at + 2].copy_from_slice(&pair(value));
        }
        f.write_str(std::str::from_utf8(&text).map_err(|_| fmt::Error)?)
    }
}

impl FromStr for DateTime {
    type Err = ParseTimeError;

    /// Parses `YYYY-MM-DD HH:MM`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() != 16 || b[10] != b' ' || b[13] != b':' {
            return Err(ParseTimeError::new(s));
        }
        let date: Date = s[..10].parse().map_err(|_| ParseTimeError::new(s))?;
        let hour = digits(&b[11..13]).ok_or_else(|| ParseTimeError::new(s))?;
        let minute = digits(&b[14..16]).ok_or_else(|| ParseTimeError::new(s))?;
        if hour >= 24 || minute >= 60 {
            return Err(ParseTimeError::new(s));
        }
        #[allow(clippy::cast_possible_truncation)]
        let time = TimeOfDay((hour * 60 + minute) as u16);
        date.at(time).ok_or_else(|| ParseTimeError::new(s))
    }
}

/// A date or date-time string that is not in the expected format or is not a
/// real date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTimeError {
    input: String,
}

impl ParseTimeError {
    #[inline]
    fn new(input: &str) -> Self {
        Self {
            input: input.chars().take(40).collect(),
        }
    }
}

impl fmt::Display for ParseTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid date or time {:?}: expected YYYY-MM-DD or YYYY-MM-DD HH:MM",
            self.input
        )
    }
}

impl std::error::Error for ParseTimeError {}

/// ASCII decimal digits to a number; `None` if any byte is not a digit.
fn digits(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0u32, |acc, &b| {
        b.is_ascii_digit().then(|| acc * 10 + u32::from(b - b'0'))
    })
}

const fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Day-number conversions use the well-known proleptic Gregorian algorithm
// (era / year-of-era / day-of-year decomposition, epoch 1970-01-01).

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
const fn days_from_civil(year: i32, month: u8, day: u8) -> i32 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let m = month as i32;
    let mp = if m > 2 { m - 3 } else { m + 9 }; // March = 0
    let doy = (153 * mp + 2) / 5 + day as i32 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn civil_from_days(days: i32) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_optional_date_is_no_larger_than_a_date() {
        assert_eq!(size_of::<Option<DateTime>>(), size_of::<DateTime>());
    }

    #[test]
    fn dates_at_the_ends_of_the_range_keep_their_order_and_text() {
        let first = DateTime::from_ymd_hm(1, 1, 1, 0, 0).unwrap();
        let last = DateTime::from_ymd_hm(9999, 12, 31, 23, 59).unwrap();
        assert!(first < last);
        assert_eq!(first.to_string(), "0001-01-01 00:00");
        assert_eq!(last.to_string(), "9999-12-31 23:59");
        assert_eq!(first.minutes(), MIN_MINUTE);
        assert_eq!(last.minutes(), MAX_MINUTE);
    }

    #[test]
    fn datetime_display_matches_its_date_and_time_parts() {
        for (year, month, day, hour, minute) in [
            (2026, 1, 9, 17, 5),
            (999, 12, 31, 0, 0),
            (7, 2, 3, 23, 59),
            (1, 1, 1, 8, 0),
            (9999, 12, 31, 23, 59),
        ] {
            let t = DateTime::from_ymd_hm(year, month, day, hour, minute).unwrap();
            assert_eq!(t.to_string(), format!("{} {}", t.date(), t.time()));
        }
    }
    use proptest::prelude::*;

    fn dt(s: &str) -> DateTime {
        s.parse().unwrap()
    }

    #[test]
    fn minutes_checked_arithmetic() {
        let a = Minutes::new(480);
        assert_eq!(a.checked_add(Minutes::new(60)), Some(Minutes::new(540)));
        assert_eq!(a.checked_sub(Minutes::new(600)), Some(Minutes::new(-120)));
        assert_eq!(Minutes::new(i64::MAX).checked_add(Minutes::new(1)), None);
        assert_eq!(Minutes::new(i64::MIN).checked_sub(Minutes::new(1)), None);
        assert_eq!(Minutes::new(i64::MIN).checked_neg(), None);
        assert_eq!(Minutes::new(-5).checked_neg(), Some(Minutes::new(5)));
        assert_eq!(Minutes::new(8).checked_mul(60), Some(Minutes::new(480)));
        assert_eq!(Minutes::new(i64::MAX).checked_mul(2), None);
        assert_eq!(Minutes::from_hours(8), Some(Minutes::new(480)));
        assert_eq!(Minutes::from_hours(i64::MAX), None);
    }

    #[test]
    fn minutes_operators_saturate() {
        assert_eq!(
            Minutes::new(i64::MAX) + Minutes::new(1),
            Minutes::new(i64::MAX)
        );
        assert_eq!(
            Minutes::new(i64::MIN) - Minutes::new(1),
            Minutes::new(i64::MIN)
        );
        assert_eq!(-Minutes::new(i64::MIN), Minutes::new(i64::MAX));
        assert_eq!(Minutes::new(30) + Minutes::new(-45), Minutes::new(-15));
        assert_eq!(Minutes::new(i64::MIN).abs(), Minutes::new(i64::MAX));
        assert!(Minutes::new(-1).is_negative());
        assert!(!Minutes::ZERO.is_negative());
        assert_eq!(Minutes::new(480).to_string(), "480m");
    }

    #[test]
    fn known_dates() {
        assert_eq!(Date::from_ymd(1970, 1, 1).unwrap().days_since(Date(0)), 0);
        assert_eq!(
            Date::from_ymd(2000, 2, 29).unwrap().to_string(),
            "2000-02-29"
        );
        assert_eq!(Date::from_ymd(1900, 2, 29), None);
        assert_eq!(Date::from_ymd(2023, 4, 31), None);
        assert_eq!(Date::from_ymd(0, 1, 1), None);
        assert_eq!(Date::from_ymd(10_000, 1, 1), None);
        assert_eq!(
            Date::from_ymd(2026, 10, 30).unwrap().weekday(),
            Weekday::Friday
        );
        assert_eq!(Date::from_ymd(1, 1, 1).unwrap().weekday(), Weekday::Monday);
    }

    #[test]
    fn datetime_parse_format_and_parts() {
        let t = dt("2024-12-09 08:00");
        assert_eq!(t.to_string(), "2024-12-09 08:00");
        assert_eq!(t.date().to_string(), "2024-12-09");
        assert_eq!(t.time(), TimeOfDay::from_hm(8, 0).unwrap());
        assert_eq!(t.weekday(), Weekday::Monday);
        for bad in [
            "2024-12-09",
            "2024-12-09 24:00",
            "2024-13-01 00:00",
            "2024-12-09T08:00",
            "2024-12-09 8:00",
            "",
        ] {
            assert!(bad.parse::<DateTime>().is_err(), "{bad}");
        }
        assert!("2024-02-30".parse::<Date>().is_err());
    }

    #[test]
    fn datetime_arithmetic() {
        let t = dt("2024-12-31 23:30");
        assert_eq!(
            t.checked_add(Minutes::new(45)).unwrap(),
            dt("2025-01-01 00:15")
        );
        assert_eq!(dt("2025-01-01 00:15").minutes_since(t), Minutes::new(45));
        assert_eq!(t.minutes_since(dt("2025-01-01 00:15")), Minutes::new(-45));
        assert_eq!(dt("9999-12-31 23:59").checked_add(Minutes::new(1)), None);
        assert_eq!(dt("0001-01-01 00:00").checked_add(Minutes::new(-1)), None);
        // Spans whose day number would wrap an i32 must not come back in range.
        assert_eq!(
            dt("1970-01-01 00:00").checked_add(Minutes::new((1i64 << 32) * MINUTES_PER_DAY)),
            None
        );
        assert_eq!(
            dt("1970-01-01 00:00").checked_add(Minutes::new(i64::MAX)),
            None
        );
        let last = Date::from_ymd(9999, 12, 31).unwrap();
        assert_eq!(last.checked_add_days(i32::MAX - MAX_DAY), None);
        assert_eq!(last.checked_add_days(1), None);
        assert_eq!(
            Date::from_ymd(1, 1, 1)
                .unwrap()
                .checked_add_days(i32::MIN + 1),
            None
        );
    }

    #[test]
    fn end_of_day_is_next_midnight() {
        let d = Date::from_ymd(2024, 2, 28).unwrap();
        assert_eq!(d.at(TimeOfDay::END_OF_DAY), Some(dt("2024-02-29 00:00")));
        let last = Date::from_ymd(9999, 12, 31).unwrap();
        assert_eq!(last.at(TimeOfDay::END_OF_DAY), None);
        assert_eq!(
            last.at(TimeOfDay::from_hm(23, 59).unwrap()),
            Some(dt("9999-12-31 23:59"))
        );
        assert_eq!(TimeOfDay::END_OF_DAY.to_string(), "24:00");
        assert_eq!(TimeOfDay::from_hm(24, 1), None);
        assert_eq!(TimeOfDay::from_minutes(1441), None);
    }

    proptest! {
        #[test]
        fn date_round_trips(n in -719_162i32..=2_932_896) {
            // 0001-01-01 ..= 9999-12-31
            let (y, m, d) = civil_from_days(n);
            let date = Date::from_ymd(y, m, d).unwrap();
            prop_assert_eq!(date.0, n);
            prop_assert_eq!(date.to_string().parse::<Date>().unwrap(), date);
            prop_assert_eq!(date.checked_add_days(1).map(|x| x.days_since(date)), (n < 2_932_896).then_some(1));
        }

        #[test]
        fn datetime_text_round_trips(days in -719_162i64..=2_932_896, minute in 0i64..1440) {
            let t = DateTime::new(days * MINUTES_PER_DAY + minute);
            prop_assert_eq!(t.to_string().parse::<DateTime>().unwrap(), t);
            prop_assert_eq!(t.date().at(t.time()), Some(t));
        }
    }
}
