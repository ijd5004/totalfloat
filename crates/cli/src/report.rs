//! `report`: a table of activities from a template, filtered, grouped and
//! sorted, as Markdown, CSV or HTML. The rows come from `query`, so a
//! template's filters and columns mean what they mean there; grouping,
//! sorting and the layout are the report's.

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value};
use totalfloat_model::{DateTime, Minutes};

use crate::error::CliError;
use crate::loaded::Loaded;
use crate::output::{create_new_text, print, print_line, refuse_existing};
use crate::page::{Page, Row};
use crate::query::{self, Query};

/// How a report is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// A Markdown document: a heading and a table per group.
    Markdown,
    /// Comma-separated values: one header row, then a row per activity.
    Csv,
    /// One self-contained HTML page.
    Html,
}

/// The built-in templates, by name.
const BUILT_IN: [(&str, &str); 2] = [
    (
        "look-ahead",
        r#"title = "Look-ahead: work starting in the next 21 days"
columns = ["code", "name", "status", "early_start", "early_finish", "total_float_hours"]
group = "wbs_path"
sort = [{ column = "early_start" }, { column = "code" }]
days_from_data_date = 21

[filter]
status = ["not-started", "in-progress"]
"#,
    ),
    (
        "critical-path",
        r#"title = "Critical path"
columns = ["code", "name", "status", "early_start", "early_finish", "original_duration_hours", "total_float_hours"]
sort = [{ column = "early_start" }, { column = "code" }]

[filter]
critical = true
status = ["not-started", "in-progress"]
"#,
    ),
];

/// The filters a template can use: `query`'s, by their argument names.
const FILTERS: [&str; 19] = [
    "project",
    "wbs",
    "code",
    "resource",
    "status",
    "type",
    "float_min",
    "float_max",
    "critical",
    "constrained",
    "start_from",
    "start_to",
    "finish_from",
    "finish_to",
    "text",
    "preds",
    "succs",
    "driving_path_to",
    "period",
];

/// The columns a template can total: durations, units, costs and earned
/// value amounts. Floats, lags, percents and indexes don't add up; a
/// `udf:` column is totalled if its values are numbers.
const SUMMABLE: [&str; 20] = [
    "original_duration_hours",
    "remaining_duration_hours",
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
];

/// The longest window from the data date: a hundred years.
const MAX_DAYS: u32 = 36_500;

/// `query`'s filters from a TOML table, by their argument names, as a
/// template's `filter` and a threshold's `scope` give them.
///
/// # Errors
///
/// What's wrong, starting with `.NAME` for a name `query` doesn't take or
/// `: ` for a value it can't read.
pub(crate) fn filter_of(table: &toml::Table) -> Result<Query, String> {
    if let Some(key) = table.keys().find(|k| !FILTERS.contains(&k.as_str())) {
        return Err(format!(
            ".{key} isn't one of query's filters ({})",
            FILTERS.join(", ")
        ));
    }
    table
        .clone()
        .try_into()
        .map_err(|error: toml::de::Error| format!(": {}", error.message()))
}

/// A template file as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateFile {
    /// The report's heading.
    title: String,
    /// `query`'s columns, in order.
    columns: Vec<String>,
    /// `query`'s filters, by name.
    #[serde(default)]
    filter: toml::Table,
    /// A column to group rows by, a heading per value.
    group: Option<String>,
    /// Columns to sort by, first key first.
    #[serde(default)]
    sort: Vec<SortKey>,
    /// Only work starting no later than this many days after the data date.
    days_from_data_date: Option<u32>,
    /// Columns to total, per group and for the report.
    #[serde(default)]
    totals: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SortKey {
    column: String,
    #[serde(default)]
    descending: bool,
}

/// A template, read and checked.
#[derive(Debug)]
pub struct Template {
    title: String,
    columns: Vec<String>,
    filter: Query,
    group: Option<String>,
    sort: Vec<SortKey>,
    days_from_data_date: Option<u32>,
    totals: Vec<String>,
}

fn bad_template(message: String) -> CliError {
    CliError::new(
        "bad-template",
        message,
        "docs/guide/report.md sets out a template; `report look-ahead` and `report critical-path` are built in",
    )
}

