//! `totalfloat query`: find activities without reading the whole schedule.
//!
//! Values are the results stored in the file, except `--driving-path-to`, which
//! needs the engine's driving links. Results are paged by default, so a large
//! schedule never lands whole in an agent's context.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::path::Path;

use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
mod accounts;
mod earned;
mod percent;

use totalfloat_calendar::WorkCalendar;
use totalfloat_engine::schedule_with_projects;
use totalfloat_model::{
    Activity, ActivityCode, ActivityId, ActivityStatus, ActivityType, Constraint, ConstraintType,
    CriticalDefinition, DateTime, Minutes, Relationship, RelationshipType, Resource, Schedule,
    UdfSubject, UdfTypeId, UdfValue, WbsId,
};

use crate::error::{CliError, exit, parse_date};
use crate::loaded::Loaded;
use crate::output::{hours_of, print_line, rounded, same_text};

/// How a WBS node's user performance factor (`EE_PF_user`) applies to the
/// budget left, in its estimate to complete (docs/rules/TF-041-earned-value.md).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UserFactor {
    /// The budget left divided by the factor, as an efficiency index.
    #[default]
    Divide,
    /// The budget left times the factor.
    Multiply,
}

impl From<UserFactor> for totalfloat_evm::UserFactor {
    fn from(reading: UserFactor) -> Self {
        match reading {
            UserFactor::Divide => Self::Divide,
            UserFactor::Multiply => Self::Multiply,
        }
    }
}

/// An activity status to select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// `TK_NotStart`.
    NotStarted,
    /// `TK_Active`.
    InProgress,
    /// `TK_Complete`.
    Completed,
}

/// An activity type to select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Task dependent.
    Task,
    /// Resource dependent.
    Resource,
    /// Level of effort.
    Loe,
    /// Start milestone.
    StartMilestone,
    /// Finish milestone.
    FinishMilestone,
    /// WBS summary.
    WbsSummary,
}

/// What to find. Filters combine: an activity has to pass all of them.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches, each one a flag of the command"
)]
#[derive(Debug, Clone, clap::Args, Deserialize, JsonSchema)]
#[group(skip)]
#[serde(default)]
pub struct Query {
    /// Only this project (`proj_id`); default is every project in the file.
    #[arg(long)]
    pub project: Option<u64>,
    /// Activities in this WBS node and everything below it: a node's code, or its path as `wbs_path` shows it.
    #[arg(long)]
    pub wbs: Option<String>,
    /// Activities holding this code value, as `TYPE=VALUE`.
    #[arg(long, value_name = "TYPE=VALUE")]
    pub code: Option<String>,
    /// Activities assigned this resource, by short name or name.
    #[arg(long)]
    pub resource: Option<String>,
    /// Status. Several are allowed.
    #[arg(long, value_enum)]
    pub status: Vec<Status>,
    /// Activity type. Several are allowed.
    #[arg(long = "type", value_enum)]
    #[serde(rename = "type")]
    pub kind: Vec<Kind>,
    /// Total float of at least this many hours.
    #[arg(long)]
    pub float_min: Option<f64>,
    /// Total float of at most this many hours.
    #[arg(long)]
    pub float_max: Option<f64>,
    /// Only critical activities, by the project's definition of critical.
    #[arg(long)]
    pub critical: bool,
    /// Only activities with a primary or secondary constraint.
    #[arg(long)]
    pub constrained: bool,
    /// Early start on or after this date, as `YYYY-MM-DD HH:MM`. Finished work
    /// counts only with `--status completed`, by its actual dates.
    #[arg(long)]
    pub start_from: Option<String>,
    /// Early start on or before this date, or actual start for finished work.
    #[arg(long)]
    pub start_to: Option<String>,
    /// Early finish on or after this date, or actual finish for finished work.
    #[arg(long)]
    pub finish_from: Option<String>,
    /// Early finish on or before this date, or actual finish for finished work.
    #[arg(long)]
    pub finish_to: Option<String>,
    /// Text in the activity's code or name, ignoring case.
    #[arg(long)]
    pub text: Option<String>,
    /// Groups of conditions, of which an activity has to meet at least one
    /// whole group, besides every other filter. Each `--any` is one group:
    /// `query`'s filters as NAME=VALUE, separated by `;`, such as
    /// `--any 'code=Phase=DESIGN;critical=true' --any float_max=0`.
    #[arg(long = "any", value_name = "CONDITIONS", value_parser = group)]
    pub any: Vec<Conditions>,
    /// Predecessors of this activity (`task_code` or `task_id`).
    #[arg(long, value_name = "ACTIVITY", group = "relation")]
    pub preds: Option<String>,
    /// Successors of this activity (`task_code` or `task_id`).
    #[arg(long, value_name = "ACTIVITY", group = "relation")]
    pub succs: Option<String>,
    /// What drives this activity, nearest first. Runs the engine.
    #[arg(long, value_name = "ACTIVITY", group = "relation")]
    pub driving_path_to: Option<String>,
    /// Compare with a baseline: another XER of this project, matched by
    /// Activity ID. Adds the baseline columns.
    #[arg(long, value_name = "FILE")]
    pub baseline: Option<String>,
    /// The baseline file's project (`proj_id`), if it has several.
    #[arg(long, requires = "baseline")]
    pub baseline_project: Option<u64>,
    /// A resource's units per period, actual and remaining, against its
    /// limit, instead of activities: its short name or name. Uses the
    /// stored remaining dates, so schedule the file first.
    #[arg(
        long,
        value_name = "RESOURCE",
        conflicts_with_all = [
            "wbs", "code", "resource", "status", "kind", "float_min", "float_max", "critical",
            "constrained", "start_from", "start_to", "finish_from", "finish_to", "text", "any",
            "preds", "succs", "driving_path_to", "baseline", "columns"
        ]
    )]
    #[serde(skip)]
    #[schemars(skip)]
    pub usage: Option<String>,
    /// Every matching activity as CSV on stdout, for a spreadsheet: a header
    /// row of column names, then a row per activity.
    #[arg(long, conflicts_with_all = ["limit", "offset", "usage", "xlsx", "json"])]
    #[serde(skip)]
    #[schemars(skip)]
    pub csv: bool,
    /// Every matching activity as an Excel workbook, in this new file.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["limit", "offset", "usage", "json"])]
    #[serde(skip)]
    #[schemars(skip)]
    pub xlsx: Option<std::path::PathBuf>,
    /// With `--usage`, the period: `week` (from Monday) or `month`.
    #[arg(long, value_enum, requires = "usage")]
    #[serde(skip)]
    #[schemars(skip)]
    pub by: Option<crate::usage::By>,
    /// How a WBS node's user performance factor (`EE_PF_user`) applies to the
    /// budget left in the estimate to complete: `divide` (the default), as
    /// public estimate-at-completion formulas divide by an efficiency index, or
    /// `multiply`. A file doesn't say which way its factor reads.
    #[arg(
        long,
        value_enum,
        default_value_t = UserFactor::Divide,
        conflicts_with_all = ["usage", "by_cost_account"]
    )]
    pub user_factor: UserFactor,
    /// Add up the matching activities' costs by the cost account they're
    /// booked to.
    #[arg(long, conflicts_with_all = ["usage", "csv", "xlsx"])]
    pub by_cost_account: bool,
    /// Columns to return, or `all`. On the command line, separate them with commas.
    #[arg(long, value_delimiter = ',')]
    pub columns: Vec<String>,
    /// Rows to return. Zero gives just the count. MCP caps this at 200.
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
    /// Rows to skip.
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            project: None,
            wbs: None,
            code: None,
            resource: None,
            status: Vec::new(),
            kind: Vec::new(),
            float_min: None,
            float_max: None,
            critical: false,
            constrained: false,
            start_from: None,
            start_to: None,
            finish_from: None,
            finish_to: None,
            text: None,
            any: Vec::new(),
            preds: None,
            succs: None,
            driving_path_to: None,
            baseline: None,
            baseline_project: None,
            usage: None,
            csv: false,
            xlsx: None,
            by: None,
            user_factor: UserFactor::Divide,
            by_cost_account: false,
            columns: Vec::new(),
            limit: 50,
            offset: 0,
        }
    }
}

