//! Spans of time as a scheduler writes them: `15d`, `120h`, `-8h`, `7.5h`,
//! and in weeks, months and years as P6 writes them: `2w`, `1m`, `1y`.

use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use totalfloat_model::{HoursPerPeriod, Minutes};

/// A span in hours, or in working days, weeks, months or years.
///
/// Days, weeks, months and years need a calendar's hours in each, so what
/// they mean depends on the activity they're for. Values are kept as written and exact: thousandths of
/// the unit, which reach whole minutes for any span written to two places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    thousandths: i64,
    unit: Unit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Hours,
    Days,
    Weeks,
    Months,
    Years,
}

/// The largest span read, in thousandths of its unit.
const LIMIT: i64 = 1_000_000_000;

/// A span that doesn't parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} isn't a span: write it as 15d, 120h, 2w, 1m or 1y, with - for a lead")]
pub struct ParseSpanError(String);

impl FromStr for Span {
    type Err = ParseSpanError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || ParseSpanError(text.to_owned());
        let trimmed = text.trim();
        let unit = match trimmed.chars().last() {
            Some('h' | 'H') => Unit::Hours,
            Some('d' | 'D') => Unit::Days,
            Some('w' | 'W') => Unit::Weeks,
            // A month, as P6 writes it.
            Some('m' | 'M') => Unit::Months,
            Some('y' | 'Y') => Unit::Years,
            _ => return Err(bad()),
        };
        let number = &trimmed[..trimmed.len() - 1];
        let (negative, digits) = number
            .strip_prefix('-')
            .map_or((false, number), |rest| (true, rest));
        let (whole, part) = digits.split_once('.').unwrap_or((digits, ""));
        let plain = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if (whole.is_empty() && part.is_empty()) || part.len() > 3 || !plain(whole) || !plain(part)
        {
            return Err(bad());
        }
        let whole: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| bad())?
        };
        let part: i64 = format!("{part:0<3}").parse().map_err(|_| bad())?;
        let value = whole
            .checked_mul(1000)
            .and_then(|w| w.checked_add(part))
            .ok_or_else(bad)?;
        // Beyond any calendar, and it keeps later arithmetic in range.
        if value > LIMIT {
            return Err(bad());
        }
        Ok(Self {
            thousandths: if negative { -value } else { value },
            unit,
        })
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = match self.unit {
            Unit::Hours => 'h',
            Unit::Days => 'd',
            Unit::Weeks => 'w',
            Unit::Months => 'm',
            Unit::Years => 'y',
        };
        #[allow(clippy::cast_precision_loss, reason = "display only, far below 2^52")]
        let value = self.thousandths as f64 / 1000.0;
        write!(f, "{value}{unit}")
    }
}

impl Span {
    /// No time at all.
    pub const ZERO: Self = Self {
        thousandths: 0,
        unit: Unit::Hours,
    };

    /// These minutes in hours, to the nearest thousandth of an hour, which
    /// [`Span::minutes`] turns back into the same minutes. `None` past a
    /// million hours, which a span can't hold.
    #[must_use]
    pub fn hours_of(minutes: Minutes) -> Option<Self> {
        let exact = i128::from(minutes.get()) * 1000;
        let thousandths = (exact + exact.signum() * 30) / 60;
        if thousandths.abs() > LIMIT.into() {
            return None;
        }
        Some(Self {
            thousandths: i64::try_from(thousandths).ok()?,
            unit: Unit::Hours,
        })
    }

    /// These minutes in days of `day` minutes, if that's exact to a
    /// thousandth of a day.
    #[must_use]
    pub fn days_of(minutes: Minutes, day: Minutes) -> Option<Self> {
        let exact = i128::from(minutes.get()) * 1000;
        let day = i128::from(day.get());
        if day <= 0 || exact % day != 0 || (exact / day).abs() > LIMIT.into() {
            return None;
        }
        Some(Self {
            thousandths: i64::try_from(exact / day).ok()?,
            unit: Unit::Days,
        })
    }

    /// The span in minutes, to the nearest minute. Days need `day`, the
    /// calendar's working time per day; without it they give `None`, as do
    /// weeks, months and years, which need [`Span::minutes_in`].
    #[must_use]
    pub fn minutes(self, day: Option<Minutes>) -> Option<Minutes> {
        self.minutes_in(Some(&HoursPerPeriod {
            day,
            ..HoursPerPeriod::default()
        }))
    }

    /// Whether the span is in hours or days, the units schedule text takes.
    #[must_use]
    pub const fn in_hours_or_days(self) -> bool {
        matches!(self.unit, Unit::Hours | Unit::Days)
    }

    /// The period its unit needs a calendar to say the length of: `None` for
    /// hours, else `day`, `week`, `month` or `year`.
    #[must_use]
    pub const fn period(self) -> Option<&'static str> {
        match self.unit {
            Unit::Hours => None,
            Unit::Days => Some("day"),
            Unit::Weeks => Some("week"),
            Unit::Months => Some("month"),
            Unit::Years => Some("year"),
        }
    }

    /// The span in minutes, to the nearest minute, with a calendar's hours in
    /// a day, week, month and year (`CALENDAR.day_hr_cnt` and the rest).
    /// `None` when the calendar doesn't give the length its unit needs.
    #[must_use]
    pub fn minutes_in(self, lengths: Option<&HoursPerPeriod>) -> Option<Minutes> {
        let per_unit = match self.unit {
            Unit::Hours => 60,
            Unit::Days => lengths?.day?.get(),
            Unit::Weeks => lengths?.week?.get(),
            Unit::Months => lengths?.month?.get(),
            Unit::Years => lengths?.year?.get(),
        };
        let exact = i128::from(self.thousandths) * i128::from(per_unit);
        let rounded = (exact + exact.signum() * 500) / 1000;
        i64::try_from(rounded).ok().map(Minutes::new)
    }
}

impl<'de> Deserialize<'de> for Span {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Written back in its shortest form, such as `15d` for `15.0D`.
impl Serialize for Span {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl JsonSchema for Span {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Span".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^-?[0-9]*\\.?[0-9]{0,3}[hHdDwWmMyY]$",
            "description": "A span such as 15d, 120h, 2w, 1m or 1y. Days, weeks, months (m) and years use the calendar's hours in each."
        })
    }
}
