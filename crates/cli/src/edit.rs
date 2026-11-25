//! `totalfloat edit`: change a schedule, recalculate, and say what the change did.
//!
//! The file is calculated as it stands first, so stale results in it don't count
//! as the edit's impact. The source file is never changed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Serialize;
use serde_json::Value;
use totalfloat_check::{Check, Outcome, Thresholds, assess, assess_calculated};
use totalfloat_diff::compare;
use totalfloat_edit::{
    COMMAND_SCHEMAS_READ, CommandOptions, Commands, ConstraintKind, Edit, EditError,
    ExpenseAccrual, LinkType, Name, NextIds, ResourceKind, Scope, Span, SpreadPeriod, UdfFor,
    UdfKind, activities_named, apply, apply_between, calendar_changed, calendars_named,
    check_loop_across, holds, holds_wbs, resolve, resource_named, suggest, wbs_named,
};
use totalfloat_model::{
    Activity, ActivityId, CalendarId, DateTime, RelationshipId, ResourceId, Schedule,
};
use totalfloat_xer::{PatchError, RawXer, patch_all};

use crate::error::{CliError, Problem, exit, parse_date, read_xer};
use crate::history::{Asked, Head, Recorded, head_of, read_head, read_snapshot, record_edit};
use crate::loaded::Loaded;
use crate::location::{Located, locate};
use crate::output::{create_new_text, permissions_of, print_line, refuse_existing, write_new};
use crate::query::critical;
use crate::schedule::{self, ProjectReport};

/// Items kept in each list of a report. Counts stay complete.
const MAX_LISTED: usize = 25;

/// The edits a command line gives, by kind.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches, each one a flag of the command"
)]
#[derive(Debug, Default, clap::Args)]
pub struct Flags {
    /// Original duration, as `ACTIVITY=15d` or `ACTIVITY=120h`.
    #[arg(long, value_name = "ACTIVITY=SPAN")]
    duration: Vec<String>,
    /// Remaining duration, as `ACTIVITY=5d`.
    #[arg(long, value_name = "ACTIVITY=SPAN")]
    remaining: Vec<String>,
    /// Primary constraint, as `ACTIVITY=start-on-or-after@2026-03-02 08:00`.
    #[arg(long, value_name = "ACTIVITY=KIND@DATE")]
    constraint: Vec<String>,
    /// Secondary constraint, in the same form.
    #[arg(long, value_name = "ACTIVITY=KIND@DATE")]
    secondary_constraint: Vec<String>,
    /// Remove an activity's primary constraint.
    #[arg(long, value_name = "ACTIVITY")]
    clear_constraint: Vec<String>,
    /// Remove an activity's secondary constraint.
    #[arg(long, value_name = "ACTIVITY")]
    clear_secondary_constraint: Vec<String>,
    /// Add a WBS node, as `CODE;NAME;PARENT`.
    #[arg(long, value_name = "CODE;NAME;PARENT")]
    add_wbs: Vec<String>,
    /// Rename a WBS node, as `WBS=NAME`.
    #[arg(long, value_name = "WBS=NAME")]
    rename_wbs: Vec<String>,
    /// Move a WBS node under another, as `WBS=PARENT`.
    #[arg(long, value_name = "WBS=PARENT")]
    move_wbs: Vec<String>,
    /// Move an activity to a WBS node, as `ACTIVITY=WBS`.
    #[arg(long, value_name = "ACTIVITY=WBS")]
    wbs: Vec<String>,
    /// Put a resource on an activity, as `ACTIVITY=RESOURCE`, or with units as
    /// `ACTIVITY=RESOURCE:40` (hours for labor).
    #[arg(long, value_name = "ACTIVITY=RESOURCE[:UNITS]")]
    assign: Vec<String>,
    /// Take a resource off an activity, as `ACTIVITY=RESOURCE`.
    #[arg(long, value_name = "ACTIVITY=RESOURCE")]
    unassign: Vec<String>,
    /// Put a role on an activity with no resource yet, as `ACTIVITY=ROLE`, or
    /// with units as `ACTIVITY=ROLE:40` (hours).
    #[arg(long, value_name = "ACTIVITY=ROLE[:UNITS]")]
    assign_role: Vec<String>,
    /// Take a role with no resource off an activity, as `ACTIVITY=ROLE`.
    #[arg(long, value_name = "ACTIVITY=ROLE")]
    unassign_role: Vec<String>,
    /// Spread a resource's units on an activity by a resource curve, as
    /// `ACTIVITY=RESOURCE;CURVE`.
    #[arg(long, value_name = "ACTIVITY=RESOURCE;CURVE")]
    curve: Vec<String>,
    /// Spread a resource's units on an activity evenly, with no curve or
    /// typed units, as `ACTIVITY=RESOURCE`.
    #[arg(long, value_name = "ACTIVITY=RESOURCE")]
    spread_evenly: Vec<String>,
    /// Type a resource's remaining units on an activity for each day, week or
    /// month from a first day, as `ACTIVITY=RESOURCE;day=2026-03-02;8,8,0,16`.
    #[arg(long, value_name = "ACTIVITY=RESOURCE;PERIOD=DATE;UNITS,...")]
    spread: Vec<String>,
    /// Add an expense, as `ACTIVITY=ITEM;COST`, with a cost account, an
    /// expense category and an accrual (start, end or uniform) after it if
    /// wanted: `ACTIVITY=ITEM;COST;ACCOUNT;CATEGORY;ACCRUAL`, any left empty.
    #[arg(long, value_name = "ACTIVITY=ITEM;COST[;ACCOUNT;CATEGORY;ACCRUAL]")]
    add_expense: Vec<String>,
    /// Set an expense's costs, as `ACTIVITY=ITEM;budget=1800;actual=500;remaining=1300`,
    /// any of the three, and its accrual as `accrual=start`.
    #[arg(long, value_name = "ACTIVITY=ITEM;KIND=COST")]
    set_expense: Vec<String>,
    /// Remove an expense, as `ACTIVITY=ITEM`.
    #[arg(long, value_name = "ACTIVITY=ITEM")]
    delete_expense: Vec<String>,
    /// Book costs to a cost account, as `ACTIVITY=resource:NAME;ACCOUNT`, or
    /// `role:NAME` for a role with no resource, or `expense:ITEM`. Leave the
    /// account empty, as `A1=expense:Permit;`, to book them to none.
    #[arg(long, value_name = "ACTIVITY=KIND:NAME;ACCOUNT")]
    set_cost_account: Vec<String>,
    /// Give an activity a code value, as `ACTIVITY=TYPE=VALUE`.
    #[arg(long, value_name = "ACTIVITY=TYPE=VALUE")]
    code: Vec<String>,
    /// Take a code off an activity, as `ACTIVITY=TYPE`.
    #[arg(long, value_name = "ACTIVITY=TYPE")]
    clear_code: Vec<String>,
    /// Add an activity code type for this project, as `NAME`, or for every
    /// project as `NAME;global`.
    #[arg(long, value_name = "NAME[;global]")]
    add_code_type: Vec<String>,
    /// Give an activity a user-defined field value, as `ACTIVITY=FIELD=VALUE`.
    #[arg(long, value_name = "ACTIVITY=FIELD=VALUE")]
    udf: Vec<String>,
    /// Take a user-defined field value off an activity, as `ACTIVITY=FIELD`.
    #[arg(long, value_name = "ACTIVITY=FIELD")]
    clear_udf: Vec<String>,
    /// Add a user-defined field, as `FIELD;TYPE` for activities or
    /// `FIELD;TYPE;FOR` with FOR one of activity, wbs, resource or assignment.
    /// TYPE is text, number, integer, cost, start-date, finish-date or
    /// indicator.
    #[arg(long, value_name = "FIELD;TYPE[;FOR]")]
    add_udf_type: Vec<String>,
    /// Add a step to an activity, as `ACTIVITY=NAME` or with a weight as
    /// `ACTIVITY=NAME;WEIGHT`.
    #[arg(long, value_name = "ACTIVITY=NAME[;WEIGHT]")]
    add_step: Vec<String>,
    /// Set a step's percent complete, as `ACTIVITY=STEP=PERCENT`.
    #[arg(long, value_name = "ACTIVITY=STEP=PERCENT")]
    step: Vec<String>,
    /// Mark a step complete, as `ACTIVITY=STEP`.
    #[arg(long, value_name = "ACTIVITY=STEP")]
    complete_step: Vec<String>,
    /// Delete a step, as `ACTIVITY=STEP`.
    #[arg(long, value_name = "ACTIVITY=STEP")]
    delete_step: Vec<String>,
    /// Add a note to an activity, as `ACTIVITY=TOPIC=TEXT`.
    #[arg(long, value_name = "ACTIVITY=TOPIC=TEXT")]
    add_note: Vec<String>,
    /// Replace an activity's note, as `ACTIVITY=TOPIC=TEXT`.
    #[arg(long, value_name = "ACTIVITY=TOPIC=TEXT")]
    set_note: Vec<String>,
    /// Delete an activity's note, as `ACTIVITY=TOPIC`.
    #[arg(long, value_name = "ACTIVITY=TOPIC")]
    delete_note: Vec<String>,
    /// Add an OBS node, as `NAME`, or under another as `NAME;PARENT`.
    #[arg(long, value_name = "NAME[;PARENT]")]
    add_obs: Vec<String>,
    /// Make an OBS node a WBS node's responsible manager, as `WBS=OBS`.
    #[arg(long, value_name = "WBS=OBS")]
    wbs_obs: Vec<String>,
    /// Give the project a project code value, as `TYPE=VALUE`.
    #[arg(long, value_name = "TYPE=VALUE")]
    project_code: Vec<String>,
    /// Take a project code off the project, as `TYPE`.
    #[arg(long, value_name = "TYPE")]
    clear_project_code: Vec<String>,
    /// Add a project code type, as `NAME`.
    #[arg(long, value_name = "NAME")]
    add_project_code_type: Vec<String>,
    /// Add a value to a project code type, as `TYPE=VALUE` or
    /// `TYPE=VALUE;DESCRIPTION`.
    #[arg(long, value_name = "TYPE=VALUE")]
    add_project_code_value: Vec<String>,
    /// Add a resource, as `CODE;NAME` or `CODE;NAME;TYPE` with type labor,
    /// nonlabor or material.
    #[arg(long, value_name = "CODE;NAME[;TYPE]")]
    add_resource: Vec<String>,
    /// Set a resource's limit in units per hour, as `RESOURCE=2`, or from a
    /// day on as `RESOURCE=2@2027-01-04`. `RESOURCE=` clears it.
    #[arg(long, value_name = "RESOURCE=UNITS[@DATE]")]
    resource_limit: Vec<String>,
    /// Add a role, as `CODE;NAME`, or under another role as
    /// `CODE;NAME;PARENT`.
    #[arg(long, value_name = "CODE;NAME[;PARENT]")]
    add_role: Vec<String>,
    /// Set a role's limit in units per hour, as `ROLE=2`, or from a day on
    /// as `ROLE=2@2027-01-04`. `ROLE=` clears it.
    #[arg(long, value_name = "ROLE=UNITS[@DATE]")]
    role_limit: Vec<String>,
    /// Add a value to a code type, as `TYPE=VALUE` or `TYPE=VALUE;DESCRIPTION`.
    #[arg(long, value_name = "TYPE=VALUE")]
    add_code_value: Vec<String>,
    /// Make a date a holiday, as `CALENDAR=2026-12-24`.
    #[arg(long, value_name = "CALENDAR=DATE")]
    holiday: Vec<String>,
    /// Put a date back to the normal week, as `CALENDAR=2026-12-24`.
    #[arg(long, value_name = "CALENDAR=DATE")]
    no_holiday: Vec<String>,
    /// A calendar's hours in a day, week, month or year, as
    /// `CALENDAR=week=40`: what spans such as `2w` or `1m` mean on it.
    #[arg(long, value_name = "CALENDAR=PERIOD=HOURS")]
    hours_per: Vec<String>,
    /// Actual start, as `ACTIVITY=2026-03-02 08:00`.
    #[arg(long, value_name = "ACTIVITY=DATE")]
    actual_start: Vec<String>,
    /// Actual finish, as `ACTIVITY=2026-03-06 17:00`.
    #[arg(long, value_name = "ACTIVITY=DATE")]
    actual_finish: Vec<String>,
    /// Physical percent complete, as `ACTIVITY=50`.
    #[arg(long, value_name = "ACTIVITY=PERCENT")]
    percent: Vec<String>,
    /// Take away recorded progress.
    #[arg(long, value_name = "ACTIVITY")]
    clear_progress: Vec<String>,
    /// Suspend work in progress until a resume date, as
    /// `ACTIVITY=2026-01-12 17:00/2026-01-19 08:00`.
    #[arg(long, value_name = "ACTIVITY=SUSPEND/RESUME")]
    suspend: Vec<String>,
    /// Take away an activity's suspend and resume dates.
    #[arg(long, value_name = "ACTIVITY")]
    clear_suspend: Vec<String>,
    /// Add an activity, as `CODE;NAME;WBS;DURATION`. A milestone or a chosen
    /// calendar takes `--edits`.
    #[arg(long, value_name = "CODE;NAME;WBS;DURATION")]
    add: Vec<String>,
    /// Delete an activity, with its links and assignments.
    #[arg(long, value_name = "ACTIVITY")]
    delete: Vec<String>,
    /// Move an activity to a calendar, by name or ID.
    #[arg(long, value_name = "ACTIVITY=CALENDAR")]
    calendar: Vec<String>,
    /// Rename an activity.
    #[arg(long, value_name = "ACTIVITY=NAME")]
    name: Vec<String>,
    /// Add a relationship, as `A>B`, `A>B:SS` or `A>B:FS+5d`.
    #[arg(long, value_name = "LINK")]
    link: Vec<String>,
    /// Remove a relationship, as `A>B` or `A>B:SS`.
    #[arg(long, value_name = "LINK")]
    unlink: Vec<String>,
    /// Change a relationship's lag, as `A>B+2d`, or type and lag, as `A>B:FF+2d`.
    #[arg(long, value_name = "LINK")]
    relink: Vec<String>,
    /// Global change: the activities `query` would find, given its filters
    /// as `KEY=VALUE` pairs separated by `;`, such as
    /// `wbs=CIV;code=Phase=CON;status=not-started`. Each takes every `--set`.
    #[arg(long = "where", value_name = "FILTER", requires = "set")]
    filter: Option<String>,
    /// A change for every activity `--where` finds: `duration=5d`,
    /// `duration+=2d`, `duration-=1d`, `calendar=NAME`,
    /// `constraint=KIND@DATE`, `code=TYPE=VALUE`, `udf=FIELD=VALUE`,
    /// `prefix=TEXT` or `suffix=TEXT`; or `duration`, `remaining_duration`,
    /// `constraint_date` or `udf:FIELD` set to an expression over the
    /// activity's own fields, such as `remaining_duration=original_duration*0.5`.
    #[arg(long, value_name = "FIELD=VALUE", requires = "filter")]
    set: Vec<String>,
    /// Edits made in a spreadsheet, read back: the `.xlsx` or `.csv` that
    /// `query --xlsx` or `--csv` wrote, or a folder of CSV files, with an
    /// Activities sheet keyed by `code` and, if wanted, a Relationships
    /// sheet. Each cell that differs from the schedule becomes an edit.
    #[arg(long, value_name = "SHEET", conflicts_with_all = ["filter", "apply", "skip_failed"])]
    from_sheet: Option<std::path::PathBuf>,
    /// A timesheet a team member returned: the `.xlsx`, `.csv` or folder
    /// that `timesheet` wrote, with hours and progress filled in. Repeat for
    /// each one. `hours` adds to actual units, `remaining_hours` sets
    /// remaining units, and the progress columns set each activity's.
    #[arg(long, value_name = "SHEET", conflicts_with_all = ["filter", "apply", "skip_failed"])]
    from_timesheet: Vec<std::path::PathBuf>,
    /// A JSON array of edits, applied before the flags above.
    #[arg(long, value_name = "FILE")]
    edits: Option<PathBuf>,
    /// Make the schedule match a schedule text file (`.tfs`,
    /// docs/guide/text.md): the difference becomes edits, applied first.
    #[arg(long, value_name = "FILE")]
    apply: Option<PathBuf>,
    /// With --apply, delete the activities and links the text leaves out.
    #[arg(long, requires = "apply", conflicts_with = "keep_missing")]
    delete_missing: bool,
    /// With --apply, keep what the text leaves out.
    #[arg(long, requires = "apply")]
    keep_missing: bool,
    /// A status update as CSV, one activity a row: `activity` and any of
    /// `actual_start`, `actual_finish`, `remaining` and `percent`. Applied first.
    #[arg(long, value_name = "FILE")]
    progress_table: Option<PathBuf>,
    /// Save the edits that applied, with their settings, as a
    /// `totalfloat.command/1` document that `--edits` reads back. A new file.
    #[arg(long, value_name = "FILE")]
    emit_commands: Option<PathBuf>,
    /// On a history store: why, recorded with the change.
    #[arg(short, long)]
    message: Option<String>,
    /// On a history store: who; default `TOTALFLOAT_AUTHOR`, else the
    /// account's user name.
    #[arg(long)]
    author: Option<String>,
    /// Report the impact and write or record nothing.
    #[arg(long, conflicts_with = "output")]
    dry_run: bool,
    /// On a history store: record the change as a proposal for someone to
    /// accept or reject, leaving the branch where it is.
    #[arg(long, conflicts_with = "dry_run")]
    propose: bool,
}

fn invalid(what: &str, text: &str, form: &str) -> CliError {
    CliError::new(
        "invalid-edit",
        format!("{text} isn't a valid {what}"),
        format!("use the form {form}"),
    )
}

/// `--hours-per CALENDAR=PERIOD=HOURS`: hours in a day, week, month or year.
fn hours_per(text: &str) -> Result<Edit, CliError> {
    let form = "CALENDAR=week=40, with day, week, month or year";
    let bad = || invalid("hours-per", text, form);
    let (rest, hours) = text.rsplit_once('=').ok_or_else(bad)?;
    let (calendar, period) = rest.rsplit_once('=').ok_or_else(bad)?;
    let hours: f64 = hours.trim().parse().map_err(|_| bad())?;
    let calendar = calendar.trim().to_owned();
    if calendar.is_empty() {
        return Err(bad());
    }
    let period = match period.trim().to_lowercase().as_str() {
        "day" => return Ok(Edit::SetHoursPerDay { calendar, hours }),
        "week" => totalfloat_edit::Period::Week,
        "month" => totalfloat_edit::Period::Month,
        "year" => totalfloat_edit::Period::Year,
        _ => return Err(bad()),
    };
    Ok(Edit::SetHoursPerPeriod {
        calendar,
        period,
        hours,
    })
}

fn pair<'a>(text: &'a str, what: &str, form: &str) -> Result<(&'a str, &'a str), CliError> {
    text.split_once('=')
        .filter(|(activity, value)| !activity.is_empty() && !value.is_empty())
        .ok_or_else(|| invalid(what, text, form))
}

fn span(text: &str) -> Result<Span, CliError> {
    text.parse()
        .map_err(|_| invalid("span", text, "15d, 120h, 2w, 1m or 1y, with - for a lead"))
}

fn link_type(text: &str) -> Result<LinkType, CliError> {
    serde_json::from_value(Value::from(text.to_uppercase()))
        .map_err(|_| invalid("link type", text, "FS, SS, FF or SF"))
}

fn lag_span(text: &str) -> Result<Span, CliError> {
    span(text.strip_prefix('+').unwrap_or(text))
}

/// `CREW:40` or `CREW:40h` as the resource and its units. A tail that isn't a
/// number is part of the resource's name.
fn with_units(text: &str) -> (&str, Option<f64>) {
    let Some((resource, tail)) = text.rsplit_once(':') else {
        return (text, None);
    };
    let number = tail.strip_suffix('h').unwrap_or(tail);
    // Plain digits only, so `nan` or `1e3` stays part of a name.
    let plain = number.split_once('.').map_or(number, |(whole, _)| whole);
    let digits = number.bytes().all(|b| b.is_ascii_digit() || b == b'.') && !plain.is_empty();
    match number.parse::<f64>() {
        Ok(units) if digits && !resource.is_empty() => (resource, Some(units)),
        _ => (text, None),
    }
}

fn reads_as_type(tail: &str) -> bool {
    match tail.rfind(['+', '-']) {
        Some(at) => link_type(&tail[..at]).is_ok() && lag_span(&tail[at..]).is_ok(),
        None => link_type(tail).is_ok(),
    }
}

/// The parts of `A>B:SS+5d`.
struct Link {
    from: String,
    to: String,
    kind: Option<LinkType>,
    lag: Option<Span>,
}