/// One group of `--any`: `query`'s filters, of which an activity has to
/// meet every one to meet the group.
#[allow(
    clippy::struct_excessive_bools,
    reason = "the two switches are filters of their own"
)]
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Conditions {
    /// As `wbs`.
    pub wbs: Option<String>,
    /// As `code`.
    pub code: Option<String>,
    /// As `resource`.
    pub resource: Option<String>,
    /// As `status`.
    pub status: Vec<Status>,
    /// As `type`.
    #[serde(rename = "type")]
    pub kind: Vec<Kind>,
    /// As `float_min`.
    pub float_min: Option<f64>,
    /// As `float_max`.
    pub float_max: Option<f64>,
    /// As `critical`.
    pub critical: bool,
    /// As `constrained`.
    pub constrained: bool,
    /// As `start_from`.
    pub start_from: Option<String>,
    /// As `start_to`.
    pub start_to: Option<String>,
    /// As `finish_from`.
    pub finish_from: Option<String>,
    /// As `finish_to`.
    pub finish_to: Option<String>,
    /// As `text`.
    pub text: Option<String>,
}

impl Conditions {
    /// True if the group holds no condition; blank text counts as none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let blank = |text: &Option<String>| text.as_deref().is_none_or(str::is_empty);
        blank(&self.wbs)
            && blank(&self.code)
            && blank(&self.resource)
            && self.status.is_empty()
            && self.kind.is_empty()
            && self.float_min.is_none()
            && self.float_max.is_none()
            && !self.critical
            && !self.constrained
            && blank(&self.start_from)
            && blank(&self.start_to)
            && blank(&self.finish_from)
            && blank(&self.finish_to)
            && blank(&self.text)
    }

    /// A query that filters by this group alone.
    fn as_query(&self) -> Query {
        Query {
            wbs: self.wbs.clone(),
            code: self.code.clone(),
            resource: self.resource.clone(),
            status: self.status.clone(),
            kind: self.kind.clone(),
            float_min: self.float_min,
            float_max: self.float_max,
            critical: self.critical,
            constrained: self.constrained,
            start_from: self.start_from.clone(),
            start_to: self.start_to.clone(),
            finish_from: self.finish_from.clone(),
            finish_to: self.finish_to.clone(),
            text: self.text.clone(),
            ..Query::default()
        }
    }
}

/// Filters whose value is a list, given with commas between the items.
const LIST_FILTERS: [&str; 2] = ["status", "type"];
/// Filters that are switches: `true` or `false`.
const SWITCH_FILTERS: [&str; 2] = ["critical", "constrained"];
/// Filters whose value is a number.
const NUMBER_FILTERS: [&str; 2] = ["float_min", "float_max"];

/// A group of conditions written as `query`'s parameters, NAME to VALUE, as
/// the app, named filters and `--any` give them.
///
/// # Errors
///
/// `bad-filter`, naming the condition that can't be read.
pub fn conditions(written: &BTreeMap<String, String>) -> Result<Conditions, CliError> {
    let bad = |name: &str, value: &str, hint: &str| {
        CliError::new(
            "bad-filter",
            format!("{name}={value} can't be read as a condition"),
            hint.to_owned(),
        )
    };
    let mut args = Map::new();
    for (name, value) in written {
        let value = value.trim();
        let read = if LIST_FILTERS.contains(&name.as_str()) {
            Value::from(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>(),
            )
        } else if SWITCH_FILTERS.contains(&name.as_str()) {
            match value {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return Err(bad(name, value, "a switch is true or false")),
            }
        } else if NUMBER_FILTERS.contains(&name.as_str()) {
            let number: serde_json::Number = value
                .parse()
                .map_err(|_| bad(name, value, "give hours, such as 40 or -8"))?;
            Value::Number(number)
        } else {
            Value::from(value)
        };
        args.insert(name.clone(), read);
    }
    let read: Conditions = serde_json::from_value(Value::Object(args)).map_err(|error| {
        CliError::new(
            "bad-filter",
            format!("a group of conditions can't be read: {error}"),
            "conditions are query's filters: code, wbs, resource, status, type, float_min, float_max, critical, constrained, start_from, start_to, finish_from, finish_to, text",
        )
    })?;
    if read.is_empty() {
        return Err(CliError::new(
            "bad-filter",
            "a group of conditions holds none",
            "give at least one of query's filters, such as critical=true",
        ));
    }
    Ok(read)
}

/// One `--any` group: conditions as NAME=VALUE separated by `;`, with `\;`
/// a semicolon in a value.
fn group(text: &str) -> Result<Conditions, String> {
    let mut parts = vec![String::new()];
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.as_str().starts_with(';') => {
                parts.last_mut().into_iter().for_each(|p| p.push(';'));
                chars.next();
            }
            ';' => parts.push(String::new()),
            _ => parts.last_mut().into_iter().for_each(|p| p.push(c)),
        }
    }
    let mut written = BTreeMap::new();
    for part in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        let (name, value) = part
            .split_once('=')
            .ok_or_else(|| format!("{part} isn't NAME=VALUE, such as critical=true"))?;
        if written
            .insert(name.trim().to_owned(), value.trim().to_owned())
            .is_some()
        {
            return Err(format!("{} is given twice in one group", name.trim()));
        }
    }
    conditions(&written).map_err(|error| format!("{} ({})", error.message, error.hint))
}

const DEFAULT_COLUMNS: [&str; 7] = [
    "code",
    "name",
    "status",
    "early_start",
    "early_finish",
    "total_float_hours",
    "critical",
];

/// Every column. docs/schemas/query-1.schema.json lists the same names.
const COLUMNS: [&str; 64] = [
    "project",
    "id",
    "code",
    "name",
    "status",
    "type",
    "wbs",
    "wbs_path",
    "obs",
    "calendar",
    "codes",
    "resources",
    "roles",
    "expenses",
    "notes",
    "steps",
    "steps_percent",
    "cost_accounts",
    "early_start",
    "early_finish",
    "late_start",
    "late_finish",
    "actual_start",
    "actual_finish",
    "original_duration_hours",
    "remaining_duration_hours",
    "constraint",
    "constraint_date",
    "secondary_constraint",
    "secondary_constraint_date",
    "total_float_hours",
    "free_float_hours",
    "critical",
    "longest_path",
    "float_path",
    "float_path_order",
    "link_type",
    "link_lag_hours",
    "depth",
    "percent_complete",
    "percent_type",
    "duration_percent",
    "physical_percent",
    "units_percent",
    "budgeted_units",
    "actual_units",
    "remaining_units",
    "budgeted_cost",
    "actual_cost",
    "remaining_cost",
    "at_completion_cost",
    "budgeted_expense_cost",
    "actual_expense_cost",
    "remaining_expense_cost",
    "at_completion_expense_cost",
    "budget_at_completion",
    "planned_value",
    "earned_value",
    "estimate_to_complete",
    "estimate_at_completion",
    "schedule_variance",
    "cost_variance",
    "schedule_performance_index",
    "cost_performance_index",
];

/// Columns that need `--baseline`. Variances are baseline less current, in
/// working hours on the activity's calendar, so a negative one is late.
const BASELINE_COLUMNS: [&str; 6] = [
    "baseline_start",
    "baseline_finish",
    "baseline_duration_hours",
    "start_variance_hours",
    "finish_variance_hours",
    "duration_variance_hours",
];

/// Values an error names before saying how many more there are.
const MAX_HINTED: usize = 30;

/// `totalfloat.query/1`.
#[derive(Debug, Serialize)]
pub struct Report {
    schema: &'static str,
    file: String,
    /// Activities matching, before paging.
    total: usize,
    offset: usize,
    limit: usize,
    returned: usize,
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Map<String, Value>>,
    /// How the activities matched the baseline, with `--baseline`.
    #[serde(skip_serializing_if = "Option::is_none")]
    baseline: Option<BaselineSummary>,
    /// Earned value over every matching activity, with an earned value column.
    #[serde(skip_serializing_if = "Option::is_none")]
    earned_value_total: Option<earned::Total>,
    /// Costs by cost account over every matching activity, with
    /// `--by-cost-account`.
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_accounts: Option<Vec<accounts::AccountCosts>>,
}