impl Template {
    /// A built-in template by name, or a template file at `name`.
    ///
    /// # Errors
    ///
    /// `no-such-template`, `file-not-readable` or `bad-template`.
    pub fn named(name: &str) -> Result<Self, CliError> {
        if let Some((_, text)) = BUILT_IN.iter().find(|(built, _)| *built == name) {
            return Self::parse(text, name);
        }
        let path = Path::new(name);
        if !path.exists() {
            return Err(CliError::new(
                "no-such-template",
                format!("{name} isn't a built-in template or a file"),
                "the built-in templates are look-ahead and critical-path; or give a template file",
            ));
        }
        let text = std::fs::read_to_string(path).map_err(|error| {
            CliError::new(
                "file-not-readable",
                format!("can't read {name}: {error}"),
                "check the path and that the file is readable",
            )
        })?;
        Self::parse(&text, name)
    }

    /// A built-in template by name, never a file: what an MCP client names.
    ///
    /// # Errors
    ///
    /// `no-such-template`.
    pub fn built_in(name: &str) -> Result<Self, CliError> {
        let (_, text) = BUILT_IN
            .iter()
            .find(|(built, _)| *built == name)
            .ok_or_else(|| {
                CliError::new(
                    "no-such-template",
                    format!("{name} isn't a built-in template"),
                    "the built-in templates are look-ahead and critical-path; or give a template's TOML as text",
                )
            })?;
        Self::parse(text, name)
    }

    /// Reads a template's text.
    ///
    /// # Errors
    ///
    /// `bad-template`, saying what's wrong.
    pub fn parse(text: &str, name: &str) -> Result<Self, CliError> {
        let file: TemplateFile = toml::from_str(text)
            .map_err(|error| bad_template(format!("{name}: {}", error.message())))?;
        if file.columns.is_empty() {
            return Err(bad_template(format!("{name}: columns is empty")));
        }
        if file.columns.iter().any(|c| c == "all") {
            return Err(bad_template(format!(
                "{name}: columns lists each column a report shows; `all` is query's shorthand"
            )));
        }
        let filter = filter_of(&file.filter)
            .map_err(|problem| bad_template(format!("{name}: filter{problem}")))?;
        if file.days_from_data_date.is_some_and(|days| days > MAX_DAYS) {
            return Err(bad_template(format!(
                "{name}: days_from_data_date is at most {MAX_DAYS}"
            )));
        }
        if file.days_from_data_date.is_some() && filter.start_to.is_some() {
            return Err(bad_template(format!(
                "{name}: days_from_data_date and filter.start_to both end the window"
            )));
        }
        for total in &file.totals {
            if !file.columns.contains(total) {
                return Err(bad_template(format!(
                    "{name}: totals.{total} isn't one of the report's columns"
                )));
            }
            if !SUMMABLE.contains(&total.as_str()) && !total.starts_with("udf:") {
                return Err(bad_template(format!(
                    "{name}: totals.{total} isn't a number that adds up; totals take durations, units, costs, earned value amounts and number fields (udf:NAME)"
                )));
            }
        }
        if !file.totals.is_empty() && file.columns.iter().all(|c| file.totals.contains(c)) {
            return Err(bad_template(format!(
                "{name}: totals.{} and the rest total every column, which leaves none to label the total rows; add one, such as code",
                file.totals[0]
            )));
        }
        Ok(Self {
            title: file.title,
            columns: file.columns,
            filter,
            group: file.group,
            sort: file.sort,
            days_from_data_date: file.days_from_data_date,
            totals: file.totals,
        })
    }

    /// The columns to ask `query` for: the shown ones, then any the grouping
    /// or sorting needs.
    fn wanted(&self) -> Vec<String> {
        let mut wanted = self.columns.clone();
        let extra = self.group.iter().chain(self.sort.iter().map(|k| &k.column));
        for column in extra {
            if !wanted.contains(column) {
                wanted.push(column.clone());
            }
        }
        wanted
    }
}

/// A report, ready to write.
struct Table {
    title: String,
    file: String,
    /// The data dates of the projects covered, each once.
    data_dates: Vec<DateTime>,
    columns: Vec<String>,
    group: Option<String>,
    /// Each group's value and rows, in order; one unnamed group without
    /// grouping.
    groups: Vec<(Value, Vec<Map<String, Value>>)>,
    count: usize,
    /// The columns totalled; empty for none.
    totals: Vec<String>,
}

