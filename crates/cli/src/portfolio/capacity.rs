//! `totalfloat portfolio capacity`: each resource's and role's demand
//! across every project of a portfolio against what it can supply, by week
//! or month.
//!
//! Demand is each assignment's units spread as `query --usage` spreads them
//!: actual over the actual dates, remaining over the remaining dates
//! the file stores. A resource across files is one resource when its short
//! name (its Resource ID in P6) is the same; IDs aren't shared between
//! databases. Roles are matched the same way, and count assignments by role
//! that name no resource. Supply is the maximum units per hour in force,
//! over the work hours of the resource's calendar (a role's project's
//! default calendar), from the first project in the portfolio that has it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Serialize;
use totalfloat_calendar::WorkCalendar;
use totalfloat_model::{Date, ResourceType, Schedule};

use super::Portfolio;
use crate::error::CliError;
use crate::usage::{By, limit_by, next, spread_assignment};

/// Two decimal places, and never a negative zero.
fn rounded(value: f64) -> f64 {
    crate::usage::rounded(value) + 0.0
}

/// `totalfloat.portfolio-capacity/1`, documented in
/// docs/schemas/portfolio-capacity-1.schema.json.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Report {
    schema: &'static str,
    file: String,
    by: By,
    /// Resources with any demand, by short name.
    resources: Vec<Line>,
    /// Roles with demand from assignments that name no resource, by short name.
    roles: Vec<Line>,
    /// Assignments counted against nothing, as the resource or role they
    /// name isn't in the file.
    not_counted: Vec<NotSpread>,
}

