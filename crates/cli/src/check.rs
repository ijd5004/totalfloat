//! `totalfloat check`: schedule quality checks, DCMA 14-point and more.

use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use totalfloat_check::{Assessment, Check, MAX_LISTED, Outcome, Thresholds, assess};
use totalfloat_model::Schedule;

use crate::error::{CliError, exit};
use crate::loaded::Loaded;
use crate::output::{create_new_text, print, print_line, refuse_existing};
use crate::page::{Cell, Format, Page, Row};

/// `totalfloat.check/1`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Report {
    #[schemars(extend("const" = "totalfloat.check/1"))]
    schema: &'static str,
    file: String,
    /// Failed checks across every project, thresholds of the user's included.
    failed: usize,
    projects: Vec<Assessment>,
    /// The user's thresholds (`[[threshold]]` in the config), per project
    ///, then those P6 stores in the file (`PROJTHRS`). Left out
    /// when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    thresholds: Vec<ThresholdCheck>,
}

/// How a value compares with a threshold's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
enum Comparison {
    #[serde(rename = "<")]
    Below,
    #[serde(rename = "<=")]
    AtMost,
    #[serde(rename = ">")]
    Above,
    #[serde(rename = ">=")]
    AtLeast,
    #[serde(rename = "=")]
    Equal,
    #[serde(rename = "!=")]
    Other,
}

impl Comparison {
    const fn sign(self) -> &'static str {
        match self {
            Self::Below => "<",
            Self::AtMost => "<=",
            Self::Above => ">",
            Self::AtLeast => ">=",
            Self::Equal => "=",
            Self::Other => "!=",
        }
    }

    fn holds(self, order: std::cmp::Ordering) -> bool {
        use std::cmp::Ordering::{Equal, Greater, Less};
        match self {
            Self::Below => order == Less,
            Self::AtMost => order != Greater,
            Self::Above => order == Greater,
            Self::AtLeast => order != Less,
            Self::Equal => order == Equal,
            Self::Other => order != Equal,
        }
    }
}

/// A threshold of the user's: activities in its scope
/// whose `column` compares with `value` as `comparison` says are raised.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserThreshold {
    /// What it's called in the report; by default the rule itself.
    name: Option<String>,
    /// One of `query`'s columns, such as `total_float_hours`.
    column: String,
    comparison: Comparison,
    /// A number, or text such as a date `"2026-06-30 17:00"`.
    value: toml::Value,
    /// `query`'s filters, for the activities it covers; every activity by default.
    #[serde(default)]
    scope: toml::Table,
    /// `scope`, read once the config is.
    #[serde(skip)]
    filter: crate::query::Query,
}

impl UserThreshold {
    fn rule(&self) -> String {
        let value = match &self.value {
            toml::Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        format!("{} {} {value}", self.column, self.comparison.sign())
    }
}

/// What a config adds to the checks: the user's own thresholds, and whether
/// the thresholds P6 stores in the file are measured too.
#[derive(Debug, Clone)]
pub struct Own {
    /// `[[threshold]]` entries, in order.
    pub thresholds: Vec<UserThreshold>,
    /// `stored_thresholds` in the config; true unless it says `false`.
    pub stored: bool,
}

impl Default for Own {
    fn default() -> Self {
        Self {
            thresholds: Vec::new(),
            stored: true,
        }
    }
}

/// A threshold measured on one project, in a check's shape.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ThresholdCheck {
    /// Internal project ID.
    project: u64,
    /// `PROJECT.proj_short_name`.
    code: String,
    /// `threshold-N`, the Nth in the config, or `p6-threshold-ID` for one
    /// stored in the file (`PROJTHRS.thresh_id`).
    id: String,
    name: String,
    outcome: Outcome,
    measure: String,
    /// Activities raised, counting all of them.
    offenders: usize,
    /// Activities in its scope.
    population: usize,
    /// The first 50 raised, in Activity ID order.
    listed: Vec<String>,
}

/// A check or a threshold, as a line of the report.
struct Line<'a> {
    outcome: Outcome,
    name: &'a str,
    measure: &'a str,
    offenders: usize,
    population: usize,
    listed: &'a [String],
}

impl<'a> From<&'a Check> for Line<'a> {
    fn from(check: &'a Check) -> Self {
        Line {
            outcome: check.outcome,
            name: check.name,
            measure: &check.measure,
            offenders: check.offenders,
            population: check.population,
            listed: &check.listed,
        }
    }
}