impl Table {
    /// The totals of `rows`, by column: a number, or nothing when no row has
    /// a value.
    fn total(&self, rows: &[Map<String, Value>]) -> Map<String, Value> {
        self.totals
            .iter()
            .map(|column| {
                let values: Vec<f64> = rows
                    .iter()
                    .filter_map(|row| row.get(column).and_then(Value::as_f64))
                    .collect();
                let total = if values.is_empty() {
                    Value::Null
                } else {
                    // Rounded to a millionth, so adding binary fractions
                    // doesn't show as 0.30000000000000004.
                    let sum: f64 = values.iter().sum();
                    Value::from((sum * 1e6).round() / 1e6)
                };
                (column.clone(), total)
            })
            .collect()
    }

    /// Every row, across the groups.
    fn every_row(&self) -> Vec<Map<String, Value>> {
        self.groups
            .iter()
            .flat_map(|(_, rows)| rows.iter().cloned())
            .collect()
    }

    /// The column a total row's label goes in: the first shown column that
    /// isn't totalled.
    fn label_column(&self) -> Option<&String> {
        self.columns.iter().find(|c| !self.totals.contains(c))
    }

    /// A total row's cells as a table shows them, labelled `label`.
    fn total_cells(&self, rows: &[Map<String, Value>], label: &str) -> Vec<String> {
        let totals = self.total(rows);
        let label_column = self.label_column();
        self.columns
            .iter()
            .map(|c| match totals.get(c) {
                Some(value) => cell(value),
                None if Some(c) == label_column => label.to_owned(),
                None => String::new(),
            })
            .collect()
    }
}

/// Refuses a `udf:` total whose field isn't a number, cost or integer in
/// any project the report covers.
fn check_totals(
    totals: &[String],
    loaded: &Loaded,
    covered: &[(u64, Option<DateTime>)],
) -> Result<(), CliError> {
    use totalfloat_model::{UdfDataType, UdfSubject};
    for total in totals {
        let Some(label) = total.strip_prefix("udf:") else {
            continue;
        };
        let text = loaded
            .schedules
            .iter()
            .filter(|s| covered.iter().any(|(id, _)| *id == s.project.id.0))
            .filter_map(|s| {
                let id = totalfloat_edit::udf_named(s, label, &UdfSubject::Activity).ok()?;
                s.udf_types.get(&id)
            })
            .find(|t| {
                !matches!(
                    t.data_type,
                    UdfDataType::Number | UdfDataType::Integer | UdfDataType::Cost
                )
            });
        if let Some(field) = text {
            let kind = match field.data_type {
                UdfDataType::StartDate | UdfDataType::FinishDate => "date",
                UdfDataType::Indicator => "indicator",
                _ => "text",
            };
            return Err(bad_template(format!(
                "totals.{total} is a {kind} field, not a number, so it can't be totalled"
            )));
        }
    }
    Ok(())
}

/// The projects a report covers: the one asked for, or every one.
fn projects(
    loaded: &Loaded,
    project: Option<u64>,
) -> Result<Vec<(u64, Option<DateTime>)>, CliError> {
    let all = loaded
        .schedules
        .iter()
        .map(|s| (s.project.id.0, s.project.data_date));
    match project {
        None => Ok(all.collect()),
        Some(id) => {
            let found: Vec<_> = all.filter(|(p, _)| *p == id).collect();
            if found.is_empty() {
                return Err(CliError::new(
                    "no-such-project",
                    format!("the file has no project {id}"),
                    "`inspect` lists the projects in the file",
                ));
            }
            Ok(found)
        }
    }
}