fn link(text: &str) -> Result<Link, CliError> {
    let form = "A>B, A>B:SS or A>B:FS+5d";
    let (from, rest) = text
        .split_once('>')
        .filter(|(from, rest)| !from.is_empty() && !rest.is_empty())
        .ok_or_else(|| invalid("link", text, form))?;
    // A colon can also be `PROJECT:CODE`, so what follows it has to read as a type.
    let (rest, kind) = match rest.rsplit_once(':') {
        Some((to, tail)) if reads_as_type(tail) => (to, Some(tail)),
        _ => (rest, None),
    };
    // The lag follows the type, or the activity if there's no type.
    let split = |text: &'_ str| -> Option<usize> { text.rfind(['+', '-']) };
    let (to, kind, lag) = match kind {
        Some(tail) => match split(tail) {
            Some(at) => (rest, Some(&tail[..at]), Some(&tail[at..])),
            None => (rest, Some(tail), None),
        },
        None => match split(rest).filter(|at| lag_span(&rest[*at..]).is_ok()) {
            Some(at) => (&rest[..at], None, Some(&rest[at..])),
            None => (rest, None, None),
        },
    };
    if to.is_empty() {
        return Err(invalid("link", text, form));
    }
    Ok(Link {
        from: from.to_owned(),
        to: to.to_owned(),
        kind: kind.map(link_type).transpose()?,
        lag: lag.map(lag_span).transpose()?,
    })
}

pub(crate) fn constraint(text: &str, secondary: bool) -> Result<Edit, CliError> {
    let form = "ACTIVITY=start-on-or-after@2026-03-02 08:00";
    let (activity, value) = pair(text, "constraint", form)?;
    let (kind, date) = match value.split_once('@') {
        Some((kind, date)) => (kind, Some(date.to_owned())),
        None => (value, None),
    };
    Ok(Edit::SetConstraint {
        activity: activity.to_owned(),
        constraint: constraint_kind(kind)?,
        date,
        secondary,
    })
}

/// A constraint kind as the CLI and sheets write it, such as `start-on-or-after`.
pub(crate) fn constraint_kind(kind: &str) -> Result<ConstraintKind, CliError> {
    serde_json::from_value(Value::from(kind)).map_err(|_| {
        invalid(
            "constraint",
            kind,
            "start-on, start-on-or-after, start-on-or-before, finish-on, finish-on-or-after, \
             finish-on-or-before, as-late-as-possible, mandatory-start or mandatory-finish",
        )
    })
}

fn new_activity(text: &str) -> Result<Edit, CliError> {
    let form = "CODE;NAME;WBS;DURATION";
    // The name is in the middle, so it can hold the separator.
    let bad = || invalid("new activity", text, form);
    let (code, rest) = text.split_once(';').ok_or_else(bad)?;
    let (rest, duration) = rest.rsplit_once(';').ok_or_else(bad)?;
    let (name, wbs) = rest.rsplit_once(';').ok_or_else(bad)?;
    Ok(Edit::AddActivity {
        code: code.to_owned(),
        name: name.to_owned(),
        wbs: wbs.to_owned(),
        activity_type: None,
        calendar: None,
        duration: Some(span(duration)?),
    })
}

fn activities_named_any(edits: &[Edit], code: &str) -> bool {
    edits
        .iter()
        .any(|edit| activities_named(edit).contains(&code))
}

fn pair_starts(text: &str, code: &str) -> bool {
    text.split_once('=').is_some_and(|(left, _)| left == code)
}

/// Reads edits from JSON: a `totalfloat.command/1` document, or a bare list
/// of edits.
///
/// # Errors
///
/// `file-not-readable`, `invalid-edit`, or `unsupported-schema` for a
/// document in a schema this build doesn't read.
pub fn read_edits(path: &Path) -> Result<Commands, CliError> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "file-not-readable",
            format!("can't read {}: {error}", path.display()),
            "check the path and that the file is readable",
        )
    })?;
    let not_edits = |error: serde_json::Error| {
        CliError::new(
            "invalid-edit",
            format!("{} isn't a list of edits: {error}", path.display()),
            "docs/guide/edit.md shows the form",
        )
    };
    if text.trim_start().starts_with('[') {
        let edits = serde_json::from_str(&text).map_err(not_edits)?;
        return Ok(Commands::new(edits, CommandOptions::default()));
    }
    // The schema is checked first, so a later version isn't misread as a
    // broken one.
    let schema = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|doc| doc.get("schema")?.as_str().map(str::to_owned));
    if let Some(schema) = schema
        && !COMMAND_SCHEMAS_READ.contains(&schema.as_str())
    {
        return Err(CliError::new(
            "unsupported-schema",
            format!("{} is in schema {schema}", path.display()),
            format!(
                "this build reads {}; a newer TotalFloat may read it",
                COMMAND_SCHEMAS_READ.join(", ")
            ),
        ));
    }
    serde_json::from_str(&text).map_err(|error| {
        CliError::new(
            "invalid-edit",
            format!("{} isn't a command document: {error}", path.display()),
            "docs/schemas/command-1.schema.json gives its form",
        )
    })
}

/// Edits from the command line, each with the flag that gave it, and the
/// flags that couldn't be read.
#[derive(Debug, Default)]
pub struct Built {
    /// The edits, in the order they apply.
    pub edits: Vec<Edit>,
    /// Where each came from, such as `--duration A100=15d`.
    pub sources: Vec<String>,
    /// Flags that couldn't be read.
    pub problems: Vec<Problem>,
    /// The settings an `--edits` document gave.
    pub options: CommandOptions,
}

impl Built {
    fn add(&mut self, source: String, edit: Result<Edit, CliError>) {
        match edit {
            Ok(edit) => {
                self.edits.push(edit);
                self.sources.push(source);
            }
            Err(error) => self.problems.push(Problem {
                edit: None,
                source,
                code: error.code,
                message: error.message,
                hint: error.hint,
                depends_on: None,
                line: None,
                column: None,
            }),
        }
    }

    /// Adds one edit per flag value.
    fn each(
        &mut self,
        flag: &str,
        values: &[String],
        read: impl Fn(&str) -> Result<Edit, CliError>,
    ) {
        for text in values {
            self.add(format!("--{flag} {text}"), read(text));
        }
    }
}

fn add_wbs(text: &str) -> Result<Edit, CliError> {
    let bad = || invalid("WBS node", text, "CODE;NAME;PARENT");
    let (code, rest) = text.split_once(';').ok_or_else(bad)?;
    let (name, parent) = rest.rsplit_once(';').ok_or_else(bad)?;
    Ok(Edit::AddWbs {
        code: code.to_owned(),
        name: name.to_owned(),
        parent: parent.to_owned(),
    })
}

fn add_code_type(text: &str) -> Result<Edit, CliError> {
    let (name, scope) = match text.rsplit_once(';') {
        Some((name, "global")) => (name, Some(Scope::Global)),
        Some((name, "project")) => (name, Some(Scope::Project)),
        Some(_) => return Err(invalid("code type", text, "NAME or NAME;global")),
        None => (text, None),
    };
    Ok(Edit::AddCodeType {
        name: name.to_owned(),
        scope,
        max_length: None,
    })
}

fn add_resource(text: &str) -> Result<Edit, CliError> {
    let form = "CODE;NAME or CODE;NAME;TYPE";
    let bad = || invalid("resource", text, form);
    let (resource, rest) = text.split_once(';').ok_or_else(bad)?;
    let kind = |word: &str| match word {
        "labor" => Some(ResourceKind::Labor),
        "nonlabor" => Some(ResourceKind::Nonlabor),
        "material" => Some(ResourceKind::Material),
        _ => None,
    };
    let (name, resource_type) = match rest.rsplit_once(';') {
        Some((name, word)) => (name, Some(kind(word).ok_or_else(bad)?)),
        None => (rest, None),
    };
    Ok(Edit::AddResource {
        resource: resource.to_owned(),
        name: name.to_owned(),
        resource_type,
        units_per_hour: None,
        calendar: None,
        max_units_per_hour: None,
    })
}

/// `--resource-limit RESOURCE=UNITS[@DATE]`, with no units to clear it.
fn resource_limit(text: &str) -> Result<Edit, CliError> {
    let form = "RESOURCE=2, RESOURCE=2@2027-01-04, or RESOURCE= to clear it";
    let (resource, max_units_per_hour, from) = limit(text, "resource limit", form)?;
    Ok(Edit::SetResourceLimit {
        resource,
        max_units_per_hour,
        from,
    })
}

/// `--role-limit ROLE=UNITS[@DATE]`, with no units to clear it.
fn role_limit(text: &str) -> Result<Edit, CliError> {
    let form = "ROLE=2, ROLE=2@2027-01-04, or ROLE= to clear it";
    let (role, max_units_per_hour, from) = limit(text, "role limit", form)?;
    Ok(Edit::SetRoleLimit {
        role,
        max_units_per_hour,
        from,
    })
}

/// `NAME=UNITS[@DATE]`: who, the units per hour (none to clear), and the day.
fn limit(
    text: &str,
    what: &'static str,
    form: &'static str,
) -> Result<(String, Option<f64>, Option<String>), CliError> {
    let bad = || invalid(what, text, form);
    let (name, rest) = text.split_once('=').ok_or_else(bad)?;
    let (units, from) = match rest.split_once('@') {
        Some((units, from)) => (units.trim(), Some(from.trim().to_owned())),
        None => (rest.trim(), None),
    };
    let max_units_per_hour = if units.is_empty() {
        None
    } else {
        Some(units.parse::<f64>().map_err(|_| bad())?)
    };
    if name.trim().is_empty() || from.as_ref().is_some_and(String::is_empty) {
        return Err(bad());
    }
    Ok((name.trim().to_owned(), max_units_per_hour, from))
}

fn add_role(text: &str) -> Result<Edit, CliError> {
    let bad = || invalid("role", text, "CODE;NAME or CODE;NAME;PARENT");
    let mut parts = text.splitn(3, ';');
    let (Some(role), Some(name)) = (parts.next(), parts.next()) else {
        return Err(bad());
    };
    Ok(Edit::AddRole {
        role: role.to_owned(),
        name: name.to_owned(),
        parent: parts.next().map(str::to_owned),
        max_units_per_hour: None,
    })
}

fn assign_role(text: &str) -> Result<Edit, CliError> {
    let (activity, held) = pair(text, "role assignment", "ACTIVITY=ROLE[:UNITS]")?;
    let (role, units) = with_units(held);
    Ok(Edit::AssignRole {
        activity: activity.to_owned(),
        role: role.to_owned(),
        units,
    })
}

fn curve(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=RESOURCE;CURVE";
    let (activity, rest) = pair(text, "curve", form)?;
    let (resource, curve) = rest
        .split_once(';')
        .filter(|(resource, curve)| !resource.is_empty() && !curve.is_empty())
        .ok_or_else(|| invalid("curve", text, form))?;
    Ok(Edit::SetCurve {
        activity: activity.to_owned(),
        resource: resource.to_owned(),
        curve: Some(curve.to_owned()),
    })
}

fn spread(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=RESOURCE;PERIOD=DATE;UNITS,... with day, week or month";
    let bad = || invalid("spread", text, form);
    let (activity, rest) = pair(text, "spread", form)?;
    let mut parts = rest.split(';');
    let (Some(resource), Some(from), Some(list), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(bad());
    };
    let (period, start) = from.split_once('=').ok_or_else(bad)?;
    let period = SpreadPeriod::from_word(period).ok_or_else(bad)?;
    let units = list
        .split(',')
        .map(|u| u.trim().parse::<f64>().ok().filter(|u| u.is_finite()))
        .collect::<Option<Vec<f64>>>()
        .ok_or_else(bad)?;
    if resource.is_empty() || start.is_empty() {
        return Err(bad());
    }
    Ok(Edit::SetSpread {
        activity: activity.to_owned(),
        resource: resource.to_owned(),
        period,
        start: start.to_owned(),
        units,
    })
}

fn cost(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|c| c.is_finite())
}

fn accrual(word: &str) -> Option<ExpenseAccrual> {
    ExpenseAccrual::from_word(word)
}

fn add_expense(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=ITEM;COST[;ACCOUNT;CATEGORY;ACCRUAL]";
    let bad = || invalid("expense", text, form);
    let (activity, rest) = pair(text, "expense", form)?;
    let mut parts = rest.split(';');
    let (Some(expense), Some(amount)) = (parts.next(), parts.next().and_then(cost)) else {
        return Err(bad());
    };
    let named = |part: Option<&str>| part.filter(|p| !p.is_empty()).map(str::to_owned);
    let (account, category) = (named(parts.next()), named(parts.next()));
    let accrual = match parts.next().unwrap_or("") {
        "" => None,
        word => Some(accrual(word).ok_or_else(bad)?),
    };
    if parts.next().is_some() {
        return Err(bad());
    }
    Ok(Edit::AddExpense {
        activity: activity.to_owned(),
        expense: expense.to_owned(),
        cost: amount,
        account,
        category,
        accrual,
    })
}

fn set_expense(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=ITEM;budget=COST;actual=COST;remaining=COST;accrual=start";
    let bad = || invalid("expense cost", text, form);
    let (activity, rest) = pair(text, "expense cost", form)?;
    let mut parts = rest.split(';');
    let expense = parts.next().ok_or_else(bad)?;
    let (mut budgeted_cost, mut actual_cost, mut remaining_cost) = (None, None, None);
    let mut accrued = None;
    for part in parts {
        let (kind, value) = part.split_once('=').ok_or_else(bad)?;
        if kind == "accrual" {
            accrued = Some(accrual(value).ok_or_else(bad)?);
            continue;
        }
        let slot = match kind {
            "budget" => &mut budgeted_cost,
            "actual" => &mut actual_cost,
            "remaining" => &mut remaining_cost,
            _ => return Err(bad()),
        };
        *slot = Some(cost(value).ok_or_else(bad)?);
    }
    Ok(Edit::SetExpense {
        activity: activity.to_owned(),
        expense: expense.to_owned(),
        budgeted_cost,
        actual_cost,
        remaining_cost,
        accrual: accrued,
    })
}

fn set_cost_account(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=resource:NAME;ACCOUNT, with role: or expense: for NAME";
    let bad = || invalid("cost account", text, form);
    let (activity, rest) = pair(text, "cost account", form)?;
    let (held, account) = rest.rsplit_once(';').ok_or_else(bad)?;
    let (kind, name) = held
        .split_once(':')
        .filter(|(_, name)| !name.is_empty())
        .ok_or_else(bad)?;
    let name = Some(name.to_owned());
    let (resource, role, expense) = match kind {
        "resource" => (name, None, None),
        "role" => (None, name, None),
        "expense" => (None, None, name),
        _ => return Err(bad()),
    };
    Ok(Edit::SetCostAccount {
        activity: activity.to_owned(),
        resource,
        role,
        expense,
        account: (!account.is_empty()).then(|| account.to_owned()),
    })
}

fn add_code_value(text: &str) -> Result<Edit, CliError> {
    let (code_type, rest) = pair(text, "code value", "TYPE=VALUE;DESCRIPTION")?;
    let (value, description) = match rest.split_once(';') {
        Some((value, description)) => (value, Some(description.to_owned())),
        None => (rest, None),
    };
    Ok(Edit::AddCodeValue {
        code_type: code_type.to_owned(),
        value: value.to_owned(),
        description,
    })
}

fn add_project_code_value(text: &str) -> Result<Edit, CliError> {
    let (code_type, rest) = pair(text, "project code value", "TYPE=VALUE;DESCRIPTION")?;
    let (value, description) = match rest.split_once(';') {
        Some((value, description)) => (value, Some(description.to_owned())),
        None => (rest, None),
    };
    Ok(Edit::AddProjectCodeValue {
        code_type: code_type.to_owned(),
        value: value.to_owned(),
        description,
    })
}

fn set_udf(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=FIELD=VALUE";
    let (activity, rest) = pair(text, "field value", form)?;
    let (field, value) = rest
        .split_once('=')
        .filter(|(field, _)| !field.is_empty())
        .ok_or_else(|| invalid("field value", text, form))?;
    Ok(Edit::SetUdf {
        field: field.to_owned(),
        value: value.to_owned(),
        activity: Some(activity.to_owned()),
        wbs: None,
        resource: None,
    })
}

/// A word such as `start-date` as the enum it names.
fn snake<T: serde::de::DeserializeOwned>(word: &str) -> Result<T, serde_json::Error> {
    serde_json::from_value(Value::from(word.trim().replace('-', "_")))
}

fn add_udf_type(text: &str) -> Result<Edit, CliError> {
    let form = "FIELD;TYPE or FIELD;TYPE;FOR";
    let bad = || invalid("field", text, form);
    let mut parts = text.rsplitn(3, ';').collect::<Vec<_>>();
    parts.reverse();
    let (field, kind, applies_to) = match parts.as_slice() {
        [field, kind] => (*field, *kind, UdfFor::Activity),
        [field, kind, target] => match snake(target) {
            Ok(target) => (*field, *kind, target),
            Err(_) => return Err(bad()),
        },
        _ => return Err(bad()),
    };
    let data_type: UdfKind = snake(kind).map_err(|_| bad())?;
    Ok(Edit::AddUdfType {
        field: field.to_owned(),
        applies_to,
        data_type,
    })
}

fn add_step(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=NAME or ACTIVITY=NAME;WEIGHT";
    let (activity, rest) = pair(text, "step", form)?;
    // A tail that isn't a number is part of the name.
    let (step, weight) = match rest.rsplit_once(';') {
        Some((step, weight)) if !step.is_empty() => match weight.trim().parse::<f64>() {
            Ok(weight) => (step, Some(weight)),
            Err(_) => (rest, None),
        },
        _ => (rest, None),
    };
    Ok(Edit::AddStep {
        activity: activity.to_owned(),
        step: step.to_owned(),
        weight,
        percent: None,
    })
}

fn step_percent(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=STEP=PERCENT";
    let (activity, rest) = pair(text, "step", form)?;
    let (step, percent) = rest
        .rsplit_once('=')
        .filter(|(step, _)| !step.is_empty())
        .ok_or_else(|| invalid("step", text, form))?;
    let percent = percent
        .strip_suffix('%')
        .unwrap_or(percent)
        .parse()
        .map_err(|_| invalid("step", text, form))?;
    Ok(Edit::SetStep {
        activity: activity.to_owned(),
        step: step.to_owned(),
        name: None,
        weight: None,
        percent: Some(percent),
    })
}

/// `ACTIVITY=TOPIC=TEXT` as a note edit.
fn note(add: bool) -> impl Fn(&str) -> Result<Edit, CliError> {
    move |text| {
        let form = "ACTIVITY=TOPIC=TEXT";
        let (activity, rest) = pair(text, "note", form)?;
        let (topic, text) = pair(rest, "note", form)?;
        let (activity, topic, text) =
            (Some(activity.to_owned()), topic.to_owned(), text.to_owned());
        Ok(if add {
            Edit::AddNote {
                activity,
                wbs: None,
                topic,
                text,
            }
        } else {
            Edit::SetNote {
                activity,
                wbs: None,
                topic,
                text,
            }
        })
    }
}

fn set_code(text: &str) -> Result<Edit, CliError> {
    let form = "ACTIVITY=TYPE=VALUE";
    let (activity, rest) = pair(text, "code", form)?;
    let (code_type, value) = pair(rest, "code", form)?;
    Ok(Edit::SetCode {
        activity: activity.to_owned(),
        code_type: code_type.to_owned(),
        value: value.to_owned(),
    })
}

fn assign(text: &str) -> Result<Edit, CliError> {
    let (activity, held) = pair(text, "assignment", "ACTIVITY=RESOURCE[:UNITS]")?;
    let (resource, units) = with_units(held);
    Ok(Edit::AssignResource {
        activity: activity.to_owned(),
        resource: resource.to_owned(),
        units,
    })
}

/// An edit read from `ACTIVITY=VALUE` or `NAME=VALUE`.
fn paired(
    what: &'static str,
    form: &'static str,
    make: impl Fn(String, String) -> Edit,
) -> impl Fn(&str) -> Result<Edit, CliError> {
    move |text| {
        let (left, right) = pair(text, what, form)?;
        Ok(make(left.to_owned(), right.to_owned()))
    }
}

/// A link edit read from `A>B:TYPE+LAG`.
fn linked_edit(make: impl Fn(Link) -> Edit) -> impl Fn(&str) -> Result<Edit, CliError> {
    move |text| link(text).map(&make)
}

impl Flags {
    fn wbs_nodes(&self, built: &mut Built) {
        built.each("add-obs", &self.add_obs, |text: &str| {
            let (obs, parent) = match text.split_once(';') {
                Some((_, "")) => return Err(invalid("OBS node", text, "NAME or NAME;PARENT")),
                Some((obs, parent)) => (obs, Some(parent.to_owned())),
                None => (text, None),
            };
            Ok(Edit::AddObs {
                obs: obs.to_owned(),
                parent,
                description: None,
            })
        });
        built.each("add-wbs", &self.add_wbs, add_wbs);
        let rename = paired("WBS name", "WBS=NAME", |wbs, name| Edit::RenameWbs {
            wbs,
            code: None,
            name: Some(name),
        });
        built.each("rename-wbs", &self.rename_wbs, rename);
        let moved = paired("WBS move", "WBS=PARENT", |wbs, parent| Edit::MoveWbs {
            wbs,
            parent,
        });
        built.each("move-wbs", &self.move_wbs, moved);
        let owner = paired("WBS manager", "WBS=OBS", |wbs, obs| Edit::SetWbsObs {
            wbs,
            obs,
        });
        built.each("wbs-obs", &self.wbs_obs, owner);
    }