/// How a file's activities matched a baseline's, by Activity ID.
#[derive(Debug, Serialize)]
struct BaselineSummary {
    file: String,
    /// The baseline's projects compared, by `proj_id`.
    projects: Vec<u64>,
    /// Activities in both.
    matched: usize,
    /// Activities only in this file, added since the baseline.
    only_here: usize,
    /// Activities only in the baseline, deleted since.
    only_in_baseline: usize,
}

/// A baseline activity's dates and planned duration.
#[derive(Debug, Clone, Copy)]
struct Planned {
    start: Option<DateTime>,
    finish: Option<DateTime>,
    duration: Minutes,
    /// Its planned dates and budget, for planned value.
    plan: earned::Plan,
}

/// A baseline: for each current project, its baseline project's activities
/// by Activity ID.
struct Baseline {
    activities: BTreeMap<u64, BTreeMap<String, Planned>>,
    summary: BaselineSummary,
}

/// A WBS node's codes from the top of the tree, joined with `.`, which tell
/// apart nodes that share a code. A node whose parents loop gets the path as
/// far as the loop.
pub(crate) fn wbs_path(schedule: &Schedule, id: totalfloat_model::WbsId) -> String {
    let mut codes = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut at = Some(id);
    while let Some(node) = at.and_then(|id| schedule.wbs.get(&id)) {
        if !seen.insert(node.id) {
            break;
        }
        codes.push(node.code.as_str());
        at = node.parent;
    }
    codes.reverse();
    codes.join(".")
}

/// An activity's start: actual, or else early.
pub(crate) fn start_of(a: &Activity) -> Option<DateTime> {
    a.actual_start.or(a.stored.early_start)
}

pub(crate) fn finish_of(a: &Activity) -> Option<DateTime> {
    a.actual_finish.or(a.stored.early_finish)
}

/// The baseline project for each selected project: the one named, or else
/// the one `diff` pairs it with, by short name and then by ID.
fn baseline_pairs<'a>(
    query: &Query,
    file: &str,
    (baseline, all): (&'a [Schedule], &'a [Schedule]),
    selected: &[&'a Schedule],
) -> Result<Vec<(&'a Schedule, &'a Schedule)>, CliError> {
    if let Some(id) = query.baseline_project {
        let Some(held) = baseline.iter().find(|s| s.project.id.0 == id) else {
            return Err(CliError::new(
                "no-such-project",
                format!("the baseline {file} has no project {id}"),
                "`inspect` lists the projects in the baseline",
            ));
        };
        return match selected {
            [one] => Ok(vec![(held, *one)]),
            _ => Err(CliError::new(
                "needs-project",
                "one baseline project can't stand for several projects here",
                "name this file's project with --project too",
            )),
        };
    }
    let (pairs, ..) = crate::diff::pairs(baseline, all);
    let mut kept = Vec::new();
    for current in selected {
        let Some(pair) = pairs
            .iter()
            .find(|(_, now)| now.project.id == current.project.id)
        else {
            return Err(CliError::new(
                "needs-project",
                format!(
                    "no project in the baseline {file} pairs with {}",
                    current.project.short_name
                ),
                "name one with --baseline-project, and this file's with --project",
            ));
        };
        kept.push(*pair);
    }
    Ok(kept)
}

/// A baseline activity, with its duration at completion.
fn planned(
    schedule: &Schedule,
    a: &Activity,
    budgets: &BTreeMap<ActivityId, percent::Totals>,
    calendars: &mut Calendars,
) -> Planned {
    let calendar = calendar_for(schedule, a, calendars);
    Planned {
        start: start_of(a),
        finish: finish_of(a),
        duration: calendar
            .and_then(|c| at_completion(c, a, schedule.project.data_date))
            .unwrap_or(a.original_duration),
        // With a baseline, its budget is the plan, even when that's nothing.
        plan: earned::Plan {
            budget: Some(budgets.get(&a.id).map_or(0.0, |t| t.budgeted_cost)),
            ..earned::own_plan(a)
        },
    }
}

/// Reads the baseline the query names, and counts how the selected projects'
/// activities match it. A baseline that lost rows in mapping is refused,
/// because a lost row would read as a deletion.
fn baseline(
    query: &Query,
    (all, selected): (&[Schedule], &[&Schedule]),
    earning: bool,
) -> Result<Option<Baseline>, CliError> {
    let Some(file) = &query.baseline else {
        return Ok(None);
    };
    let loaded = Loaded::new(crate::location::read(Path::new(file))?);
    let pairs = baseline_pairs(query, file, (&loaded.schedules, all), selected)?;
    let mut activities = BTreeMap::new();
    let mut calendars = Calendars::new();
    let (mut matched, mut only_here, mut only_in_baseline) = (0, 0, 0);
    let mut projects = Vec::new();
    for (was, now) in pairs {
        loaded.refuse_incomplete(was)?;
        projects.push(was.project.id.0);
        let budgets = if earning {
            percent::totals(&[was])
        } else {
            BTreeMap::new()
        };
        let held: BTreeMap<String, Planned> = was
            .activities
            .values()
            .map(|a| (a.code.clone(), planned(was, a, &budgets, &mut calendars)))
            .collect();
        let here = now
            .activities
            .values()
            .filter(|a| held.contains_key(&a.code))
            .count();
        matched += here;
        only_here += now.activities.len() - here;
        only_in_baseline += held.len() - here;
        activities.insert(now.project.id.0, held);
    }
    Ok(Some(Baseline {
        activities,
        summary: BaselineSummary {
            file: file.clone(),
            projects,
            matched,
            only_here,
            only_in_baseline,
        },
    }))
}

/// Compiled calendars, by project and calendar.
pub(crate) type Calendars = BTreeMap<(u64, u64), Option<WorkCalendar>>;

/// A project's calendar, compiled once. `None` if it's missing or unreadable.
pub(crate) fn compiled<'c>(
    schedule: &Schedule,
    id: totalfloat_model::CalendarId,
    calendars: &'c mut Calendars,
) -> Option<&'c WorkCalendar> {
    calendars
        .entry((schedule.project.id.0, id.0))
        .or_insert_with(|| WorkCalendar::new(schedule.calendars.get(&id)?).ok())
        .as_ref()
}

fn calendar_for<'c>(
    schedule: &Schedule,
    a: &Activity,
    calendars: &'c mut Calendars,
) -> Option<&'c WorkCalendar> {
    compiled(schedule, a.calendar, calendars)
}

/// Working time from `current` to `planned` on `calendar`: positive when the
/// baseline is later, so a negative variance is late.
pub(crate) fn variance_minutes(
    calendar: &WorkCalendar,
    planned: DateTime,
    current: DateTime,
) -> Minutes {
    if planned >= current {
        calendar.work_between(current, planned)
    } else {
        Minutes::new(-calendar.work_between(planned, current).get())
    }
}

/// [`variance_minutes`] as a column, empty if a date or the calendar is missing.
fn variance(
    calendar: Option<&WorkCalendar>,
    planned: Option<DateTime>,
    current: Option<DateTime>,
) -> Value {
    let (Some(calendar), Some(planned), Some(current)) = (calendar, planned, current) else {
        return Value::Null;
    };
    hours(variance_minutes(calendar, planned, current))
}

/// An activity's duration at completion: actual working time plus remaining.
fn at_completion(
    calendar: &WorkCalendar,
    a: &Activity,
    data_date: Option<DateTime>,
) -> Option<Minutes> {
    match (a.actual_start, a.actual_finish) {
        (Some(start), Some(finish)) => Some(calendar.work_between(start, finish)),
        (Some(start), None) => {
            let so_far = calendar.work_between(start, data_date?.max(start));
            Some(Minutes::new(so_far.get() + a.remaining_duration.get()))
        }
        _ => Some(a.remaining_duration),
    }
}