/// The rows `query` finds for `filter`, with a window ending `days` after
/// each project's own data date. One query covers every project, so names in
/// the filter and columns are found across them; it ends at the latest
/// window, and each activity is then held to its own project's.
fn found(
    (file, loaded): (&str, &Loaded),
    filter: &Query,
    covered: &[(u64, Option<DateTime>)],
    days: Option<u32>,
) -> Result<Vec<Map<String, Value>>, CliError> {
    let Some(days) = days else {
        return Ok(query::report(file, loaded, filter)?.rows);
    };
    let mut ends = std::collections::BTreeMap::new();
    for (id, data_date) in covered {
        let start = data_date.ok_or_else(|| {
            CliError::new(
                "no-data-date",
                format!("the template's window starts at the data date, and project {id} has none"),
                "set the data date, or use a template without days_from_data_date",
            )
        })?;
        let end = start
            .checked_add(Minutes(i64::from(days) * 1440))
            .ok_or_else(|| bad_template("days_from_data_date is too far".to_owned()))?;
        ends.insert(*id, end);
    }
    let Some(latest) = ends.values().max().copied() else {
        return Ok(Vec::new());
    };
    let mut one = filter.clone();
    one.start_to = Some(latest.to_string());
    for hidden in ["project", "id"] {
        if !one.columns.iter().any(|c| c == hidden) {
            one.columns.push(hidden.to_owned());
        }
    }
    let mut rows = query::report(file, loaded, &one)?.rows;
    // `query`'s own start: actual for finished work, early otherwise.
    let start_of = |row: &Map<String, Value>| {
        let project = row.get("project")?.as_u64()?;
        let id = row.get("id")?.as_u64()?;
        let schedule = loaded
            .schedules
            .iter()
            .find(|s| s.project.id.0 == project)?;
        let activity = schedule.activities.get(&totalfloat_model::ActivityId(id))?;
        let start = if activity.status == totalfloat_model::ActivityStatus::Completed {
            activity.actual_start
        } else {
            activity.stored.early_start
        };
        Some((project, start?))
    };
    // A relation can reach another project's work, which the query already
    // held to the latest window.
    rows.retain(|row| {
        start_of(row)
            .is_some_and(|(project, start)| ends.get(&project).is_none_or(|end| start <= *end))
    });
    Ok(rows)
}

/// Builds the report from a file already read.
///
/// # Errors
///
/// What `query` refuses, or a window with no data date to start from.
fn build(
    template: &Template,
    (file, loaded): (&str, &Loaded),
    project: Option<u64>,
) -> Result<Table, CliError> {
    let mut filter = template.filter.clone();
    if project.is_some() {
        filter.project = project;
    }
    let covered = projects(loaded, filter.project)?;
    let mut data_dates: Vec<DateTime> = covered.iter().filter_map(|(_, d)| *d).collect();
    data_dates.sort_unstable();
    data_dates.dedup();
    check_totals(&template.totals, loaded, &covered)?;
    filter.columns = template.wanted();
    filter.limit = usize::MAX;
    filter.offset = 0;
    let mut rows = found(
        (file, loaded),
        &filter,
        &covered,
        template.days_from_data_date,
    )?;
    rows.sort_by(|a, b| {
        for key in &template.sort {
            let (x, y) = (&a[&key.column], &b[&key.column]);
            // Empty values stay last whichever way the key runs.
            let order = if key.descending && !x.is_null() && !y.is_null() {
                compare(y, x)
            } else {
                compare(x, y)
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
    });
    let count = rows.len();
    let groups = match &template.group {
        None => vec![(Value::Null, rows)],
        Some(column) => {
            // A stable sort by the group keeps the sort keys' order inside
            // each group.
            let key = |row: &Map<String, Value>| row.get(column).cloned().unwrap_or(Value::Null);
            rows.sort_by(|a, b| compare(&key(a), &key(b)));
            let mut groups: Vec<(Value, Vec<Map<String, Value>>)> = Vec::new();
            for row in rows {
                let value = key(&row);
                match groups.last_mut() {
                    Some((last, members)) if *last == value => members.push(row),
                    _ => groups.push((value, vec![row])),
                }
            }
            groups
        }
    };
    Ok(Table {
        title: template.title.clone(),
        file: file.to_owned(),
        data_dates,
        columns: template.columns.clone(),
        group: template.group.clone(),
        groups,
        count,
        totals: template.totals.clone(),
    })
}

/// Values in order: numbers by size, text and dates as written, and nothing
/// last.
fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        (Value::Number(x), Value::Number(y)) => x
            .as_f64()
            .partial_cmp(&y.as_f64())
            .unwrap_or(Ordering::Equal),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::String(x), Value::String(y)) => x.cmp(y),
        _ => cell(a).cmp(&cell(b)),
    }
}