impl<'a> From<&'a ThresholdCheck> for Line<'a> {
    fn from(check: &'a ThresholdCheck) -> Self {
        Line {
            outcome: check.outcome,
            name: &check.name,
            measure: &check.measure,
            offenders: check.offenders,
            population: check.population,
            listed: &check.listed,
        }
    }
}

/// A project's checks, then the user's thresholds on it.
fn lines<'a>(report: &'a Report, project: &'a Assessment) -> Vec<Line<'a>> {
    project
        .checks
        .iter()
        .map(Line::from)
        .chain(
            report
                .thresholds
                .iter()
                .filter(|t| t.project == project.project)
                .map(Line::from),
        )
        .collect()
}

const fn label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "pass",
        Outcome::Fail => "FAIL",
        Outcome::NotApplicable => "n/a",
    }
}

fn offenders(line: &Line<'_>) -> String {
    let listed = line.listed.join(", ");
    match line.offenders.saturating_sub(line.listed.len()) {
        0 => listed,
        more => format!("{listed} and {more} more"),
    }
}

fn human(report: &Report) -> String {
    let mut out = String::new();
    for project in &report.projects {
        let _ = writeln!(out, "Project {} ({})", project.code, project.project);
        for line in lines(report, project) {
            let count = if line.outcome == Outcome::NotApplicable {
                String::new()
            } else {
                format!(" ({} of {})", line.offenders, line.population)
            };
            let _ = writeln!(
                out,
                "  {:<4} {}: {}{count}",
                label(line.outcome),
                line.name,
                line.measure
            );
            if line.outcome == Outcome::Fail && !line.listed.is_empty() {
                let _ = writeln!(out, "       {}", offenders(&line));
            }
        }
    }
    let _ = write!(out, "{} checks failed", report.failed);
    out
}

fn markdown(report: &Report) -> String {
    let mut out = format!("# Schedule checks: {}\n", report.file);
    for project in &report.projects {
        let _ = writeln!(
            out,
            "\n## {} ({})\n\n| Check | Result | Measure | Outside the rule |\n|---|---|---|---:|",
            project.code, project.project
        );
        let lines = lines(report, project);
        for line in &lines {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} of {} |",
                crate::report::escape_markdown(line.name),
                label(line.outcome),
                line.measure,
                line.offenders,
                line.population
            );
        }
        for line in lines
            .iter()
            .filter(|line| line.outcome == Outcome::Fail && !line.listed.is_empty())
        {
            let _ = writeln!(
                out,
                "\n### {}\n\n{}",
                crate::report::escape_markdown(line.name),
                offenders(line)
            );
        }
    }
    out
}

/// A project's heading: its code, and its name where it has one that isn't
/// the code again.
pub(crate) fn project_heading(code: &str, name: Option<&str>) -> String {
    match name.filter(|n| !n.is_empty() && *n != code) {
        Some(name) => format!("{code} · {name}"),
        None => code.to_owned(),
    }
}

/// The class that marks an outcome on a page.
const fn class(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "pass",
        Outcome::Fail => "fail",
        Outcome::NotApplicable => "na",
    }
}