    fn dictionaries(&self, built: &mut Built) {
        built.each("add-code-type", &self.add_code_type, add_code_type);
        built.each("add-resource", &self.add_resource, add_resource);
        built.each("resource-limit", &self.resource_limit, resource_limit);
        built.each("add-role", &self.add_role, add_role);
        built.each("role-limit", &self.role_limit, role_limit);
        built.each("add-code-value", &self.add_code_value, add_code_value);
        built.each(
            "add-project-code-type",
            &self.add_project_code_type,
            |name: &str| {
                Ok(Edit::AddProjectCodeType {
                    name: name.to_owned(),
                    max_length: None,
                })
            },
        );
        built.each("add-udf-type", &self.add_udf_type, add_udf_type);
        built.each(
            "add-project-code-value",
            &self.add_project_code_value,
            add_project_code_value,
        );
        let form = "CALENDAR=2026-12-24";
        let holiday = paired("holiday", form, |calendar, date| {
            Edit::AddCalendarException {
                calendar,
                date,
                shifts: Vec::new(),
            }
        });
        built.each("holiday", &self.holiday, holiday);
        let back = paired("holiday", form, |calendar, date| {
            Edit::RemoveCalendarException { calendar, date }
        });
        built.each("no-holiday", &self.no_holiday, back);
        built.each("hours-per", &self.hours_per, hours_per);
    }

    fn codes(&self, built: &mut Built) {
        let wbs = paired("WBS", "ACTIVITY=WBS", |activity, wbs| Edit::SetWbs {
            activity,
            wbs,
        });
        built.each("wbs", &self.wbs, wbs);
        let unassign = paired("assignment", "ACTIVITY=RESOURCE", |activity, resource| {
            Edit::UnassignResource { activity, resource }
        });
        built.each("unassign", &self.unassign, unassign);
        built.each("assign", &self.assign, assign);
        let unassign_role = paired("role assignment", "ACTIVITY=ROLE", |activity, role| {
            Edit::UnassignRole { activity, role }
        });
        built.each("unassign-role", &self.unassign_role, unassign_role);
        built.each("assign-role", &self.assign_role, assign_role);
        built.each("curve", &self.curve, curve);
        let evenly = paired("even spread", "ACTIVITY=RESOURCE", |activity, resource| {
            Edit::SetCurve {
                activity,
                resource,
                curve: None,
            }
        });
        built.each("spread-evenly", &self.spread_evenly, evenly);
        built.each("spread", &self.spread, spread);
        let delete_expense = paired("expense", "ACTIVITY=ITEM", |activity, expense| {
            Edit::DeleteExpense { activity, expense }
        });
        built.each("delete-expense", &self.delete_expense, delete_expense);
        built.each("add-expense", &self.add_expense, add_expense);
        built.each("set-expense", &self.set_expense, set_expense);
        built.each("set-cost-account", &self.set_cost_account, set_cost_account);
        let clear = paired("code", "ACTIVITY=TYPE", |activity, code_type| {
            Edit::ClearCode {
                activity,
                code_type,
            }
        });
        built.each("clear-code", &self.clear_code, clear);
        built.each("code", &self.code, set_code);
        let delete_note = paired("note", "ACTIVITY=TOPIC", |activity, topic| {
            Edit::DeleteNote {
                activity: Some(activity),
                wbs: None,
                topic,
            }
        });
        built.each("delete-note", &self.delete_note, delete_note);
        built.each("add-note", &self.add_note, note(true));
        let clear_udf = paired("field value", "ACTIVITY=FIELD", |activity, field| {
            Edit::ClearUdf {
                field,
                activity: Some(activity),
                wbs: None,
                resource: None,
            }
        });
        built.each("clear-udf", &self.clear_udf, clear_udf);
        built.each("udf", &self.udf, set_udf);
        let delete_step = paired("step", "ACTIVITY=STEP", |activity, step| Edit::DeleteStep {
            activity,
            step,
        });
        built.each("delete-step", &self.delete_step, delete_step);
        built.each("add-step", &self.add_step, add_step);
        built.each("step", &self.step, step_percent);
        let complete = paired("step", "ACTIVITY=STEP", |activity, step| {
            Edit::CompleteStep { activity, step }
        });
        built.each("complete-step", &self.complete_step, complete);
        built.each("set-note", &self.set_note, note(false));
        built.each(
            "clear-project-code",
            &self.clear_project_code,
            |code_type: &str| {
                Ok(Edit::ClearProjectCode {
                    code_type: code_type.to_owned(),
                })
            },
        );
        let project_code = paired("project code", "TYPE=VALUE", |code_type, value| {
            Edit::SetProjectCode { code_type, value }
        });
        built.each("project-code", &self.project_code, project_code);
    }

    fn links(&self, built: &mut Built) {
        let unlink = linked_edit(|parts| Edit::RemoveLink {
            predecessor: parts.from,
            successor: parts.to,
            link_type: parts.kind,
        });
        built.each("unlink", &self.unlink, unlink);
        let add = linked_edit(|parts| Edit::AddLink {
            predecessor: parts.from,
            successor: parts.to,
            link_type: parts.kind,
            lag: parts.lag,
        });
        built.each("link", &self.link, add);
        let relink = linked_edit(|parts| Edit::ChangeLink {
            predecessor: parts.from,
            successor: parts.to,
            link_type: None,
            new_type: parts.kind,
            lag: parts.lag,
        });
        built.each("relink", &self.relink, relink);
    }

    /// One edit per activity, so its actuals, remaining and percent are checked
    /// together. `--remaining` joins in for an activity with progress flags.
    fn progress(&self, built: &mut Built) {
        #[derive(Default)]
        struct Parts {
            start: Option<String>,
            finish: Option<String>,
            remaining: Option<Span>,
            percent: Option<f64>,
            flags: Vec<String>,
        }
        let mut parts: BTreeMap<String, Parts> = BTreeMap::new();
        let mut wrong = Vec::new();
        let mut date = |flag: &str, text: &str, form: &str, set: fn(&mut Parts, String)| match pair(
            text,
            &flag.replace('-', " "),
            form,
        ) {
            Ok((activity, date)) => {
                let part = parts.entry(activity.to_owned()).or_default();
                set(part, date.to_owned());
                part.flags.push(format!("--{flag} {text}"));
            }
            Err(error) => wrong.push((format!("--{flag} {text}"), error)),
        };
        for text in &self.actual_start {
            date("actual-start", text, "ACTIVITY=2026-03-02 08:00", |p, d| {
                p.start = Some(d);
            });
        }
        for text in &self.actual_finish {
            date(
                "actual-finish",
                text,
                "ACTIVITY=2026-03-06 17:00",
                |p, d| {
                    p.finish = Some(d);
                },
            );
        }
        for text in &self.percent {
            let read = pair(text, "percent", "ACTIVITY=50").and_then(|(activity, value)| {
                let value: f64 = value
                    .parse()
                    .map_err(|_| invalid("percent", value, "a number from 0 to 100"))?;
                Ok((activity, value))
            });
            match read {
                Ok((activity, value)) => {
                    let part = parts.entry(activity.to_owned()).or_default();
                    part.percent = Some(value);
                    part.flags.push(format!("--percent {text}"));
                }
                Err(error) => wrong.push((format!("--percent {text}"), error)),
            }
        }
        for text in &self.remaining {
            let Ok((activity, value)) = pair(text, "duration", "ACTIVITY=15d") else {
                continue;
            };
            if let Some(entry) = parts.get_mut(activity) {
                match span(value) {
                    Ok(value) => {
                        entry.remaining = Some(value);
                        entry.flags.push(format!("--remaining {text}"));
                    }
                    Err(error) => wrong.push((format!("--remaining {text}"), error)),
                }
            }
        }
        for (source, error) in wrong {
            built.add(source, Err(error));
        }
        for (activity, part) in parts {
            let edit = Edit::SetProgress {
                activity,
                actual_start: part.start,
                actual_finish: part.finish,
                remaining: part.remaining,
                percent: part.percent,
            };
            built.add(part.flags.join(" "), Ok(edit));
        }
    }

    fn activities(&self, built: &mut Built) {
        for text in &self.add {
            let edit = new_activity(text).and_then(|edit| match &edit {
                Edit::AddActivity { code, .. }
                    if self.calendar.iter().any(|c| pair_starts(c, code)) =>
                {
                    Err(CliError::new(
                        "invalid-edit",
                        format!("{code} is new, and its days would use the wrong calendar"),
                        "name its calendar in add_activity with --edits",
                    ))
                }
                _ => Ok(edit),
            });
            built.add(format!("--add {text}"), edit);
        }
        let calendar = paired("calendar", "ACTIVITY=CALENDAR", |activity, calendar| {
            Edit::SetCalendar { activity, calendar }
        });
        built.each("calendar", &self.calendar, calendar);
    }

    fn durations(&self, built: &mut Built) {
        // A secondary has to go before its primary.
        for (flag, list, secondary) in [
            (
                "clear-secondary-constraint",
                &self.clear_secondary_constraint,
                true,
            ),
            ("clear-constraint", &self.clear_constraint, false),
        ] {
            built.each(flag, list, |activity| {
                Ok(Edit::ClearConstraint {
                    activity: activity.to_owned(),
                    secondary,
                })
            });
        }
        built.each("constraint", &self.constraint, |t| constraint(t, false));
        let secondary = &self.secondary_constraint;
        built.each("secondary-constraint", secondary, |t| constraint(t, true));
        let form = "ACTIVITY=15d";
        built.each("duration", &self.duration, |text| {
            let (activity, value) = pair(text, "duration", form)?;
            Ok(Edit::SetDuration {
                activity: activity.to_owned(),
                original: Some(span(value)?),
                remaining: None,
            })
        });
        let mut progress = Built::default();
        self.progress(&mut progress);
        let in_progress = |code: &str| activities_named_any(&progress.edits, code);
        for text in &self.remaining {
            let read = pair(text, "duration", form);
            if read
                .as_ref()
                .is_ok_and(|(activity, _)| in_progress(activity))
            {
                continue;
            }
            let edit = read.and_then(|(activity, value)| {
                Ok(Edit::SetDuration {
                    activity: activity.to_owned(),
                    original: None,
                    remaining: Some(span(value)?),
                })
            });
            built.add(format!("--remaining {text}"), edit);
        }
        built.edits.extend(progress.edits);
        built.sources.extend(progress.sources);
        built.problems.extend(progress.problems);
    }

    /// The edits, in a fixed order that keeps them independent: the file, then
    /// calendars before durations, which are measured on them, and clears
    /// before sets, then names and links. A flag that can't be read is listed
    /// in [`Built::problems`], and the rest are still read.
    ///
    /// # Errors
    ///
    /// When the `--edits` file can't be read at all.
    pub fn edits(&self) -> Result<Built, CliError> {
        let mut built = Built::default();
        if let Some(path) = &self.edits {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let commands = read_edits(path)?;
            for (n, edit) in commands.edits.into_iter().enumerate() {
                built.add(format!("{name} item {}", n + 1), Ok(edit));
            }
            built.options = commands.options;
        }
        self.wbs_nodes(&mut built);
        self.dictionaries(&mut built);
        built.each("clear-progress", &self.clear_progress, |activity| {
            Ok(Edit::ClearProgress {
                activity: activity.to_owned(),
            })
        });
        self.activities(&mut built);
        self.durations(&mut built);
        // After progress, which a suspension needs, and clears before sets.
        built.each("clear-suspend", &self.clear_suspend, |activity| {
            Ok(Edit::ClearSuspend {
                activity: activity.to_owned(),
            })
        });
        built.each("suspend", &self.suspend, |text| {
            let form = "ACTIVITY=SUSPEND/RESUME";
            let (activity, dates) = pair(text, "suspend", form)?;
            let (suspend, resume) = dates
                .split_once('/')
                .ok_or_else(|| invalid("suspend", text, form))?;
            Ok(Edit::SetSuspend {
                activity: activity.to_owned(),
                suspend: suspend.trim().to_owned(),
                resume: resume.trim().to_owned(),
            })
        });
        let name = paired("name", "ACTIVITY=NAME", |activity, name| Edit::SetName {
            activity,
            name,
        });
        built.each("name", &self.name, name);
        self.links(&mut built);
        self.codes(&mut built);
        built.each("delete", &self.delete, |activity| {
            Ok(Edit::DeleteActivity {
                activity: activity.to_owned(),
            })
        });
        Ok(built)
    }
}

/// `totalfloat.edit/1`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Report {
    #[schemars(extend("const" = "totalfloat.edit/1"))]
    schema: &'static str,
    file: String,
    output: Option<String>,
    /// With `--skip-failed`, the edits not applied, each with why.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skipped: Vec<Problem>,
    /// What each edit did, in order.
    applied: Vec<String>,
    /// What a scheduler should know the edits didn't do.
    warnings: Vec<String>,
    projects: Vec<Impact>,
    /// On a history store, the changeset the edits recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    recorded: Option<Recorded>,
    /// The last line of the text report, when it isn't about an output file.
    #[serde(skip)]
    closing: Option<String>,
}

/// What the edits did to a project, against the file calculated as it stands.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Impact {
    #[serde(flatten)]
    calculated: ProjectReport,
    /// Critical activities before the edits.
    previous_critical: usize,
    /// Activities the edits added.
    added: Vec<String>,
    /// Activities the edits deleted.
    deleted: Vec<String>,
    milestones_moved_total: usize,
    milestones_moved: Vec<totalfloat_diff::DateMove>,
    became_critical_total: usize,
    became_critical: Vec<String>,
    left_critical_total: usize,
    left_critical: Vec<String>,
    /// Changes `diff` would flag.
    findings_total: usize,
    findings: Vec<totalfloat_diff::Finding>,
    /// How quality checks moved, when asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    checks: Option<CheckChanges>,
}

/// What the edits did to the quality checks.
#[derive(Debug, Default, Serialize, schemars::JsonSchema)]
struct CheckChanges {
    /// Checks that passed before and fail now.
    newly_failing: Vec<CheckChange>,
    /// Checks already failing that gained offenders.
    more_offenders: Vec<CheckChange>,
    /// Checks that failed before and pass now.
    newly_passing: Vec<&'static str>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CheckChange {
    id: &'static str,
    name: &'static str,
    measure: String,
    /// Offenders the edits added, as far as `check` lists them.
    added: Vec<String>,
}

fn change(check: &Check, was: &Check) -> CheckChange {
    CheckChange {
        id: check.id,
        name: check.name,
        measure: check.measure.clone(),
        added: check
            .listed
            .iter()
            .filter(|item| !was.listed.contains(item))
            .cloned()
            .collect(),
    }
}

/// How the checks moved from one assessment to the next. Checks are compared by
/// ID, and one that wasn't measurable before counts as having passed.
fn check_changes(was: &[Check], now: &[Check]) -> CheckChanges {
    let mut changes = CheckChanges::default();
    for check in now {
        let Some(before) = was.iter().find(|c| c.id == check.id) else {
            continue;
        };
        match (before.outcome, check.outcome) {
            (Outcome::Fail, Outcome::Pass) => changes.newly_passing.push(check.name),
            (Outcome::Fail, Outcome::Fail) if check.offenders > before.offenders => {
                changes.more_offenders.push(change(check, before));
            }
            (Outcome::Pass | Outcome::NotApplicable, Outcome::Fail) => {
                changes.newly_failing.push(change(check, before));
            }
            _ => {}
        }
    }
    changes
}

/// Each project's quality checks in a held view, by project ID: the "before"
/// side of every edit measured against it, assessed once per view.
pub type Assessed = OnceLock<BTreeMap<u64, Vec<Check>>>;

/// Every project of `schedules` assessed among them.
pub(crate) fn assess_each(schedules: &[Schedule]) -> BTreeMap<u64, Vec<Check>> {
    let thresholds = Thresholds::default();
    schedules
        .iter()
        .map(|s| (s.project.id.0, assess(s, schedules, &thresholds).checks))
        .collect()
}

/// The calculation the edit made of `schedule`, where it is the one `assess`
/// would make. `assess` calculates a project among its file's other
/// projects, as `schedule_with_projects` does; the edit calculated it among
/// the projects open together. The two differ only through another project:
/// a link to one, which reads its dates, and float measured from the opened
/// projects' finish. With neither, the edit's calculation stands for it.
fn as_assess_calculates<'a>(
    schedule: &Schedule,
    calculated: &'a schedule::Kept,
) -> Option<&'a totalfloat_engine::Calculated> {
    let alone = !linked(schedule) && schedule.project.options.float_from_project_finish;
    calculated.get(&schedule.project.id.0).filter(|_| alone)
}

/// Assesses a project before and after, each among its own file's projects.
/// The before side comes from `assessed` when it is given: the edit is
/// measured against a held view, whose assessment is kept.
fn checks_moved(
    (was, every_was, assessed): (&Schedule, &[Schedule], Option<&Assessed>),
    (now, every_now, calculated): (&Schedule, &[Schedule], &schedule::Kept),
) -> CheckChanges {
    let thresholds = Thresholds::default();
    let kept = assessed
        .map(|assessed| assessed.get_or_init(|| assess_each(every_was)))
        .and_then(|by_project| by_project.get(&was.project.id.0));
    let after = match as_assess_calculates(now, calculated) {
        Some(calculated) => assess_calculated(now, every_now, &thresholds, calculated),
        None => assess(now, every_now, &thresholds),
    };
    match kept {
        Some(before) => check_changes(before, &after.checks),
        None => check_changes(&assess(was, every_was, &thresholds).checks, &after.checks),
    }
}

fn impact(
    calculated: ProjectReport,
    previous_critical: usize,
    (was, now): (&Schedule, &Schedule),
    checks: Option<CheckChanges>,
) -> Impact {
    let mut comparison = compare(was, now);
    // `compare` looks at activities on both sides, so new and removed ones are
    // counted here.
    let critical_in = |schedule: &Schedule, code: &str| {
        schedule
            .activity_by_code(code)
            .is_some_and(|a| critical(schedule, a))
    };
    for added in &comparison.added {
        if critical_in(now, &added.code) {
            comparison.became_critical.push(added.code.clone());
            comparison.counts.became_critical += 1;
        }
    }
    for deleted in &comparison.deleted {
        if critical_in(was, &deleted.code) {
            comparison.left_critical.push(deleted.code.clone());
            comparison.counts.left_critical += 1;
        }
    }
    let counts = comparison.counts;
    let codes = |list: &[totalfloat_diff::ActivityRef]| -> Vec<String> {
        list.iter().map(|a| a.code.clone()).collect()
    };
    let (added, deleted) = (codes(&comparison.added), codes(&comparison.deleted));
    comparison.limit(MAX_LISTED);
    Impact {
        previous_critical,
        calculated,
        added,
        deleted,
        milestones_moved_total: counts.milestones_moved,
        milestones_moved: comparison.milestones_moved,
        became_critical_total: counts.became_critical,
        became_critical: comparison.became_critical,
        left_critical_total: counts.left_critical,
        left_critical: comparison.left_critical,
        findings_total: counts.findings,
        findings: comparison.findings,
        checks,
    }
}

/// IDs above every row in the file, whichever project owns it.
fn next_ids(loaded: &Loaded) -> NextIds {
    let highest = |name: &str, field: &str| {
        loaded.raw.table(name).map_or(0, |table| {
            let index = table.field_index(field);
            table
                .rows
                .iter()
                .filter_map(|row| row.get(index?)?.parse::<u64>().ok())
                .max()
                .unwrap_or(0)
        })
    };
    NextIds {
        relationship: highest("TASKPRED", "task_pred_id") + 1,
        activity: highest("TASK", "task_id") + 1,
        calendar: highest("CALENDAR", "clndr_id") + 1,
        code: highest("ACTVCODE", "actv_code_id") + 1,
        assignment: highest("TASKRSRC", "taskrsrc_id") + 1,
        wbs: highest("PROJWBS", "wbs_id") + 1,
        code_type: highest("ACTVTYPE", "actv_code_type_id") + 1,
        resource: highest("RSRC", "rsrc_id") + 1,
        resource_rate: highest("RSRCRATE", "rsrc_rate_id") + 1,
        role: highest("ROLES", "role_id") + 1,
        role_rate: highest("ROLERATE", "role_rate_id") + 1,
        expense: highest("PROJCOST", "cost_item_id") + 1,
        project_code_type: highest("PCATTYPE", "proj_catg_type_id") + 1,
        project_code: highest("PCATVAL", "proj_catg_id") + 1,
        obs: highest("OBS", "obs_id") + 1,
        activity_note: highest("TASKMEMO", "memo_id") + 1,
        wbs_note: highest("WBSMEMO", "wbs_memo_id") + 1,
        step: highest("TASKPROC", "proc_id") + 1,
        udf_type: highest("UDFTYPE", "udf_type_id") + 1,
    }
}

