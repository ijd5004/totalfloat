//! `totalfloat.usage/1`: a resource's units per week or month, actual and
//! remaining, against its limit.
//!
//! Each assignment's remaining units are spread over its activity's
//! remaining dates as stored, and its actual units over its actual dates,
//! by the assignment's curve or typed spread, on the calendar its work runs
//! on (TF-039). The limit is the resource's maximum units per hour in force
//! at the period's start, over the work hours of the resource's calendar in
//! the period.

use std::collections::BTreeMap;

use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use totalfloat_calendar::WorkCalendar;
use totalfloat_engine::{
    Distribution, Period, assignment_calendar, remaining_distribution, spread,
};
use totalfloat_model::{
    Activity, Assignment, CalendarId, Date, DateTime, Resource, ResourceId, ResourceType, Schedule,
    TimeOfDay, Weekday,
};

use crate::error::CliError;
use crate::loaded::Loaded;

/// The length of a usage period.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum By {
    /// Weeks, each starting on Monday.
    #[default]
    Week,
    /// Calendar months.
    Month,
}

impl By {
    const fn period(self) -> Period {
        match self {
            Self::Week => Period::Week {
                starts_on: Weekday::Monday,
            },
            Self::Month => Period::Month,
        }
    }
}

/// `totalfloat.usage/1`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Report {
    #[schemars(extend("const" = "totalfloat.usage/1"))]
    schema: &'static str,
    file: String,
    /// The resource.
    resource: Named,
    /// `week` or `month`.
    by: By,
    /// What units are in: `hours` for labor and nonlabor, `units` for material.
    unit: &'static str,
    /// The resource's limit over time, in units per hour (TF-001): the one
    /// in force before any change, then each change, earliest first. Empty
    /// when it has no rate.
    limits: Vec<LimitFrom>,
    /// Every period from the first with units to the last, earliest first.
    periods: Vec<Usage>,
    /// Units over every period.
    totals: Totals,
    /// Assignments of the resource spread into the periods.
    assignments: usize,
    /// Assignments with units that couldn't be spread, each with why.
    not_spread: Vec<NotSpread>,
}

/// A limit and when it takes effect.
#[derive(Debug, PartialEq, Serialize, JsonSchema)]
struct LimitFrom {
    /// When it takes effect, `YYYY-MM-DD HH:MM`; `null` for the limit in
    /// force before the first change, and before every rate starts.
    from: Option<String>,
    /// The most units per hour; `null` for no limit.
    units_per_hour: Option<f64>,
}

/// A resource's limits over time: the rate in force before any has started,
/// then the rate in force from each start date, where its limit differs.
fn limits_of(resource: &Resource) -> Vec<LimitFrom> {
    if resource.rates.is_empty() {
        return Vec::new();
    }
    let mut starts: Vec<DateTime> = resource.rates.iter().filter_map(|r| r.effective).collect();
    starts.sort_unstable();
    starts.dedup();
    let at = |when| resource.rate_at(when).and_then(|r| r.max_units_per_hour);
    let mut limits = vec![LimitFrom {
        from: None,
        units_per_hour: at(None),
    }];
    for when in starts {
        let units_per_hour = at(Some(when));
        if limits
            .last()
            .is_some_and(|l| l.units_per_hour != units_per_hour)
        {
            limits.push(LimitFrom {
                from: Some(when.to_string()),
                units_per_hour,
            });
        }
    }
    limits
}

#[derive(Debug, Serialize, JsonSchema)]
struct Named {
    short_name: String,
    name: String,
}