/// The report as one HTML page: per project, a table of every check, then a
/// table of what each failed check lists, as many as the Markdown report.
fn html(report: &Report, loaded: &Loaded) -> String {
    let mut page = Page::marked(format!("Schedule checks: {}", report.file));
    page.paragraph(format!("{} checks failed", report.failed));
    for project in &report.projects {
        let schedule = loaded
            .schedules
            .iter()
            .find(|s| s.project.id.0 == project.project);
        let name = schedule.and_then(|s| s.project.name.as_deref());
        page.heading(2, project_heading(&project.code, name));
        let data_date = schedule
            .and_then(|s| s.project.data_date)
            .map_or_else(|| "none".to_owned(), |d| d.to_string());
        page.paragraph(format!(
            "Project ID {} · data date {data_date}",
            project.project
        ));
        let lines = lines(report, project);
        let rows = lines
            .iter()
            .map(|line| {
                let failed = line.outcome == Outcome::Fail;
                Row::marked(
                    vec![
                        Cell::from(line.name),
                        Cell::marked(label(line.outcome), Some(class(line.outcome))),
                        Cell::from(line.measure),
                        Cell::from(format!("{} of {}", line.offenders, line.population)),
                    ],
                    failed.then_some("fail"),
                )
            })
            .collect();
        page.table(
            ["Check", "Result", "Measure", "Outside the rule"]
                .map(str::to_owned)
                .to_vec(),
            rows,
            None,
        );
        let names: std::collections::BTreeMap<&str, &str> = schedule
            .map(|s| {
                s.activities
                    .values()
                    .map(|a| (a.code.as_str(), a.name.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        for line in lines
            .iter()
            .filter(|line| line.outcome == Outcome::Fail && !line.listed.is_empty())
        {
            page.heading(
                3,
                format!("{}: {} of {}", line.name, line.offenders, line.population),
            );
            // Activities get their names; relationships, such as `A1 to A2`,
            // are listed as they are.
            if line
                .listed
                .iter()
                .all(|item| names.contains_key(item.as_str()))
            {
                let rows = line
                    .listed
                    .iter()
                    .map(|code| Row::from(vec![code.as_str(), names[code.as_str()]]))
                    .collect();
                page.table(
                    vec!["Activity ID".to_owned(), "Activity name".to_owned()],
                    rows,
                    None,
                );
            } else {
                let rows = line
                    .listed
                    .iter()
                    .map(|item| Row::from(vec![item.as_str()]))
                    .collect();
                page.table(vec!["Outside the rule".to_owned()], rows, None);
            }
            let more = line.offenders.saturating_sub(line.listed.len());
            if more > 0 {
                page.paragraph(format!("And {more} more, not listed."));
            }
        }
    }
    page.html()
}

/// The report as Markdown, or as one HTML page.
pub fn render(report: &Report, loaded: &Loaded, format: Format) -> String {
    match format {
        Format::Markdown => markdown(report),
        Format::Html => html(report, loaded),
    }
}

/// Reads thresholds from a TOML file: the checks' limits, anything left out
/// keeping its default, and the user's own `[[threshold]]` entries.
///
/// # Errors
///
/// `invalid-config`, saying what's wrong.
pub fn thresholds(path: Option<&Path>) -> Result<(Thresholds, Own), CliError> {
    let Some(path) = path else {
        return Ok((Thresholds::default(), Own::default()));
    };
    let refused = |error: &dyn std::fmt::Display| {
        CliError::new(
            "invalid-config",
            format!("can't use {}: {error}", path.display()),
            "docs/CHECKS.md lists the thresholds",
        )
    };
    let text = std::fs::read_to_string(path).map_err(|e| refused(&e))?;
    let mut table: toml::Table = toml::from_str(&text).map_err(|e| refused(&e))?;
    let stored = match table.remove("stored_thresholds") {
        None => true,
        Some(toml::Value::Boolean(on)) => on,
        Some(_) => return Err(refused(&"stored_thresholds is true or false")),
    };
    let mut own: Vec<UserThreshold> = match table.remove("threshold") {
        None => Vec::new(),
        Some(list) => list.try_into().map_err(|e: toml::de::Error| refused(&e))?,
    };
    for (k, threshold) in own.iter_mut().enumerate() {
        threshold.filter = crate::report::filter_of(&threshold.scope)
            .map_err(|problem| refused(&format!("threshold {}: scope{problem}", k + 1)))?;
        if !matches!(
            threshold.value,
            toml::Value::String(_)
                | toml::Value::Integer(_)
                | toml::Value::Float(_)
                | toml::Value::Boolean(_)
        ) {
            return Err(refused(&format!(
                "threshold {}: value is a number, text or true/false",
                k + 1
            )));
        }
    }
    let limits = table.try_into().map_err(|e: toml::de::Error| refused(&e))?;
    Ok((
        limits,
        Own {
            thresholds: own,
            stored,
        },
    ))
}

/// How two values compare: numbers by size, text as written. Different kinds,
/// and empty values, including empty text, don't compare.
fn order(found: &Value, wanted: &toml::Value) -> Option<std::cmp::Ordering> {
    match (found, wanted) {
        (Value::Number(n), toml::Value::Integer(w)) => {
            #[allow(
                clippy::cast_precision_loss,
                reason = "a threshold's value is far below 2^52"
            )]
            let w = *w as f64;
            n.as_f64()?.partial_cmp(&w)
        }
        (Value::Number(n), toml::Value::Float(w)) => n.as_f64()?.partial_cmp(w),
        (Value::String(s), toml::Value::String(w)) if !s.is_empty() => {
            Some(s.as_str().cmp(w.as_str()))
        }
        (Value::Bool(b), toml::Value::Boolean(w)) => Some(b.cmp(w)),
        _ => None,
    }
}

/// What kind of value a column holds, for refusing a value of another kind.
const fn kind_of(found: &Value) -> &'static str {
    match found {
        Value::Number(_) => "numbers",
        Value::Bool(_) => "true/false",
        _ => "text",
    }
}

const fn kind_of_wanted(wanted: &toml::Value) -> &'static str {
    match wanted {
        toml::Value::Integer(_) | toml::Value::Float(_) => "numbers",
        toml::Value::Boolean(_) => "true/false",
        _ => "text",
    }
}