/// The one calendar a name means across the projects in reach.
fn resolve_calendar(
    schedules: &[Schedule],
    wanted: &str,
    project: Option<u64>,
) -> Result<CalendarId, CliError> {
    let ids: BTreeSet<CalendarId> = schedules
        .iter()
        .filter(|s| project.is_none_or(|id| s.project.id.0 == id))
        .flat_map(|s| calendars_named(s, wanted))
        .collect();
    let mut found = ids.iter();
    match (found.next(), found.next()) {
        (Some(id), None) => Ok(*id),
        (None, _) => Err(CliError::new(
            "no-such-calendar",
            format!("no calendar matches {wanted}"),
            "`inspect` lists calendars",
        )),
        _ => Err(CliError::new(
            "ambiguous-calendar",
            format!("{} calendars are named {wanted}", ids.len()),
            "use the calendar's ID, from `inspect`",
        )),
    }
}

/// True if this calendar is `id`, or built on it.
fn built_on(schedule: &Schedule, calendar: CalendarId, id: CalendarId) -> bool {
    let mut at = Some(calendar);
    let mut seen = BTreeSet::new();
    while let Some(here) = at.filter(|here| seen.insert(*here)) {
        if here == id {
            return true;
        }
        at = schedule.calendars.get(&here).and_then(|c| c.base);
    }
    false
}

/// The projects whose work runs on a calendar, directly or through one built on it.
fn calendar_users(schedules: &[Schedule], id: CalendarId) -> Vec<usize> {
    schedules
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.activities.values().any(|a| built_on(s, a.calendar, id))
                || s.resources
                    .values()
                    .filter_map(|r| r.calendar)
                    .any(|calendar| built_on(s, calendar, id))
        })
        .map(|(index, _)| index)
        .collect()
}

/// The project that takes a calendar edit: one whose work uses it, if any does,
/// because that's the project whose dates move.
fn calendar_owner(
    schedules: &[Schedule],
    wanted: &str,
    project: Option<u64>,
) -> Result<usize, CliError> {
    let id = resolve_calendar(schedules, wanted, project)?;
    let in_reach = |s: &Schedule| project.is_none_or(|p| s.project.id.0 == p);
    let user = calendar_users(schedules, id)
        .into_iter()
        .find(|index| in_reach(&schedules[*index]));
    user.or_else(|| {
        schedules
            .iter()
            .position(|s| in_reach(s) && s.calendars.contains_key(&id))
    })
    .ok_or_else(|| {
        CliError::new(
            "no-such-calendar",
            format!("no calendar matches {wanted}"),
            "`inspect` lists calendars",
        )
    })
}

/// A calendar built on another copies its exceptions on reading, so changing both
/// in one call would write stale copies back.
fn check_calendar_edits(
    schedules: &[Schedule],
    edits: &[Edit],
    project: Option<u64>,
    refused: &mut BTreeMap<usize, CliError>,
) -> Result<(), CliError> {
    let new = |wanted: &str| {
        edits
            .iter()
            .any(|edit| matches!(edit, Edit::AddCalendar { name, .. } if name == wanted))
    };
    let mut ids = BTreeSet::new();
    for (index, edit) in edits.iter().enumerate() {
        let Some(wanted) = calendar_changed(edit) else {
            continue;
        };
        if new(wanted) {
            continue;
        }
        match resolve_calendar(schedules, wanted, project) {
            Ok(id) => {
                ids.insert(id);
            }
            Err(error) => {
                refused.insert(index, error);
            }
        }
    }
    let related = ids.iter().any(|id| {
        ids.iter().any(|other| {
            id != other
                && schedules
                    .iter()
                    .any(|s| s.calendars.contains_key(other) && built_on(s, *other, *id))
        })
    });
    if related {
        return Err(CliError::new(
            "related-calendars",
            "these edits change a calendar and one built on it",
            "make those changes in separate calls",
        ));
    }
    Ok(())
}

/// The project taking a new code value. A global type is one type whichever
/// project holds it, but two projects' own types can share a name.
fn code_owner(
    schedules: &[Schedule],
    candidates: &[usize],
    code_type: &str,
) -> Result<usize, CliError> {
    let lower = code_type.to_lowercase();
    let holders: Vec<(usize, u64)> = candidates
        .iter()
        .flat_map(|index| {
            schedules[*index]
                .activity_code_types
                .values()
                .filter(|t| t.name.to_lowercase() == lower)
                .map(move |t| (*index, t.id.0))
        })
        .collect();
    let kinds: BTreeSet<u64> = holders.iter().map(|(_, id)| *id).collect();
    match (holders.first(), kinds.len()) {
        (Some((index, _)), 1) => Ok(*index),
        (None, _) => Err(CliError::new(
            "no-such-code",
            format!("no code type is named {code_type}"),
            "`inspect` lists activity codes",
        )),
        _ => Err(CliError::new(
            "needs-project",
            format!("{} code types are named {code_type}", kinds.len()),
            "name the project: --project, or `project` over MCP",
        )),
    }
}

/// A global change's edits, put first, since relative changes are worked out
/// from the file as it stands; and the run's options with the project the
/// change found.
fn global_change<'a>(
    input: &Path,
    flags: &Flags,
    loaded: &Loaded,
    mut options: Options<'a>,
    (edits, sources): (&mut Vec<Edit>, &mut Vec<String>),
) -> Result<Options<'a>, CliError> {
    let Some(filter) = &flags.filter else {
        return Ok(options);
    };
    if flags.apply.is_some() {
        return Err(CliError::new(
            "conflicting-edits",
            "--where and --apply can't be used together",
            "make the global change in its own run",
        ));
    }
    let mut query = crate::global::parse_where(filter)?;
    if let (Some(asked), Some(given)) = (query.project, options.project)
        && asked != given
    {
        return Err(CliError::new(
            "conflicting-project",
            format!("--where names project {asked} and --project {given}"),
            "give one",
        ));
    }
    query.project = query.project.or(options.project);
    let changes = flags
        .set
        .iter()
        .map(|text| crate::global::parse_set(text).map(|c| (text.clone(), c)))
        .collect::<Result<Vec<_>, _>>()?;
    let file = input.display().to_string();
    let expanded = crate::global::expand(&file, loaded, query, &changes)?;
    // The edits name activities by ID, which only the project found makes one.
    options.project = Some(expanded.project);
    edits.splice(0..0, expanded.edits);
    sources.splice(0..0, expanded.sources);
    Ok(options)
}

/// `--from-sheet`: the sheet's changed cells as edits, before the run's
/// others, since they're worked out from the file as it stands.
fn from_sheet<'a>(
    input: &Path,
    flags: &Flags,
    loaded: &Loaded,
    mut options: Options<'a>,
    (edits, sources): (&mut Vec<Edit>, &mut Vec<String>),
) -> Result<Options<'a>, CliError> {
    let Some(sheet) = &flags.from_sheet else {
        return Ok(options);
    };
    let file = input.display().to_string();
    let expanded = crate::resheet::expand(&file, loaded, sheet, options.project)?;
    // An unchanged sheet is fine beside other edits; alone it has nothing to do.
    if expanded.edits.is_empty() && edits.is_empty() && options.data_date.is_none() {
        return Err(CliError::new(
            "nothing-to-change",
            format!("{} says what the schedule already says", sheet.display()),
            "change cells in the sheet first; dates, floats and other worked-out columns are left as they are",
        ));
    }
    options.project = Some(expanded.project);
    edits.splice(0..0, expanded.edits);
    sources.splice(0..0, expanded.sources);
    Ok(options)
}

/// `--from-timesheet`: the returned timesheets' hours and progress as
/// edits, before the run's others, since they're checked against the file
/// as it stands.
fn from_timesheet<'a>(
    flags: &Flags,
    loaded: &Loaded,
    mut options: Options<'a>,
    (edits, sources): (&mut Vec<Edit>, &mut Vec<String>),
) -> Result<Options<'a>, CliError> {
    if flags.from_timesheet.is_empty() {
        return Ok(options);
    }
    let expanded = crate::timesheet::expand(loaded, &flags.from_timesheet, options.project)?;
    options.project = Some(expanded.project);
    edits.splice(0..0, expanded.edits);
    sources.splice(0..0, expanded.sources);
    Ok(options)
}

/// The project for an edit about the project or the file rather than one
/// activity, if this is one.
fn file_wide(edit: &Edit, found: &[usize]) -> Option<Result<usize, CliError>> {
    // Project code types and values are the file's: every project holds them
    // and a new one reaches every project, so any project in reach can take it.
    if let (
        Edit::AddProjectCodeType { .. }
        | Edit::AddProjectCodeValue { .. }
        | Edit::AddObs { .. }
        | Edit::AddUdfType { .. }
        | Edit::SetRoleLimit { .. },
        Some(first),
    ) = (edit, found.first())
    {
        return Some(Ok(*first));
    }
    if let (
        Edit::SetProjectCode { .. }
        | Edit::ClearProjectCode { .. }
        | Edit::SetUdf {
            activity: None,
            wbs: None,
            ..
        }
        | Edit::ClearUdf {
            activity: None,
            wbs: None,
            ..
        },
        true,
    ) = (edit, found.len() > 1)
    {
        return Some(Err(CliError::new(
            "needs-project",
            format!("{} projects could take this edit", found.len()),
            "name the project: --project, or `project` over MCP",
        )));
    }
    None
}

/// The one project all of an edit's activities are in, counting activities
/// that edits before it add.
fn owner(
    schedules: &[Schedule],
    edit: &Edit,
    project: Option<u64>,
    (added, new_types): (&BTreeMap<&str, usize>, &BTreeMap<String, usize>),
) -> Result<usize, CliError> {
    if let Some(wanted) = calendar_changed(edit) {
        if let Some(index) = added.get(wanted) {
            return Ok(*index);
        }
        return calendar_owner(schedules, wanted, project);
    }
    let names = activities_named(edit);
    let holds = |index: usize, s: &Schedule| {
        names
            .iter()
            .all(|code| s.activity_by_code(code).is_some() || added.get(code) == Some(&index))
    };
    let found: Vec<usize> = schedules
        .iter()
        .enumerate()
        .filter(|(index, s)| project.is_none_or(|id| s.project.id.0 == id) && holds(*index, s))
        .map(|(index, _)| index)
        .collect();
    if let Edit::AddCodeValue { code_type, .. } = edit {
        if let Some(index) = new_types.get(&code_type.to_lowercase()) {
            return Ok(*index);
        }
        return code_owner(schedules, &found, code_type);
    }
    if let Some(owner) = file_wide(edit, &found) {
        return owner;
    }
    // A resource is the file's, and its rates reach every project's copy, so
    // any project in reach holding it can take its limit.
    if let Edit::SetResourceLimit { resource, .. } = edit {
        let holder = found
            .iter()
            .find(|index| resource_named(&schedules[**index], resource).is_some());
        match (holder, found.as_slice()) {
            (Some(index), _) | (None, [index]) => return Ok(*index),
            (None, _) => {
                return Err(CliError::new(
                    "no-such-resource",
                    format!("no project in reach holds the resource {resource}"),
                    "`inspect` lists resources; for one added in this run, name the project: --project, or `project` over MCP",
                ));
            }
        }
    }
    if let Some(wanted) = wbs_named(edit) {
        if let Some(index) = added.get(wanted) {
            return Ok(*index);
        }
        let holders: Vec<usize> = found
            .iter()
            .copied()
            .filter(|index| holds_wbs(&schedules[*index], wanted))
            .collect();
        return match holders.as_slice() {
            [one] => Ok(*one),
            [] => Err(CliError::new(
                "no-such-wbs",
                format!("no project holds the WBS node {wanted}"),
                "`query --columns code,wbs` shows WBS codes",
            )),
            _ => Err(CliError::new(
                "needs-project",
                format!("{} projects hold the WBS node {wanted}", holders.len()),
                "name the project: --project, or `project` over MCP",
            )),
        };
    }
    if let (
        Edit::AddActivity { code: name, .. }
        | Edit::AddCalendar { name, .. }
        | Edit::AddCodeType { name, .. }
        | Edit::AddResource { resource: name, .. }
        | Edit::AddRole { role: name, .. },
        true,
    ) = (edit, found.len() > 1)
    {
        return Err(CliError::new(
            "needs-project",
            format!("{} projects could take {name}", found.len()),
            "name the project: --project, or `project` over MCP",
        ));
    }
    match found.as_slice() {
        [one] => Ok(*one),
        [] => Err(CliError::new(
            "no-such-activity",
            format!("no project holds {}", names.join(" and ")),
            "use Activity IDs from `query`; a link to another project names it PROJECT:CODE",
        )),
        _ => Err(CliError::new(
            "ambiguous-activity",
            format!("{} projects hold {}", found.len(), names.join(" and ")),
            "name the project: --project, or `project` over MCP",
        )),
    }
}

/// A link's ends once `PROJECT:CODE` has told them apart.
struct Ends {
    /// Where in the schedules the predecessor and successor are.
    projects: (usize, usize),
    /// The edit, naming them by plain Activity ID.
    edit: Edit,
}

/// True if the project at `index` holds this Activity ID, or will once this
/// run's new activities are in.
fn holds_code(
    index: usize,
    schedule: &Schedule,
    code: &str,
    added: &BTreeMap<&str, usize>,
) -> bool {
    schedule.activity_by_code(code).is_some() || added.get(code) == Some(&index)
}

/// The project and Activity ID in `PROJECT:CODE`. Either can hold a colon.
fn qualified<'a>(
    schedules: &[Schedule],
    name: &'a str,
    added: &BTreeMap<&str, usize>,
) -> Option<(usize, &'a str)> {
    name.match_indices(':').find_map(|(at, _)| {
        let (project, code) = (&name[..at], &name[at + 1..]);
        let index = schedules
            .iter()
            .position(|s| s.project.short_name == project)?;
        holds_code(index, &schedules[index], code, added).then_some((index, code))
    })
}

/// The one project holding a plain Activity ID.
fn only_holder<'a>(
    schedules: &[Schedule],
    name: &'a str,
    added: &BTreeMap<&str, usize>,
) -> Result<(usize, &'a str), CliError> {
    let holders: Vec<usize> = schedules
        .iter()
        .enumerate()
        .filter(|(index, s)| holds_code(*index, s, name, added))
        .map(|(index, _)| index)
        .collect();
    match holders.as_slice() {
        [one] => Ok((*one, name)),
        [] => Err(CliError::new(
            "no-such-activity",
            format!("no project holds {name}"),
            "use Activity IDs from `query`; a link to another project names it PROJECT:CODE",
        )),
        _ => Err(CliError::new(
            "ambiguous-activity",
            format!("{} projects hold {name}", holders.len()),
            "name it PROJECT:CODE",
        )),
    }
}

/// What's wrong with `A>B:XX` when `B` is an activity and `XX` isn't: it can
/// only have been meant as a link type and lag.
fn misread_tail(
    schedules: &[Schedule],
    to: &str,
    added: &BTreeMap<&str, usize>,
) -> Option<CliError> {
    let anywhere = |code: &str| {
        let mut all = schedules.iter().enumerate();
        all.any(|(index, s)| holds_code(index, s, code, added))
            || qualified(schedules, code, added).is_some()
    };
    let (code, tail) = to.rsplit_once(':')?;
    if anywhere(to) || !anywhere(code) {
        return None;
    }
    match tail.rfind(['+', '-']) {
        Some(at) if link_type(&tail[..at]).is_ok() => lag_span(&tail[at..]).err(),
        _ => link_type(tail).err(),
    }
}

/// Where a link's ends are, if `PROJECT:CODE` says. Plain IDs that one project
/// holds together are left to [`owner`]. A link reaches another project only if
/// an end names one; the other end may then be in any one project.
fn link_ends(
    schedules: &[Schedule],
    edit: &Edit,
    added: &BTreeMap<&str, usize>,
) -> Result<Option<Ends>, CliError> {
    let [from, to] = activities_named(edit)[..] else {
        return Ok(None);
    };
    let together = |(index, s): (usize, &Schedule)| {
        holds_code(index, s, from, added) && holds_code(index, s, to, added)
    };
    if schedules.iter().enumerate().any(together) {
        return Ok(None);
    }
    if let Some(error) = misread_tail(schedules, to, added) {
        return Err(error);
    }
    let (near, far) = (
        qualified(schedules, from, added),
        qualified(schedules, to, added),
    );
    if near.is_none() && far.is_none() {
        return Ok(None);
    }
    let near = near.map_or_else(|| only_holder(schedules, from, added), Ok)?;
    let far = far.map_or_else(|| only_holder(schedules, to, added), Ok)?;
    let mut plain = edit.clone();
    if let Edit::AddLink {
        predecessor,
        successor,
        ..
    }
    | Edit::RemoveLink {
        predecessor,
        successor,
        ..
    }
    | Edit::ChangeLink {
        predecessor,
        successor,
        ..
    } = &mut plain
    {
        near.1.clone_into(predecessor);
        far.1.clone_into(successor);
    }
    Ok(Some(Ends {
        projects: (near.0, far.0),
        edit: plain,
    }))
}

/// The project an edit belongs to, and a link's ends if it names them.
type Owned = (usize, Option<Ends>);

/// With `--skip-failed`, the file as read and the whole batch, so an edit
/// needing a refused one is skipped too.
type Skipping<'a> = (&'a [Schedule], &'a [Edit]);

/// An edit that found its project: its place in the batch, the edit, the
/// project and a link's ends.
type Kept<'a> = (usize, &'a Edit, usize, Option<&'a Ends>);

/// The edits that found their project.
fn kept<'a>(edits: &'a [Edit], found: &'a [Option<Owned>]) -> Vec<Kept<'a>> {
    found
        .iter()
        .enumerate()
        .filter_map(|(at, owner)| {
            let (index, ends) = owner.as_ref()?;
            Some((at, &edits[at], *index, ends.as_ref()))
        })
        .collect()
}

/// Which project each edit belongs to, and for each link named `PROJECT:CODE`,
/// where its ends are. An edit with no owner is refused, and gets `None`.
fn owners(
    schedules: &[Schedule],
    edits: &[Edit],
    project: Option<u64>,
    refused: &mut BTreeMap<usize, CliError>,
) -> Result<Vec<Option<Owned>>, CliError> {
    if let Some(id) = project
        && !schedules.iter().any(|s| s.project.id.0 == id)
    {
        return Err(CliError::new(
            "no-such-project",
            format!("the file has no project {id}"),
            "`inspect` lists the projects in the file",
        ));
    }
    let mut added = BTreeMap::new();
    // Code types the edits add, by lowered name. Kept apart from `added`, as
    // a code type can share its name with an activity, calendar or WBS node.
    let mut new_types: BTreeMap<String, usize> = BTreeMap::new();
    let mut owners = Vec::with_capacity(edits.len());
    for (at, edit) in edits.iter().enumerate() {
        if refused.contains_key(&at) {
            owners.push(None);
            continue;
        }
        let found = owned(schedules, edit, project, (&added, &new_types));
        let index = match found {
            Ok((index, ends)) => {
                owners.push(Some((index, ends)));
                index
            }
            Err(error) => {
                refused.insert(at, error);
                owners.push(None);
                continue;
            }
        };
        if let Edit::AddActivity { code, .. } = edit {
            added.insert(code.as_str(), index);
        }
        if let Edit::AddCalendar { name, .. } = edit {
            added.insert(name.as_str(), index);
        }
        if let Edit::AddCodeType { name, .. } = edit {
            new_types.insert(name.to_lowercase(), index);
        }
        if let Edit::AddWbs { code, .. }
        | Edit::RenameWbs {
            code: Some(code), ..
        } = edit
        {
            added.insert(code.as_str(), index);
        }
    }
    Ok(owners)
}

