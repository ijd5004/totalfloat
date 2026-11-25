//! An assignment's units spread over time, evenly, by a resource curve or by
//! hours typed in by hand (TF-040).
//!
//! Point 0 of a curve is the start, and point `i` from 1 to 20 is the percent
//! of the units in the `i`-th twentieth of the work between the dates. A
//! typed spread gives the units in stretches of work time, in order. Within a
//! twentieth or a stretch, and over the whole span when there's no curve,
//! units follow work time on the calendar, so a holiday or a weekend gets
//! none.

use totalfloat_calendar::WorkCalendar;
use totalfloat_model::{
    ActivityType, Assignment, CURVE_POINTS, CalendarId, Date, DateTime, ResourceCurve, Schedule,
    SpreadSegment, TimeOfDay, Weekday,
};

/// The twentieths a curve's points after the first cover.
const SEGMENTS: i128 = 20;

/// The periods units are spread into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// Calendar days.
    Day,
    /// Seven days, each period beginning on `starts_on`.
    Week {
        /// The first day of every week.
        starts_on: Weekday,
    },
    /// Calendar months.
    Month,
}

/// How units are shaped over the work between two dates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distribution<'a> {
    /// Evenly over work time.
    Even,
    /// By a resource curve.
    Curve(&'a ResourceCurve),
    /// By stretches typed in by hand. The stretches are stretched or shrunk
    /// to the work between the dates and their units scaled to the units
    /// spread, so a spread whose work and units match the assignment's gives
    /// each stretch exactly its typed units. With no stretches, or none with
    /// units, the spread is even.
    Typed(&'a [SpreadSegment]),
}

/// The units in one period.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeriodUnits {
    /// The period's first day.
    pub start: Date,
    /// Units in the period; zero when it holds no work.
    pub units: f64,
}

/// Why units can't be spread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SpreadError {
    /// The finish is before the start.
    #[error("the finish is before the start")]
    Backwards,
    /// The units aren't a finite number.
    #[error("the units aren't a finite number")]
    Units,
    /// A curve point is negative or not finite, or the points sum to zero.
    #[error("a curve point is negative or not finite, or the points sum to zero")]
    Curve,
    /// A typed stretch's units are negative or not finite, or its work isn't
    /// positive.
    #[error("a typed stretch's units are negative or not finite, or its work isn't positive")]
    Typed,
    /// A period boundary is outside the supported civil range.
    #[error("a period is outside the supported civil range")]
    OutOfRange,
}

/// The calendar an assignment's work runs on: its resource's on a
/// resource-dependent activity, when the resource has one, and otherwise its
/// activity's (TF-028). `None` if its activity or resource isn't in the
/// schedule, which scheduling refuses too.
#[must_use]
pub fn assignment_calendar(schedule: &Schedule, assignment: &Assignment) -> Option<CalendarId> {
    let activity = schedule.activities.get(&assignment.activity)?;
    let resource = match assignment.resource {
        Some(id) => Some(schedule.resources.get(&id)?),
        None => None,
    };
    match resource.and_then(|r| r.calendar) {
        Some(calendar) if activity.activity_type == ActivityType::ResourceDependent => {
            Some(calendar)
        }
        _ => Some(activity.calendar),
    }
}

/// How an assignment's remaining units are shaped: by its typed remaining
/// spread if it has one, evenly if its typed spread has no remaining part,
/// and otherwise by its curve, or evenly with none or one the schedule
/// lacks.
#[must_use]
pub fn remaining_distribution<'a>(
    schedule: &'a Schedule,
    assignment: &'a Assignment,
) -> Distribution<'a> {
    if let Some(typed) = &assignment.typed_spread {
        return Distribution::Typed(&typed.remaining);
    }
    assignment
        .curve
        .and_then(|c| schedule.resource_curves.get(&c))
        .map_or(Distribution::Even, Distribution::Curve)
}

/// Spreads `units` over the work between `start` and `finish` on `calendar`,
/// shaped by `distribution`, into every period from the one holding `start`
/// to the one holding `finish`, earliest first. A
/// finish at midnight ends the day before, as XER files spell finishes, so the
/// period that midnight begins isn't included.
///
/// The periods' units sum to `units`, to rounding. With no work between the
/// dates, the units all fall in the first period.
///
/// # Errors
///
/// [`SpreadError`] for a finish before the start, units that aren't finite, a
/// curve with a negative or non-finite point or nothing in it, a typed
/// stretch with negative or non-finite units or no work, or a period beyond
/// the civil range.
pub fn spread(
    units: f64,
    start: DateTime,
    finish: DateTime,
    distribution: Distribution<'_>,
    calendar: &WorkCalendar,
    period: Period,
) -> Result<Vec<PeriodUnits>, SpreadError> {
    if finish < start {
        return Err(SpreadError::Backwards);
    }
    if !units.is_finite() {
        return Err(SpreadError::Units);
    }
    let shape = Shape::new(distribution)?;
    let total = calendar.work_between(start, finish).get();
    let mut first = period_start(start.date(), period).ok_or(SpreadError::OutOfRange)?;
    let mut result = Vec::new();
    let mut before = 0.0;
    loop {
        let next = next_period(first, period).ok_or(SpreadError::OutOfRange)?;
        let boundary = next
            .at(TimeOfDay::MIDNIGHT)
            .ok_or(SpreadError::OutOfRange)?;
        if boundary >= finish {
            // The last period takes what's left, so the sum is exact but for
            // the rounding of the sum itself.
            result.push(PeriodUnits {
                start: first,
                units: units - before,
            });
            return Ok(result);
        }
        let done = calendar.work_between(start, boundary).get();
        let through = units * shape.fraction(done, total);
        result.push(PeriodUnits {
            start: first,
            units: through - before,
        });
        before = through;
        first = next;
    }
}

