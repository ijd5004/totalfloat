//! Deterministic integer-minute work arithmetic over effective model calendars.
//!
//! Compile once with [`WorkCalendar::new`]. Intervals are half-open for work
//! minutes. Positive addition selects a finish boundary; negative addition
//! selects a start boundary. A finish and the next start can have the same
//! work coordinate. See TF-013 for conventions, evidence and limitations.
#![forbid(unsafe_code)]

use std::{collections::BTreeMap, sync::Arc};
use totalfloat_model::{Calendar, Date, DateTime, Minutes, TimeOfDay, WorkInterval, WorkWeek};

/// Failure to compile or navigate a work calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CalendarError {
    /// No recurring work; finite exceptions are not a replacement workweek.
    #[error("the recurring workweek contains no work")]
    EmptyWeek,
    /// Reversed, zero-length or overlapping input intervals.
    #[error("calendar intervals are reversed, empty or overlapping")]
    InvalidIntervals,
    /// No representable work instant or amount in the requested direction.
    #[error("work arithmetic exceeds the supported civil date range")]
    OutOfRange,
}
#[derive(Debug)]
struct Pattern {
    intervals: Vec<WorkInterval>,
    total: i64,
}
impl Pattern {
    fn before(&self, minute: u16) -> i64 {
        self.intervals
            .iter()
            .map(|interval| {
                i64::from(
                    minute
                        .min(interval.finish.minutes())
                        .saturating_sub(interval.start.minutes()),
                )
            })
            .sum()
    }
    fn is_work(&self, minute: u16) -> bool {
        self.intervals.iter().any(|interval| {
            interval.start.minutes() <= minute && minute < interval.finish.minutes()
        })
    }
}
fn intern(
    intervals: &[WorkInterval],
    cache: &mut BTreeMap<Vec<WorkInterval>, Arc<Pattern>>,
) -> Result<Arc<Pattern>, CalendarError> {
    let mut sorted = intervals.to_vec();
    sorted.sort_unstable();
    let mut normalized: Vec<WorkInterval> = Vec::new();
    for interval in sorted {
        if interval.start >= interval.finish {
            return Err(CalendarError::InvalidIntervals);
        }
        if let Some(previous) = normalized.last_mut() {
            if interval.start < previous.finish {
                return Err(CalendarError::InvalidIntervals);
            }
            if interval.start == previous.finish {
                previous.finish = interval.finish;
                continue;
            }
        }
        normalized.push(interval);
    }
    if let Some(pattern) = cache.get(&normalized) {
        return Ok(Arc::clone(pattern));
    }
    let total = normalized
        .iter()
        .map(|interval| interval.duration().get())
        .sum();
    let pattern = Arc::new(Pattern {
        intervals: normalized.clone(),
        total,
    });
    cache.insert(normalized, Arc::clone(&pattern));
    Ok(pattern)
}
/// The work coordinate at the midnight beginning a day, counted from the
/// calendar's first day, from the weekly pattern alone.
fn plain_day_start(prefix: &[i64; 8], weekly: i64, days: i32) -> i64 {
    let full_weeks = i64::from(days / 7) * weekly;
    full_weeks + prefix[usize::try_from(days % 7).unwrap_or(0)]
}
/// Days `WorkCalendar::find_day` walks from its estimate before it bisects.
const WALK: usize = 8;
/// A position in a calendar's work. Friday 17:00 and Monday 08:00 are one spot on
/// a five-day calendar, with a name for each use: work can finish at the first and
/// start at the second.
#[derive(Debug, Clone, Copy)]
pub struct Spot<'a> {
    calendar: &'a WorkCalendar,
    coordinate: i64,
}

impl Spot<'_> {
    /// The spot `amount` of work later, or earlier if negative.
    ///
    /// # Errors
    /// Returns a range error on overflow.
    pub fn plus(self, amount: Minutes) -> Result<Self, CalendarError> {
        let coordinate = self
            .coordinate
            .checked_add(amount.get())
            .ok_or(CalendarError::OutOfRange)?;
        Ok(Self { coordinate, ..self })
    }
    /// The instant work can start here: the latest at this spot.
    ///
    /// # Errors
    /// Returns a range error past the end of the calendar.
    pub fn start(self) -> Result<DateTime, CalendarError> {
        let instant = self.calendar.select(self.coordinate, true)?;
        if self.calendar.is_work(instant) {
            Ok(instant)
        } else {
            Err(CalendarError::OutOfRange)
        }
    }
    /// The instant work can finish here: the earliest at this spot.
    ///
    /// # Errors
    /// Returns a range error before the start of the calendar.
    pub fn finish(self) -> Result<DateTime, CalendarError> {
        let instant = self.calendar.select(self.coordinate, false)?;
        if self.calendar.finishes_work(instant) {
            Ok(instant)
        } else {
            Err(CalendarError::OutOfRange)
        }
    }
}