/// Resources and global code types are the file's, not one project's, so a
/// new one can't repeat a name any project in the file holds.
fn refuse_repeated(schedules: &[Schedule], edit: &Edit) -> Result<(), CliError> {
    match edit {
        Edit::AddResource { resource, .. } => {
            let held = schedules.iter().flat_map(|s| s.resources.values());
            if held
                .map(|r| &r.short_name)
                .any(|name| name.eq_ignore_ascii_case(resource))
            {
                return Err(CliError::new(
                    "duplicate-resource",
                    format!("the file already has a resource {resource}"),
                    "use assign_resource to put it on an activity",
                ));
            }
        }
        Edit::AddCodeType {
            name,
            scope: Some(Scope::Global),
            ..
        } => {
            let held = schedules
                .iter()
                .flat_map(|s| s.activity_code_types.values());
            if held
                .map(|t| &t.name)
                .any(|held| held.eq_ignore_ascii_case(name))
            {
                return Err(CliError::new(
                    "duplicate-code-type",
                    format!("the file already has a code type named {name}"),
                    "use add_code_value to give it values",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

/// The project one edit belongs to, and a link's ends if it names them.
fn owned(
    schedules: &[Schedule],
    edit: &Edit,
    project: Option<u64>,
    (added, new_types): (&BTreeMap<&str, usize>, &BTreeMap<String, usize>),
) -> Result<Owned, CliError> {
    let ends = link_ends(schedules, edit, added)?;
    if let (Some(ends), Some(id)) = (&ends, project) {
        let (near, far) = ends.projects;
        let outside = [near, far].map(|i| schedules[i].project.id.0 != id);
        if outside == [true, true] {
            return Err(CliError::new(
                "no-such-activity",
                format!("that link isn't in project {id}"),
                "name the project it's in with --project, or leave it out",
            ));
        }
        if outside.contains(&true) {
            return Err(CliError::new(
                "link-leaves-project",
                format!("that link reaches beyond project {id}"),
                "leave out --project, so both its projects recalculate",
            ));
        }
    }
    let index = if let Some(ends) = &ends {
        ends.projects.0
    } else {
        refuse_repeated(schedules, edit)?;
        owner(schedules, edit, project, (added, new_types))?
    };
    Ok((index, ends))
}

fn has_assignments(schedule: &Schedule, activity: &Activity) -> bool {
    schedule
        .assignments
        .values()
        .any(|a| a.activity == activity.id)
}

/// What a progress edit leaves for P6: actuals are recorded on the activity,
/// and its assignments' actual units and costs aren't worked out from them,
/// except where a units percent on work under way split them.
fn assignment_warnings(schedule: &Schedule, edit: &Edit) -> Vec<String> {
    let progress = matches!(edit, Edit::SetProgress { .. } | Edit::ClearProgress { .. });
    if !progress {
        return Vec::new();
    }
    let percent = matches!(
        edit,
        Edit::SetProgress {
            percent: Some(_),
            ..
        }
    );
    // A units percent splits only labor and nonlabor resources' units, so
    // any other assignment still needs its actuals.
    let split_by_units = |activity: &Activity| {
        percent
            && totalfloat_edit::splits_units(activity)
            && schedule
                .assignments
                .values()
                .filter(|a| a.activity == activity.id)
                .all(|a| {
                    a.resource
                        .and_then(|r| schedule.resources.get(&r))
                        .is_some_and(|r| r.kind != totalfloat_model::ResourceType::Material)
                })
    };
    activities_named(edit)
        .into_iter()
        .filter(|code| {
            schedule.activity_by_code(code).is_some_and(|activity| {
                has_assignments(schedule, activity) && !split_by_units(activity)
            })
        })
        .map(|code| match edit {
            Edit::ClearProgress { .. } => {
                format!(
                    "{code}'s assignments keep their actual units and costs, so clear them in P6"
                )
            }
            _ => format!(
                "{code}'s assignments have no new actual units or costs, so record them in P6"
            ),
        })
        .collect()
}

/// Whether the batch gives every assignment on the activity new actual
/// units, so a progress edit on it leaves nothing to record in P6. It's
/// asked before the progress edit applies, while the assignments still hold
/// the actuals the file recorded.
fn units_given(schedule: &Schedule, kept: &[Kept<'_>], index: usize, code: &str) -> bool {
    let Some(activity) = schedule.activity_by_code(code) else {
        return false;
    };
    let given: Vec<(ResourceId, f64)> = kept
        .iter()
        .filter(|&&(_, _, at, ends)| at == index && ends.is_none())
        .filter_map(|&(_, edit, ..)| match edit {
            Edit::SetAssignmentUnits {
                activity,
                resource,
                actual_units: Some(units),
                ..
            } if activity == code => {
                totalfloat_edit::resource_named(schedule, resource).map(|r| (r, *units))
            }
            _ => None,
        })
        .collect();
    schedule
        .assignments
        .values()
        .filter(|a| a.activity == activity.id)
        .all(|a| {
            let recorded = a.amounts.actual_units.unwrap_or(0.0);
            a.resource.is_some_and(|r| {
                given
                    .iter()
                    .any(|&(g, units)| g == r && (units - recorded).abs() > f64::EPSILON)
            })
        })
}

/// Actuals left after a moved data date.
fn actuals_after(
    schedules: &[Schedule],
    project: Option<u64>,
    data_date: DateTime,
) -> Option<String> {
    let late: Vec<&str> = schedules
        .iter()
        .filter(|s| project.is_none_or(|id| s.project.id.0 == id))
        .flat_map(|s| s.activities.values())
        .filter(|a| {
            let after = |date: Option<DateTime>| date.is_some_and(|date| date > data_date);
            after(a.actual_start) || after(a.actual_finish)
        })
        .map(|a| a.code.as_str())
        .collect();
    (!late.is_empty()).then(|| {
        let shown: Vec<&str> = late.iter().take(MAX_LISTED).copied().collect();
        format!(
            "{} activities have actuals after the data date: {}",
            late.len(),
            shown.join(", ")
        )
    })
}

/// Estimates the engine had to ignore, which would otherwise look applied.
fn ignored_estimates(edits: &[Edit], schedules: &[Schedule]) -> Vec<String> {
    edits
        .iter()
        .filter_map(|edit| {
            let Edit::SetExpectedFinish { activity, date } = edit else {
                return None;
            };
            let wanted: DateTime = date.as_deref()?.parse().ok()?;
            let found = schedules
                .iter()
                .find_map(|s| s.activity_by_code(activity))?;
            let start = found.stored.early_start?;
            (wanted <= start).then(|| {
                format!("{activity}'s expected finish is before it can start, so it has no effect")
            })
        })
        .collect()
}

/// True if a changed calendar reaches more than one project's work.
fn shared_calendars(schedules: &[Schedule], edits: &[Edit], project: Option<u64>) -> bool {
    edits.iter().filter_map(calendar_changed).any(|wanted| {
        resolve_calendar(schedules, wanted, project)
            .is_ok_and(|id| calendar_users(schedules, id).len() > 1)
    })
}

/// Every project holds its own copy of a global calendar, so all see a change.
fn share_calendar(schedules: &mut [Schedule], changed: usize, edit: &Edit) {
    let Some(wanted) = calendar_changed(edit) else {
        return;
    };
    let [id] = calendars_named(&schedules[changed], wanted)[..] else {
        return;
    };
    let calendar = schedules[changed].calendars[&id].clone();
    for schedule in schedules {
        if let Some(copy) = schedule.calendars.get_mut(&id) {
            copy.clone_from(&calendar);
        }
    }
}

/// A project holds only the resources it uses, so one that another project in
/// the file holds is copied in to be assigned.
fn borrow_resource(schedules: &mut [Schedule], index: usize, edit: &Edit) -> Option<ResourceId> {
    let Edit::AssignResource { resource, .. } = edit else {
        return None;
    };
    if resource_named(&schedules[index], resource).is_some() {
        return None;
    }
    // Names can repeat between projects, so it has to be one resource.
    let found: BTreeMap<_, _> = schedules
        .iter()
        .filter_map(|s| {
            let id = resource_named(s, resource)?;
            Some((id, s.resources.get(&id)?.clone()))
        })
        .collect();
    if let [(id, found)] = found.into_iter().collect::<Vec<_>>().as_slice() {
        schedules[index].resources.insert(*id, found.clone());
        return Some(*id);
    }
    None
}

/// A new global code type reaches every project, as it will once the file is
/// read again.
fn share_code_types(schedules: &mut [Schedule], changed: usize) {
    let new: Vec<_> = schedules[changed]
        .activity_code_types
        .values()
        .filter(|kind| kind.source.is_none() && kind.project.is_none())
        .cloned()
        .collect();
    for schedule in schedules {
        for kind in &new {
            schedule.activity_code_types.insert(kind.id, kind.clone());
        }
    }
}

/// Project code types and values belong to the whole file, as mapping gives
/// every project all of them, so a new one reaches every project.
fn share_project_codes(schedules: &mut [Schedule], changed: usize) {
    let types: Vec<_> = schedules[changed]
        .project_code_types
        .values()
        .filter(|kind| kind.source.is_none())
        .cloned()
        .collect();
    let values: Vec<_> = schedules[changed]
        .project_codes
        .values()
        .filter(|code| code.source.is_none())
        .cloned()
        .collect();
    for schedule in schedules {
        for kind in &types {
            schedule.project_code_types.insert(kind.id, kind.clone());
        }
        for code in &values {
            schedule.project_codes.insert(code.id, code.clone());
        }
    }
}

/// A resource is the file's, so its UDF values reach every project's copy.
fn share_resource_udfs(schedules: &mut [Schedule], changed: usize) {
    let values: Vec<_> = schedules[changed]
        .resources
        .values()
        .map(|r| (r.id, r.udfs.clone()))
        .collect();
    for schedule in schedules {
        for (id, udfs) in &values {
            if let Some(resource) = schedule.resources.get_mut(id) {
                resource.udfs.clone_from(udfs);
            }
        }
    }
}

/// A resource is the file's, so its rates reach every project's copy.
fn share_resource_rates(schedules: &mut [Schedule], changed: usize) {
    let values: Vec<_> = schedules[changed]
        .resources
        .values()
        .map(|r| (r.id, r.rates.clone()))
        .collect();
    for schedule in schedules {
        for (id, rates) in &values {
            if let Some(resource) = schedule.resources.get_mut(id) {
                resource.rates.clone_from(rates);
            }
        }
    }
}

/// UDF definitions belong to the whole file, as mapping gives every project
/// all of them, so a new one reaches every project.
fn share_udf_types(schedules: &mut [Schedule], changed: usize) {
    let new: Vec<_> = schedules[changed]
        .udf_types
        .values()
        .filter(|kind| kind.source.is_none())
        .cloned()
        .collect();
    for schedule in schedules {
        for kind in &new {
            schedule.udf_types.insert(kind.id, kind.clone());
        }
    }
}

/// The OBS belongs to the whole file, as mapping gives every project all of
/// it, so a new node reaches every project.
fn share_obs(schedules: &mut [Schedule], changed: usize) {
    let new: Vec<_> = schedules[changed]
        .obs
        .values()
        .filter(|node| node.source.is_none())
        .cloned()
        .collect();
    for schedule in schedules {
        for node in &new {
            schedule.obs.insert(node.id, node.clone());
        }
    }
}

/// Roles belong to the whole file, as mapping gives every project all of
/// them, so a new one reaches every project.
fn share_roles(schedules: &mut [Schedule], changed: usize) {
    let new: Vec<_> = schedules[changed]
        .roles
        .values()
        .filter(|role| role.source.is_none())
        .cloned()
        .collect();
    for schedule in schedules {
        for role in &new {
            schedule.roles.insert(role.id, role.clone());
        }
    }
}

/// A role's rates are the file's, so a limit set through one project reaches
/// every project's copy.
fn share_role_rates(schedules: &mut [Schedule], changed: usize) {
    let values: Vec<_> = schedules[changed]
        .roles
        .values()
        .map(|r| (r.id, r.rates.clone()))
        .collect();
    for schedule in schedules {
        for (id, rates) in &values {
            if let Some(role) = schedule.roles.get_mut(id) {
                role.rates.clone_from(rates);
            }
        }
    }
}

/// A code type can be global, so a new value reaches each project holding it.
fn share_code_values(schedules: &mut [Schedule], changed: usize) {
    let new: Vec<_> = schedules[changed]
        .activity_codes
        .values()
        .filter(|code| code.source.is_none())
        .cloned()
        .collect();
    for schedule in schedules {
        for code in &new {
            if schedule.activity_code_types.contains_key(&code.code_type) {
                schedule.activity_codes.insert(code.id, code.clone());
            }
        }
    }
}

/// True if another project's dates can depend on this one's.
fn linked(schedule: &Schedule) -> bool {
    schedule
        .relationships
        .values()
        .any(|link| schedule.is_external(link))
}

/// Which project to calculate, and what limiting it to one leaves out. A project
/// with no links out calculates alone, so a neighbor the engine refuses doesn't
/// stop it. A moved data date reaches every project.
fn scope(
    schedules: &[Schedule],
    edits: &[Edit],
    touched: &[usize],
    project: Option<u64>,
    data_date: Option<DateTime>,
) -> (Option<u64>, Vec<String>) {
    let shared = shared_calendars(schedules, edits, project);
    let alone = match touched {
        [one] if data_date.is_none() && !shared && !linked(&schedules[*one]) => {
            Some(schedules[*one].project.id.0)
        }
        _ => None,
    };
    let mut warnings = Vec::new();
    if project.is_some() && touched.iter().any(|i| linked(&schedules[*i])) {
        warnings.push("this project links to others, which weren't recalculated".to_owned());
    }
    if project.is_some() && shared {
        warnings.push("other projects use this calendar, and weren't recalculated".to_owned());
    }
    (project.or(alone), warnings)
}

/// The file calculated as it stands, to measure edits against. An edit can be
/// what repairs a file the engine refuses, so a refusal only means changes are
/// measured against stored dates, except for progress as planned, which needs
/// dates the engine worked out.
fn calculated_base(
    loaded: &Loaded,
    scope: Option<u64>,
    as_planned: bool,
    critical_before: &mut BTreeMap<u64, usize>,
    warnings: &mut Vec<String>,
) -> Result<(Loaded, schedule::Kept), CliError> {
    match schedule::fresh(loaded, scope, None) {
        Ok(stands) => {
            for project in &stands.projects {
                critical_before.insert(project.id(), project.critical());
            }
            Ok((Loaded::new(stands.raw), stands.kept))
        }
        Err(error) if as_planned => Err(CliError::new(
            "as-planned-refused",
            format!(
                "progress as planned needs the schedule calculated first: {}",
                error.message
            ),
            "fix what stops `schedule`, or record progress activity by activity",
        )),
        Err(_) => {
            warnings.push(
                "the file can't be calculated as it stands, so changes are against its stored dates"
                    .to_owned(),
            );
            Ok((Loaded::new(loaded.raw.clone()), schedule::Kept::new()))
        }
    }
}

/// What an edit is measured against: the calculated file, its critical counts
/// by project, its kept quality checks when it is a held view's, and its
/// projects' calculations for the edited file's to take from.
type Base<'a> = (
    &'a Loaded,
    BTreeMap<u64, usize>,
    Option<&'a Assessed>,
    Option<&'a schedule::Kept>,
);

/// The calculated file an edit is measured against, its critical counts by
/// project, the held view's kept checks, and its calculations: `held`, where
/// it is that base for `scope`, or the file calculated now into `calculated`.
fn measured_against<'a>(
    (loaded, held, calculated): (
        &Loaded,
        Option<Held<'a>>,
        &'a mut Option<(Loaded, schedule::Kept)>,
    ),
    (scope, as_planned): (Option<u64>, bool),
    warnings: &mut Vec<String>,
) -> Result<Base<'a>, CliError> {
    let ours = |held: &Held<'_>| std::ptr::eq(held.read, loaded);
    if let Some(held) = held.filter(|held| ours(held) && held.covers(scope)) {
        let critical = held.critical.clone();
        return Ok((held.calculated, critical, Some(held.assessed), held.kept));
    }
    let mut critical_before = BTreeMap::new();
    let base = calculated_base(loaded, scope, as_planned, &mut critical_before, warnings)?;
    let (base, kept) = calculated.insert(base);
    Ok((base, critical_before, None, Some(kept)))
}

/// Edit kinds whose patched file maps back to the edited schedules field for
/// field, so a dry run can calculate the edited schedules directly.
/// `the_patched_file_maps_to_the_edited_schedules_for_each_kind_calculated_directly`
/// shows each one, and the corpus test does on real files. Adding or removing
/// a link isn't one: the rows it adds or removes change which source row the
/// model's links point to.
fn maps_to_itself(edit: &Edit) -> bool {
    matches!(
        edit,
        Edit::SetDuration { .. }
            | Edit::SetConstraint { .. }
            | Edit::ClearConstraint { .. }
            | Edit::SetProgress { .. }
            | Edit::SetExpectedFinish { .. }
            | Edit::ChangeLink { .. }
    )
}

/// What mapping the patched file gives: the `touched` projects as edited,
/// and the rest as they were, since only touched projects are patched.
fn as_mapped(base: &[Schedule], edited: Vec<Schedule>, touched: &[usize]) -> Vec<Schedule> {
    edited
        .into_iter()
        .enumerate()
        .map(|(at, schedule)| {
            if touched.contains(&at) {
                schedule
            } else {
                base[at].clone()
            }
        })
        .collect()
}

/// The edited file calculated: its projects' reports, schedules and the file
/// with their results, or without them when they won't be `written`.
/// Each takes what it can from `kept`, the base's calculations.
fn edited_calculated(
    patched: RawXer,
    reach: (Option<u64>, Option<DateTime>),
    written: bool,
    kept: Option<&schedule::Kept>,
) -> Result<EditCalculated, CliError> {
    let patched = Loaded::new(patched);
    if written {
        let mut fresh = schedule::fresh_reusing(&patched, reach, kept)?;
        let calculation = Calculation::of_whole(&patched.schedules, &mut fresh);
        return Ok((
            fresh.projects,
            fresh.schedules,
            fresh.raw,
            calculation,
            fresh.kept,
        ));
    }
    let (reports, after, kept) = schedule::fresh_unwritten(&patched, reach, kept)?;
    Ok((reports, after, patched.raw, None, kept))
}

/// The edited file calculated: its projects' reports and schedules, the
/// file, and the calculation kept for the view after it, if any.
type EditCalculated = (
    Vec<ProjectReport>,
    Vec<Schedule>,
    RawXer,
    Option<Calculation>,
    schedule::Kept,
);

/// What calculating an edited file found about every project in it, kept so
/// the view of the new head needn't calculate it again. Calculating a
/// file that was just calculated finds the same results, as long as no
/// project's dates come from another's: a link between projects reads the
/// other project's stored dates.
#[derive(Debug, Clone)]
pub struct Calculation {
    /// Relationships that drive their successor's dates.
    pub driving: BTreeSet<RelationshipId>,
    /// Activities the engine found critical.
    pub critical: BTreeSet<ActivityId>,
    /// Critical activities by project.
    pub counts: BTreeMap<u64, usize>,
    /// Each project's calculation, for the next edit's to take from.
    pub kept: schedule::Kept,
}

impl Calculation {
    /// What `fresh` found, taken from it, if it calculated every project of
    /// `schedules` and none links to another.
    fn of_whole(schedules: &[Schedule], fresh: &mut schedule::Fresh) -> Option<Self> {
        let whole = fresh.projects.len() == schedules.len() && !schedules.iter().any(linked);
        whole.then(|| Self {
            driving: std::mem::take(&mut fresh.driving),
            critical: std::mem::take(&mut fresh.critical),
            counts: fresh
                .projects
                .iter()
                .map(|project| (project.id(), project.critical()))
                .collect(),
            kept: std::mem::take(&mut fresh.kept),
        })
    }
}

/// The edited file patched and calculated. A dry run whose `edits` all
/// [`maps_to_itself`], with no progress as planned, is calculated directly,
/// without mapping the patched file back.
fn calculated_edits(
    (base, kept): (&Loaded, Option<&schedule::Kept>),
    edited: (Vec<Schedule>, &[usize], &[Schedule]),
    reach: (Option<u64>, Option<DateTime>),
    (written, as_planned, edits): (bool, bool, &[Edit]),
) -> Result<EditCalculated, CliError> {
    if !written && !as_planned && edits.iter().all(maps_to_itself) {
        return unwritten_directly((base, kept), edited, reach);
    }
    let patched =
        patch_all(&base.raw, &base.schedules, edited.2).map_err(|(_, e)| not_written(&e))?;
    edited_calculated(patched, reach, written, kept)
}

/// A dry run's edited file calculated without mapping the patched file back
///: the edits are all kinds that [`maps_to_itself`], so the edited
/// schedules are what mapping would give. The file is still patched, beside
/// the calculation, so an edit that can't be written fails here as it would
/// when recorded, and before any calculation error, as it did.
fn unwritten_directly(
    (base, kept): (&Loaded, Option<&schedule::Kept>),
    (schedules, touched, changed): (Vec<Schedule>, &[usize], &[Schedule]),
    reach: (Option<u64>, Option<DateTime>),
) -> Result<EditCalculated, CliError> {
    std::thread::scope(|threads| {
        let patch = || patch_all(&base.raw, &base.schedules, changed);
        // The main thread's stack, which the command line patched on before.
        let patching = std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn_scoped(threads, patch);
        let mapped = as_mapped(&base.schedules, schedules, touched);
        let parts = (&base.raw, mapped.as_slice(), &base.diagnostics);
        let calculated = schedule::unwritten_from(parts, reach, kept);
        let patched = match patching {
            Ok(patching) => patching
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            // No thread to be had: patch here instead.
            Err(_) => patch(),
        }
        .map_err(|(_, e)| not_written(&e))?;
        match calculated {
            Some(calculated) => {
                let (reports, after, kept) = calculated?;
                Ok((reports, after, patched, None, kept))
            }
            // The file can't hold every result, so writing them is tried.
            None => edited_calculated(patched, reach, false, kept),
        }
    })
}

/// Activities a run's own progress edits name, by project, which progress as
/// planned leaves to them, so a correction can say what really happened.
fn progressed_by_hand<'a>(edits: &'a [Edit], owners: &[usize]) -> BTreeSet<(usize, &'a str)> {
    edits
        .iter()
        .zip(owners)
        .filter_map(|(edit, index)| match edit {
            Edit::SetProgress { activity, .. } | Edit::ClearProgress { activity } => {
                Some((*index, activity.as_str()))
            }
            _ => None,
        })
        .collect()
}

/// Records the progress planned before `data_date` in each project in reach,
/// with a summary line per project ahead of each activity's line, and adds
/// each project it changes to `touched`.
fn progress_as_planned(
    schedules: &mut [Schedule],
    before: &[Schedule],
    (project, data_date): (Option<u64>, Option<DateTime>),
    by_hand: &BTreeSet<(usize, &str)>,
    (ids, warnings): (&mut NextIds, &mut Vec<String>),
) -> Result<Vec<String>, CliError> {
    let Some(data_date) = data_date else {
        return Err(CliError::new(
            "needs-data-date",
            "progress as planned needs the new data date",
            "give --data-date, or data_date over MCP",
        ));
    };
    let mut lines = Vec::new();
    for (index, schedule) in schedules.iter_mut().enumerate() {
        if project.is_some_and(|id| schedule.project.id.0 != id) {
            continue;
        }
        let was = before.get(index).and_then(|s| s.project.data_date);
        if let Some(was) = was.filter(|was| *was >= data_date) {
            return Err(CliError::new(
                "data-date-not-later",
                format!(
                    "{} is already at {was}, so there's no planned progress before {data_date}",
                    schedule.project.short_name
                ),
                "give a later data date",
            ));
        }
        let skip: BTreeSet<&str> = by_hand
            .iter()
            .filter(|(at, _)| *at == index)
            .map(|(_, code)| *code)
            .collect();
        let planned = crate::progress::as_planned(schedule, data_date, &skip)?;
        lines.push(
            format!(
                "{} progressed as planned to {data_date}: {} started, {} finished, {} left as they were, {} with no calculated dates",
                schedule.project.short_name, planned.started, planned.finished, planned.left, planned.undated
            ),
        );
        for edit in &planned.edits {
            let line =
                apply(schedule, edit, ids).map_err(|e| CliError::new(e.code, e.message, e.hint))?;
            warnings.extend(assignment_warnings(schedule, edit));
            lines.push(line);
        }
    }
    Ok(lines)
}

/// Each calculated project's impact, measured against `before`, with its
/// checks when asked to `check`, the before side kept in `assessed` if given.
fn impacts(
    (projects, after, kept): (Vec<schedule::ProjectReport>, &[Schedule], &schedule::Kept),
    before: &[Schedule],
    critical_before: &BTreeMap<u64, usize>,
    (check, assessed): (bool, Option<&Assessed>),
) -> Vec<Impact> {
    projects
        .into_iter()
        .zip(after.iter())
        .filter_map(|(calculated, now)| {
            let was = before.iter().find(|s| s.project.id == now.project.id)?;
            let previous = critical_before
                .get(&calculated.id())
                .copied()
                .unwrap_or_else(|| {
                    let stored = was.activities.values();
                    stored.filter(|a| critical(was, a)).count()
                });
            let moved = check.then(|| checks_moved((was, before, assessed), (now, after, kept)));
            Some(impact(calculated, previous, (was, now), moved))
        })
        .collect()
}

/// The result of edits: a report, and the file with them in it.
pub struct Edited {
    /// What happened.
    pub report: Report,
    /// The edited, calculated file. For a report that won't be written, such
    /// as a dry run's, the edited file without its new results.
    pub raw: totalfloat_xer::RawXer,
    /// What calculating it found about every project, where calculating it
    /// again would find the same. Only for a file that is written.
    pub calculation: Option<Calculation>,
}

/// Why the file can't take the edit, with what to do about a missing column.
fn not_written(error: &PatchError) -> CliError {
    match error {
        PatchError::MissingColumn { table, field } => {
            let instead = if (*table, *field) == ("RSRC", "def_qty_per_hr") {
                ", or leave units per hour at the default of 1"
            } else {
                ""
            };
            CliError::new(
                "edit-not-written",
                format!("the edit can't be saved in this file: {error}"),
                format!(
                    "nothing was changed; the edit needs {table}.{field}, which this file was exported without: export it from P6 again with that column{instead}"
                ),
            )
        }
        _ => no_write(error),
    }
}

/// What new rows say that their file can't hold, because its table lacks the
/// column and a default stands for it there: each such new resource reads
/// back with no units per hour.
fn unstated_defaults(raw: &RawXer, before: &[Schedule], after: &[Schedule]) -> Vec<String> {
    // The patch writer appends to a table's first section.
    let lacks = |table: &str, field: &str| {
        raw.tables()
            .iter()
            .find(|t| t.name == table)
            .is_some_and(|t| t.field_index(field).is_none())
    };
    if !lacks("RSRC", "def_qty_per_hr") {
        return Vec::new();
    }
    let known: BTreeSet<_> = before.iter().flat_map(|s| s.resources.keys()).collect();
    let mut warnings: Vec<String> = after
        .iter()
        .flat_map(|s| s.resources.values())
        .filter(|r| !known.contains(&r.id) && r.default_units_per_hour == Some(1.0))
        .map(|r| {
            format!(
                "{} is written without its 1 unit an hour: this file's RSRC table has no def_qty_per_hr column, so it reads back with none, like a resource whose cell is empty; give units when assigning it",
                r.short_name
            )
        })
        .collect();
    warnings.sort();
    warnings.dedup();
    warnings
}

fn no_write(error: impl std::fmt::Display) -> CliError {
    CliError::new(
        "edit-not-written",
        format!("the edit can't be saved in this file: {error}"),
        "nothing was changed",
    )
}

/// Makes each edit in its project, and returns what each did. A refused edit
/// leaves its project as it was, and goes in `refused` by its place in `kept`.
fn apply_each(
    schedules: &mut [Schedule],
    (kept, skipping): (&[Kept<'_>], Option<Skipping<'_>>),
    ids: &mut NextIds,
    (warnings, refused): (&mut Vec<String>, &mut BTreeMap<usize, CliError>),
) -> Vec<String> {
    let refusal = |e: EditError| CliError::new(e.code, e.message, e.hint);
    let mut applied = Vec::with_capacity(kept.len());
    for &(at, edit, index, ends) in kept {
        if let Some((original, all)) = skipping
            && needed_refused(&missing(original, edit), all, at, refused).is_some()
        {
            refused.insert(at, needs_refused());
            continue;
        }
        let mut borrowed = None;
        let line = if let Some(Ends { projects, edit }) = ends {
            if projects.0 == projects.1 {
                apply(&mut schedules[projects.0], edit, ids)
            } else {
                apply_between(schedules, edit, *projects, ids)
            }
        } else {
            borrowed = borrow_resource(schedules, index, edit);
            check_loop_across(schedules, index, edit).and_then(|()| {
                let schedule = &mut schedules[index];
                let units_set = matches!(
                    edit,
                    Edit::SetProgress { activity, .. } if units_given(schedule, kept, index, activity)
                );
                let line = apply(schedule, edit, ids)?;
                if !units_set {
                    warnings.extend(assignment_warnings(schedule, edit));
                }
                Ok(line)
            })
        };
        match line {
            Ok(line) => {
                if ends.is_none() {
                    share_calendar(schedules, index, edit);
                    if matches!(edit, Edit::AddCodeType { .. }) {
                        share_code_types(schedules, index);
                    }
                    if matches!(edit, Edit::AddCodeValue { .. }) {
                        share_code_values(schedules, index);
                    }
                    if matches!(edit, Edit::AddRole { .. }) {
                        share_roles(schedules, index);
                    }
                    if matches!(edit, Edit::SetRoleLimit { .. }) {
                        share_role_rates(schedules, index);
                    }
                    if matches!(
                        edit,
                        Edit::AddProjectCodeType { .. } | Edit::AddProjectCodeValue { .. }
                    ) {
                        share_project_codes(schedules, index);
                    }
                    if matches!(edit, Edit::AddObs { .. }) {
                        share_obs(schedules, index);
                    }
                    if matches!(edit, Edit::AddUdfType { .. }) {
                        share_udf_types(schedules, index);
                    }
                    if let Edit::SetUdf {
                        activity: None,
                        resource: Some(_),
                        ..
                    }
                    | Edit::ClearUdf {
                        activity: None,
                        resource: Some(_),
                        ..
                    } = edit
                    {
                        share_resource_udfs(schedules, index);
                    }
                    if matches!(edit, Edit::SetResourceLimit { .. }) {
                        share_resource_rates(schedules, index);
                    }
                }
                applied.push(line);
            }
            Err(error) => {
                if let Some(id) = borrowed {
                    schedules[index].resources.remove(&id);
                }
                refused.insert(at, refusal(error));
            }
        }
    }
    applied
}

/// The names an edit needs that the file doesn't hold.
fn missing(schedules: &[Schedule], edit: &Edit) -> Vec<Name> {
    let held = |name: &Name| schedules.iter().any(|s| holds(s, name));
    edit.needs().into_iter().filter(|n| !held(n)).collect()
}

/// The first refused edit before `at` that would have made one of the
/// `missing` names, by its place in the batch.
fn needed_refused(
    missing: &[Name],
    all: &[Edit],
    at: usize,
    refused: &BTreeMap<usize, CliError>,
) -> Option<usize> {
    let needs = |earlier: &Edit| {
        let made = earlier.creates();
        missing.iter().any(|n| made.iter().any(|m| m.same(n)))
    };
    refused
        .range(..at)
        .map(|(&earlier, _)| earlier)
        .find(|&earlier| needs(&all[earlier]))
}

/// What an edit skipped for needing a refused one is refused with.
fn needs_refused() -> CliError {
    CliError::new(
        "needs-refused-edit",
        "it needs an earlier edit that was refused",
        "it's skipped with that edit; correct that one and run again",
    )
}

/// The edits refused, after any flags that couldn't be read, each with where
/// it came from, the refused edit it needed, and a name it may have meant.
fn problems(
    schedules: &[Schedule],
    (all, sources): (&[Edit], &[String]),
    unread: &[Problem],
    refused: &BTreeMap<usize, CliError>,
) -> Vec<Problem> {
    let mut problems = unread.to_vec();
    for (&at, error) in refused {
        let missing = missing(schedules, &all[at]);
        let depends_on = needed_refused(&missing, all, at, refused).map(|earlier| earlier + 1);
        let mut hint = error.hint.clone();
        if depends_on.is_none() && error.code.starts_with("no-such") {
            let made = |name: &Name| {
                all[..at]
                    .iter()
                    .any(|e| e.creates().iter().any(|m| m.same(name)))
            };
            let near = missing
                .iter()
                .find(|name| !made(name))
                .map(|name| suggest(schedules, name))
                .unwrap_or_default();
            if !near.is_empty() {
                hint = format!("did you mean {}? {hint}", near.join(" or "));
            }
        }
        problems.push(Problem {
            edit: Some(at + 1),
            source: sources
                .get(at)
                .cloned()
                .unwrap_or_else(|| format!("edit {}", at + 1)),
            code: error.code,
            message: error.message.clone(),
            hint,
            depends_on,
            line: None,
            column: None,
        });
    }
    problems
}

/// The error for problems in a batch: one problem's own, or `edits-refused`
/// for several. Every problem is listed either way.
pub fn refused_error(problems: Vec<Problem>, total: usize) -> CliError {
    let mut error = match problems.as_slice() {
        [one] => CliError::new(one.code, one.message.clone(), one.hint.clone()),
        _ => CliError::new(
            "edits-refused",
            format!(
                "{} of {total} edits were refused, so nothing was changed",
                problems.len()
            ),
            "correct them and run again; each has its own hint",
        ),
    };
    error.problems = problems;
    error
}

/// How to run a set of edits.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options<'a> {
    /// Only this project (`proj_id`).
    pub project: Option<u64>,
    /// Move the data date too, as `YYYY-MM-DD HH:MM`.
    pub data_date: Option<&'a str>,
    /// Record the progress planned before the new data date first.
    pub as_planned: bool,
    /// Compare quality checks before and after.
    pub check: bool,
    /// Apply the edits that can be applied, and skip the rest with every edit
    /// that needs them.
    pub skip_failed: bool,
}

impl<'a> Options<'a> {
    /// The settings a command document gives.
    #[must_use]
    pub fn from_document(document: &'a CommandOptions) -> Self {
        Self {
            project: document.project,
            data_date: document.data_date.as_deref(),
            as_planned: document.as_planned,
            ..Self::default()
        }
    }

    /// These settings, as a command document holds them.
    #[must_use]
    pub fn settings(&self) -> CommandOptions {
        CommandOptions {
            project: self.project,
            data_date: self.data_date.map(str::to_owned),
            as_planned: self.as_planned,
        }
    }
}

impl Report {
    /// The changeset these edits recorded in a store, if any.
    #[must_use]
    pub fn recorded(&self) -> Option<&Recorded> {
        self.recorded.as_ref()
    }

    /// Adds a warning, such as an apply that looks like a rename.
    pub fn warn(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// What each edit did, and what the edits didn't do.
    #[must_use]
    pub fn notes(self) -> (Vec<String>, Vec<String>) {
        (self.applied, self.warnings)
    }

    /// True if checking was asked for and a check newly fails, so the edits
    /// shouldn't be saved.
    #[must_use]
    pub fn blocked(&self) -> bool {
        self.projects.iter().any(|p| {
            p.checks
                .as_ref()
                .is_some_and(|checks| !checks.newly_failing.is_empty())
        })
    }
}

fn nothing_to_change() -> CliError {
    CliError::new(
        "nothing-to-change",
        "no edits were given",
        "docs/guide/edit.md lists them",
    )
}

/// The projects a batch changes: each edit's, the far end of each link
/// between projects, and with `--as-planned` every project in reach.
fn touched(
    schedules: &[Schedule],
    kept: &[Kept<'_>],
    as_planned: bool,
    project: Option<u64>,
) -> Vec<usize> {
    let mut touched: Vec<usize> = kept.iter().map(|k| k.2).collect();
    touched.extend(kept.iter().filter_map(|k| k.3).map(|ends| ends.projects.1));
    if as_planned {
        // Every project in reach gets progress, so each is changed.
        touched.extend(
            schedules
                .iter()
                .enumerate()
                .filter(|(_, s)| project.is_none_or(|id| s.project.id.0 == id))
                .map(|(i, _)| i),
        );
    }
    touched.sort_unstable();
    touched.dedup();
    touched
}

/// Gives the projects in reach a new data date, if one is given.
fn move_data_date(schedules: &mut [Schedule], project: Option<u64>, data_date: Option<DateTime>) {
    if data_date.is_none() {
        return;
    }
    for schedule in schedules {
        if project.is_none_or(|id| schedule.project.id.0 == id) {
            schedule.project.data_date = data_date;
        }
    }
}

/// That assignment dates stay as they were, if any project in scope has some.
fn assignment_dates(schedules: &[Schedule], scope: Option<u64>) -> Option<String> {
    schedules
        .iter()
        .filter(|s| scope.is_none_or(|id| s.project.id.0 == id))
        .any(|s| !s.assignments.is_empty())
        .then(|| "resource assignment dates aren't recalculated".to_owned())
}

/// The refusals of a batch already known to be refused: the rest of its edits
/// are tried on the file as it stands, with no calculation, since nothing
/// will be written.
fn checked_only(
    loaded: &Loaded,
    kept: &[Kept<'_>],
    data_date: Option<DateTime>,
    project: Option<u64>,
    mut refused: BTreeMap<usize, CliError>,
    (batch, unread): ((&[Edit], &[String]), &[Problem]),
) -> CliError {
    let mut schedules = loaded.schedules.clone();
    move_data_date(&mut schedules, project, data_date);
    let mut ids = next_ids(loaded);
    apply_each(
        &mut schedules,
        (kept, None),
        &mut ids,
        (&mut Vec::new(), &mut refused),
    );
    let problems = problems(&loaded.schedules, batch, unread, &refused);
    refused_error(problems, batch.0.len() + unread.len())
}

/// Refuses output paths that exist, or that are the same file twice.
fn refuse_outputs(output: Option<&Path>, flags: &Flags) -> Result<(), CliError> {
    for path in [output, flags.emit_commands.as_deref()]
        .into_iter()
        .flatten()
    {
        refuse_existing(path)?;
    }
    if output.is_some() && output == flags.emit_commands.as_deref() {
        return Err(CliError::new(
            "output-exists",
            "-o and --emit-commands name the same file",
            "give the commands their own file",
        ));
    }
    Ok(())
}

/// What `--apply` added to the batch.
#[derive(Default)]
struct TextApplied {
    /// Where each of its edits came from in the text.
    places: Vec<totalfloat_text::At>,
    /// The text's data date, when it moves it.
    data_date: Option<String>,
    /// Deleted and added activities that look like one renamed.
    warnings: Vec<String>,
}

/// Puts the edits `--apply` asks for first in the batch.
///
/// # Errors
///
/// What [`crate::apply::from_text`] refuses, or `data-date-conflict` when
/// `--data-date` and the text give different dates.
fn apply_first(
    loaded: &Loaded,
    flags: &Flags,
    (options, json): (Options<'_>, bool),
    (edits, sources): (&mut Vec<Edit>, &mut Vec<String>),
) -> Result<TextApplied, CliError> {
    let Some(path) = &flags.apply else {
        return Ok(TextApplied::default());
    };
    let applied = text_edits(loaded, path, flags, options.project, json)?;
    if let (Some(given), Some(text)) = (options.data_date, &applied.data_date)
        && parse_date(given)?.to_string() != *text
    {
        return Err(CliError::new(
            "data-date-conflict",
            format!("--data-date is {given} and {} says {text}", path.display()),
            "give one data date: change the text, or leave out --data-date",
        ));
    }
    edits.splice(0..0, applied.edits);
    sources.splice(0..0, applied.sources);
    Ok(TextApplied {
        places: applied.places,
        data_date: applied.data_date,
        warnings: applied.warnings,
    })
}

/// When `--apply` finds nothing to change: says so, and still writes `-o`
/// (an unchanged copy) and `--emit-commands` (no edits), so scripts find
/// what they asked for.
fn nothing_applied(
    path: &Path,
    (input, output): (&Path, Option<&Path>),
    loaded: &Loaded,
    flags: &Flags,
    options: Options<'_>,
) -> Result<u8, CliError> {
    if let Some(output) = output {
        write_new(&loaded.raw, permissions_of(input)?, output)?;
    }
    if let Some(commands) = &flags.emit_commands {
        let text = serde_json::to_string_pretty(&applied_commands(&[], &[], options))
            .unwrap_or_default()
            + "\n";
        create_new_text(&text, commands)?;
    }
    print_line(&format!(
        "{} matches the schedule; nothing to change",
        path.display()
    ));
    Ok(exit::OK)
}

/// The edits `--apply` asks for, on the project the text holds.
fn text_edits(
    loaded: &Loaded,
    path: &Path,
    flags: &Flags,
    project: Option<u64>,
    json: bool,
) -> Result<crate::apply::FromText, CliError> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "file-not-readable",
            format!("can't read {}: {error}", path.display()),
            "check the path and that the file is text",
        )
    })?;
    let choice = if flags.delete_missing {
        crate::apply::Choice::Delete
    } else if flags.keep_missing {
        crate::apply::Choice::Keep
    } else {
        crate::apply::Choice::Refuse
    };
    let file = path.display().to_string();
    crate::apply::from_text(loaded, project, (&file, &source), choice, json)
}

/// Applies edits to a file that's been read, then calculates. Every edit
/// applies, or nothing does.
pub fn report(
    file: &str,
    loaded: &Loaded,
    edits: &[Edit],
    options: Options<'_>,
) -> Result<Edited, CliError> {
    report_from(file, loaded, (edits, &[]), &[], options)
}

/// [`report`], with where each edit came from, such as `--duration A1=5d`, so
/// refusals can say. Every refused edit is listed, and nothing is changed.
///
/// # Errors
///
/// One refused edit's own error, `edits-refused` listing several, or an error
/// that stops the whole batch.
pub fn report_from(
    file: &str,
    loaded: &Loaded,
    batch: (&[Edit], &[String]),
    unread: &[Problem],
    options: Options<'_>,
) -> Result<Edited, CliError> {
    report_with(file, (loaded, None), batch, unread, options, true)
}

/// A file already read and calculated, as `serve` holds the schedule it
/// shows, so an edit measured against it needn't read and calculate it again
///.
#[derive(Clone, Copy)]
pub struct Held<'a> {
    /// The snapshot it was read from.
    pub snapshot: &'a totalfloat_store::BlobHash,
    /// The file as read.
    pub read: &'a Loaded,
    /// The file calculated, every project, with no data date given: what
    /// [`schedule::fresh`] gives for it.
    pub calculated: &'a Loaded,
    /// Critical activities after calculating, by project.
    pub critical: &'a BTreeMap<u64, usize>,
    /// Its quality checks, assessed on the first edit that asks for them and
    /// kept for every later one.
    pub assessed: &'a Assessed,
    /// Its projects' calculations, for an edit's to take from.
    pub kept: Option<&'a schedule::Kept>,
}

impl Held<'_> {
    /// Whether this is the base an edit scoped to `scope` measures against:
    /// the same projects calculated, which is all of them or the file's only
    /// one.
    fn covers(&self, scope: Option<u64>) -> bool {
        match (scope, self.read.schedules.as_slice()) {
            (None, _) => true,
            (Some(id), [only]) => only.project.id.0 == id,
            (Some(_), _) => false,
        }
    }
}

/// The warnings once the edits are applied, sorted, with actuals after the
/// new data date and assignment dates that stay as they were.
fn after_applying(
    warnings: &mut Vec<String>,
    schedules: &[Schedule],
    (project, scope, data_date): (Option<u64>, Option<u64>, Option<DateTime>),
) {
    warnings.sort();
    warnings.dedup();
    if let Some(data_date) = data_date {
        warnings.extend(actuals_after(schedules, project, data_date));
    }
    warnings.extend(assignment_dates(schedules, scope));
}

/// [`report_from`], measured against `held` where it is the file's calculated
/// base, and with the results written into the file returned only when
/// `written`. A report that won't be written needs none: it comes from the
/// calculated schedules.
fn report_with(
    file: &str,
    (loaded, held): (&Loaded, Option<Held<'_>>),
    (all, sources): (&[Edit], &[String]),
    unread: &[Problem],
    options: Options<'_>,
    written: bool,
) -> Result<Edited, CliError> {
    let Options {
        project,
        data_date,
        as_planned,
        check,
        skip_failed,
    } = options;
    if all.is_empty() && unread.is_empty() && data_date.is_none() {
        return Err(nothing_to_change());
    }
    let data_date: Option<DateTime> = data_date.map(parse_date).transpose()?;
    let mut refused = BTreeMap::new();
    let batch = (all, sources);
    let with_problems = |mut error: CliError, refused: &BTreeMap<usize, CliError>| {
        if error.problems.is_empty() {
            error.problems = problems(&loaded.schedules, batch, unread, refused);
        }
        error
    };
    if let Err(error) = check_calendar_edits(&loaded.schedules, all, project, &mut refused) {
        return Err(with_problems(error, &refused));
    }
    let found = owners(&loaded.schedules, all, project, &mut refused)
        .map_err(|error| with_problems(error, &refused))?;
    let kept = kept(all, &found);
    if (!refused.is_empty() || !unread.is_empty()) && !skip_failed {
        return Err(checked_only(
            loaded,
            &kept,
            data_date,
            project,
            refused,
            (batch, unread),
        ));
    }
    let (kept_edits, owners): (Vec<Edit>, Vec<usize>) =
        kept.iter().map(|k| (k.1.clone(), k.2)).unzip();
    let edits = kept_edits.as_slice();
    let touched = touched(&loaded.schedules, &kept, as_planned, project);
    let (scope, mut warnings) = scope(&loaded.schedules, edits, &touched, project, data_date);
    let mut calculated = None;
    let (base, critical_before, assessed, calculations) = measured_against(
        (loaded, held, &mut calculated),
        (scope, as_planned),
        &mut warnings,
    )?;
    let mut schedules = base.schedules.clone();
    // Actuals are measured against the new data date.
    move_data_date(&mut schedules, project, data_date);
    let mut ids = next_ids(base);
    let mut applied = if as_planned {
        let by_hand = progressed_by_hand(edits, &owners);
        let reach = (project, data_date);
        let held = (&mut ids, &mut warnings);
        progress_as_planned(&mut schedules, &base.schedules, reach, &by_hand, held)?
    } else {
        Vec::new()
    };
    let skipping = skip_failed.then_some((loaded.schedules.as_slice(), all));
    applied.extend(apply_each(
        &mut schedules,
        (&kept, skipping),
        &mut ids,
        (&mut warnings, &mut refused),
    ));
    let skipped = problems(&loaded.schedules, batch, unread, &refused);
    if !skipped.is_empty() && !skip_failed {
        return Err(refused_error(skipped, all.len()));
    }
    after_applying(&mut warnings, &schedules, (project, scope, data_date));
    let changed: Vec<Schedule> = touched.iter().map(|i| schedules[*i].clone()).collect();
    let edited = (schedules, touched.as_slice(), changed.as_slice());
    let how = (written, as_planned, edits);
    let (reports, after, raw, found, kept) =
        calculated_edits((base, calculations), edited, (scope, data_date), how)?;
    warnings.extend(unstated_defaults(&base.raw, &base.schedules, &changed));
    warnings.extend(ignored_estimates(edits, &after));
    // A recorded edit's calculation goes on to the view after it.
    let kept = found.as_ref().map_or(&kept, |found| &found.kept);
    let projects = impacts(
        (reports, after.as_slice(), kept),
        &base.schedules,
        &critical_before,
        (check, assessed),
    );
    Ok(Edited {
        report: Report {
            schema: "totalfloat.edit/1",
            file: file.to_owned(),
            output: None,
            skipped,
            applied,
            warnings,
            projects,
            recorded: None,
            closing: None,
        },
        raw,
        calculation: found,
    })
}

/// The skipped edits, each with why: its refusal, or the edit it needed.
fn skipped_lines(s: &mut String, skipped: &[Problem]) {
    if skipped.is_empty() {
        return;
    }
    let _ = writeln!(s, "skipped {}, not applied:", skipped.len());
    for problem in skipped {
        let needed = problem
            .depends_on
            .and_then(|edit| skipped.iter().find(|p| p.edit == Some(edit)));
        let why = needed.map_or_else(
            || problem.message.clone(),
            |needed| format!("needs {}", needed.source),
        );
        let _ = writeln!(s, "  {}: {why}", problem.source);
    }
}

fn human(report: &Report) -> String {
    let mut s = String::new();
    skipped_lines(&mut s, &report.skipped);
    for line in &report.applied {
        let _ = writeln!(s, "{line}");
    }
    for line in &report.warnings {
        let _ = writeln!(s, "warning: {line}");
    }
    for p in &report.projects {
        let _ = writeln!(s, "{}", p.calculated.heading());
        let _ = writeln!(s, "  {}", p.calculated.finish_line());
        let _ = writeln!(
            s,
            "  critical: {} (was {})",
            p.calculated.critical(),
            p.previous_critical
        );
        if p.milestones_moved_total > 0 {
            let _ = writeln!(s, "  milestones moved:");
            for m in &p.milestones_moved {
                let _ = writeln!(
                    s,
                    "    {}: {} to {}",
                    m.code,
                    m.from.as_deref().unwrap_or("none"),
                    m.to.as_deref().unwrap_or("none")
                );
            }
            let more = p.milestones_moved_total - p.milestones_moved.len();
            if more > 0 {
                let _ = writeln!(s, "    and {more} more");
            }
        }
        for (label, list, total) in [
            (
                "became critical",
                &p.became_critical,
                p.became_critical_total,
            ),
            ("left critical", &p.left_critical, p.left_critical_total),
        ] {
            if !list.is_empty() {
                let more = total - list.len();
                let more = if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                };
                let _ = writeln!(s, "  {label}: {}{more}", list.join(", "));
            }
        }
        for finding in &p.findings {
            let _ = writeln!(s, "  flagged: {} {}", finding.subject, finding.detail);
        }
        if let Some(checks) = &p.checks {
            for (label, list) in [
                ("now failing", &checks.newly_failing),
                ("more offenders", &checks.more_offenders),
            ] {
                for check in list {
                    let added = if check.added.is_empty() {
                        String::new()
                    } else {
                        format!(", adds {}", check.added.join(", "))
                    };
                    let _ = writeln!(s, "  {label}: {} ({}){added}", check.name, check.measure);
                }
            }
            if !checks.newly_passing.is_empty() {
                let _ = writeln!(s, "  now passing: {}", checks.newly_passing.join(", "));
            }
        }
        if p.findings_total > p.findings.len() {
            let more = p.findings_total - p.findings.len();
            let _ = writeln!(s, "  and {more} more flagged");
        }
    }
    let _ = write!(
        s,
        "{}",
        match (&report.output, &report.closing) {
            (_, Some(closing)) => closing.clone(),
            (Some(path), None) => format!("Wrote {path}"),
            (None, None) if report.blocked() => {
                "Nothing written, because a check newly fails; leave out --check to save anyway"
                    .to_owned()
            }
            (None, None) => "Nothing written; pass -o to write a new file".to_owned(),
        }
    );
    s
}

/// The command line's settings, with any an `--edits` document gave. Both
/// may give one only if they agree.
fn with_document<'a>(
    mut options: Options<'a>,
    document: &'a CommandOptions,
) -> Result<Options<'a>, CliError> {
    let clash = |what: &str| {
        CliError::new(
            "conflicting-options",
            format!("the edits file and the command line give different {what}"),
            "give it in one place",
        )
    };
    if let (Some(a), Some(b)) = (options.project, document.project)
        && a != b
    {
        return Err(clash("projects"));
    }
    if let (Some(a), Some(b)) = (options.data_date, document.data_date.as_deref())
        && a != b
    {
        return Err(clash("data dates"));
    }
    options.project = options.project.or(document.project);
    options.data_date = options.data_date.or(document.data_date.as_deref());
    options.as_planned |= document.as_planned;
    if options.as_planned && options.data_date.is_none() {
        return Err(CliError::new(
            "invalid-edit",
            "progress as planned needs a new data date",
            "give data_date in the edits file, or --data-date",
        ));
    }
    Ok(options)
}

