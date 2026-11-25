//! Calendars: work weeks, exceptions and hours per period.

use std::collections::BTreeMap;

use crate::{CalendarId, CalendarType, Date, Minutes, ProjectId, SourceRef, TimeOfDay, Weekday};

/// A work calendar. XER: a `CALENDAR` row; the work week and exceptions are
/// encoded in `CALENDAR.clndr_data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    /// XER: `CALENDAR.clndr_id`.
    pub id: CalendarId,
    /// XER: `CALENDAR.clndr_name`.
    pub name: String,
    /// XER: `CALENDAR.clndr_type`.
    pub kind: CalendarType,
    /// Owning project for project calendars. XER: `CALENDAR.proj_id`.
    pub project: Option<ProjectId>,
    /// Base calendar this one inherits from (resource calendars). XER:
    /// `CALENDAR.base_clndr_id`.
    pub base: Option<CalendarId>,
    /// The database's default calendar. XER: `CALENDAR.default_flag`.
    pub is_default: bool,
    /// Conversion factors for displaying hours as days, weeks, months and years.
    pub hours_per_period: HoursPerPeriod,
    /// Standard working times for each day of the week.
    pub work_week: WorkWeek,
    /// Dates whose working times differ from the work week. An empty list
    /// means a non-work day.
    pub exceptions: BTreeMap<Date, Vec<WorkInterval>>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// Work time per period, used only for display conversions (P6 stores
/// durations in hours and shows them in days using these factors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HoursPerPeriod {
    /// XER: `CALENDAR.day_hr_cnt` (decimal hours).
    pub day: Option<Minutes>,
    /// XER: `CALENDAR.week_hr_cnt` (decimal hours).
    pub week: Option<Minutes>,
    /// XER: `CALENDAR.month_hr_cnt` (decimal hours).
    pub month: Option<Minutes>,
    /// XER: `CALENDAR.year_hr_cnt` (decimal hours).
    pub year: Option<Minutes>,
}

/// Working intervals for each day of the week, Sunday first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkWeek(pub [Vec<WorkInterval>; 7]);

impl WorkWeek {
    /// The working intervals on `day` (empty for a non-work day).
    #[must_use]
    pub fn day(&self, day: Weekday) -> &[WorkInterval] {
        &self.0[day.index()]
    }

    /// Mutable access to the intervals on `day`.
    pub fn day_mut(&mut self, day: Weekday) -> &mut Vec<WorkInterval> {
        &mut self.0[day.index()]
    }
}

/// A span of working time within one day, from `start` to `finish`.
/// `finish` may be `24:00`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkInterval {
    /// Start of work.
    pub start: TimeOfDay,
    /// End of work (exclusive).
    pub finish: TimeOfDay,
}

impl WorkInterval {
    /// The interval, or `None` unless `start < finish`.
    #[must_use]
    pub fn new(start: TimeOfDay, finish: TimeOfDay) -> Option<Self> {
        (start < finish).then_some(Self { start, finish })
    }

    /// Working minutes in the interval.
    #[must_use]
    #[inline]
    pub fn duration(self) -> Minutes {
        Minutes::new(i64::from(self.finish.minutes()) - i64::from(self.start.minutes()))
    }
}