/// A value as a table shows it.
fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(true) => "yes".to_owned(),
        Value::Bool(false) => "no".to_owned(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(cell).collect::<Vec<_>>().join(", "),
        Value::Object(fields) => fields
            .iter()
            .map(|(k, v)| format!("{k}: {}", cell(v)))
            .collect::<Vec<_>>()
            .join("; "),
    }
}

/// A column's heading: the usual names for the common ones, a code type's or
/// field's own name, and otherwise the column's name in words.
fn heading(column: &str) -> String {
    match column {
        "code" => return "Activity ID".to_owned(),
        "name" => return "Activity name".to_owned(),
        "wbs" => return "WBS".to_owned(),
        "wbs_path" => return "WBS path".to_owned(),
        "obs" => return "OBS".to_owned(),
        "id" => return "ID".to_owned(),
        _ => {}
    }
    if let Some(name) = column
        .strip_prefix("code:")
        .or_else(|| column.strip_prefix("udf:"))
    {
        return name.to_owned();
    }
    let words = column.replace('_', " ");
    let mut letters = words.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// The line under a report's title.
fn subtitle(table: &Table) -> String {
    let data_date = if table.data_dates.is_empty() {
        "none".to_owned()
    } else {
        let dates: Vec<String> = table.data_dates.iter().map(ToString::to_string).collect();
        dates.join(", ")
    };
    let activities = if table.count == 1 {
        "1 activity".to_owned()
    } else {
        format!("{} activities", table.count)
    };
    let label = if table.data_dates.len() > 1 {
        "data dates"
    } else {
        "data date"
    };
    format!("{} · {label} {data_date} · {activities}", table.file)
}

/// A group's heading: the column grouped by, its value, and how many.
fn group_heading(column: &str, value: &Value, rows: usize) -> String {
    let value = if value.is_null() {
        "(none)".to_owned()
    } else {
        cell(value)
    };
    format!("{}: {value} ({rows})", heading(column))
}

/// Text as Markdown shows it literally: markup characters, and HTML's, each
/// escaped with a backslash, so it still reads plainly on a terminal; line
/// breaks become spaces.
pub(crate) fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for letter in text.chars() {
        match letter {
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '~' | '#' | '<' | '>' | '&' => {
                out.push('\\');
                out.push(letter);
            }
            '\n' | '\r' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

fn markdown(table: &Table) -> String {
    let escape = |text: String| escape_markdown(&text);
    let mut out = format!(
        "# {}\n\n{}\n",
        escape_markdown(&table.title),
        escape_markdown(&subtitle(table))
    );
    if table.count == 0 {
        out.push_str("\nNo activities match.\n");
        return out;
    }
    for (value, rows) in &table.groups {
        if let Some(column) = &table.group {
            let _ = writeln!(
                out,
                "\n## {}",
                escape(group_heading(column, value, rows.len()))
            );
        }
        out.push('\n');
        let headings: Vec<String> = table.columns.iter().map(|c| escape(heading(c))).collect();
        let _ = writeln!(out, "| {} |", headings.join(" | "));
        let _ = writeln!(out, "|{}", " --- |".repeat(headings.len()));
        for row in rows {
            let cells: Vec<String> = table
                .columns
                .iter()
                .map(|c| escape(cell(row.get(c).unwrap_or(&Value::Null))))
                .collect();
            let _ = writeln!(out, "| {} |", cells.join(" | "));
        }
        if !table.totals.is_empty() {
            let cells: Vec<String> = table
                .total_cells(rows, "Total")
                .into_iter()
                .map(|text| {
                    if text.is_empty() {
                        text
                    } else {
                        format!("**{}**", escape(text))
                    }
                })
                .collect();
            let _ = writeln!(out, "| {} |", cells.join(" | "));
        }
    }
    if table.group.is_some() && !table.totals.is_empty() {
        let _ = writeln!(out, "\n**{}**", escape(report_total(table)));
    }
    out
}

/// The line that totals a grouped report: each total column with its total.
fn report_total(table: &Table) -> String {
    let totals = table.total(&table.every_row());
    let parts: Vec<String> = table
        .totals
        .iter()
        .map(|c| format!("{} {}", heading(c), cell(&totals[c])))
        .collect();
    let activities = if table.count == 1 {
        "1 activity".to_owned()
    } else {
        format!("{} activities", table.count)
    };
    format!("Total, {activities}: {}", parts.join(" · "))
        .trim_end()
        .to_owned()
}

/// A CSV field, quoted when it holds a comma, quote or line break.
pub(crate) fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// A cell for CSV: text a spreadsheet would read as a formula gets a leading
/// `'`, so it's shown as written. Numbers are left alone.
pub(crate) fn csv_cell(value: &Value) -> String {
    let text = cell(value);
    match value {
        Value::String(_) if text.starts_with(['=', '+', '-', '@', '\t', '\r']) => {
            format!("'{text}")
        }
        _ => text,
    }
}

fn csv(table: &Table) -> String {
    // A grouped report keeps its group as the first column, so the rows can
    // be sorted and filtered again in a spreadsheet.
    let mut columns = Vec::new();
    if let Some(group) = table.group.as_ref().filter(|g| !table.columns.contains(g)) {
        columns.push(group.clone());
    }
    columns.extend(table.columns.iter().cloned());
    let mut out = columns
        .iter()
        .map(|c| csv_field(c))
        .collect::<Vec<_>>()
        .join(",");
    out.push('\n');
    for (value, rows) in &table.groups {
        for row in rows {
            let cells: Vec<String> = columns
                .iter()
                .map(|c| csv_field(&csv_cell(row.get(c).unwrap_or(&Value::Null))))
                .collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        if !table.totals.is_empty() && table.group.is_some() && !rows.is_empty() {
            let label = format!("total: {}", cell(value));
            out.push_str(&csv_total(table, &columns, rows, &label));
        }
    }
    if !table.totals.is_empty() && table.count > 0 {
        out.push_str(&csv_total(table, &columns, &table.every_row(), "total"));
    }
    out
}

/// A CSV total row: `label` in the first column, totals in theirs. When the
/// first column is itself totalled, the label goes in the first one that
/// isn't.
fn csv_total(
    table: &Table,
    columns: &[String],
    rows: &[Map<String, Value>],
    label: &str,
) -> String {
    let totals = table.total(rows);
    let label_column = columns.iter().find(|c| !table.totals.contains(c));
    let cells: Vec<String> = columns
        .iter()
        .map(|c| match totals.get(c) {
            Some(value) => csv_field(&cell(value)),
            None if Some(c) == label_column => csv_field(label),
            None => String::new(),
        })
        .collect();
    format!("{}\n", cells.join(","))
}

fn html(table: &Table) -> String {
    let mut page = Page::new(&table.title);
    page.paragraph(subtitle(table));
    if table.count == 0 {
        page.paragraph("No activities match.");
    }
    for (value, rows) in table.groups.iter().filter(|(_, rows)| !rows.is_empty()) {
        if let Some(column) = &table.group {
            page.heading(2, group_heading(column, value, rows.len()));
        }
        let headings = table.columns.iter().map(|c| heading(c)).collect();
        let cells = rows
            .iter()
            .map(|row| {
                Row::from(
                    table
                        .columns
                        .iter()
                        .map(|column| cell(row.get(column).unwrap_or(&Value::Null)))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let foot = (!table.totals.is_empty()).then(|| table.total_cells(rows, "Total"));
        page.table(headings, cells, foot);
    }
    if table.group.is_some() && !table.totals.is_empty() && table.count > 0 {
        page.strong(report_total(table));
    }
    page.html()
}

/// The report as text in `format`.
///
/// # Errors
///
/// As [`build`].
pub fn render(
    template: &Template,
    (file, loaded): (&str, &Loaded),
    project: Option<u64>,
    format: Format,
) -> Result<String, CliError> {
    let table = build(template, (file, loaded), project)?;
    Ok(match format {
        Format::Markdown => markdown(&table),
        Format::Csv => csv(&table),
        Format::Html => html(&table),
    })
}

/// Runs the command: the report on stdout, or in a new file with `-o`.
///
/// # Errors
///
/// What reading the template or the file refuses, or what `query` refuses.
pub fn run(
    template: &str,
    input: &Path,
    (project, format): (Option<u64>, Format),
    output: Option<&PathBuf>,
) -> Result<u8, CliError> {
    if let Some(output) = output {
        refuse_existing(output)?;
    }
    let template = Template::named(template)?;
    let loaded = Loaded::new(crate::location::read(input)?);
    let text = render(
        &template,
        (&input.display().to_string(), &loaded),
        project,
        format,
    )?;
    match output {
        Some(output) => {
            create_new_text(&text, output)?;
            print_line(&format!("Wrote {}", output.display()));
        }
        None => print(&text),
    }
    Ok(crate::error::exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_templates_read() {
        for (name, text) in BUILT_IN {
            Template::parse(text, name).unwrap();
        }
    }

    #[test]
    fn a_template_s_mistakes_are_named() {
        let base = "title = \"T\"\ncolumns = [\"code\"]\n";
        for (text, said) in [
            ("title = \"T\"\ncolumns = []\n", "columns is empty"),
            (&format!("{base}colour = 1\n") as &str, "colour"),
            (&format!("{base}[filter]\nbogus = 1\n"), "filter.bogus"),
            (&format!("{base}[filter]\nstatus = [\"late\"]\n"), "filter"),
            (
                &format!(
                    "{base}days_from_data_date = 7\n[filter]\nstart_to = \"2026-01-01 00:00\"\n"
                ),
                "both end the window",
            ),
        ] {
            let error = Template::parse(text, "t.toml").unwrap_err();
            assert_eq!(error.code, "bad-template", "{text}");
            assert!(error.message.contains(said), "{text}: {}", error.message);
        }
    }

    #[test]
    fn values_order_numbers_by_size_and_nothing_last() {
        let mut values = vec![
            Value::Null,
            serde_json::json!(10),
            serde_json::json!(9.5),
            serde_json::json!("B"),
            serde_json::json!("A"),
        ];
        values.sort_by(compare);
        assert_eq!(
            values,
            vec![
                serde_json::json!(9.5),
                serde_json::json!(10),
                serde_json::json!("A"),
                serde_json::json!("B"),
                Value::Null
            ]
        );
    }

    #[test]
    fn markdown_shows_activity_text_as_written() {
        assert_eq!(
            escape_markdown("**bolt** <phase> a_b | c\nd"),
            "\\*\\*bolt\\*\\* \\<phase\\> a\\_b \\| c d"
        );
    }

    #[test]
    fn a_template_can_t_ask_for_all_columns() {
        let error = Template::parse("title = \"T\"\ncolumns = [\"all\"]\n", "t.toml").unwrap_err();
        assert!(error.message.contains("`all`"), "{}", error.message);
    }

    #[test]
    fn csv_keeps_a_spreadsheet_from_running_text() {
        assert_eq!(
            csv_cell(&serde_json::json!("=HYPERLINK(\"x\")")),
            "'=HYPERLINK(\"x\")"
        );
        assert_eq!(csv_cell(&serde_json::json!("@SUM(A1)")), "'@SUM(A1)");
        assert_eq!(csv_cell(&serde_json::json!(-1.5)), "-1.5");
        assert_eq!(csv_cell(&serde_json::json!("Pour")), "Pour");
    }

    #[test]
    fn csv_quotes_what_needs_quoting() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn headings_use_the_usual_names() {
        assert_eq!(heading("code"), "Activity ID");
        assert_eq!(heading("total_float_hours"), "Total float hours");
        assert_eq!(heading("code:Phase"), "Phase");
        assert_eq!(heading("udf:Crew size"), "Crew size");
    }

    fn row(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    #[test]
    fn totals_add_fractions_without_noise_and_leave_nothing_blank() {
        let table = Table {
            title: String::new(),
            file: String::new(),
            data_dates: Vec::new(),
            columns: vec![
                "code".to_owned(),
                "budgeted_cost".to_owned(),
                "actual_cost".to_owned(),
            ],
            group: None,
            groups: Vec::new(),
            count: 0,
            totals: vec!["budgeted_cost".to_owned(), "actual_cost".to_owned()],
        };
        let rows = [
            row(&[
                ("budgeted_cost", Value::from(0.1)),
                ("actual_cost", Value::Null),
            ]),
            row(&[
                ("budgeted_cost", Value::from(0.2)),
                ("actual_cost", Value::Null),
            ]),
        ];
        assert_eq!(table.total_cells(&rows, "Total"), ["Total", "0.3", ""]);
    }
}