/// The edits that applied, with their settings, as `totalfloat.command/1`.
fn applied_commands(edits: &[Edit], skipped: &[Problem], options: Options<'_>) -> Commands {
    let dropped: BTreeSet<usize> = skipped.iter().filter_map(|p| p.edit).collect();
    let kept = edits
        .iter()
        .enumerate()
        .filter(|(at, _)| !dropped.contains(&(at + 1)))
        .map(|(_, edit)| edit.clone())
        .collect();
    Commands::new(kept, options.settings())
}

/// Refuses flags that don't fit the input: `-o` on a store, whose edits are
/// recorded in it, and a reason or author on a file, which records nothing.
fn check_store_flags(store: bool, output: Option<&Path>, flags: &Flags) -> Result<(), CliError> {
    if store && output.is_some() {
        return Err(CliError::new(
            "store-output",
            "an edit to a history store is recorded in it, not written with -o",
            "leave out -o; `totalfloat history export` writes any point as a file",
        ));
    }
    if !store && (flags.message.is_some() || flags.author.is_some() || flags.propose) {
        return Err(CliError::new(
            "not-a-store",
            "-m, --author and --propose are for a change in a history store, and this is a file",
            "start a history with `totalfloat history init`, or leave them out",
        ));
    }
    Ok(())
}