/// Measures the user's thresholds on each selected project. One query per
/// threshold covers them all, so names in its scope and column are found
/// across the file, as `query` finds them; each activity counts towards its
/// own project, and only a selected project's do.
fn measure(
    file: &str,
    loaded: &Loaded,
    (selected, project): (&[&Schedule], Option<u64>),
    own: &[UserThreshold],
) -> Result<Vec<ThresholdCheck>, CliError> {
    let mut out = Vec::new();
    for (k, threshold) in own.iter().enumerate() {
        let mut query = threshold.filter.clone();
        // The scope's project and `check`'s together: both must hold.
        let rows = match (query.project, project) {
            (Some(a), Some(b)) if a != b => Vec::new(),
            (scoped, checked) => {
                query.project = scoped.or(checked);
                query.columns = vec![
                    "project".to_owned(),
                    "code".to_owned(),
                    threshold.column.clone(),
                ];
                query.limit = usize::MAX;
                query.offset = 0;
                crate::query::report(file, loaded, &query)
                    .map_err(|mut error| {
                        error.message = format!("threshold {}: {}", k + 1, error.message);
                        error
                    })?
                    .rows
            }
        };
        // A value that can't compare with what the column holds would never
        // raise anything: refuse it rather than report a pass.
        if let Some(found) = rows
            .iter()
            .filter_map(|row| row.get(&threshold.column))
            .find(|found| !found.is_null() && kind_of(found) != kind_of_wanted(&threshold.value))
        {
            return Err(CliError::new(
                "invalid-config",
                format!(
                    "threshold {}: {} holds {}, and the value is {}",
                    k + 1,
                    threshold.column,
                    kind_of(found),
                    kind_of_wanted(&threshold.value)
                ),
                "give a value of the kind the column holds, such as 40 or \"2026-06-30 17:00\"",
            ));
        }
        let rule = threshold.rule();
        let mut by_project: std::collections::BTreeMap<u64, Vec<&serde_json::Map<String, Value>>> =
            std::collections::BTreeMap::new();
        for row in &rows {
            if let Some(id) = row.get("project").and_then(Value::as_u64) {
                by_project.entry(id).or_default().push(row);
            }
        }
        for schedule in selected {
            let id = schedule.project.id.0;
            let mine = by_project.get(&id).map_or(&[][..], Vec::as_slice);
            let mut raised: Vec<String> = mine
                .iter()
                .filter(|row| {
                    row.get(&threshold.column)
                        .and_then(|found| order(found, &threshold.value))
                        .is_some_and(|o| threshold.comparison.holds(o))
                })
                .filter_map(|row| row.get("code")?.as_str().map(str::to_owned))
                .collect();
            raised.sort();
            let outcome = if mine.is_empty() {
                Outcome::NotApplicable
            } else if raised.is_empty() {
                Outcome::Pass
            } else {
                Outcome::Fail
            };
            out.push(ThresholdCheck {
                project: id,
                code: schedule.project.short_name.clone(),
                id: format!("threshold-{}", k + 1),
                name: threshold.name.clone().unwrap_or_else(|| rule.clone()),
                outcome,
                measure: format!("{} of {} activities have {rule}", raised.len(), mine.len()),
                offenders: raised.len(),
                population: mine.len(),
                listed: raised.into_iter().take(MAX_LISTED).collect(),
            });
        }
    }
    // A project's thresholds together, in the config's order.
    out.sort_by_key(|t| t.project);
    Ok(out)
}