/// One period's units.
#[derive(Debug, Default, Serialize, JsonSchema)]
struct Usage {
    /// The period's first day, `YYYY-MM-DD`.
    start: String,
    actual: f64,
    remaining: f64,
    /// Actual and remaining together.
    total: f64,
    /// The most the resource can do in the period: its maximum units per
    /// hour over its calendar's work hours. `null` without a maximum.
    limit: Option<f64>,
    /// More than the limit.
    over: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Totals {
    actual: f64,
    remaining: f64,
    total: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct NotSpread {
    /// The Activity ID.
    activity: String,
    /// Why.
    reason: String,
}

pub(crate) fn rounded(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// The resource a name means across the schedules: its short name, or else
/// its name.
fn resource(schedules: &[&Schedule], wanted: &str) -> Result<ResourceId, CliError> {
    let ids: std::collections::BTreeSet<ResourceId> = schedules
        .iter()
        .filter_map(|s| totalfloat_edit::resource_named(s, wanted))
        .collect();
    let mut found = ids.iter();
    match (found.next(), found.next()) {
        (Some(id), None) => Ok(*id),
        (None, _) => {
            let mut names: Vec<&str> = schedules
                .iter()
                .flat_map(|s| s.resources.values())
                .map(|r| r.short_name.as_str())
                .collect();
            names.sort_unstable();
            names.dedup();
            Err(CliError::new(
                "no-such-resource",
                format!("no resource is named {wanted}"),
                format!("resources: {}", names.join(", ")),
            ))
        }
        _ => Err(CliError::new(
            "ambiguous-resource",
            format!("{} resources are named {wanted}", ids.len()),
            "name it by its short name, which no other resource has",
        )),
    }
}

/// Spreads `units` from `start` to `finish` into the totals, by `column`:
/// 0 for actual, 1 for remaining.
fn add(
    totals: &mut BTreeMap<Date, [f64; 2]>,
    (units, column): (f64, usize),
    (start, finish): (DateTime, DateTime),
    distribution: Distribution<'_>,
    (calendar, by): (&WorkCalendar, By),
) -> Result<(), String> {
    let periods = spread(units, start, finish, distribution, calendar, by.period())
        .map_err(|e| e.to_string())?;
    for period in periods {
        totals.entry(period.start).or_default()[column] += period.units;
    }
    Ok(())
}

/// One assignment's actual and remaining units, spread into the totals.
pub(crate) fn spread_assignment(
    schedule: &Schedule,
    (assignment, activity): (&Assignment, &Activity),
    (by, calendars): (By, &mut BTreeMap<CalendarId, WorkCalendar>),
    totals: &mut BTreeMap<Date, [f64; 2]>,
) -> Result<(), String> {
    let id = assignment_calendar(schedule, assignment)
        .filter(|id| schedule.calendars.contains_key(id))
        .ok_or("its calendar isn't in the file")?;
    let calendar = match calendars.entry(id) {
        std::collections::btree_map::Entry::Occupied(held) => held.into_mut(),
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(WorkCalendar::new(&schedule.calendars[&id]).map_err(|e| e.to_string())?)
        }
    };
    let calendar = &*calendar;
    let a = &assignment.amounts;
    let actual = a.actual_units.unwrap_or(0.0) + a.actual_overtime_units.unwrap_or(0.0);
    if actual > 0.0 {
        let start = activity
            .actual_start
            .ok_or("it has actual units and no actual start")?;
        let finish = activity
            .actual_finish
            .or(schedule.project.data_date)
            .ok_or("it has actual units, no actual finish and no data date")?;
        let shape = match &assignment.typed_spread {
            Some(typed) if !typed.actual.is_empty() => Distribution::Typed(&typed.actual),
            _ => Distribution::Even,
        };
        add(
            totals,
            (actual, 0),
            (start, finish.max(start)),
            shape,
            (calendar, by),
        )?;
    }
    let remaining = a.remaining_units.unwrap_or(0.0);
    if remaining > 0.0 {
        // Remaining dates as a pair, or else early dates as a pair, which
        // on started work are the remaining work's too (TF-020).
        let stored = &activity.stored;
        let dates = stored
            .remaining_early_start
            .zip(stored.remaining_early_finish)
            .or_else(|| stored.early_start.zip(stored.early_finish));
        let (start, finish) =
            dates.ok_or("it has remaining units and no remaining dates; schedule it first")?;
        let shape = remaining_distribution(schedule, assignment);
        add(
            totals,
            (remaining, 1),
            (start, finish),
            shape,
            (calendar, by),
        )?;
    }
    Ok(())
}

/// The first day of the period after the one beginning on `first`.
pub(crate) fn next(first: Date, by: By) -> Option<Date> {
    match by {
        By::Week => first.checked_add_days(7),
        By::Month => {
            let (year, month, _) = first.ymd();
            if month == 12 {
                Date::from_ymd(year.checked_add(1)?, 1, 1)
            } else {
                Date::from_ymd(year, month + 1, 1)
            }
        }
    }
}

/// The resource's limit in the period beginning on `first`: its maximum
/// units per hour then, over its calendar's work hours to the next period.
fn limit(
    schedule: &Schedule,
    id: ResourceId,
    calendar: Option<&WorkCalendar>,
    (first, by): (Date, By),
) -> Option<f64> {
    let resource = schedule.resources.get(&id)?;
    limit_by(
        resource.rates.iter().filter_map(|r| r.effective),
        |when| resource.rate_at(Some(when))?.max_units_per_hour,
        calendar?,
        (first, by),
    )
}

/// A limit in the period beginning on `first`: the maximum units per hour
/// in force at each moment, over `calendar`'s work hours to the next period.
/// Rates take effect at `effective`; one taking effect within the period
/// limits the rest of it. `None` if no maximum is in force.
pub(crate) fn limit_by(
    effective: impl Iterator<Item = DateTime>,
    per_hour_at: impl Fn(DateTime) -> Option<f64>,
    calendar: &WorkCalendar,
    (first, by): (Date, By),
) -> Option<f64> {
    let at = |day: Date| day.at(TimeOfDay::MIDNIGHT);
    let start = at(first)?;
    let end = at(next(first, by)?)?;
    let mut cuts: Vec<DateTime> = effective
        .filter(|when| *when > start && *when < end)
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut total = 0.0;
    let mut from = start;
    for to in cuts.into_iter().chain([end]) {
        let per_hour = per_hour_at(from)?;
        #[allow(
            clippy::cast_precision_loss,
            reason = "work minutes are far below 2^52"
        )]
        let hours = calendar.work_between(from, to).get() as f64 / 60.0;
        total += per_hour * hours;
        from = to;
    }
    Some(total)
}