/// The file to edit: an XER, or a history store's branch (the checked-out
/// one, or one named as `plant.tfdb@what-if`), with the store and the
/// branch's head. A store is told by its first bytes, not its name.
fn read_input(located: &Located) -> Result<(Loaded, Option<(PathBuf, Head)>), CliError> {
    match located {
        Located::File(file) => Ok((Loaded::new(read_xer(file)?), None)),
        Located::Store { path, at } => {
            let (head, raw) = read_head(path, at.as_deref())?;
            Ok((Loaded::new(raw), Some((path.clone(), head))))
        }
    }
}

/// An edit to a history store, ready to record.
struct Recording<'a> {
    path: &'a Path,
    head: &'a Head,
    loaded: &'a Loaded,
    raw: &'a totalfloat_xer::RawXer,
}

/// Records an edit on its store, unless it was blocked or only looked at,
/// and says so in the report.
fn record_on(
    report: &mut Report,
    recording: &Recording<'_>,
    commands: &Commands,
    asked: Asked<'_>,
    write: bool,
) -> Result<(), CliError> {
    if !write {
        report.closing = Some(if report.blocked() {
            "Nothing recorded, because a check newly fails; leave out --check to record anyway"
                .to_owned()
        } else {
            "Nothing recorded (dry run)".to_owned()
        });
        return Ok(());
    }
    // What each name meant here, so a replay elsewhere finds it.
    let mut stored = commands.clone();
    stored.resolved = resolve(&recording.loaded.schedules, &stored.edits);
    let parts = (
        serde_json::to_string(&stored).unwrap_or_default(),
        serde_json::to_string(&report.skipped).unwrap_or_default(),
        serde_json::to_string(&*report).unwrap_or_default(),
    );
    report.recorded = record_edit(recording.path, recording.head, recording.raw, parts, asked)?;
    report.closing = Some(report.recorded.as_ref().map_or_else(
        || "Nothing changed, so nothing was recorded".to_owned(),
        |r| match &r.proposal {
            Some(name) => format!(
                "Proposed {} for {} as {name}; `history accept` or `history reject` decides",
                r.short_id, r.branch
            ),
            None => format!("Recorded {} on {}", r.short_id, r.branch),
        },
    ));
    Ok(())
}