/// The baseline columns for one activity, in [`BASELINE_COLUMNS`] order.
fn baseline_cells(found: &Found<'_>, baseline: &Baseline, calendars: &mut Calendars) -> [Value; 6] {
    let Found {
        schedule,
        activity: a,
        ..
    } = found;
    let held = baseline.activities.get(&schedule.project.id.0);
    let Some(planned) = held.and_then(|h| h.get(&a.code)) else {
        return std::array::from_fn(|_| Value::Null);
    };
    let calendar = calendar_for(schedule, a, calendars);
    let duration = calendar
        .and_then(|c| at_completion(c, a, schedule.project.data_date))
        .map_or(Value::Null, |now| {
            hours(Minutes::new(planned.duration.get() - now.get()))
        });
    [
        date(planned.start),
        date(planned.finish),
        hours(planned.duration),
        variance(calendar, planned.start, start_of(a)),
        variance(calendar, planned.finish, finish_of(a)),
        duration,
    ]
}

/// An activity's place in a relationship query.
#[derive(Debug, Clone, Copy, Default)]
struct Relation<'a> {
    link: Option<&'a Relationship>,
    depth: Option<usize>,
}

/// Each activity's assigned resources and roles, by short name, its expense
/// items, and the cost accounts its costs are booked to.
#[derive(Default)]
struct Assigned<'a> {
    resources: BTreeMap<ActivityId, Vec<&'a str>>,
    roles: BTreeMap<ActivityId, Vec<&'a str>>,
    expenses: BTreeMap<ActivityId, Vec<&'a str>>,
    /// By path (`totalfloat_edit::cost_account_path`), which names one
    /// account where short names repeat.
    cost_accounts: BTreeMap<ActivityId, Vec<String>>,
}

fn assigned<'a>(schedules: &[&'a Schedule]) -> Assigned<'a> {
    let mut out = Assigned::default();
    for s in schedules {
        for assignment in s.assignments.values() {
            if let Some(resource) = assignment.resource.and_then(|r| s.resources.get(&r)) {
                out.resources
                    .entry(assignment.activity)
                    .or_default()
                    .push(resource.short_name.as_str());
            }
            if let Some(role) = assignment.role.and_then(|r| s.roles.get(&r)) {
                out.roles
                    .entry(assignment.activity)
                    .or_default()
                    .push(role.short_name.as_str());
            }
        }
    }
    for s in schedules {
        for x in s.expenses.values() {
            out.expenses
                .entry(x.activity)
                .or_default()
                .push(x.name.as_str());
        }
        let booked = s
            .assignments
            .values()
            .map(|a| (a.activity, a.cost_account))
            .chain(s.expenses.values().map(|x| (x.activity, x.cost_account)));
        for (activity, account) in booked {
            if let Some(path) = account.and_then(|id| totalfloat_edit::cost_account_path(s, id)) {
                out.cost_accounts.entry(activity).or_default().push(path);
            }
        }
    }
    let lists = out.resources.values_mut().chain(out.roles.values_mut());
    for names in lists.chain(out.expenses.values_mut()) {
        names.sort_unstable();
        names.dedup();
    }
    for paths in out.cost_accounts.values_mut() {
        paths.sort_unstable();
        paths.dedup();
    }
    out
}

fn code_cell(schedule: &Schedule, a: &Activity) -> Value {
    let mut held: Vec<(i64, String)> = a
        .codes
        .iter()
        .filter_map(|(kind, code)| {
            let kind = schedule.activity_code_types.get(kind)?;
            let code = schedule.activity_codes.get(code)?;
            Some((kind.sequence, format!("{}={}", kind.name, code.value)))
        })
        .collect();
    held.sort();
    let held: Vec<String> = held.into_iter().map(|(_, text)| text).collect();
    Value::from(held.join(", "))
}

#[derive(Clone, Copy)]
struct Found<'a> {
    schedule: &'a Schedule,
    activity: &'a Activity,
    relation: Relation<'a>,
}

fn hours(minutes: Minutes) -> Value {
    Value::from(hours_of(minutes))
}

fn date(value: Option<DateTime>) -> Value {
    value.map_or(Value::Null, |date| Value::from(date.to_string()))
}

pub(crate) const fn status_label(status: ActivityStatus) -> &'static str {
    match status {
        ActivityStatus::NotStarted => "not-started",
        ActivityStatus::InProgress => "in-progress",
        ActivityStatus::Completed => "completed",
    }
}

pub(crate) const fn type_label(kind: ActivityType) -> &'static str {
    match kind {
        ActivityType::TaskDependent => "task",
        ActivityType::ResourceDependent => "resource",
        ActivityType::LevelOfEffort => "loe",
        ActivityType::StartMilestone => "start-milestone",
        ActivityType::FinishMilestone => "finish-milestone",
        ActivityType::WbsSummary => "wbs-summary",
    }
}

pub(crate) const fn constraint_label(kind: ConstraintType) -> &'static str {
    match kind {
        ConstraintType::StartOn => "start-on",
        ConstraintType::StartOnOrAfter => "start-on-or-after",
        ConstraintType::StartOnOrBefore => "start-on-or-before",
        ConstraintType::FinishOn => "finish-on",
        ConstraintType::FinishOnOrAfter => "finish-on-or-after",
        ConstraintType::FinishOnOrBefore => "finish-on-or-before",
        ConstraintType::AsLateAsPossible => "as-late-as-possible",
        ConstraintType::MandatoryStart => "mandatory-start",
        ConstraintType::MandatoryFinish => "mandatory-finish",
    }
}

fn constraint(value: Option<Constraint>) -> Value {
    value.map_or(Value::Null, |c| Value::from(constraint_label(c.kind)))
}

pub(crate) const fn link_label(kind: RelationshipType) -> &'static str {
    match kind {
        RelationshipType::FinishToStart => "FS",
        RelationshipType::StartToStart => "SS",
        RelationshipType::FinishToFinish => "FF",
        RelationshipType::StartToFinish => "SF",
    }
}

/// True if the project's own definition makes this activity critical, from
/// its stored results. On total float, completed work is never critical
/// (TF-026), whatever float it keeps.
#[must_use]
pub fn critical(schedule: &Schedule, activity: &Activity) -> bool {
    match schedule.project.options.critical {
        CriticalDefinition::TotalFloatAtMost(limit) => {
            activity.status != ActivityStatus::Completed
                && activity.stored.total_float.is_some_and(|f| f <= limit)
        }
        CriticalDefinition::LongestPath => activity.stored.on_longest_path == Some(true),
    }
}

/// An activity's notes as text, topic by topic in the topics' order, each as
/// `TOPIC: TEXT`, a blank line between them.
fn notes_cell(schedule: &Schedule, a: &Activity) -> Value {
    let mut held: Vec<_> = a
        .notes
        .iter()
        .filter_map(|(topic, note)| Some((schedule.notebook_topics.get(topic)?, note)))
        .collect();
    held.sort_by_key(|(topic, _)| (topic.sequence, topic.id));
    let notes: Vec<String> = held
        .into_iter()
        .map(|(topic, note)| format!("{}: {}", topic.name, note.text()))
        .collect();
    Value::from(notes.join("\n\n"))
}

/// The activity field a label or `udf_type_id` means here.
fn activity_udf(schedule: &Schedule, wanted: &str) -> Option<UdfTypeId> {
    totalfloat_edit::udf_named(schedule, wanted, &UdfSubject::Activity).ok()
}

/// An activity's value of one activity code type, by the type's name (any
/// case): the value it holds, or null.
fn code_type_cell(schedule: &Schedule, a: &Activity, kind: &str) -> Value {
    a.codes
        .iter()
        .find(|(held, _)| {
            schedule
                .activity_code_types
                .get(held)
                .is_some_and(|t| same_text(&t.name, kind))
        })
        .and_then(|(_, code)| schedule.activity_codes.get(code))
        .map_or(Value::Null, |code| Value::from(code.value.clone()))
}

/// An activity's value of a user-defined field, as JSON.
fn udf_cell(schedule: &Schedule, a: &Activity, field: &str) -> Value {
    let Some(value) = activity_udf(schedule, field).and_then(|id| a.udfs.get(&id)) else {
        return Value::Null;
    };
    match value {
        UdfValue::Text(text) => Value::from(text.clone()),
        UdfValue::Number(number) => Value::from(*number),
        UdfValue::Date(date) => Value::from(date.to_string()),
        UdfValue::Code(code) => Value::from(code.0),
    }
}