/// The `query` column a P6 threshold parameter's field is, where it's one
/// an activity holds: hours of float or duration, as the field is.
const STORED_FIELDS: [(&str, &str); 4] = [
    ("total_float_hr_cnt", "total_float_hours"),
    ("free_float_hr_cnt", "free_float_hours"),
    ("remain_drtn_hr_cnt", "remaining_duration_hours"),
    ("target_drtn_hr_cnt", "original_duration_hours"),
];

/// A number as a limit reads, such as `40` or `-7.5`.
fn limit_text(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A WBS node and every node under it.
fn under(
    schedule: &Schedule,
    node: totalfloat_model::WbsId,
) -> std::collections::BTreeSet<totalfloat_model::WbsId> {
    let mut nodes = std::collections::BTreeSet::from([node]);
    loop {
        let more: Vec<_> = schedule
            .wbs
            .values()
            .filter(|w| w.parent.is_some_and(|p| nodes.contains(&p)) && !nodes.contains(&w.id))
            .map(|w| w.id)
            .collect();
        if more.is_empty() {
            return nodes;
        }
        nodes.extend(more);
    }
}

/// The `query` column a stored threshold measures, or why it isn't measured.
fn measurable(threshold: &totalfloat_xer::monitoring::Threshold) -> Result<&'static str, String> {
    if threshold.status != "TS_Enable" {
        return Err(format!("not watched in the file ({})", threshold.status));
    }
    if threshold.level != "TM_Task" {
        return Err("watches WBS nodes as a whole, which isn't measured yet".to_owned());
    }
    if threshold.window.is_some() {
        return Err("watches a window of dates, which isn't measured yet".to_owned());
    }
    if threshold.low.is_none() && threshold.high.is_none() {
        return Err("has no limits in the file".to_owned());
    }
    STORED_FIELDS
        .iter()
        .find(|(field, _)| *field == threshold.field)
        .map(|(_, column)| *column)
        .ok_or_else(|| {
            if threshold.field.starts_with("var_") {
                format!("watches {}, which needs a baseline", threshold.field)
            } else {
                format!("watches {}, which isn't measured yet", threshold.field)
            }
        })
}

/// The thresholds P6 stores in the file (`PROJTHRS`, on `THRSPARM`
/// parameters), measured the same way: an activity in the
/// threshold's WBS node is raised when its value is below the low limit or
/// above the high one. One the file doesn't enable, or that this can't measure yet,
/// is listed as not applicable, saying why.
fn stored(
    file: &str,
    loaded: &Loaded,
    selected: &[&Schedule],
) -> Result<Vec<ThresholdCheck>, CliError> {
    let mut out = Vec::new();
    for threshold in totalfloat_xer::monitoring::thresholds(&loaded.raw) {
        let Some(schedule) = selected
            .iter()
            .find(|s| Some(s.project.id.0) == threshold.project)
        else {
            continue;
        };
        let name = if threshold.name.is_empty() {
            format!("P6 threshold {}", threshold.id)
        } else {
            format!("P6 threshold: {}", threshold.name)
        };
        let not_measured = |why: String| ThresholdCheck {
            project: schedule.project.id.0,
            code: schedule.project.short_name.clone(),
            id: format!("p6-threshold-{}", threshold.id),
            name: name.clone(),
            outcome: Outcome::NotApplicable,
            measure: why,
            offenders: 0,
            population: 0,
            listed: Vec::new(),
        };
        let column = match measurable(&threshold) {
            Ok(column) => column,
            Err(why) => {
                out.push(not_measured(why));
                continue;
            }
        };
        let query = crate::query::Query {
            project: Some(schedule.project.id.0),
            columns: vec!["id".to_owned(), "code".to_owned(), column.to_owned()],
            limit: usize::MAX,
            ..crate::query::Query::default()
        };
        // The threshold's node and everything under it, by ID: a short code
        // can name nodes in other branches too.
        let within = threshold
            .wbs
            .map(|id| under(schedule, totalfloat_model::WbsId(id)));
        let rows: Vec<_> = crate::query::report(file, loaded, &query)?
            .rows
            .into_iter()
            .filter(|row| {
                within.as_ref().is_none_or(|nodes| {
                    row.get("id")
                        .and_then(Value::as_u64)
                        .and_then(|id| schedule.activities.get(&totalfloat_model::ActivityId(id)))
                        .is_some_and(|a| nodes.contains(&a.wbs))
                })
            })
            .collect();
        let outside = |v: f64| {
            threshold.low.is_some_and(|low| v < low) || threshold.high.is_some_and(|high| v > high)
        };
        let mut raised: Vec<String> = rows
            .iter()
            .filter(|row| row.get(column).and_then(Value::as_f64).is_some_and(outside))
            .filter_map(|row| row.get("code")?.as_str().map(str::to_owned))
            .collect();
        raised.sort();
        let range = match (threshold.low, threshold.high) {
            (Some(low), Some(high)) => {
                format!("outside {} to {}", limit_text(low), limit_text(high))
            }
            (Some(low), None) => format!("below {}", limit_text(low)),
            (None, Some(high)) => format!("above {}", limit_text(high)),
            (None, None) => String::new(),
        };
        let outcome = if rows.is_empty() {
            Outcome::NotApplicable
        } else if raised.is_empty() {
            Outcome::Pass
        } else {
            Outcome::Fail
        };
        out.push(ThresholdCheck {
            project: schedule.project.id.0,
            code: schedule.project.short_name.clone(),
            id: format!("p6-threshold-{}", threshold.id),
            name,
            outcome,
            measure: format!(
                "{} of {} activities have {column} {range}",
                raised.len(),
                rows.len()
            ),
            offenders: raised.len(),
            population: rows.len(),
            listed: raised.into_iter().take(MAX_LISTED).collect(),
        });
    }
    Ok(out)
}