/// Edits a store's branch (the one named, or the checked-out one) and
/// records the change, for MCP and `serve`. Nothing is recorded on a dry run
/// or when a check newly fails. With `expected`, the branch must still be at
/// that changeset, so a handle opened earlier can't record against a schedule
/// it hasn't seen.
///
/// `shown` is a head already read and calculated, as `serve` holds the one it
/// shows. It's used only if it is this head's snapshot, and then the file
/// isn't read or calculated again. A dry run's returned file may be
/// the edited one without its new results, as nothing will be written.
///
/// # Errors
///
/// Refused edits, `branch-moved`, or what the store refuses.
pub fn edit_store(
    place: (&Path, Option<&str>, Option<&str>),
    edits: &[Edit],
    held: (Options<'_>, Option<Held<'_>>),
    asked: (Asked<'_>, bool),
) -> Result<(Report, totalfloat_xer::RawXer), CliError> {
    edit_store_calculated(place, edits, held, asked).map(|(report, raw, _)| (report, raw))
}

/// [`edit_store`], with what calculating the edited file found when the edit
/// was recorded on the branch, it moved, and calculating the new head again
/// would find the same: what `serve` shows next needn't calculate it.
///
/// # Errors
///
/// As [`edit_store`].
pub fn edit_store_calculated(
    (path, at, expected): (&Path, Option<&str>, Option<&str>),
    edits: &[Edit],
    (options, shown): (Options<'_>, Option<Held<'_>>),
    (asked, dry_run): (Asked<'_>, bool),
) -> Result<(Report, totalfloat_xer::RawXer, Option<Calculation>), CliError> {
    let head = head_of(path, at)?;
    if let Some(expected) = expected
        && expected != head.id.as_str()
    {
        return Err(CliError::new(
            "branch-moved",
            format!("{} has moved on since this handle was opened", head.branch),
            "open the store again to see what changed, then edit",
        ));
    }
    let current = shown.filter(|shown| *shown.snapshot == head.snapshot);
    let read;
    let loaded = if let Some(current) = current {
        current.read
    } else {
        read = Loaded::new(read_snapshot(path, &head)?);
        &read
    };
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let label = format!("{name}@{}", head.branch);
    let Edited {
        mut report,
        raw,
        calculation,
    } = report_with(
        &label,
        (loaded, current),
        (edits, &[]),
        &[],
        options,
        !dry_run,
    )?;
    let write = !report.blocked() && !dry_run;
    let commands = applied_commands(edits, &report.skipped, options);
    let recording = Recording {
        path,
        head: &head,
        loaded,
        raw: &raw,
    };
    record_on(&mut report, &recording, &commands, asked, write)?;
    let moved = report.recorded().is_some_and(|r| r.proposal.is_none());
    Ok((report, raw, calculation.filter(|_| moved)))
}

/// Runs the command. The source file is never changed; on a history store,
/// the edits are recorded on its checked-out branch.
pub fn run(
    input: &Path,
    flags: &Flags,
    output: Option<&Path>,
    options: Options<'_>,
    json: bool,
) -> Result<u8, CliError> {
    let located = locate(input);
    let store = matches!(located, Located::Store { .. });
    check_store_flags(store, output, flags)?;
    refuse_outputs(output, flags)?;
    let rows = flags
        .progress_table
        .as_deref()
        .map(crate::table::read)
        .transpose()?
        .unwrap_or_default();
    let mut edits: Vec<Edit> = rows.iter().map(|row| row.edit.clone()).collect();
    let mut sources: Vec<String> = rows
        .iter()
        .map(|row| format!("line {}", row.line))
        .collect();
    let built = flags.edits()?;
    edits.extend(built.edits);
    sources.extend(built.sources);
    let options = with_document(options, &built.options)?;
    let (loaded, head) = read_input(&located)?;
    let options = global_change(input, flags, &loaded, options, (&mut edits, &mut sources))?;
    let options = from_sheet(input, flags, &loaded, options, (&mut edits, &mut sources))?;
    let options = from_timesheet(flags, &loaded, options, (&mut edits, &mut sources))?;
    let applied = apply_first(&loaded, flags, (options, json), (&mut edits, &mut sources))?;
    let options = Options {
        data_date: options.data_date.or(applied.data_date.as_deref()),
        ..options
    };
    if let Some(path) = &flags.apply
        && edits.is_empty()
        && options.data_date.is_none()
        && built.problems.is_empty()
    {
        return nothing_applied(path, (input, output), &loaded, flags, options);
    }
    // A store's report is kept in its history, so it names the store, not
    // the path it was reached by.
    let label = match &head {
        Some((path, head)) => {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            format!("{name}@{}", head.branch)
        }
        None => input.display().to_string(),
    };
    let batch = (edits.as_slice(), sources.as_slice());
    let Edited {
        mut report, raw, ..
    } = report_with(
        &label,
        (&loaded, None),
        batch,
        &built.problems,
        options,
        !flags.dry_run,
    )
    .map_err(|e| crate::apply::place_problems(e, &applied.places))?;
    for warning in applied.warnings {
        report.warn(warning);
    }
    let blocked = report.blocked();
    let write = !blocked && !flags.dry_run;
    if let (Some(output), true) = (output, write) {
        write_new(&raw, permissions_of(input)?, output)?;
        report.output = Some(output.display().to_string());
    }
    let commands = applied_commands(&edits, &report.skipped, options);
    if let Some((path, head)) = &head {
        let asked = Asked {
            message: flags.message.as_deref(),
            author: flags.author.as_deref(),
            propose: flags.propose,
        };
        let recording = Recording {
            path,
            head,
            loaded: &loaded,
            raw: &raw,
        };
        record_on(&mut report, &recording, &commands, asked, write)?;
    } else if flags.dry_run {
        report.closing = Some("Nothing written (dry run)".to_owned());
    }
    if let (Some(path), true) = (&flags.emit_commands, write) {
        let text = serde_json::to_string_pretty(&commands).unwrap_or_default() + "\n";
        create_new_text(&text, path)?;
    }
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&human(&report));
    }
    let skipped = !report.skipped.is_empty();
    Ok(if blocked || skipped {
        exit::FINDINGS
    } else {
        exit::OK
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use totalfloat_model::{
        Activity, ActivityId, Calendar, CalendarId, CalendarType, HoursPerPeriod, Project,
        ProjectId, Schedule, WbsId, WorkWeek,
    };

    use super::{LinkType, link, link_ends, with_units};
    use super::{calendar_owner, calendar_users, check_calendar_edits, resolve_calendar};
    use totalfloat_edit::Edit;

    fn calendar(id: u64, name: &str, base: Option<u64>) -> Calendar {
        Calendar {
            id: CalendarId(id),
            name: name.to_owned(),
            kind: CalendarType::Global,
            project: None,
            base: base.map(CalendarId),
            is_default: false,
            hours_per_period: HoursPerPeriod::default(),
            work_week: WorkWeek::default(),
            exceptions: BTreeMap::new(),
            source: None,
        }
    }

    /// A project holding calendars 1 and 2, with one activity on `uses`.
    fn project(id: u64, uses: u64) -> Schedule {
        let mut s = Schedule::new(Project::new(ProjectId(id), "AUTHORED"));
        s.calendars.insert(CalendarId(1), calendar(1, "Base", None));
        s.calendars
            .insert(CalendarId(2), calendar(2, "Built", Some(1)));
        let a = Activity::new(
            ActivityId(id),
            ProjectId(id),
            WbsId(1),
            CalendarId(uses),
            format!("A{id}"),
            "Authored",
        );
        s.activities.insert(a.id, a);
        s
    }

    /// The file calculated, as an edit is measured against it.
    fn calculated(fixture: &str) -> crate::loaded::Loaded {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/xer")
            .join(fixture);
        let read = crate::loaded::Loaded::new(crate::error::read_xer(&path).unwrap());
        crate::loaded::Loaded::new(crate::schedule::fresh(&read, None, None).unwrap().raw)
    }

    /// `edits` applied to `base` as a dry run applies them, the patched file
    /// mapped, and what the edited schedules give for it: the edited
    /// projects, the rest as they were. Why not, when the edits are refused
    /// or can't be written.
    fn mapped_and_edited(
        base: &crate::loaded::Loaded,
        edits: &[Edit],
        moved_to: Option<totalfloat_model::DateTime>,
    ) -> Result<(crate::loaded::Loaded, Vec<Schedule>), String> {
        assert!(edits.iter().all(super::maps_to_itself), "{edits:?}");
        let mut refused = BTreeMap::new();
        let found =
            super::owners(&base.schedules, edits, None, &mut refused).map_err(|e| e.message)?;
        let kept = super::kept(edits, &found);
        let touched = super::touched(&base.schedules, &kept, false, None);
        let mut schedules = base.schedules.clone();
        super::move_data_date(&mut schedules, None, moved_to);
        let mut ids = super::next_ids(base);
        let mut warnings = Vec::new();
        let held = (&mut warnings, &mut refused);
        super::apply_each(&mut schedules, (&kept, None), &mut ids, held);
        if let Some(refusal) = refused.values().next() {
            return Err(refusal.message.clone());
        }
        let changed: Vec<Schedule> = touched.iter().map(|i| schedules[*i].clone()).collect();
        let patched = totalfloat_xer::patch_all(&base.raw, &base.schedules, &changed)
            .map_err(|(_, e)| e.to_string())?;
        let mapped = crate::loaded::Loaded::new(patched);
        Ok((
            mapped,
            super::as_mapped(&base.schedules, schedules, &touched),
        ))
    }

    /// The edits the corpus test tries on one project: an unstarted and a
    /// started activity, and a link inside it and one between projects, each
    /// with its shape.
    fn corpus_cases(
        base: &crate::loaded::Loaded,
        schedule: &Schedule,
        data_date: totalfloat_model::DateTime,
    ) -> Vec<(&'static str, serde_json::Value)> {
        let project = schedule.project.id;
        let activity = schedule.activities.values().find(|a| {
            a.status == totalfloat_model::ActivityStatus::NotStarted
                && !a.activity_type.is_milestone()
                && !a.activity_type.is_span()
        });
        let started = schedule.activities.values().find(|a| {
            a.status == totalfloat_model::ActivityStatus::InProgress
                && !a.activity_type.is_milestone()
                && !a.activity_type.is_span()
        });
        let inside = schedule
            .relationships
            .values()
            .find(|l| l.predecessor_project == project && l.successor_project == project);
        let between = schedule
            .relationships
            .values()
            .find(|l| l.predecessor_project != l.successor_project);
        let mut cases = Vec::new();
        if let Some(a) = activity {
            let code = &a.code;
            let finish = a.stored.early_finish.unwrap_or(data_date);
            cases.extend([
                ("", serde_json::json!([{"op": "set_duration", "activity": code, "original": "40h", "remaining": "32h"}])),
                ("", serde_json::json!([{"op": "set_constraint", "activity": code, "constraint": "start-on-or-after", "date": data_date.to_string()}])),
                ("", serde_json::json!([
                    {"op": "set_constraint", "activity": code, "constraint": "mandatory-finish", "date": finish.to_string()},
                    {"op": "clear_constraint", "activity": code},
                ])),
                ("", serde_json::json!([{"op": "set_expected_finish", "activity": code, "date": finish.to_string()}])),
                ("", serde_json::json!([{"op": "set_progress", "activity": code, "actual_start": data_date.to_string(), "remaining": "8h"}])),
            ]);
        }
        if let Some(a) = started {
            let code = &a.code;
            cases.extend([
                ("started", serde_json::json!([{"op": "set_duration", "activity": code, "remaining": "16h"}])),
                ("started", serde_json::json!([{"op": "set_progress", "activity": code, "remaining": "8h", "percent": 60.0}])),
            ]);
        }
        // A link's ends by project and Activity ID, wherever each is.
        let code = |id| {
            base.schedules.iter().find_map(|s| {
                let a = s.activities.get(&id)?;
                Some(format!("{}:{}", s.project.short_name, a.code))
            })
        };
        for (l, place) in [(inside, "inside"), (between, "between projects")] {
            let Some(l) = l else { continue };
            if let (Some(p), Some(s)) = (code(l.predecessor), code(l.successor)) {
                let edit = serde_json::json!([{"op": "change_link", "predecessor": p, "successor": s, "lag": "8h"}]);
                cases.push((place, edit));
            }
        }
        cases
    }

    /// The direct kinds on every corpus file that can be calculated: one
    /// unstarted activity and one link per project, each kind alone. Counts
    /// only, never content. Skips when `TOTALFLOAT_CORPUS` is unset.
    #[test]
    fn on_every_corpus_file_the_patched_file_maps_to_the_edited_schedules() {
        let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
            eprintln!("TOTALFLOAT_CORPUS unset; skipping the round-trip corpus test");
            return;
        };
        let mut files = Vec::new();
        for entry in std::fs::read_dir(std::path::Path::new(&root).join("files")).unwrap() {
            for file in std::fs::read_dir(entry.unwrap().path()).unwrap() {
                files.push(file.unwrap().path());
            }
        }
        files.sort();
        let (mut checked, mut skipped, mut uncalculated) = (0, 0, 0);
        let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
        for (index, file) in files.iter().enumerate() {
            let Ok(raw) = crate::error::read_xer(file) else {
                continue;
            };
            let read = crate::loaded::Loaded::new(raw);
            let Ok(fresh) = crate::schedule::fresh(&read, None, None) else {
                uncalculated += 1;
                continue;
            };
            let base = crate::loaded::Loaded::new(fresh.raw);
            for schedule in &base.schedules {
                let Some(data_date) = schedule.project.data_date else {
                    continue;
                };
                for (shape, case) in corpus_cases(&base, schedule, data_date) {
                    let op = case[0]["op"].as_str().unwrap_or_default();
                    let kind = format!("{op} {shape}").trim().to_owned();
                    let edits: Vec<Edit> = serde_json::from_value(case).unwrap();
                    let Ok((mapped, expected)) = mapped_and_edited(&base, &edits, None) else {
                        skipped += 1;
                        continue;
                    };
                    assert!(mapped.schedules == expected, "file {index}, {kind}");
                    assert!(
                        mapped.diagnostics.omitted == base.diagnostics.omitted,
                        "file {index}, {kind}"
                    );
                    checked += 1;
                    *by_kind.entry(kind).or_default() += 1;
                }
            }
        }
        eprintln!(
            "round trip: {checked} edits checked {by_kind:?}, {skipped} refused or unwritable, {uncalculated} files not calculated"
        );
        assert!(checked > 0);
    }

    /// Edits of each kind a dry run calculates directly, on fixtures, with the
    /// data date each moves to.
    fn direct_cases() -> Vec<(&'static str, Option<&'static str>, serde_json::Value)> {
        vec![
            (
                "fresh-writer.xer",
                None,
                serde_json::json!([
                    {"op": "set_duration", "activity": "TF-001", "original": "24h"},
                ]),
            ),
            (
                "fresh-writer.xer",
                None,
                serde_json::json!([
                    {"op": "set_duration", "activity": "TF-001", "original": "13.5h", "remaining": "6.25h"},
                ]),
            ),
            (
                "fresh-writer.xer",
                Some("2026-01-12 08:00"),
                serde_json::json!([
                    {"op": "set_duration", "activity": "TF-001", "remaining": "4h"},
                ]),
            ),
            (
                "fresh-writer.xer",
                None,
                serde_json::json!([
                    {"op": "set_constraint", "activity": "TF-001", "constraint": "start-on-or-after", "date": "2026-02-02 08:00"},
                    {"op": "set_constraint", "activity": "TF-001", "constraint": "finish-on-or-before", "date": "2026-03-02 17:00", "secondary": true},
                ]),
            ),
            (
                "fresh-writer.xer",
                None,
                serde_json::json!([
                    {"op": "set_constraint", "activity": "TF-002", "constraint": "as-late-as-possible"},
                ]),
            ),
            (
                "fresh-writer.xer",
                None,
                serde_json::json!([
                    {"op": "set_constraint", "activity": "TF-001", "constraint": "mandatory-start", "date": "2026-02-02 08:00"},
                    {"op": "clear_constraint", "activity": "TF-001"},
                ]),
            ),
            (
                "fresh-writer.xer",
                None,
                serde_json::json!([
                    {"op": "change_link", "predecessor": "TF-001", "successor": "TF-002", "new_type": "SS", "lag": "8h"},
                ]),
            ),
            (
                "fresh-writer.xer",
                Some("2026-01-07 08:00"),
                serde_json::json!([
                    {"op": "set_progress", "activity": "TF-001", "actual_start": "2026-01-05 08:00", "remaining": "4h", "percent": 50.0},
                ]),
            ),
            (
                "fresh-writer.xer",
                Some("2026-01-08 08:00"),
                serde_json::json!([
                    {"op": "set_progress", "activity": "TF-001", "actual_start": "2026-01-05 08:00", "actual_finish": "2026-01-06 17:00"},
                ]),
            ),
            (
                "fresh-writer.xer",
                None,
                serde_json::json!([
                    {"op": "set_expected_finish", "activity": "TF-001", "date": "2026-01-09 17:00"},
                ]),
            ),
            (
                "two-projects.xer",
                None,
                serde_json::json!([
                    {"op": "set_duration", "activity": "A1", "original": "40h"},
                ]),
            ),
        ]
        .into_iter()
        .chain(shaped_cases())
        .collect()
    }

    /// Edits of the direct kinds on work under way with assignments, units
    /// percent complete, and a link between projects.
    fn shaped_cases() -> Vec<(&'static str, Option<&'static str>, serde_json::Value)> {
        vec![
            // Work under way with an assignment, whose units follow the edit.
            (
                "curves.xer",
                None,
                serde_json::json!([
                    {"op": "set_duration", "activity": "A1", "remaining": "16h"},
                ]),
            ),
            (
                "curves.xer",
                None,
                serde_json::json!([
                    {"op": "set_progress", "activity": "A1", "remaining": "8h", "percent": 60.0},
                ]),
            ),
            // Units percent complete, which moves the assignments' units.
            (
                "two-projects.xer",
                None,
                serde_json::json!([
                    {"op": "set_progress", "activity": "B1", "actual_start": "2026-01-05 08:00", "percent": 50.0},
                ]),
            ),
            // A link between two projects.
            (
                "two-projects.xer",
                None,
                serde_json::json!([
                    {"op": "change_link", "predecessor": "SYN1:A1", "successor": "SYN2:B1", "link_type": "SS", "lag": "8h"},
                ]),
            ),
        ]
    }

    /// For each edit kind a dry run calculates without mapping the patched
    /// file: mapping it gives the edited schedules, field for field,
    /// and leaves out the same rows.
    #[test]
    fn the_patched_file_maps_to_the_edited_schedules_for_each_kind_calculated_directly() {
        for (fixture, data_date, edits) in direct_cases() {
            let edits: Vec<Edit> = serde_json::from_value(edits.clone()).unwrap();
            assert!(edits.iter().all(super::maps_to_itself), "{edits:?}");
            let base = calculated(fixture);
            let moved_to = data_date.map(|d| crate::error::parse_date(d).unwrap());
            let (mapped, expected) = mapped_and_edited(&base, &edits, moved_to)
                .unwrap_or_else(|why| panic!("{fixture} {edits:?}: {why}"));
            assert_eq!(mapped.schedules.len(), expected.len());
            for (got, want) in mapped.schedules.iter().zip(&expected) {
                assert_eq!(got.relationships, want.relationships, "{fixture} {edits:?}");
                assert_eq!(got.activities, want.activities, "{fixture} {edits:?}");
                assert_eq!(got, want, "{fixture} {edits:?}");
            }
            assert_eq!(
                mapped.diagnostics.omitted, base.diagnostics.omitted,
                "{fixture}"
            );
            // A dry run, calculated directly, reports what the edit itself does.
            let options = super::Options {
                project: None,
                data_date,
                as_planned: false,
                check: true,
                skip_failed: false,
            };
            let report = |written| {
                let batch = (edits.as_slice(), &[] as &[String]);
                let edited =
                    super::report_with(fixture, (&base, None), batch, &[], options, written);
                serde_json::to_value(edited.unwrap().report).unwrap()
            };
            assert_eq!(report(false), report(true), "{fixture} {edits:?}");
        }
    }

    /// The IDs of the checks a dry run's first project newly fails.
    fn newly_failing(report: &super::Report) -> Vec<String> {
        let doc = serde_json::to_value(report).unwrap();
        doc["projects"][0]["checks"]["newly_failing"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_owned())
            .collect()
    }

    /// A dry run with checks against a held view assesses the view once and
    /// keeps it: a later dry run reads the kept assessment.
    #[test]
    fn a_held_view_is_assessed_once_for_every_dry_run_with_checks() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/xer/batches/001/rel-fs-positive.xer");
        let read = crate::loaded::Loaded::new(crate::error::read_xer(&path).unwrap());
        let fresh = crate::schedule::fresh(&read, None, None).unwrap();
        let critical = fresh
            .projects
            .iter()
            .map(|project| (project.id(), project.critical()))
            .collect();
        let calculated = crate::loaded::Loaded::new(fresh.raw);
        let snapshot = totalfloat_store::BlobHash::of(read.raw.source());
        let assessed = super::Assessed::new();
        let held = super::Held {
            snapshot: &snapshot,
            read: &read,
            calculated: &calculated,
            critical: &critical,
            assessed: &assessed,
            kept: None,
        };
        let unlink = [Edit::RemoveLink {
            predecessor: "A1".to_owned(),
            successor: "A2".to_owned(),
            link_type: None,
        }];
        let options = super::Options {
            project: None,
            data_date: None,
            as_planned: false,
            check: true,
            skip_failed: false,
        };
        let dry_run = |held| {
            let edited = super::report_with(
                "held",
                (&read, Some(held)),
                (&unlink, &[]),
                &[],
                options,
                false,
            )
            .unwrap();
            newly_failing(&edited.report)
        };
        let without = super::report_from("held", &read, (&unlink, &[]), &[], options).unwrap();
        let first = dry_run(held);
        assert!(first.iter().any(|id| id == "open-ends"), "{first:?}");
        assert_eq!(first, newly_failing(&without.report));
        assert_eq!(
            assessed.get(),
            Some(&super::assess_each(&calculated.schedules)),
            "the first dry run keeps the view's assessment"
        );
        // Kept as if open ends had already failed: a dry run that assessed
        // the view again would find them newly failing.
        let mut doctored = assessed.get().unwrap().clone();
        for check in doctored.values_mut().flatten() {
            if check.id == "open-ends" {
                check.outcome = totalfloat_check::Outcome::Fail;
                check.offenders = usize::MAX;
            }
        }
        let kept = super::Assessed::from(doctored);
        let second = dry_run(super::Held {
            assessed: &kept,
            ..held
        });
        assert!(!second.iter().any(|id| id == "open-ends"), "{second:?}");
    }

    #[test]
    fn a_calendar_edit_goes_to_a_project_using_it_not_the_first_holding_it() {
        let schedules = [project(1, 1), project(2, 1)];
        let mut only_second = schedules.clone();
        only_second[0].activities.clear();
        assert_eq!(calendar_owner(&only_second, "Base", None).unwrap(), 1);
    }

    #[test]
    fn work_on_a_calendar_built_on_another_counts_as_using_it() {
        let schedules = [project(1, 1), project(2, 2)];
        assert_eq!(calendar_users(&schedules, CalendarId(1)), [0, 1]);
        assert_eq!(calendar_users(&schedules, CalendarId(2)), [1]);
    }

    #[test]
    fn a_new_code_value_needs_a_project_when_two_types_share_a_name() {
        use totalfloat_model::{ActivityCodeType, ActivityCodeTypeId, CodeScope};
        let kind = |id: u64| ActivityCodeType {
            id: ActivityCodeTypeId(id),
            name: "Phase".to_owned(),
            scope: CodeScope::Project,
            project: None,
            max_length: None,
            sequence: 1,
            source: None,
        };
        let mut schedules = [project(1, 1), project(2, 1)];
        schedules[0]
            .activity_code_types
            .insert(ActivityCodeTypeId(1), kind(1));
        schedules[1]
            .activity_code_types
            .insert(ActivityCodeTypeId(1), kind(1));
        assert_eq!(super::code_owner(&schedules, &[0, 1], "phase").unwrap(), 0);
        schedules[1].activity_code_types.clear();
        schedules[1]
            .activity_code_types
            .insert(ActivityCodeTypeId(2), kind(2));
        let error = super::code_owner(&schedules, &[0, 1], "phase").unwrap_err();
        assert_eq!(error.code, "needs-project");
    }

    #[test]
    fn an_assignment_can_carry_its_units() {
        assert_eq!(with_units("CREW:40"), ("CREW", Some(40.0)));
        assert_eq!(with_units("CREW:7.5h"), ("CREW", Some(7.5)));
        assert_eq!(with_units("CREW"), ("CREW", None));
        // A name with a colon keeps it when what follows isn't a number.
        assert_eq!(with_units("SITE:CREW"), ("SITE:CREW", None));
        assert_eq!(with_units("SITE:nan"), ("SITE:nan", None));
        assert_eq!(with_units("SITE:1e3"), ("SITE:1e3", None));
        assert_eq!(with_units(":40"), (":40", None));
    }

    #[test]
    fn a_link_can_name_another_project() {
        let parts = |text| {
            let held = link(text).unwrap();
            (held.from, held.to, held.kind, held.lag.is_some())
        };
        let ends = |to: &str| ("P1:A".to_owned(), to.to_owned());
        let (from, to, kind, lag) = parts("P1:A>P2:B");
        assert_eq!((from, to), ends("P2:B"));
        assert!(kind.is_none() && !lag);
        let (from, to, kind, lag) = parts("P1:A>P2:B:SS+2d");
        assert_eq!((from, to), ends("P2:B"));
        assert_eq!(kind, Some(LinkType::SS));
        assert!(lag);
        let (from, to, kind, lag) = parts("P1:A>P2:B+2d");
        assert_eq!((from, to), ends("P2:B"));
        assert!(kind.is_none() && lag);
        // A hyphen in an Activity ID isn't a lag, even after a type's letters.
        let (from, to, kind, lag) = parts("P1:A>P2:FS-100");
        assert_eq!((from, to), ends("P2:FS-100"));
        assert!(kind.is_none() && !lag);
    }

    #[test]
    fn a_plain_end_of_a_link_has_to_be_one_project_s() {
        let mut one = project(1, 1);
        let mut two = project(2, 1);
        two.project.short_name = "TWO".to_owned();
        one.project.short_name = "ONE".to_owned();
        let add = |code: &str, s: &mut Schedule, id: u64| {
            let (project, wbs) = (s.project.id, WbsId(1));
            let activity = Activity::new(
                ActivityId(id),
                project,
                wbs,
                CalendarId(1),
                code.to_owned(),
                code.to_owned(),
            );
            s.activities.insert(activity.id, activity);
        };
        add("SAME", &mut one, 1);
        add("SAME", &mut two, 2);
        add("B", &mut two, 3);
        let schedules = [one, two];
        let edit: Edit = serde_json::from_value(serde_json::json!({
            "op": "add_link", "predecessor": "SAME", "successor": "TWO:B"
        }))
        .unwrap();
        let added = BTreeMap::new();
        let Err(error) = link_ends(&schedules, &edit, &added) else {
            panic!("a plain ID two projects hold was accepted");
        };
        assert_eq!(error.code, "ambiguous-activity");
    }

    #[test]
    fn a_name_meaning_two_calendars_is_refused() {
        let mut schedules = [project(1, 1), project(2, 1)];
        schedules[0]
            .calendars
            .insert(CalendarId(3), calendar(3, "Night", None));
        schedules[1]
            .calendars
            .insert(CalendarId(4), calendar(4, "Night", None));
        let error = resolve_calendar(&schedules, "night", None).unwrap_err();
        assert_eq!(error.code, "ambiguous-calendar");
        assert_eq!(
            resolve_calendar(&schedules, "night", Some(2)).unwrap(),
            CalendarId(4)
        );
    }

    #[test]
    fn a_calendar_and_one_built_on_it_can_t_change_together() {
        let schedules = [project(1, 1)];
        let holiday = |calendar: &str| Edit::AddCalendarException {
            calendar: calendar.to_owned(),
            date: "2026-12-25".to_owned(),
            shifts: Vec::new(),
        };
        let edits = [holiday("Base"), holiday("Built")];
        let error =
            check_calendar_edits(&schedules, &edits, None, &mut BTreeMap::new()).unwrap_err();
        assert_eq!(error.code, "related-calendars");
    }
}
