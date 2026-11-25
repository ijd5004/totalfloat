//! `totalfloat inspect`: what's in an XER file, without scheduling it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;
use totalfloat_model as m;
use totalfloat_xer::raw::{Encoding, LineEnding};

use crate::error::{CliError, exit};
use crate::loaded::Loaded;
use crate::output::{print, print_line};

/// `totalfloat.inspect/1`.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Report {
    #[schemars(extend("const" = "totalfloat.inspect/1"))]
    schema: &'static str,
    file: String,
    bytes: usize,
    encoding: &'static str,
    line_ending: &'static str,
    export: Export,
    projects: Vec<Project>,
    tables: Vec<Table>,
    calendars: Vec<Calendar>,
    activity_code_types_total: usize,
    /// Code types, up to [`MAX_LISTED`].
    activity_codes: Vec<CodeType>,
    project_code_types_total: usize,
    /// Project code types, up to [`MAX_LISTED`].
    project_codes: Vec<CodeType>,
    activity_udfs_total: usize,
    /// Activity user-defined fields, up to [`MAX_LISTED`], by label.
    activity_udfs: Vec<Udf>,
    /// Resources in the file, up to [`MAX_LISTED`].
    resources_total: usize,
    resources: Vec<Resource>,
    /// Roles in the file, up to [`MAX_LISTED`], by short name as `resources`.
    roles_total: usize,
    roles: Vec<Resource>,
    /// Cost accounts in the file, up to [`MAX_LISTED`], each by its path in
    /// `short_name` (`totalfloat_edit::cost_account_path`), which edits take.
    cost_accounts_total: usize,
    cost_accounts: Vec<Resource>,
    diagnostics: Diagnostics,
}

/// Values shown for a code type, and resources shown. Totals stay complete.
const MAX_LISTED: usize = 100;
/// Distinct diagnostics named.
const MAX_NAMED: usize = 20;

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CodeType {
    name: String,
    values_total: usize,
    values: Vec<CodeValue>,
}