/// One resource's or role's demand and supply.
#[derive(Debug, Serialize, JsonSchema)]
struct Line {
    short_name: String,
    name: String,
    /// `hours` for labor and nonlabor, `units` for material.
    unit: &'static str,
    /// Every period from the first with demand to the last, earliest first.
    periods: Vec<Period>,
    total_demand: f64,
    /// Periods with more demand than supply.
    periods_over: usize,
    /// The period most over supply, with what each project asks of it then.
    peak: Option<Peak>,
    /// Assignments spread into the periods.
    assignments: usize,
    /// Assignments with units that couldn't be spread, each with why.
    not_spread: Vec<NotSpread>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Period {
    /// The period's first day, `YYYY-MM-DD`.
    start: String,
    /// Actual and remaining units together, across every project.
    demand: f64,
    /// The most it can do in the period; `null` without a maximum.
    supply: Option<f64>,
    over: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Peak {
    start: String,
    demand: f64,
    supply: f64,
    over_by: f64,
    /// Each project's demand in the period, most first.
    projects: Vec<Share>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Share {
    /// The project's code.
    project: String,
    file: String,
    demand: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct NotSpread {
    project: String,
    activity: String,
    reason: String,
}

/// Whether a line is a resource's or a role's.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Resource,
    Role,
}

/// A line's identity across files: its kind, its short name in capitals (a
/// short name matches whatever its case), and whether it counts units of
/// material, which never add to hours.
type Key = (Kind, String, bool);

/// What's gathered for one resource or role.
#[derive(Default)]
struct Gathered {
    /// The short name and name as the first project with demand writes them.
    short: String,
    name: String,
    /// Demand by project (its place in the portfolio) and period start.
    demand: BTreeMap<usize, BTreeMap<Date, [f64; 2]>>,
    assignments: usize,
    not_spread: Vec<NotSpread>,
}

/// Each placed project's place in the portfolio and its schedule.
fn schedules(portfolio: &Portfolio) -> Result<Vec<(usize, &Schedule)>, CliError> {
    portfolio
        .projects
        .iter()
        .enumerate()
        .map(|(place, p)| {
            portfolio
                .files
                .get(&p.source)
                .and_then(|f| f.schedules.iter().find(|s| s.project.id.0 == p.id))
                .map(|s| (place, s))
                .ok_or_else(|| {
                    CliError::new(
                        "portfolio-refused",
                        format!("project {} of {} wasn't kept for capacity", p.id, p.file),
                        "read the portfolio with its schedules kept; this is a bug",
                    )
                })
        })
        .collect()
}

/// The line an assignment counts against: its resource, or its role when
/// it names no resource. `Err` with why when it names one the file lacks.
fn key_of(
    schedule: &Schedule,
    assignment: &totalfloat_model::Assignment,
) -> Result<Option<(Key, String, String)>, String> {
    if let Some(id) = assignment.resource {
        let r = schedule
            .resources
            .get(&id)
            .ok_or("its resource isn't in the file")?;
        let material = r.kind == ResourceType::Material;
        return Ok(Some((
            (Kind::Resource, r.short_name.to_uppercase(), material),
            r.short_name.clone(),
            r.name.clone(),
        )));
    }
    if let Some(id) = assignment.role {
        let r = schedule
            .roles
            .get(&id)
            .ok_or("its role isn't in the file")?;
        return Ok(Some((
            (Kind::Role, r.short_name.to_uppercase(), false),
            r.short_name.clone(),
            r.name.clone(),
        )));
    }
    Ok(None)
}

/// Demand and supply across the portfolio, for every resource and role or
/// only those asked for.
///
/// # Errors
///
/// `no-such-resource` or `no-such-role` for a name no project has demand
/// on, and `portfolio-refused` for a portfolio read without its schedules.
pub fn report(
    file: &str,
    portfolio: &Portfolio,
    by: By,
    (resource, role): (Option<&str>, Option<&str>),
) -> Result<Report, CliError> {
    let schedules = schedules(portfolio)?;
    let mut lines: BTreeMap<Key, Gathered> = BTreeMap::new();
    let mut not_counted = Vec::new();
    for &(place, schedule) in &schedules {
        let mut calendars = BTreeMap::new();
        for assignment in schedule.assignments.values() {
            let Some(activity) = schedule.activities.get(&assignment.activity) else {
                continue;
            };
            let (key, short, name) = match key_of(schedule, assignment) {
                Ok(Some(found)) => found,
                Ok(None) => continue,
                Err(reason) => {
                    not_counted.push(NotSpread {
                        project: schedule.project.short_name.clone(),
                        activity: activity.code.clone(),
                        reason,
                    });
                    continue;
                }
            };
            let line = lines.entry(key).or_insert_with(|| Gathered {
                short,
                name,
                ..Gathered::default()
            });
            let totals = line.demand.entry(place).or_default();
            match spread_assignment(
                schedule,
                (assignment, activity),
                (by, &mut calendars),
                totals,
            ) {
                Ok(()) => line.assignments += 1,
                Err(reason) => line.not_spread.push(NotSpread {
                    project: schedule.project.short_name.clone(),
                    activity: activity.code.clone(),
                    reason,
                }),
            }
        }
    }
    let wanted = |kind: Kind, short: &str| match (kind, resource, role) {
        (_, None, None) => true,
        (Kind::Resource, Some(r), _) | (Kind::Role, _, Some(r)) => r.eq_ignore_ascii_case(short),
        _ => false,
    };
    for (kind, asked, code) in [
        (Kind::Resource, resource, "no-such-resource"),
        (Kind::Role, role, "no-such-role"),
    ] {
        if let Some(asked) = asked
            && !lines
                .keys()
                .any(|(k, short, _)| *k == kind && short.eq_ignore_ascii_case(asked))
        {
            return Err(CliError::new(
                code,
                format!("no project in the portfolio has demand on {asked}"),
                "`portfolio capacity` without it lists every resource and role with demand",
            ));
        }
    }
    // The first project in the portfolio holding each resource and role,
    // assigned or not, supplies it.
    let mut holders: BTreeMap<(Kind, String), &Schedule> = BTreeMap::new();
    for &(_, s) in &schedules {
        for r in s.resources.values() {
            holders
                .entry((Kind::Resource, r.short_name.to_uppercase()))
                .or_insert(s);
        }
        for r in s.roles.values() {
            holders
                .entry((Kind::Role, r.short_name.to_uppercase()))
                .or_insert(s);
        }
    }
    let mut report = Report {
        schema: "totalfloat.portfolio-capacity/1",
        file: file.to_owned(),
        by,
        resources: Vec::new(),
        roles: Vec::new(),
        not_counted,
    };
    for ((kind, upper, material), gathered) in lines {
        if !wanted(kind, &upper) {
            continue;
        }
        let holder = holders.get(&(kind, upper.clone())).copied();
        let supply = holder.and_then(|s| Supply::of(s, (kind, &upper)));
        let line = line_of(portfolio, (supply.as_ref(), material), gathered, by);
        match kind {
            Kind::Resource => report.resources.push(line),
            Kind::Role => report.roles.push(line),
        }
    }
    Ok(report)
}

/// What a resource or role can do: its rates, chosen by `rate_at` as
/// `query --usage` chooses them, and the calendar its hours are counted on,
/// worked out once.
struct Supply {
    holder: Holder,
    calendar: WorkCalendar,
}

enum Holder {
    Resource(totalfloat_model::Resource),
    Role(totalfloat_model::Role),
}

impl Supply {
    /// From the project holding it: a resource on its calendar, a role on
    /// the project's default calendar.
    fn of(s: &Schedule, (kind, upper): (Kind, &str)) -> Option<Self> {
        let (holder, calendar) = match kind {
            Kind::Resource => {
                let r = s
                    .resources
                    .values()
                    .find(|r| r.short_name.to_uppercase() == upper)?;
                (
                    Holder::Resource(r.clone()),
                    r.calendar.or(s.project.default_calendar)?,
                )
            }
            Kind::Role => {
                let r = s
                    .roles
                    .values()
                    .find(|r| r.short_name.to_uppercase() == upper)?;
                (Holder::Role(r.clone()), s.project.default_calendar?)
            }
        };
        Some(Supply {
            holder,
            calendar: WorkCalendar::new(s.calendars.get(&calendar)?).ok()?,
        })
    }

    /// The most it can do in the period starting `day`.
    fn at(&self, (day, by): (Date, By)) -> Option<f64> {
        match &self.holder {
            Holder::Resource(r) => limit_by(
                r.rates.iter().filter_map(|x| x.effective),
                |when| r.rate_at(Some(when))?.max_units_per_hour,
                &self.calendar,
                (day, by),
            ),
            Holder::Role(r) => limit_by(
                r.rates.iter().filter_map(|x| x.effective),
                |when| r.rate_at(Some(when))?.max_units_per_hour,
                &self.calendar,
                (day, by),
            ),
        }
    }
}

/// One resource's or role's periods, supply and peak.
fn line_of(
    portfolio: &Portfolio,
    (supply, material): (Option<&Supply>, bool),
    gathered: Gathered,
    by: By,
) -> Line {
    let total = |place: &BTreeMap<Date, [f64; 2]>, day: Date| {
        place
            .get(&day)
            .map_or(0.0, |[actual, remaining]| actual + remaining)
    };
    let days: Vec<Date> = gathered
        .demand
        .values()
        .flat_map(BTreeMap::keys)
        .copied()
        .collect();
    let (first, last) = (days.iter().min().copied(), days.iter().max().copied());
    let mut periods = Vec::new();
    let mut peak: Option<Peak> = None;
    // The worst shortage so far, unrounded, so close ones compare right.
    let mut worst = f64::NEG_INFINITY;
    if let (Some(first), Some(last)) = (first, last) {
        let mut day = first;
        while day <= last {
            let demand: f64 = gathered
                .demand
                .values()
                .map(|place| total(place, day))
                .sum();
            let supplied = supply.and_then(|s| s.at((day, by)));
            let over = supplied.is_some_and(|s| rounded(demand) > rounded(s));
            if let Some(supplied) = supplied.filter(|_| over) {
                let over_by = demand - supplied;
                if over_by > worst {
                    worst = over_by;
                    let mut projects: Vec<Share> = gathered
                        .demand
                        .iter()
                        .filter_map(|(place, totals)| {
                            let p = portfolio.projects.get(*place)?;
                            let d = total(totals, day);
                            (d > 0.0).then(|| Share {
                                project: p.code.clone(),
                                file: p.file.clone(),
                                demand: rounded(d),
                            })
                        })
                        .collect();
                    projects.sort_by(|a, b| {
                        b.demand
                            .total_cmp(&a.demand)
                            .then(a.project.cmp(&b.project))
                            .then(a.file.cmp(&b.file))
                    });
                    peak = Some(Peak {
                        start: day.to_string(),
                        demand: rounded(demand),
                        supply: rounded(supplied),
                        over_by: rounded(over_by),
                        projects,
                    });
                }
            }
            periods.push(Period {
                start: day.to_string(),
                demand: rounded(demand),
                supply: supplied.map(rounded),
                over,
            });
            let Some(following) = next(day, by) else {
                break;
            };
            day = following;
        }
    }
    let total_demand: f64 = gathered
        .demand
        .values()
        .flat_map(BTreeMap::values)
        .map(|[a, r]| a + r)
        .sum();
    Line {
        short_name: gathered.short,
        name: gathered.name,
        unit: if material { "units" } else { "hours" },
        periods_over: periods.iter().filter(|p| p.over).count(),
        periods,
        total_demand: rounded(total_demand),
        peak,
        assignments: gathered.assignments,
        not_spread: gathered.not_spread,
    }
}

/// The report as text: one line per resource and role, and each period
/// when one is asked for.
#[must_use]
pub fn human(report: &Report, detail: bool) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for (heading, lines) in [("Resources", &report.resources), ("Roles", &report.roles)] {
        if lines.is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            "{heading}, by {}:",
            match report.by {
                By::Week => "week",
                By::Month => "month",
            }
        );
        for line in lines {
            let peak = line.peak.as_ref().map_or_else(
                || "never over".to_owned(),
                |p| {
                    let who: Vec<String> = p
                        .projects
                        .iter()
                        .map(|s| format!("{} in {} {}", s.project, s.file, s.demand))
                        .collect();
                    format!(
                        "over in {} period{}; most in the one starting {}: {} against {} ({})",
                        line.periods_over,
                        if line.periods_over == 1 { "" } else { "s" },
                        p.start,
                        p.demand,
                        p.supply,
                        who.join(", ")
                    )
                },
            );
            let _ = writeln!(
                out,
                "  {} {}: {} {} over {} assignments, {peak}",
                line.short_name, line.name, line.total_demand, line.unit, line.assignments
            );
            if detail {
                for p in &line.periods {
                    let supply = p.supply.map_or_else(|| "-".to_owned(), |s| s.to_string());
                    let _ = writeln!(
                        out,
                        "    {}  {:>10}  {:>10}{}",
                        p.start,
                        p.demand,
                        supply,
                        if p.over { "  over" } else { "" }
                    );
                }
            }
            for skipped in &line.not_spread {
                let _ = writeln!(
                    out,
                    "    {} {} not spread: {}",
                    skipped.project, skipped.activity, skipped.reason
                );
            }
        }
    }
    for skipped in &report.not_counted {
        let _ = writeln!(
            out,
            "{} {} not counted: {}",
            skipped.project, skipped.activity, skipped.reason
        );
    }
    if out.is_empty() {
        "No resource or role has demand in the portfolio".to_owned()
    } else {
        out.trim_end().to_owned()
    }
}