fn cell(found: &Found<'_>, column: &str, resources: &Assigned<'_>) -> Value {
    let Found {
        schedule,
        activity: a,
        relation,
    } = found;
    let stored = &a.stored;
    let wbs = schedule.wbs.get(&a.wbs).map(|node| node.code.clone());
    match column {
        "project" => Value::from(schedule.project.id.0),
        "id" => Value::from(a.id.0),
        "code" => Value::from(a.code.clone()),
        "name" => Value::from(a.name.clone()),
        "status" => Value::from(status_label(a.status)),
        "type" => Value::from(type_label(a.activity_type)),
        "wbs" => wbs.map_or(Value::Null, Value::from),
        "wbs_path" => Value::from(wbs_path(schedule, a.wbs)),
        "obs" => schedule
            .wbs
            .get(&a.wbs)
            .and_then(|node| node.obs)
            .and_then(|id| schedule.obs.get(&id))
            .map_or(Value::Null, |node| Value::from(node.name.clone())),
        "calendar" => schedule
            .calendars
            .get(&a.calendar)
            .map_or(Value::Null, |c| Value::from(c.name.clone())),
        "codes" => code_cell(schedule, a),
        "notes" => notes_cell(schedule, a),
        udf if udf.starts_with("udf:") => udf_cell(schedule, a, &udf["udf:".len()..]),
        kind if kind.starts_with("code:") => code_type_cell(schedule, a, &kind["code:".len()..]),
        "steps" => {
            let mut steps: Vec<_> = a.steps.values().collect();
            steps.sort_by_key(|s| (s.sequence, s.id));
            let steps: Vec<String> = steps
                .iter()
                .map(|s| {
                    format!(
                        "{} {}%",
                        s.name,
                        (s.percent_complete() * 100.0).round() / 100.0
                    )
                })
                .collect();
            Value::from(steps.join(", "))
        }
        "steps_percent" => a
            .steps_percent()
            .map_or(Value::Null, |p| Value::from((p * 100.0).round() / 100.0)),
        "resources" => Value::from(
            resources
                .resources
                .get(&a.id)
                .map_or_else(String::new, |r| r.join(", ")),
        ),
        "roles" => Value::from(
            resources
                .roles
                .get(&a.id)
                .map_or_else(String::new, |r| r.join(", ")),
        ),
        "expenses" => Value::from(
            resources
                .expenses
                .get(&a.id)
                .map_or_else(String::new, |x| x.join(", ")),
        ),
        "cost_accounts" => Value::from(
            resources
                .cost_accounts
                .get(&a.id)
                .map_or_else(String::new, |x| x.join(", ")),
        ),
        "early_start" => date(stored.early_start),
        "early_finish" => date(stored.early_finish),
        "late_start" => date(stored.late_start),
        "late_finish" => date(stored.late_finish),
        "actual_start" => date(a.actual_start),
        "actual_finish" => date(a.actual_finish),
        "original_duration_hours" => hours(a.original_duration),
        "remaining_duration_hours" => hours(a.remaining_duration),
        "constraint" => constraint(a.primary_constraint),
        "constraint_date" => date(a.primary_constraint.and_then(|c| c.date)),
        "secondary_constraint" => constraint(a.secondary_constraint),
        "secondary_constraint_date" => date(a.secondary_constraint.and_then(|c| c.date)),
        "total_float_hours" => stored.total_float.map_or(Value::Null, hours),
        "free_float_hours" => stored.free_float.map_or(Value::Null, hours),
        "critical" => Value::from(critical(schedule, a)),
        "longest_path" => stored.on_longest_path.map_or(Value::Null, Value::from),
        "float_path" => stored.float_path.map_or(Value::Null, Value::from),
        "float_path_order" => stored.float_path_order.map_or(Value::Null, Value::from),
        "link_type" => relation
            .link
            .map_or(Value::Null, |l| Value::from(link_label(l.kind))),
        "link_lag_hours" => relation.link.map_or(Value::Null, |l| hours(l.lag)),
        "depth" => relation.depth.map_or(Value::Null, Value::from),
        _ => Value::Null,
    }
}

/// The filters with their text already checked against the file.
struct Filter<'a> {
    query: std::borrow::Cow<'a, Query>,
    /// The `--any` groups, each a filter of its own; an activity has to pass
    /// one of them, when there are any.
    any: Vec<Filter<'a>>,
    wbs: Option<BTreeSet<WbsId>>,
    code: Option<BTreeSet<totalfloat_model::ActivityCodeId>>,
    resource: Option<BTreeSet<ActivityId>>,
    start_from: Option<DateTime>,
    start_to: Option<DateTime>,
    finish_from: Option<DateTime>,
    finish_to: Option<DateTime>,
    text: Option<String>,
}

fn optional_date(text: Option<&String>) -> Result<Option<DateTime>, CliError> {
    text.map(|text| parse_date(text)).transpose()
}

/// The WBS nodes `code` names, and every node below them. `code` is a node's
/// own code, which picks every node that has it, or its path from the top of
/// the tree as `wbs_path` gives it, which picks that node alone though another
/// branch has a node with the same code.
fn wbs_nodes(schedules: &[&Schedule], code: &str) -> Result<BTreeSet<WbsId>, CliError> {
    let mut nodes: BTreeSet<WbsId> = schedules
        .iter()
        .flat_map(|s| {
            s.wbs
                .values()
                .filter(move |node| node.code == code || wbs_path(s, node.id) == code)
        })
        .map(|node| node.id)
        .collect();
    if nodes.is_empty() {
        return Err(CliError::new(
            "no-such-wbs",
            format!("no WBS node has the code or path {code}"),
            "give a node's code, or its path from the top as the WBS path column shows it",
        ));
    }
    // Parents come in any order, so repeat until a pass adds nothing.
    loop {
        let before = nodes.len();
        for node in schedules.iter().flat_map(|s| s.wbs.values()) {
            if node.parent.is_some_and(|parent| nodes.contains(&parent)) {
                nodes.insert(node.id);
            }
        }
        if nodes.len() == before {
            return Ok(nodes);
        }
    }
}

fn code_values(
    schedules: &[&Schedule],
    wanted: &str,
) -> Result<BTreeSet<totalfloat_model::ActivityCodeId>, CliError> {
    let unknown = || {
        CliError::new(
            "no-such-code",
            format!("no activity code matches {wanted}"),
            format!("use TYPE=VALUE; {}", known_codes(schedules)),
        )
    };
    let (kind, value) = wanted.split_once('=').ok_or_else(unknown)?;
    if value.is_empty() {
        return Err(unknown());
    }
    let holding = |part: fn(&ActivityCode) -> &str| -> BTreeSet<_> {
        schedules
            .iter()
            .flat_map(|s| {
                s.activity_codes.values().filter(move |code| {
                    same_text(part(code), value)
                        && s.activity_code_types
                            .get(&code.code_type)
                            .is_some_and(|t| same_text(&t.name, kind))
                })
            })
            .map(|code| code.id)
            .collect()
    };
    // A value is exact. Its description is only a fallback, so one code can't
    // silently take in another.
    let mut values = holding(|code| &code.value);
    if values.is_empty() {
        values = holding(|code| &code.description);
    }
    if values.is_empty() {
        return Err(unknown());
    }
    Ok(values)
}

/// The file's code types and values, for an error to list.
fn known_codes(schedules: &[&Schedule]) -> String {
    let mut known: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for s in schedules {
        for code in s.activity_codes.values() {
            if let Some(kind) = s.activity_code_types.get(&code.code_type) {
                known
                    .entry(kind.name.as_str())
                    .or_default()
                    .insert(code.value.as_str());
            }
        }
    }
    if known.is_empty() {
        return "this file has no activity codes".to_owned();
    }
    let list: Vec<String> = known
        .iter()
        .map(|(kind, values)| {
            let shown: Vec<&str> = values.iter().take(MAX_HINTED).copied().collect();
            let more = values.len().saturating_sub(MAX_HINTED);
            let more = if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            format!("{kind}: {}{more}", shown.join(", "))
        })
        .collect();
    format!("this file has {}", list.join("; "))
}