/// A user-defined field.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Udf {
    /// XER: `UDFTYPE.udf_type_label`.
    label: String,
    /// `text`, `start-date`, `finish-date`, `cost`, `number`, `integer`,
    /// `indicator`, `code`, or another `UDFTYPE.logical_data_type` as written.
    data_type: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct CodeValue {
    value: String,
    description: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Resource {
    short_name: String,
    name: String,
}

#[derive(Debug, Default, Serialize, schemars::JsonSchema)]
struct StatusCounts {
    not_started: usize,
    in_progress: usize,
    completed: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Export {
    /// P6 version from the `ERMHDR` line.
    version: Option<String>,
    /// Export date from the `ERMHDR` line.
    date: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Project {
    id: u64,
    short_name: String,
    name: Option<String>,
    data_date: Option<String>,
    /// `PROJECT.plan_end_date`.
    must_finish_by: Option<String>,
    /// `PROJECT.scd_end_date`, as stored in the file.
    finish: Option<String>,
    activities: usize,
    status: StatusCounts,
    relationships: usize,
    wbs_nodes: usize,
    expenses: Expenses,
    /// The project's project codes: each type's name and the value it holds.
    codes: BTreeMap<String, String>,
    options: Options,
}

/// Every cost account by its path, which edits take. Accounts are file-wide,
/// so the first schedule holds them all.
fn cost_accounts(schedules: &[m::Schedule]) -> BTreeMap<String, Resource> {
    schedules
        .first()
        .map(|s| {
            s.cost_accounts
                .values()
                .map(|a| {
                    let path = totalfloat_edit::cost_account_path(s, a.id)
                        .unwrap_or_else(|| a.short_name.clone());
                    let account = Resource {
                        short_name: path.clone(),
                        name: a.name.clone(),
                    };
                    (path, account)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The project's expenses: how many, and their costs added up.
#[derive(Debug, Default, Serialize, schemars::JsonSchema)]
struct Expenses {
    count: usize,
    budgeted_cost: f64,
    actual_cost: f64,
    remaining_cost: f64,
}

fn expenses(s: &m::Schedule) -> Expenses {
    let cents = |value: f64| (value * 100.0).round() / 100.0;
    let mut out = Expenses {
        count: s.expenses.len(),
        ..Expenses::default()
    };
    for x in s.expenses.values() {
        out.budgeted_cost += x.budgeted_cost.unwrap_or(0.0);
        out.actual_cost += x.actual_cost.unwrap_or(0.0);
        out.remaining_cost += x.remaining_cost.unwrap_or(0.0);
    }
    out.budgeted_cost = cents(out.budgeted_cost);
    out.actual_cost = cents(out.actual_cost);
    out.remaining_cost = cents(out.remaining_cost);
    out
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "mirrors ScheduleOptions' independent flags"
)]
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Options {
    out_of_sequence: &'static str,
    lag_calendar: &'static str,
    float_type: &'static str,
    critical: String,
    make_open_ended_critical: bool,
    ss_lag_from_early_start: bool,
    use_expected_finish: bool,
    multiple_float_paths: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Table {
    name: String,
    rows: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Calendar {
    id: u64,
    name: String,
    kind: &'static str,
    project: Option<u64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Diagnostics {
    /// Raw reader findings by kind.
    reader: BTreeMap<String, usize>,
    /// Model mapping findings by kind.
    mapping: BTreeMap<String, usize>,
    /// What those findings are, most frequent first, up to [`MAX_NAMED`].
    mapping_details: Vec<Named>,
    /// Findings not in `mapping_details`: past its limit, or past the 1,000
    /// the mapper keeps details for. `mapping` still counts them.
    mapping_unnamed: usize,
    /// Raw rows left out of the model (strict conversion, TF-012).
    omitted_rows: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct Named {
    kind: String,
    field: String,
    message: String,
    count: usize,
}

/// Runs the command and returns the exit code.
pub fn run(path: &Path, json: bool) -> Result<u8, CliError> {
    let raw = crate::location::read(path)?;
    let bytes = raw.source().len();
    let report = report(&path.display().to_string(), bytes, &Loaded::new(raw));
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print(&human(&report));
    }
    Ok(exit::OK)
}

/// A calendar type as reports name it.
#[must_use]
pub const fn calendar_kind(kind: m::CalendarType) -> &'static str {
    match kind {
        m::CalendarType::Global => "global",
        m::CalendarType::Project => "project",
        m::CalendarType::Resource => "resource",
    }
}

/// Summarizes a file that's already been read.
#[must_use]
pub fn report(file: &str, bytes: usize, loaded: &Loaded) -> Report {
    let (raw, schedules, mapping) = (&loaded.raw, &loaded.schedules, &loaded.diagnostics);
    let mut calendars = BTreeMap::new();
    for s in schedules {
        for c in s.calendars.values() {
            calendars.entry(c.id.0).or_insert_with(|| Calendar {
                id: c.id.0,
                name: c.name.clone(),
                kind: calendar_kind(c.kind),
                project: c.project.map(|p| p.0),
            });
        }
    }
    let resources: BTreeMap<u64, Resource> = schedules
        .iter()
        .flat_map(|s| s.resources.values())
        .map(|r| {
            let resource = Resource {
                short_name: r.short_name.clone(),
                name: r.name.clone(),
            };
            (r.id.0, resource)
        })
        .collect();
    let roles: BTreeMap<u64, Resource> = schedules
        .iter()
        .flat_map(|s| s.roles.values())
        .map(|r| {
            let role = Resource {
                short_name: r.short_name.clone(),
                name: r.name.clone(),
            };
            (r.id.0, role)
        })
        .collect();
    let cost_accounts = cost_accounts(schedules);
    let code_types = activity_codes(schedules, MAX_LISTED);
    let project_code_types = project_codes(schedules);
    let udfs = activity_udfs(schedules);
    Report {
        schema: "totalfloat.inspect/1",
        file: file.to_owned(),
        bytes,
        encoding: match raw.encoding {
            Encoding::Utf8 { bom: true } => "utf-8 with BOM",
            Encoding::Utf8 { bom: false } => "utf-8",
            Encoding::Windows1252 => "windows-1252",
        },
        line_ending: match raw.line_ending {
            LineEnding::CrLf => "CRLF",
            LineEnding::Lf => "LF",
        },
        export: Export {
            version: raw.header.fields.first().cloned(),
            date: raw.header.fields.get(1).cloned(),
        },
        projects: schedules.iter().map(project).collect(),
        tables: raw
            .tables()
            .iter()
            .map(|t| Table {
                name: t.name.clone(),
                rows: t.rows.len(),
            })
            .collect(),
        calendars: calendars.into_values().collect(),
        activity_code_types_total: code_types.len(),
        activity_codes: code_types.into_iter().take(MAX_LISTED).collect(),
        project_code_types_total: project_code_types.len(),
        project_codes: project_code_types.into_iter().take(MAX_LISTED).collect(),
        activity_udfs_total: udfs.len(),
        activity_udfs: udfs.into_iter().take(MAX_LISTED).collect(),
        resources_total: resources.len(),
        resources: resources.into_values().take(MAX_LISTED).collect(),
        roles_total: roles.len(),
        roles: roles.into_values().take(MAX_LISTED).collect(),
        cost_accounts_total: cost_accounts.len(),
        cost_accounts: cost_accounts.into_values().take(MAX_LISTED).collect(),
        diagnostics: diagnostics(raw, mapping),
    }
}

fn diagnostics(
    raw: &totalfloat_xer::RawXer,
    mapping: &totalfloat_xer::mapping::Diagnostics,
) -> Diagnostics {
    let mut reader = BTreeMap::new();
    for d in &raw.diagnostics {
        *reader.entry(format!("{:?}", d.kind)).or_default() += 1;
    }
    let details = named(mapping);
    Diagnostics {
        reader,
        mapping: mapping
            .counts
            .iter()
            .filter(|(_, v)| **v > 0)
            .map(|(k, v)| (format!("{k:?}"), *v))
            .collect(),
        mapping_unnamed: mapping
            .counts
            .values()
            .sum::<usize>()
            .saturating_sub(details.iter().map(|n| n.count).sum()),
        mapping_details: details,
        omitted_rows: mapping.omitted.len(),
    }
}

/// The activity fields in any project, once each, by label and then id.
fn activity_udfs(schedules: &[m::Schedule]) -> Vec<Udf> {
    let mut fields: BTreeMap<(String, u64), Udf> = BTreeMap::new();
    for t in schedules
        .iter()
        .flat_map(|s| s.udf_types.values())
        .filter(|t| t.subject == m::UdfSubject::Activity)
    {
        let data_type = match &t.data_type {
            m::UdfDataType::Text => "text",
            m::UdfDataType::StartDate => "start-date",
            m::UdfDataType::FinishDate => "finish-date",
            m::UdfDataType::Cost => "cost",
            m::UdfDataType::Number => "number",
            m::UdfDataType::Integer => "integer",
            m::UdfDataType::Indicator => "indicator",
            m::UdfDataType::Code => "code",
            m::UdfDataType::Other(code) => code,
        };
        fields
            .entry((t.label.to_lowercase(), t.id.0))
            .or_insert_with(|| Udf {
                label: t.label.clone(),
                data_type: data_type.to_owned(),
            });
    }
    fields.into_values().collect()
}

/// Every activity code type, each with up to `limit` values in sequence.
fn activity_codes(schedules: &[m::Schedule], limit: usize) -> Vec<CodeType> {
    let mut types: BTreeMap<u64, CodeType> = BTreeMap::new();
    for s in schedules {
        for t in s.activity_code_types.values() {
            types.entry(t.id.0).or_insert_with(|| {
                let mut values: Vec<_> = s
                    .activity_codes
                    .values()
                    .filter(|c| c.code_type == t.id)
                    .collect();
                values.sort_by_key(|c| (c.sequence, c.id));
                CodeType {
                    name: t.name.clone(),
                    values_total: values.len(),
                    values: values
                        .into_iter()
                        .take(limit)
                        .map(|c| CodeValue {
                            value: c.value.clone(),
                            description: c.description.clone(),
                        })
                        .collect(),
                }
            });
        }
    }
    types.into_values().collect()
}

/// `GET /api/1/fields`: every activity code type with all its values in
/// sequence, and every activity user-defined field, with no cap, for the
/// app's filter builder and Group menu. `inspect` lists at most
/// [`MAX_LISTED`] of each.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Fields {
    #[schemars(extend("const" = "totalfloat.fields/1"))]
    schema: &'static str,
    /// The activity code types, as `inspect` orders them, each with every
    /// value in its sequence.
    activity_codes: Vec<FieldCodes>,
    /// The activity user-defined fields, by label.
    activity_udfs: Vec<Udf>,
}

/// An activity code type and every value it has.
#[derive(Debug, Serialize, schemars::JsonSchema)]
struct FieldCodes {
    /// XER: `ACTVTYPE.actv_code_type`.
    name: String,
    /// In the code type's sequence.
    values: Vec<CodeValue>,
}

/// Every activity code type and user-defined field in `schedules`.
#[must_use]
pub fn fields(schedules: &[m::Schedule]) -> Fields {
    Fields {
        schema: "totalfloat.fields/1",
        activity_codes: activity_codes(schedules, usize::MAX)
            .into_iter()
            .map(|t| FieldCodes {
                name: t.name,
                values: t.values,
            })
            .collect(),
        activity_udfs: activity_udfs(schedules),
    }
}

/// Project code types, which every project holds, with their values.
fn project_codes(schedules: &[m::Schedule]) -> Vec<CodeType> {
    let Some(s) = schedules.first() else {
        return Vec::new();
    };
    let mut kinds: Vec<_> = s.project_code_types.values().collect();
    kinds.sort_by_key(|t| (t.sequence, t.id));
    kinds
        .into_iter()
        .map(|t| {
            let mut values: Vec<_> = s
                .project_codes
                .values()
                .filter(|c| c.code_type == t.id)
                .collect();
            values.sort_by_key(|c| (c.sequence, c.id));
            CodeType {
                name: t.name.clone(),
                values_total: values.len(),
                values: values
                    .into_iter()
                    .take(MAX_LISTED)
                    .map(|c| CodeValue {
                        value: c.value.clone(),
                        description: c.description.clone(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// A project's codes, by type name.
fn codes_of(s: &m::Schedule) -> BTreeMap<String, String> {
    s.project
        .codes
        .iter()
        .filter_map(|(kind, code)| {
            let kind = s.project_code_types.get(kind)?;
            let code = s.project_codes.get(code)?;
            Some((kind.name.clone(), code.value.clone()))
        })
        .collect()
}

/// Groups the mapping details, which never hold schedule text.
fn named(mapping: &totalfloat_xer::mapping::Diagnostics) -> Vec<Named> {
    let mut counts: BTreeMap<(String, &str, &str), usize> = BTreeMap::new();
    for d in &mapping.details {
        let key = (
            format!("{:?}", d.kind),
            d.field.as_str(),
            d.message.as_str(),
        );
        *counts.entry(key).or_default() += 1;
    }
    let mut named: Vec<Named> = counts
        .into_iter()
        .map(|((kind, field, message), count)| Named {
            kind,
            field: field.to_owned(),
            message: message.to_owned(),
            count,
        })
        .collect();
    named.sort_by_key(|n| std::cmp::Reverse(n.count));
    named.truncate(MAX_NAMED);
    named
}

fn status_counts(s: &m::Schedule) -> StatusCounts {
    let mut counts = StatusCounts::default();
    for a in s.activities.values() {
        match a.status {
            m::ActivityStatus::NotStarted => counts.not_started += 1,
            m::ActivityStatus::InProgress => counts.in_progress += 1,
            m::ActivityStatus::Completed => counts.completed += 1,
        }
    }
    counts
}

fn project(s: &m::Schedule) -> Project {
    let o = &s.project.options;
    Project {
        id: s.project.id.0,
        short_name: s.project.short_name.clone(),
        name: s.project.name.clone(),
        data_date: s.project.data_date.map(|d| d.to_string()),
        must_finish_by: s.project.must_finish_by.map(|d| d.to_string()),
        finish: s.project.stored.scheduled_finish.map(|d| d.to_string()),
        activities: s.activities.len(),
        status: status_counts(s),
        relationships: s.relationships.len(),
        wbs_nodes: s.wbs.len(),
        expenses: expenses(s),
        codes: codes_of(s),
        options: Options {
            out_of_sequence: match o.out_of_sequence {
                m::OutOfSequenceMode::RetainedLogic => "retained-logic",
                m::OutOfSequenceMode::ProgressOverride => "progress-override",
                m::OutOfSequenceMode::ActualDates => "actual-dates",
            },
            lag_calendar: match o.lag_calendar {
                m::LagCalendar::Predecessor => "predecessor",
                m::LagCalendar::Successor => "successor",
                m::LagCalendar::TwentyFourHour => "24-hour",
                m::LagCalendar::ProjectDefault => "project-default",
            },
            float_type: match o.float_type {
                m::FloatType::StartFloat => "start",
                m::FloatType::FinishFloat => "finish",
                m::FloatType::SmallestFloat => "smallest",
            },
            critical: match o.critical {
                m::CriticalDefinition::TotalFloatAtMost(t) => totalfloat_xer::patch::hours(t)
                    .map_or_else(
                        || format!("total-float<={}m", t.get()),
                        |h| format!("total-float<={h}h"),
                    ),
                m::CriticalDefinition::LongestPath => "longest-path".to_owned(),
            },
            make_open_ended_critical: o.make_open_ended_critical,
            ss_lag_from_early_start: o.ss_lag_from_early_start,
            use_expected_finish: o.use_expected_finish,
            multiple_float_paths: o.float_paths.enabled,
        },
    }
}

fn dictionaries(s: &mut String, r: &Report) {
    if !r.activity_codes.is_empty() {
        let _ = writeln!(s, "\nactivity codes ({}):", r.activity_code_types_total);
        for t in &r.activity_codes {
            let values: Vec<&str> = t.values.iter().map(|v| v.value.as_str()).collect();
            let more = t.values_total - t.values.len();
            let more = if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            let _ = writeln!(s, "  {}: {}{more}", t.name, values.join(", "));
        }
        if r.activity_code_types_total > r.activity_codes.len() {
            let more = r.activity_code_types_total - r.activity_codes.len();
            let _ = writeln!(s, "  and {more} more");
        }
    }
    if r.activity_udfs_total > 0 {
        let _ = writeln!(s, "\nactivity fields ({}):", r.activity_udfs_total);
        for f in &r.activity_udfs {
            let _ = writeln!(s, "  {} ({})", f.label, f.data_type);
        }
        if r.activity_udfs_total > r.activity_udfs.len() {
            let _ = writeln!(
                s,
                "  and {} more",
                r.activity_udfs_total - r.activity_udfs.len()
            );
        }
    }
    if !r.project_codes.is_empty() {
        let _ = writeln!(s, "\nproject codes ({}):", r.project_code_types_total);
        for t in &r.project_codes {
            let values: Vec<&str> = t.values.iter().map(|v| v.value.as_str()).collect();
            let more = t.values_total - t.values.len();
            let more = if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            let _ = writeln!(s, "  {}: {}{more}", t.name, values.join(", "));
        }
        if r.project_code_types_total > r.project_codes.len() {
            let more = r.project_code_types_total - r.project_codes.len();
            let _ = writeln!(s, "  and {more} more");
        }
    }
    if r.resources_total > 0 {
        let _ = writeln!(s, "\nresources ({}):", r.resources_total);
        for resource in &r.resources {
            let _ = writeln!(s, "  {} {}", resource.short_name, resource.name);
        }
        if r.resources_total > r.resources.len() {
            let _ = writeln!(s, "  and {} more", r.resources_total - r.resources.len());
        }
    }
    if r.roles_total > 0 {
        let _ = writeln!(s, "\nroles ({}):", r.roles_total);
        for role in &r.roles {
            let _ = writeln!(s, "  {} {}", role.short_name, role.name);
        }
        if r.roles_total > r.roles.len() {
            let _ = writeln!(s, "  and {} more", r.roles_total - r.roles.len());
        }
    }
    if r.cost_accounts_total > 0 {
        let _ = writeln!(s, "\ncost accounts ({}):", r.cost_accounts_total);
        for account in &r.cost_accounts {
            let _ = writeln!(s, "  {} {}", account.short_name, account.name);
        }
        if r.cost_accounts_total > r.cost_accounts.len() {
            let more = r.cost_accounts_total - r.cost_accounts.len();
            let _ = writeln!(s, "  and {more} more");
        }
    }
}

fn project_codes_line(s: &mut String, p: &Project) {
    if !p.codes.is_empty() {
        let codes: Vec<String> = p.codes.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let _ = writeln!(s, "    project codes: {}", codes.join(", "));
    }
}

fn human(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} ({} bytes, {}, {})",
        r.file, r.bytes, r.encoding, r.line_ending
    );
    let _ = writeln!(
        s,
        "XER version {}, exported {}",
        r.export.version.as_deref().unwrap_or("?"),
        r.export.date.as_deref().unwrap_or("?")
    );
    let _ = writeln!(s, "\nprojects ({}):", r.projects.len());
    for p in &r.projects {
        let _ = writeln!(
            s,
            "  {} {}{}: data date {}; {} activities, {} relationships, {} WBS nodes",
            p.id,
            p.short_name,
            p.name
                .as_ref()
                .map(|n| format!(" \"{n}\""))
                .unwrap_or_default(),
            p.data_date.as_deref().unwrap_or("none"),
            p.activities,
            p.relationships,
            p.wbs_nodes
        );
        let _ = writeln!(
            s,
            "    status: {} not started, {} in progress, {} completed",
            p.status.not_started, p.status.in_progress, p.status.completed
        );
        if let Some(date) = &p.finish {
            let _ = writeln!(s, "    finish {date}");
        }
        let x = &p.expenses;
        if x.count > 0 {
            let _ = writeln!(
                s,
                "    expenses: {} item{}, budget {}, actual {}, remaining {}",
                x.count,
                if x.count == 1 { "" } else { "s" },
                x.budgeted_cost,
                x.actual_cost,
                x.remaining_cost
            );
        }
        if let Some(date) = &p.must_finish_by {
            let _ = writeln!(s, "    must finish by {date}");
        }
        project_codes_line(&mut s, p);
        let o = &p.options;
        let _ = writeln!(
            s,
            "    options: {}; lag calendar {}; {} float; critical {}{}{}",
            o.out_of_sequence,
            o.lag_calendar,
            o.float_type,
            o.critical,
            if o.make_open_ended_critical {
                "; open ends critical"
            } else {
                ""
            },
            if o.multiple_float_paths {
                "; multiple float paths"
            } else {
                ""
            },
        );
    }
    let _ = writeln!(s, "\ntables ({}):", r.tables.len());
    for t in &r.tables {
        let _ = writeln!(s, "  {:<14} {:>8}", t.name, t.rows);
    }
    let _ = writeln!(s, "\ncalendars ({}):", r.calendars.len());
    for c in &r.calendars {
        let _ = writeln!(s, "  {} {} ({})", c.id, c.name, c.kind);
    }
    dictionaries(&mut s, r);
    let d = &r.diagnostics;
    let _ = writeln!(s, "\ndiagnostics:");
    let _ = writeln!(s, "  reader: {}", summary(&d.reader));
    let _ = writeln!(s, "  mapping: {}", summary(&d.mapping));
    for n in &d.mapping_details {
        let place = if n.field.is_empty() {
            n.kind.clone()
        } else {
            format!("{} {}", n.kind, n.field)
        };
        let _ = writeln!(s, "    {place}: {} ({})", n.message, n.count);
    }
    if d.mapping_unnamed > 0 {
        let _ = writeln!(s, "    and {} more not named here", d.mapping_unnamed);
    }
    let _ = writeln!(s, "  rows left out of the model: {}", d.omitted_rows);
    s
}

fn summary(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_owned();
    }
    counts
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}