/// How units accumulate over the work between the dates.
enum Shape {
    Even,
    /// A curve's points, and their sum.
    Curve([f64; CURVE_POINTS], f64),
    /// Typed stretches as (work minutes at their end, units), their total
    /// work and their total units.
    Typed(Vec<(i128, f64)>, i128, f64),
}

impl Shape {
    fn new(distribution: Distribution<'_>) -> Result<Self, SpreadError> {
        let curve = match distribution {
            Distribution::Even => return Ok(Self::Even),
            Distribution::Typed(segments) => return Self::typed(segments),
            Distribution::Curve(curve) => curve,
        };
        let points = curve.points;
        if points.iter().any(|p| !p.is_finite() || *p < 0.0) {
            return Err(SpreadError::Curve);
        }
        let sum: f64 = points.iter().sum();
        if sum <= 0.0 || !sum.is_finite() {
            return Err(SpreadError::Curve);
        }
        Ok(Self::Curve(points, sum))
    }

    fn typed(segments: &[SpreadSegment]) -> Result<Self, SpreadError> {
        let mut ends = Vec::with_capacity(segments.len());
        let (mut work, mut units) = (0_i128, 0.0);
        for segment in segments {
            if !segment.units.is_finite() || segment.units < 0.0 || segment.work.get() <= 0 {
                return Err(SpreadError::Typed);
            }
            work += i128::from(segment.work.get());
            units += segment.units;
            ends.push((work, segment.units));
        }
        if units <= 0.0 || !units.is_finite() {
            return Ok(Self::Even);
        }
        Ok(Self::Typed(ends, work, units))
    }

    /// The share of the units placed by `done` of `total` work minutes.
    /// Point 0 is placed as soon as work begins, and everything at once when
    /// there's no work.
    #[allow(
        clippy::cast_precision_loss,
        reason = "work minutes stay far below 2^52"
    )]
    fn fraction(&self, done: i64, total: i64) -> f64 {
        if total <= 0 || done >= total {
            return 1.0;
        }
        if done <= 0 {
            return 0.0;
        }
        match self {
            Self::Even => done as f64 / total as f64,
            Self::Curve(points, sum) => {
                let scaled = i128::from(done) * SEGMENTS;
                let total = i128::from(total);
                // `done < total`, so the segment is 0 to 19.
                let segment = usize::try_from(scaled / total).unwrap_or(0);
                let part = (scaled % total) as f64 / total as f64;
                let placed: f64 = points[..=segment].iter().sum();
                (placed + points[segment + 1] * part) / sum
            }
            Self::Typed(ends, work, sum) => {
                // The stretches' work is scaled to `total`: `done` is at
                // `done * work / total` of it, compared in whole numbers so
                // a stretch's end falls exactly on a period boundary.
                let at = i128::from(done) * work;
                let total = i128::from(total);
                let mut placed = 0.0;
                let mut begin = 0;
                for &(end, units) in ends {
                    if end * total > at {
                        let part = (at - begin * total) as f64 / ((end - begin) * total) as f64;
                        return (placed + units * part) / sum;
                    }
                    placed += units;
                    begin = end;
                }
                1.0
            }
        }
    }
}

/// The first day of the period holding `day`.
fn period_start(day: Date, period: Period) -> Option<Date> {
    match period {
        Period::Day => Some(day),
        Period::Week { starts_on } => {
            let back = (day.weekday().index() + 7 - starts_on.index()) % 7;
            day.checked_add_days(-i32::try_from(back).ok()?)
        }
        Period::Month => {
            let (year, month, _) = day.ymd();
            Date::from_ymd(year, month, 1)
        }
    }
}

/// The first day of the period after the one beginning on `first`.
fn next_period(first: Date, period: Period) -> Option<Date> {
    match period {
        Period::Day => first.checked_add_days(1),
        Period::Week { .. } => first.checked_add_days(7),
        Period::Month => {
            let (year, month, _) = first.ymd();
            if month == 12 {
                Date::from_ymd(year.checked_add(1)?, 1, 1)
            } else {
                Date::from_ymd(year, month + 1, 1)
            }
        }
    }
}

#[cfg(test)]
#[path = "spread_tests.rs"]
mod tests;