/// Checks a file that's already been read.
pub fn report(
    file: &str,
    loaded: &Loaded,
    project: Option<u64>,
    (thresholds, own): (&Thresholds, &Own),
) -> Result<Report, CliError> {
    let schedules = &loaded.schedules;
    let selected: Vec<&Schedule> = schedules
        .iter()
        .filter(|s| project.is_none_or(|id| s.project.id.0 == id))
        .collect();
    if selected.is_empty() {
        return Err(CliError::new(
            "no-such-project",
            "the file has no matching project",
            "`inspect` lists the projects in the file",
        ));
    }
    for schedule in &selected {
        loaded.refuse_incomplete(schedule)?;
    }
    let mut measured = measure(file, loaded, (&selected, project), &own.thresholds)?;
    if own.stored {
        measured.extend(stored(file, loaded, &selected)?);
    }
    // A project's thresholds together: the user's in the config's order, then the file's.
    measured.sort_by_key(|t| t.project);
    let projects: Vec<Assessment> = selected
        .into_iter()
        .map(|schedule| assess(schedule, schedules, thresholds))
        .collect();
    let failed = projects.iter().map(Assessment::failed).sum::<usize>()
        + measured
            .iter()
            .filter(|t| t.outcome == Outcome::Fail)
            .count();
    Ok(Report {
        schema: "totalfloat.check/1",
        file: file.to_owned(),
        failed,
        projects,
        thresholds: measured,
    })
}

/// Runs the command. Failed checks exit 3, like other findings. `format`
/// is a Markdown or HTML document instead of the text report; `output` is a
/// new file to write it to instead of stdout.
pub fn run(
    file: &Path,
    project: Option<u64>,
    config: Option<&Path>,
    (format, json): (Option<Format>, bool),
    output: Option<&Path>,
) -> Result<u8, CliError> {
    if let Some(output) = output {
        refuse_existing(output)?;
    }
    let (limits, own) = thresholds(config)?;
    let raw = crate::location::read(file)?;
    let loaded = Loaded::new(raw);
    let report = report(
        &file.display().to_string(),
        &loaded,
        project,
        (&limits, &own),
    )?;
    let text = if json {
        serde_json::to_string_pretty(&report).unwrap_or_default() + "\n"
    } else if let Some(format) = format {
        render(&report, &loaded, format)
    } else {
        human(&report) + "\n"
    };
    match output {
        Some(output) => {
            create_new_text(&text, output)?;
            print_line(&format!("Wrote {}", output.display()));
        }
        None => print(&text),
    }
    Ok(if report.failed > 0 {
        exit::FINDINGS
    } else {
        exit::OK
    })
}