fn resource_holders(
    schedules: &[&Schedule],
    wanted: &str,
) -> Result<BTreeSet<ActivityId>, CliError> {
    let is_it = |r: &Resource| same_text(&r.short_name, wanted) || same_text(&r.name, wanted);
    let known = schedules.iter().any(|s| s.resources.values().any(is_it));
    if !known {
        let names: BTreeSet<&str> = schedules
            .iter()
            .flat_map(|s| s.resources.values())
            .map(|r| r.short_name.as_str())
            .collect();
        let shown: Vec<&str> = names.iter().take(MAX_HINTED).copied().collect();
        return Err(CliError::new(
            "no-such-resource",
            format!("no resource matches {wanted}"),
            if names.is_empty() {
                "this file has no resources".to_owned()
            } else {
                format!(
                    "this file has {} resources, including {}",
                    names.len(),
                    shown.join(", ")
                )
            },
        ));
    }
    Ok(schedules
        .iter()
        .flat_map(|s| {
            s.assignments.values().filter(|a| {
                a.resource
                    .and_then(|r| s.resources.get(&r))
                    .is_some_and(is_it)
            })
        })
        .map(|a| a.activity)
        .collect())
}

impl<'a> Filter<'a> {
    fn new(
        query: &'a Query,
        schedules: &[&Schedule],
        every: &[&Schedule],
    ) -> Result<Self, CliError> {
        let any = query
            .any
            .iter()
            .map(|group| Self::of(std::borrow::Cow::Owned(group.as_query()), schedules, every))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            any,
            ..Self::of(std::borrow::Cow::Borrowed(query), schedules, every)?
        })
    }

    fn of(
        query: std::borrow::Cow<'a, Query>,
        schedules: &[&Schedule],
        every: &[&Schedule],
    ) -> Result<Self, CliError> {
        Ok(Self {
            any: Vec::new(),
            wbs: query
                .wbs
                .as_deref()
                .map(|code| wbs_nodes(schedules, code))
                .transpose()?,
            code: query
                .code
                .as_deref()
                .map(|wanted| code_values(schedules, wanted))
                .transpose()?,
            resource: query
                .resource
                .as_deref()
                .map(|wanted| resource_holders(every, wanted))
                .transpose()?,
            start_from: optional_date(query.start_from.as_ref())?,
            start_to: optional_date(query.start_to.as_ref())?,
            finish_from: optional_date(query.finish_from.as_ref())?,
            finish_to: optional_date(query.finish_to.as_ref())?,
            text: query.text.as_ref().map(|text| text.to_lowercase()),
            query,
        })
    }

    fn passes(&self, schedule: &Schedule, a: &Activity) -> bool {
        if self.any.is_empty() {
            return self.meets(schedule, a, false);
        }
        // Asking for completed work, outside a group or in it, lets the
        // date filters on either side find finished work. This filter's own
        // answer depends only on that, so it's found at most twice.
        let completed = |f: &Filter| f.query.status.contains(&Status::Completed);
        let mut mine = [None, None];
        self.any.iter().any(|group| {
            let asked = completed(group);
            *mine[usize::from(asked)].get_or_insert_with(|| self.meets(schedule, a, asked))
                && group.meets(schedule, a, completed(self))
        })
    }

    /// Whether the activity meets this filter's own conditions; `completed`
    /// when completed work is asked for alongside them.
    fn meets(&self, schedule: &Schedule, a: &Activity, completed: bool) -> bool {
        let q = &*self.query;
        let within = |date: Option<DateTime>, from: Option<DateTime>, to: Option<DateTime>| {
            (from.is_none() && to.is_none())
                || date.is_some_and(|d| from.is_none_or(|f| d >= f) && to.is_none_or(|t| d <= t))
        };
        // Finished work stores the data date as its early dates (TF-020), so
        // it's only found by asking for it, and then by when it happened.
        let finished = a.status == ActivityStatus::Completed;
        let (start, finish) = if finished {
            (a.actual_start, a.actual_finish)
        } else {
            (a.stored.early_start, a.stored.early_finish)
        };
        let dated = self.start_from.is_some()
            || self.start_to.is_some()
            || self.finish_from.is_some()
            || self.finish_to.is_some();
        let float = a.stored.total_float.map(hours).and_then(|h| h.as_f64());
        let at_least = q
            .float_min
            .is_none_or(|min| float.is_some_and(|f| f >= min));
        let at_most = q
            .float_max
            .is_none_or(|max| float.is_some_and(|f| f <= max));
        self.wbs.as_ref().is_none_or(|w| w.contains(&a.wbs))
            && self
                .code
                .as_ref()
                .is_none_or(|c| a.codes.values().any(|code| c.contains(code)))
            && self.resource.as_ref().is_none_or(|r| r.contains(&a.id))
            && (q.status.is_empty() || q.status.iter().any(|s| matches(*s, a.status)))
            && (q.kind.is_empty() || q.kind.iter().any(|k| is_kind(*k, a.activity_type)))
            && at_least
            && at_most
            && (!q.critical || critical(schedule, a))
            && (!q.constrained
                || a.primary_constraint.is_some()
                || a.secondary_constraint.is_some())
            && (!dated || !finished || completed || q.status.contains(&Status::Completed))
            && within(start, self.start_from, self.start_to)
            && within(finish, self.finish_from, self.finish_to)
            && self.text.as_ref().is_none_or(|text| {
                a.code.to_lowercase().contains(text) || a.name.to_lowercase().contains(text)
            })
    }
}

const fn matches(wanted: Status, status: ActivityStatus) -> bool {
    matches!(
        (wanted, status),
        (Status::NotStarted, ActivityStatus::NotStarted)
            | (Status::InProgress, ActivityStatus::InProgress)
            | (Status::Completed, ActivityStatus::Completed)
    )
}

const fn is_kind(wanted: Kind, kind: ActivityType) -> bool {
    matches!(
        (wanted, kind),
        (Kind::Task, ActivityType::TaskDependent)
            | (Kind::Resource, ActivityType::ResourceDependent)
            | (Kind::Loe, ActivityType::LevelOfEffort)
            | (Kind::StartMilestone, ActivityType::StartMilestone)
            | (Kind::FinishMilestone, ActivityType::FinishMilestone)
            | (Kind::WbsSummary, ActivityType::WbsSummary)
    )
}

fn no_activity(text: &str) -> CliError {
    CliError::new(
        "no-such-activity",
        format!("no activity matches {text}"),
        "use the activity's `task_code` or `task_id`",
    )
}

/// Finds an activity by code, or by ID if no code matches.
pub fn find<'a>(
    schedules: &[&'a Schedule],
    text: &str,
) -> Result<(&'a Schedule, &'a Activity), CliError> {
    let by_code: Vec<_> = schedules
        .iter()
        .flat_map(|s| s.activities.values().map(move |a| (*s, a)))
        .filter(|(_, a)| a.code == text)
        .collect();
    match by_code.as_slice() {
        [one] => return Ok(*one),
        [] => {}
        _ => {
            return Err(CliError::new(
                "ambiguous-activity",
                format!("{} activities have the code {text}", by_code.len()),
                "name the project, or use the activity's `task_id`",
            ));
        }
    }
    let id = text
        .parse()
        .map(ActivityId)
        .map_err(|_| no_activity(text))?;
    schedules
        .iter()
        .find_map(|s| s.activities.get(&id).map(|a| (*s, a)))
        .ok_or_else(|| no_activity(text))
}

/// The other ends of `target`'s links, in relationship order, in any project.
fn neighbors<'a>(
    schedules: &'a [Schedule],
    target: &Activity,
    schedule: &'a Schedule,
    predecessors: bool,
) -> Vec<Found<'a>> {
    let mut found = Vec::new();
    for link in schedule.relationships.values() {
        let (near, far) = if predecessors {
            (link.successor, link.predecessor)
        } else {
            (link.predecessor, link.successor)
        };
        if near != target.id {
            continue;
        }
        let other = schedules
            .iter()
            .find_map(|s| s.activities.get(&far).map(|a| (s, a)));
        if let Some((schedule, activity)) = other {
            found.push(Found {
                schedule,
                activity,
                relation: Relation {
                    link: Some(link),
                    depth: None,
                },
            });
        }
    }
    found
}