/// The resource's usage over the projects in reach.
///
/// # Errors
///
/// [`CliError`] for a project or resource that isn't there.
pub fn report(
    file: &str,
    loaded: &Loaded,
    wanted: &str,
    by: By,
    project: Option<u64>,
) -> Result<Report, CliError> {
    let schedules: Vec<&Schedule> = loaded
        .schedules
        .iter()
        .filter(|s| project.is_none_or(|id| s.project.id.0 == id))
        .collect();
    if let Some(id) = project.filter(|_| schedules.is_empty()) {
        return Err(CliError::new(
            "no-such-project",
            format!("the file has no project {id}"),
            "`inspect` lists the projects in the file",
        ));
    }
    let id = resource(&schedules, wanted)?;
    let mut totals: BTreeMap<Date, [f64; 2]> = BTreeMap::new();
    let (mut assignments, mut not_spread) = (0, Vec::new());
    for schedule in &schedules {
        let mut calendars = BTreeMap::new();
        for assignment in schedule.assignments.values() {
            if assignment.resource != Some(id) {
                continue;
            }
            let Some(activity) = schedule.activities.get(&assignment.activity) else {
                continue;
            };
            match spread_assignment(
                schedule,
                (assignment, activity),
                (by, &mut calendars),
                &mut totals,
            ) {
                Ok(()) => assignments += 1,
                Err(reason) => not_spread.push(NotSpread {
                    activity: activity.code.clone(),
                    reason,
                }),
            }
        }
    }
    // The resource as the first project holding it has it.
    let holder = schedules
        .iter()
        .find(|s| s.resources.contains_key(&id))
        .copied();
    let resource = holder.and_then(|s| s.resources.get(&id));
    let calendar = holder.and_then(|s| {
        let calendar = resource
            .and_then(|r| r.calendar)
            .or(s.project.default_calendar)?;
        WorkCalendar::new(s.calendars.get(&calendar)?).ok()
    });
    let mut periods = Vec::new();
    if let (Some(&first), Some(&last)) = (totals.keys().next(), totals.keys().next_back()) {
        let mut day = first;
        while day <= last {
            let [actual, remaining] = totals.get(&day).copied().unwrap_or_default();
            let limit = holder.and_then(|s| limit(s, id, calendar.as_ref(), (day, by)));
            let total = actual + remaining;
            periods.push(Usage {
                start: day.to_string(),
                actual: rounded(actual),
                remaining: rounded(remaining),
                total: rounded(total),
                limit: limit.map(rounded),
                over: limit.is_some_and(|l| rounded(total) > rounded(l)),
            });
            let Some(following) = next(day, by) else {
                break;
            };
            day = following;
        }
    }
    let (actual, remaining) = totals
        .values()
        .fold((0.0, 0.0), |(a, r), [x, y]| (a + x, r + y));
    Ok(Report {
        schema: "totalfloat.usage/1",
        file: file.to_owned(),
        resource: Named {
            short_name: resource.map(|r| r.short_name.clone()).unwrap_or_default(),
            name: resource.map(|r| r.name.clone()).unwrap_or_default(),
        },
        by,
        limits: resource.map(limits_of).unwrap_or_default(),
        unit: if resource.is_some_and(|r| r.kind == ResourceType::Material) {
            "units"
        } else {
            "hours"
        },
        periods,
        totals: Totals {
            actual: rounded(actual),
            remaining: rounded(remaining),
            total: rounded(actual + remaining),
        },
        assignments,
        not_spread,
    })
}

/// The report as a table.
#[must_use]
pub fn human(report: &Report) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "{} {} by {}, in {}\n",
        report.resource.short_name,
        report.resource.name,
        match report.by {
            By::Week => "week",
            By::Month => "month",
        },
        report.unit
    );
    let _ = writeln!(
        out,
        "{:<10}  {:>10}  {:>10}  {:>10}  {:>10}",
        "start", "actual", "remaining", "total", "limit"
    );
    for p in &report.periods {
        let limit = p.limit.map_or_else(|| "-".to_owned(), |l| l.to_string());
        let _ = writeln!(
            out,
            "{:<10}  {:>10}  {:>10}  {:>10}  {:>10}{}",
            p.start,
            p.actual,
            p.remaining,
            p.total,
            limit,
            if p.over { "  over" } else { "" }
        );
    }
    let _ = write!(
        out,
        "Total {} actual, {} remaining over {} assignments",
        report.totals.actual, report.totals.remaining, report.assignments
    );
    for skipped in &report.not_spread {
        let _ = write!(out, "\n{} not spread: {}", skipped.activity, skipped.reason);
    }
    out
}