#[derive(Debug)]
struct Exception {
    date: Date,
    pattern: Arc<Pattern>,
    /// The work coordinate at the midnight beginning `date`.
    start: i64,
    delta_after: i64,
}

/// An immutable snapshot of resolved calendar work patterns and exceptions.
///
/// Repeated day patterns share cached storage. Weekly prefixes and cumulative
/// exception adjustments give work coordinates in O(log exceptions), and weekly
/// totals place a coordinate near its day without scanning the days before it.
/// Hours-per-period display factors and base IDs are not used in arithmetic:
/// callers must first resolve inherited exceptions (the XER mapper does this).
#[derive(Debug)]
pub struct WorkCalendar {
    week: [Arc<Pattern>; 7],
    exceptions: Vec<Exception>,
    weekly: i64,
    prefix: [i64; 8],
    /// How far exceptions move any coordinate from the plain weekly count.
    reach: i64,
    first: DateTime,
    last: DateTime,
    /// The dates of `first` and `last`, and the coordinate of `last`. Every
    /// lookup needs them.
    first_date: Date,
    last_day: i32,
    top: i64,
}
impl WorkCalendar {
    /// Compile a validated effective calendar, independent of future edits.
    ///
    /// # Errors
    /// Rejects an empty recurring week or invalid/overlapping intervals.
    pub fn new(calendar: &Calendar) -> Result<Self, CalendarError> {
        Self::compile(&calendar.work_week, &calendar.exceptions)
    }
    /// A continuous 24-hour, seven-day calendar for elapsed relationship lag.
    ///
    /// # Errors
    /// Returns a range error if the model's civil bounds are unavailable.
    pub fn continuous() -> Result<Self, CalendarError> {
        let day = WorkInterval {
            start: TimeOfDay::MIDNIGHT,
            finish: TimeOfDay::END_OF_DAY,
        };
        Self::compile(
            &WorkWeek(std::array::from_fn(|_| vec![day])),
            &BTreeMap::new(),
        )
    }
    fn compile(
        week: &WorkWeek,
        exceptions: &BTreeMap<Date, Vec<WorkInterval>>,
    ) -> Result<Self, CalendarError> {
        let first = DateTime::from_ymd_hm(1, 1, 1, 0, 0).ok_or(CalendarError::OutOfRange)?;
        let last = DateTime::from_ymd_hm(9999, 12, 31, 23, 59).ok_or(CalendarError::OutOfRange)?;
        let mut cache = BTreeMap::new();
        let mut days = Vec::new();
        for day in &week.0 {
            days.push(intern(day, &mut cache)?);
        }
        let week: [Arc<Pattern>; 7] = days
            .try_into()
            .map_err(|_| CalendarError::InvalidIntervals)?;
        let weekly = week.iter().map(|day| day.total).sum();
        if weekly == 0 {
            return Err(CalendarError::EmptyWeek);
        }
        let mut prefix = [0; 8];
        for index in 0..7 {
            prefix[index + 1] = prefix[index] + week[(first.weekday().index() + index) % 7].total;
        }
        let first_date = first.date();
        let mut entries = Vec::with_capacity(exceptions.len());
        let mut delta = 0;
        let mut reach = 0;
        for (date, intervals) in exceptions {
            let pattern = intern(intervals, &mut cache)?;
            let start = plain_day_start(&prefix, weekly, date.days_since(first_date)) + delta;
            delta += pattern.total - week[date.weekday().index()].total;
            reach = i64::max(reach, delta.abs());
            entries.push(Exception {
                date: *date,
                pattern,
                start,
                delta_after: delta,
            });
        }
        let mut calendar = Self {
            week,
            exceptions: entries,
            weekly,
            prefix,
            reach,
            first,
            last,
            first_date,
            last_day: last.date().days_since(first_date),
            top: 0,
        };
        calendar.top = calendar.coordinate(last);
        Ok(calendar)
    }
    fn pattern(&self, date: Date) -> &Pattern {
        self.exceptions
            .binary_search_by_key(&date, |entry| entry.date)
            .map_or_else(
                |_| self.week[date.weekday().index()].as_ref(),
                |index| self.exceptions[index].pattern.as_ref(),
            )
    }
    /// The work coordinate at the midnight beginning `date`.
    fn day_start(&self, date: Date) -> i64 {
        let count = self.exceptions.partition_point(|entry| entry.date < date);
        self.day_start_after(date, count)
    }
    /// [`Self::day_start`] with `count` exceptions before `date`.
    fn day_start_after(&self, date: Date, count: usize) -> i64 {
        let delta = count
            .checked_sub(1)
            .map_or(0, |index| self.exceptions[index].delta_after);
        plain_day_start(&self.prefix, self.weekly, date.days_since(self.first_date)) + delta
    }
    fn coordinate(&self, instant: DateTime) -> i64 {
        let date = instant.date();
        // One search gives both the day's pattern and the exceptions before it.
        let (count, pattern) = match self
            .exceptions
            .binary_search_by_key(&date, |entry| entry.date)
        {
            Ok(index) => (index, self.exceptions[index].pattern.as_ref()),
            Err(index) => (index, self.week[date.weekday().index()].as_ref()),
        };
        self.day_start_after(date, count) + pattern.before(instant.time().minutes())
    }
    /// The first day, counted from the epoch, where `found` holds. `found` must
    /// not go back to false.
    ///
    /// The day is estimated from the weekly pattern and the exceptions before
    /// the coordinate, and found by trying the days next to the estimate. That
    /// is almost always a day or two away. A longer walk bisects instead the
    /// days weekly totals and exceptions bound it to; that bracket holds the
    /// first day `found` holds, so both ways give the same day.
    fn find_day(
        &self,
        coordinate: i64,
        found: impl Fn(&Self, i32, Date) -> bool,
    ) -> Result<i32, CalendarError> {
        let last = self.last_day;
        let test = |day: i32| {
            self.first_date
                .checked_add_days(day)
                .ok_or(CalendarError::OutOfRange)
                .map(|date| found(self, day, date))
        };
        let mut day = self.estimate_day(coordinate).clamp(0, last);
        if test(day)? {
            for _ in 0..WALK {
                if day == 0 || !test(day - 1)? {
                    return Ok(day);
                }
                day -= 1;
            }
        } else {
            for _ in 0..WALK {
                if day == last {
                    return Ok(day);
                }
                day += 1;
                if test(day)? {
                    return Ok(day);
                }
            }
        }
        let (low, high) = self.bracket(coordinate);
        self.bisect_day(low, high, found)
    }
    /// The days `find_day` searches: weekly totals and the most exceptions can
    /// move a coordinate put the answer between them.
    fn bracket(&self, coordinate: i64) -> (i32, i32) {
        let weeks = |rank: i64| i32::try_from(rank / self.weekly * 7).unwrap_or(i32::MAX);
        let low = weeks(coordinate - self.reach - 2 * self.weekly).max(0);
        let high = weeks(coordinate + self.reach + 2 * self.weekly).min(self.last_day);
        (low, high)
    }
    /// [`Self::find_day`] by bisecting the bracket. The answer is the same.
    fn bisect_day(
        &self,
        mut low: i32,
        mut high: i32,
        found: impl Fn(&Self, i32, Date) -> bool,
    ) -> Result<i32, CalendarError> {
        while low < high {
            let middle = low.midpoint(high);
            let date = self
                .first_date
                .checked_add_days(middle)
                .ok_or(CalendarError::OutOfRange)?;
            if found(self, middle, date) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        Ok(low)
    }
    /// About the day a work coordinate falls on: the weekly pattern, moved by
    /// the exceptions on days that start at or before it. Only a starting point.
    fn estimate_day(&self, coordinate: i64) -> i32 {
        let count = self
            .exceptions
            .partition_point(|entry| entry.start <= coordinate);
        let delta = count
            .checked_sub(1)
            .map_or(0, |index| self.exceptions[index].delta_after);
        let rank = (coordinate - delta).max(0);
        let (weeks, rest) = (rank / self.weekly, rank % self.weekly);
        let day = self.prefix[1..7].iter().take_while(|&&p| p <= rest).count();
        i32::try_from(weeks * 7 + i64::try_from(day).unwrap_or(0)).unwrap_or(i32::MAX)
    }
    /// The earliest or latest instant at a work coordinate: find the day, then
    /// walk its shifts.
    fn select(&self, coordinate: i64, latest: bool) -> Result<DateTime, CalendarError> {
        if coordinate < 0 || coordinate > self.top {
            return Err(CalendarError::OutOfRange);
        }
        let start = self.first_date;
        let at = |day: i32, minute: i64| {
            self.first
                .checked_add(Minutes(i64::from(day) * 1440 + minute))
                .ok_or(CalendarError::OutOfRange)
        };
        let last = self.last_day;
        let day = if latest {
            // The last day starting at or before the coordinate.
            let after = self.find_day(coordinate, |c, _, date| c.day_start(date) > coordinate)?;
            let after_starts_later = start
                .checked_add_days(after)
                .is_some_and(|date| self.day_start(date) > coordinate);
            if after_starts_later { after - 1 } else { after }
        } else {
            // The first day ending at or after the coordinate.
            self.find_day(coordinate, |c, day, date| {
                day == last
                    || date
                        .checked_add_days(1)
                        .is_some_and(|next| c.day_start(next) >= coordinate)
            })?
        };
        let date = start
            .checked_add_days(day)
            .ok_or(CalendarError::OutOfRange)?;
        let mut left = coordinate - self.day_start(date);
        if !latest && left <= 0 {
            return at(day, 0);
        }
        for interval in &self.pattern(date).intervals {
            let length = interval.duration().get();
            if left < length || (!latest && left == length) {
                return at(day, i64::from(interval.start.minutes()) + left);
            }
            left -= length;
        }
        // Past the day's work only at the very end of the calendar.
        Ok(self.last)
    }
    #[cfg(test)]
    fn select_by_search(&self, coordinate: i64, latest: bool) -> Result<DateTime, CalendarError> {
        if coordinate < 0 || coordinate > self.coordinate(self.last) {
            return Err(CalendarError::OutOfRange);
        }
        let (mut low, mut high) = (0, self.last.minutes_since(self.first).get());
        while low < high {
            let middle = low.midpoint(high + i64::from(latest));
            let instant = self
                .first
                .checked_add(Minutes(middle))
                .ok_or(CalendarError::OutOfRange)?;
            let rank = self.coordinate(instant);
            if latest {
                if rank <= coordinate {
                    low = middle;
                } else {
                    high = middle - 1;
                }
            } else if rank >= coordinate {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        self.first
            .checked_add(Minutes(low))
            .ok_or(CalendarError::OutOfRange)
    }
    /// Whether the minute beginning at `instant` is work: starts included,
    /// finishes excluded. Use [`Self::prev_work_instant`] for finish boundaries.
    #[must_use]
    pub fn is_work(&self, instant: DateTime) -> bool {
        self.pattern(instant.date())
            .is_work(instant.time().minutes())
    }
    /// The work position of `instant`. Instants with no work between them share
    /// one (TF-025).
    #[must_use]
    pub fn spot(&self, instant: DateTime) -> Spot<'_> {
        Spot {
            calendar: self,
            coordinate: self.coordinate(instant),
        }
    }
    /// Signed work minutes in `[a,b)`; reversing endpoints negates the count.
    #[must_use]
    pub fn work_between(&self, a: DateTime, b: DateTime) -> Minutes {
        Minutes(self.coordinate(b) - self.coordinate(a))
    }
    /// Add signed work minutes. Positive amounts choose the earliest finish;
    /// negative amounts choose the latest start at the target work coordinate.
    /// Zero preserves the input instant, including a nonworking instant.
    ///
    /// # Errors
    /// Returns [`CalendarError::OutOfRange`] when no endpoint is representable.
    pub fn add_work(&self, start: DateTime, amount: Minutes) -> Result<DateTime, CalendarError> {
        if amount.get() == 0 {
            return Ok(start);
        }
        let target = self.spot(start).plus(amount)?;
        self.select(target.coordinate, amount.get() < 0)
    }
    /// First start-eligible work instant at or after `instant`.
    ///
    /// # Errors
    /// Returns a range error if no future work instant is representable.
    pub fn next_work_instant(&self, instant: DateTime) -> Result<DateTime, CalendarError> {
        if self.is_work(instant) {
            return Ok(instant);
        }
        self.spot(instant).start()
    }
    /// Last finish-eligible instant at or before `instant`: the minute ending
    /// there is work. May return a shift finish even though `is_work` is false.
    ///
    /// # Errors
    /// Returns a range error if no earlier work finish is representable.
    pub fn prev_work_instant(&self, instant: DateTime) -> Result<DateTime, CalendarError> {
        if self.finishes_work(instant) {
            return Ok(instant);
        }
        self.spot(instant).finish()
    }
    /// Whether the minute ending at `instant` is work.
    fn finishes_work(&self, instant: DateTime) -> bool {
        instant
            .checked_add(Minutes(-1))
            .is_some_and(|previous| self.is_work(previous))
    }
}
#[cfg(test)]
mod tests;