/// Everything that drives `target`, nearest first, ties in ID order.
fn driving_path<'a>(
    all: &[Schedule],
    schedule: &'a Schedule,
    target: &Activity,
) -> Result<Vec<Found<'a>>, CliError> {
    let result = schedule_with_projects(schedule, &schedule.project.options, all).map_err(|e| {
        CliError::new(
            "schedule-refused",
            format!("project {}: {e}", schedule.project.id.0),
            "a driving path needs a calculated schedule; listing predecessors works without one",
        )
    })?;
    let mut seen = BTreeSet::from([target.id]);
    let mut pending = VecDeque::from([(target.id, 0)]);
    let mut found = Vec::new();
    while let Some((id, depth)) = pending.pop_front() {
        let Some(activity) = result.activities.get(&id) else {
            continue;
        };
        let mut links: Vec<_> = activity
            .driving_relationships
            .iter()
            .filter_map(|link| schedule.relationships.get(link))
            .collect();
        links.sort_by_key(|link| link.predecessor);
        for link in links {
            if !seen.insert(link.predecessor) {
                continue;
            }
            pending.push_back((link.predecessor, depth + 1));
            if let Some(activity) = schedule.activities.get(&link.predecessor) {
                found.push(Found {
                    schedule,
                    activity,
                    relation: Relation {
                        link: Some(link),
                        depth: Some(depth + 1),
                    },
                });
            }
        }
    }
    Ok(found)
}

fn columns(query: &Query) -> Result<Vec<String>, CliError> {
    let with_baseline = query.baseline.is_some();
    if query.columns.is_empty() {
        let mut names: Vec<String> = DEFAULT_COLUMNS.map(str::to_owned).to_vec();
        if with_baseline {
            let shown = ["baseline_start", "baseline_finish", "finish_variance_hours"];
            names.extend(shown.map(str::to_owned));
        }
        if query.preds.is_some() || query.succs.is_some() || query.driving_path_to.is_some() {
            names.extend(["link_type", "link_lag_hours"].map(str::to_owned));
        }
        if query.driving_path_to.is_some() {
            names.push("depth".to_owned());
        }
        return Ok(names);
    }
    if query.columns == ["all"] {
        let extra = if with_baseline {
            &BASELINE_COLUMNS[..]
        } else {
            &[]
        };
        return Ok(COLUMNS
            .iter()
            .chain(extra)
            .map(|c| (*c).to_owned())
            .collect());
    }
    for name in &query.columns {
        if ["udf:", "code:"].iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|field| !field.is_empty())
        }) {
            continue;
        }
        if BASELINE_COLUMNS.contains(&name.as_str()) && !with_baseline {
            return Err(CliError::new(
                "needs-baseline",
                format!("the {name} column needs a baseline"),
                "give --baseline FILE, or baseline over MCP",
            ));
        }
        if !COLUMNS.contains(&name.as_str()) && !BASELINE_COLUMNS.contains(&name.as_str()) {
            return Err(CliError::new(
                "no-such-column",
                format!("there's no column {name}"),
                format!(
                    "columns: {}, udf:FIELD for a user-defined field, and code:TYPE for an activity code type; with --baseline also {}",
                    COLUMNS.join(", "),
                    BASELINE_COLUMNS.join(", ")
                ),
            ));
        }
    }
    Ok(query.columns.clone())
}

fn select<'a>(
    all: &'a [Schedule],
    schedules: &[&'a Schedule],
    query: &Query,
    filter: &Filter<'_>,
    loaded: &Loaded,
) -> Result<Vec<Found<'a>>, CliError> {
    let found = if let Some(text) = &query.preds {
        let (schedule, target) = find(schedules, text)?;
        neighbors(all, target, schedule, true)
    } else if let Some(text) = &query.succs {
        let (schedule, target) = find(schedules, text)?;
        neighbors(all, target, schedule, false)
    } else if let Some(text) = &query.driving_path_to {
        let (schedule, target) = find(schedules, text)?;
        loaded.refuse_incomplete(schedule)?;
        driving_path(all, schedule, target)?
    } else {
        schedules
            .iter()
            .flat_map(|schedule| {
                schedule.activities.values().map(move |activity| Found {
                    schedule,
                    activity,
                    relation: Relation::default(),
                })
            })
            .collect()
    };
    Ok(found
        .into_iter()
        .filter(|f| filter.passes(f.schedule, f.activity))
        .collect())
}

fn human(report: &Report) -> String {
    let mut out = table(report);
    if let Some(t) = &report.earned_value_total {
        let shown = |value: &Value| match value {
            Value::Null => "-".to_owned(),
            other => other.to_string(),
        };
        let _ = write!(
            out,
            "\nEarned value over {} activities: budget {}, planned {}, earned {}, actual {}, SPI {}, CPI {}",
            t.activities,
            shown(&t.budget_at_completion),
            shown(&t.planned_value),
            shown(&t.earned_value),
            shown(&t.actual_cost),
            shown(&t.schedule_performance_index),
            shown(&t.cost_performance_index)
        );
        if t.unmeasured > 0 {
            let _ = write!(
                out,
                "; {} more use an earned value technique not yet known, and aren't counted",
                t.unmeasured
            );
        }
    }
    if let Some(costs) = &report.cost_accounts {
        out.push_str(&accounts::lines(costs));
    }
    if let Some(b) = &report.baseline {
        let _ = write!(
            out,
            "\nBaseline {}: {} matched, {} only in this file, {} only in the baseline",
            b.file, b.matched, b.only_here, b.only_in_baseline
        );
    }
    out
}

fn table(report: &Report) -> String {
    if report.total == 0 {
        return "No activities match.".to_owned();
    }
    if report.returned == 0 {
        return format!(
            "{} match, none shown at this limit and offset",
            report.total
        );
    }
    let text = |row: &Map<String, Value>, name: &String| match row.get(name) {
        // Notes hold line breaks, which a table row can't.
        Some(Value::String(text)) => text.replace("\n\n", " | ").replace('\n', " / "),
        Some(Value::Null) | None => "-".to_owned(),
        Some(Value::Number(number)) if number.is_f64() => number
            .as_f64()
            .map_or_else(String::new, |n| rounded(n).to_string()),
        Some(other) => other.to_string(),
    };
    let widths: BTreeMap<&String, usize> = report
        .columns
        .iter()
        .map(|name| {
            let widest = report.rows.iter().map(|r| text(r, name).chars().count());
            (name, widest.max().unwrap_or(0).max(name.len()))
        })
        .collect();
    let mut out = String::new();
    let line = |out: &mut String, cells: Vec<String>| {
        let padded: Vec<_> = cells
            .iter()
            .zip(&report.columns)
            .map(|(cell, name)| format!("{cell:<width$}", width = widths[name]))
            .collect();
        let _ = writeln!(out, "{}", padded.join("  ").trim_end());
    };
    line(&mut out, report.columns.clone());
    for row in &report.rows {
        let cells = report.columns.iter().map(|name| text(row, name)).collect();
        line(&mut out, cells);
    }
    let (first, last) = (report.offset + 1, report.offset + report.returned);
    let _ = write!(out, "Showing {first}-{last} of {}", report.total);
    out
}

/// Rules on the arguments alone. Clap can't enforce them for MCP.
fn check(query: &Query) -> Result<(), CliError> {
    if query.baseline_project.is_some() && query.baseline.is_none() {
        return Err(CliError::new(
            "needs-baseline",
            "a baseline project needs a baseline file",
            "give --baseline FILE, or baseline over MCP",
        ));
    }
    if query.any.iter().any(Conditions::is_empty) {
        return Err(CliError::new(
            "bad-filter",
            "a group of conditions holds none",
            "give each group at least one of query's filters, such as critical=true",
        ));
    }
    let relations = [&query.preds, &query.succs, &query.driving_path_to];
    if relations.iter().filter(|r| r.is_some()).count() > 1 {
        return Err(CliError::new(
            "conflicting-relations",
            "predecessors, successors and a driving path can't be asked for together",
            "make one call for each",
        ));
    }
    Ok(())
}

/// Refuses a `udf:` or `code:` column that names no field or code type in
/// the projects asked about.
fn named_fields(selected: &[&Schedule], names: &[String]) -> Result<(), CliError> {
    for field in names.iter().filter_map(|n| n.strip_prefix("udf:")) {
        let known = selected.iter().any(|s| activity_udf(s, field).is_some());
        if !known {
            let mut labels: Vec<&str> = selected
                .iter()
                .flat_map(|s| s.udf_types.values())
                .filter(|t| t.subject == UdfSubject::Activity)
                .map(|t| t.label.as_str())
                .collect();
            labels.sort_unstable();
            labels.dedup();
            return Err(CliError::new(
                "no-such-udf",
                format!("no activity field is named {field}"),
                if labels.is_empty() {
                    "the file has no activity fields".to_owned()
                } else {
                    format!("activity fields: {}", labels.join(", "))
                },
            ));
        }
    }
    for kind in names.iter().filter_map(|n| n.strip_prefix("code:")) {
        let types = || {
            selected
                .iter()
                .flat_map(|s| s.activity_code_types.values())
                .map(|t| t.name.as_str())
        };
        if !types().any(|name| same_text(name, kind)) {
            let mut names: Vec<&str> = types().collect();
            names.sort_unstable();
            names.dedup();
            return Err(CliError::new(
                "no-such-code-type",
                format!("no activity code type is named {kind}"),
                if names.is_empty() {
                    "the file has no activity codes".to_owned()
                } else {
                    format!("code types: {}", names.join(", "))
                },
            ));
        }
    }
    Ok(())
}

/// Finds activities in a file that's already been read.
pub fn report(file: &str, loaded: &Loaded, query: &Query) -> Result<Report, CliError> {
    check(query)?;
    let names = columns(query)?;
    let schedules = &loaded.schedules;
    let selected: Vec<&Schedule> = schedules
        .iter()
        .filter(|s| query.project.is_none_or(|id| s.project.id.0 == id))
        .collect();
    if let Some(id) = query.project.filter(|_| selected.is_empty()) {
        return Err(CliError::new(
            "no-such-project",
            format!("the file has no project {id}"),
            "`inspect` lists the projects in the file",
        ));
    }
    named_fields(&selected, &names)?;
    // Logic can lead to another project, so assignments are read from them all.
    let every: Vec<&Schedule> = schedules.iter().collect();
    let filter = Filter::new(query, &selected, &every)?;
    let found = select(schedules, &selected, query, &filter, loaded)?;
    let resources = if names.iter().any(|name| {
        matches!(
            name.as_str(),
            "resources" | "roles" | "expenses" | "cost_accounts"
        )
    }) {
        assigned(&every)
    } else {
        Assigned::default()
    };
    let earning = names.iter().any(|n| earned::COLUMNS.contains(&n.as_str()));
    let baseline = baseline(query, (schedules, &selected), earning)?;
    let (rows, earned_value_total) = rows(
        &names,
        &found,
        query,
        (&resources, baseline.as_ref()),
        &every,
    );
    Ok(Report {
        schema: "totalfloat.query/1",
        file: file.to_owned(),
        total: found.len(),
        offset: query.offset,
        limit: query.limit,
        returned: rows.len(),
        columns: names,
        rows,
        baseline: baseline.map(|b| b.summary),
        earned_value_total,
        cost_accounts: query.by_cost_account.then(|| accounts::by_account(&found)),
    })
}

/// The page of rows, and earned value over every match if a column asks for it.
fn rows(
    names: &[String],
    found: &[Found<'_>],
    query: &Query,
    (resources, baseline): (&Assigned<'_>, Option<&Baseline>),
    every: &[&Schedule],
) -> (Vec<Map<String, Value>>, Option<earned::Total>) {
    let asks = |columns: &[&str]| names.iter().any(|n| columns.contains(&n.as_str()));
    let wanted = asks(&BASELINE_COLUMNS);
    let earning = asks(&earned::COLUMNS);
    let totals = if earning || asks(&percent::COLUMNS) {
        percent::totals(every)
    } else {
        BTreeMap::new()
    };
    let mut calendars = Calendars::new();
    // Every match counts toward the total, not just the page.
    let measured: Vec<Result<totalfloat_evm::Measures, earned::Unmeasured>> = if earning {
        found
            .iter()
            .map(|f| {
                let held = baseline
                    .and_then(|b| b.activities.get(&f.schedule.project.id.0))
                    .and_then(|h| h.get(&f.activity.code));
                // With a baseline, work it didn't plan has no planned value.
                let plan = match (baseline, held) {
                    (_, Some(planned)) => planned.plan,
                    (Some(_), None) => earned::Plan {
                        budget: Some(0.0),
                        ..earned::own_plan(f.activity)
                    },
                    (None, None) => earned::own_plan(f.activity),
                };
                let calendar = calendar_for(f.schedule, f.activity, &mut calendars);
                earned::measures(
                    f.schedule,
                    f.activity,
                    totals.get(&f.activity.id),
                    (plan, calendar),
                    query.user_factor.into(),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let total = earning.then(|| {
        let counted: Vec<_> = measured
            .iter()
            .filter_map(|m| m.as_ref().ok())
            .copied()
            .collect();
        let unnamed = measured
            .iter()
            .filter(|m| **m == Err(earned::Unmeasured::Technique))
            .count();
        earned::total(&counted, unnamed)
    });
    let rows = found
        .iter()
        .enumerate()
        .skip(query.offset)
        .take(query.limit)
        .map(|(at, f)| {
            let planned = baseline
                .filter(|_| wanted)
                .map(|b| baseline_cells(f, b, &mut calendars));
            names
                .iter()
                .map(|name| {
                    let column = BASELINE_COLUMNS.iter().position(|c| c == name);
                    let value = match (column, &planned) {
                        (Some(column), Some(cells)) => cells[column].clone(),
                        _ if earned::COLUMNS.contains(&name.as_str()) => {
                            earned::cell(name, measured.get(at).and_then(|m| m.as_ref().ok()))
                        }
                        _ if percent::COLUMNS.contains(&name.as_str()) => {
                            percent::cell(name, f.activity, totals.get(&f.activity.id))
                        }
                        _ => cell(f, name, resources),
                    };
                    (name.clone(), value)
                })
                .collect()
        })
        .collect();
    (rows, total)
}

/// Runs the command.
pub fn run(file: &Path, query: &Query, json: bool) -> Result<u8, CliError> {
    if let Some(resource) = &query.usage {
        let raw = crate::location::read(file)?;
        let label = file.display().to_string();
        let by = query.by.unwrap_or_default();
        let report = crate::usage::report(&label, &Loaded::new(raw), resource, by, query.project)?;
        if json {
            print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
        } else {
            print_line(&crate::usage::human(&report));
        }
        return Ok(exit::OK);
    }
    check(query)?;
    columns(query)?;
    if query.csv || query.xlsx.is_some() {
        if let Some(output) = &query.xlsx {
            crate::output::refuse_existing(output)?;
        }
        // An export is every match, not a page of them.
        let mut every = query.clone();
        every.limit = usize::MAX;
        every.offset = 0;
        let raw = crate::location::read(file)?;
        let report = report(&file.display().to_string(), &Loaded::new(raw), &every)?;
        if let Some(output) = &query.xlsx {
            crate::export::xlsx(&report.columns, &report.rows, output)?;
            print_line(&format!(
                "Wrote {} activities to {}",
                report.rows.len(),
                output.display()
            ));
        } else {
            crate::output::print(&crate::export::csv(&report.columns, &report.rows));
        }
        return Ok(exit::OK);
    }
    let raw = crate::location::read(file)?;
    let report = report(&file.display().to_string(), &Loaded::new(raw), query)?;
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&human(&report));
    }
    Ok(exit::OK)
}
