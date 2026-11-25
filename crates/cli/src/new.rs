//! `totalfloat new`: start a schedule from nothing.
//!
//! A new file has a project, a root WBS node and a five-day calendar, and any
//! edits build on those. Output depends only on the inputs: the export date is
//! the data date, and generated IDs follow the project code.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use totalfloat_edit::{EditError, check_wbs_code, check_wbs_name};
use totalfloat_model as m;
use totalfloat_xer::RawXer;
use totalfloat_xer::fresh::{FreshOptions, seed_of, write};

use crate::edit;
use crate::error::{CliError, exit, parse_date};
use crate::loaded::Loaded;
use crate::output::{create_new, print_line, refuse_existing};

/// What a new schedule starts from.
#[derive(Debug, Clone, Copy)]
pub struct Spec<'a> {
    /// `PROJECT.proj_short_name`.
    pub code: &'a str,
    /// The project's name.
    pub name: &'a str,
    /// The data date, as `YYYY-MM-DD HH:MM`.
    pub data_date: &'a str,
    /// The must-finish-by date, as `YYYY-MM-DD HH:MM`.
    pub must_finish: Option<&'a str>,
    /// The starting calendar's name; `Standard 5-Day` if not given.
    pub calendar: Option<&'a str>,
}

/// `totalfloat.new/1`.
#[derive(Debug, Serialize)]
pub struct Report {
    schema: &'static str,
    output: String,
    code: String,
    name: String,
    data_date: String,
    must_finish: Option<String>,
    activities: usize,
    relationships: usize,
    finish: Option<String>,
    /// What each edit did, in order.
    applied: Vec<String>,
    warnings: Vec<String>,
}

const CALENDAR: u64 = 1;
const WBS: u64 = 1;
const PROJECT: u64 = 1;

fn calendar(name: &str) -> Result<m::Calendar, CliError> {
    let bad = || {
        CliError::new(
            "new-refused",
            "the starter calendar is invalid",
            "report this",
        )
    };
    let time = |hour| m::TimeOfDay::from_hm(hour, 0).ok_or_else(bad);
    let morning = m::WorkInterval::new(time(8)?, time(12)?).ok_or_else(bad)?;
    let afternoon = m::WorkInterval::new(time(13)?, time(17)?).ok_or_else(bad)?;
    let mut week = m::WorkWeek::default();
    for day in [
        m::Weekday::Monday,
        m::Weekday::Tuesday,
        m::Weekday::Wednesday,
        m::Weekday::Thursday,
        m::Weekday::Friday,
    ] {
        week.day_mut(day).extend([morning, afternoon]);
    }
    Ok(m::Calendar {
        id: m::CalendarId(CALENDAR),
        name: name.to_owned(),
        kind: m::CalendarType::Global,
        project: None,
        base: None,
        is_default: true,
        hours_per_period: m::HoursPerPeriod {
            day: Some(m::Minutes::new(480)),
            week: Some(m::Minutes::new(2400)),
            month: Some(m::Minutes::new(9600)),
            year: Some(m::Minutes::new(115_200)),
        },
        work_week: week,
        exceptions: BTreeMap::new(),
        source: None,
    })
}

fn model(spec: &Spec<'_>) -> Result<m::Schedule, CliError> {
    // The project's code and name are its root WBS node's too.
    let refused = |e: EditError| CliError::new(e.code, e.message, e.hint);
    check_wbs_code(spec.code).map_err(refused)?;
    check_wbs_name(spec.name).map_err(refused)?;
    let id = m::ProjectId(PROJECT);
    let mut project = m::Project::new(id, spec.code);
    project.name = Some(spec.name.to_owned());
    project.data_date = Some(parse_date(spec.data_date)?);
    project.planned_start = project.data_date;
    project.must_finish_by = spec.must_finish.map(parse_date).transpose()?;
    if project.must_finish_by.is_some() && project.must_finish_by < project.data_date {
        return Err(CliError::new(
            "invalid-new-schedule",
            "the must-finish date is before the data date",
            "check the year",
        ));
    }
    project.default_calendar = Some(m::CalendarId(CALENDAR));
    let mut schedule = m::Schedule::new(project);
    schedule.calendars.insert(
        m::CalendarId(CALENDAR),
        calendar(spec.calendar.unwrap_or(totalfloat_text::STARTER_CALENDAR))?,
    );
    schedule.wbs.insert(
        m::WbsId(WBS),
        m::Wbs {
            id: m::WbsId(WBS),
            project: id,
            parent: None,
            code: spec.code.to_owned(),
            name: spec.name.to_owned(),
            sequence: 0,
            is_project_node: true,
            earned_value: m::EarnedValue::default(),
            obs: None,
            notes: std::collections::BTreeMap::new(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    Ok(schedule)
}

/// The file for a new schedule, with nothing in it yet.
pub fn start(spec: &Spec<'_>) -> Result<RawXer, CliError> {
    let schedule = model(spec)?;
    let date = schedule
        .project
        .data_date
        .map(m::DateTime::date)
        .ok_or_else(|| {
            CliError::new(
                "invalid-new-schedule",
                "a new schedule needs a data date",
                "use the form YYYY-MM-DD HH:MM",
            )
        })?;
    // Two schedules sharing a code still get their own GUIDs.
    let mut identity = [
        spec.code,
        spec.name,
        spec.data_date,
        spec.must_finish.unwrap_or_default(),
    ]
    .join("\0");
    if let Some(calendar) = spec.calendar {
        identity.push('\0');
        identity.push_str(calendar);
    }
    let options = FreshOptions {
        seed: seed_of(identity.as_bytes()),
        ..FreshOptions::new(date)
    };
    write(&schedule, options).map_err(|e| {
        CliError::new(
            "new-refused",
            format!("can't write that schedule: {e}"),
            "check the project code and name are plain text",
        )
    })
}

pub fn report(
    output: &Path,
    loaded: &Loaded,
    applied: Vec<String>,
    warnings: Vec<String>,
) -> Report {
    let schedule = loaded.schedules.first();
    let project = schedule.map(|s| &s.project);
    Report {
        schema: "totalfloat.new/1",
        output: output.display().to_string(),
        code: project.map_or_else(String::new, |p| p.short_name.clone()),
        name: project.and_then(|p| p.name.clone()).unwrap_or_default(),
        data_date: project
            .and_then(|p| p.data_date)
            .map_or_else(String::new, |d| d.to_string()),
        must_finish: project
            .and_then(|p| p.must_finish_by)
            .map(|d| d.to_string()),
        activities: schedule.map_or(0, |s| s.activities.len()),
        relationships: schedule.map_or(0, |s| s.relationships.len()),
        finish: project
            .and_then(|p| p.stored.scheduled_finish)
            .map(|d| d.to_string()),
        applied,
        warnings,
    }
}

pub fn human(r: &Report) -> String {
    let mut lines = r.applied.clone();
    lines.extend(r.warnings.iter().map(|w| format!("warning: {w}")));
    lines.push(format!(
        "Project {} \"{}\": data date {}, {} activities, {} relationships",
        r.code, r.name, r.data_date, r.activities, r.relationships
    ));
    if let Some(finish) = &r.finish {
        lines.push(format!("finish: {finish}"));
    }
    lines.push(format!("Wrote {}", r.output));
    lines.join("\n")
}

/// Runs the command.
pub fn run(
    spec: &Spec<'_>,
    edits: Option<&Path>,
    output: &Path,
    json: bool,
) -> Result<u8, CliError> {
    refuse_existing(output)?;
    let commands = edits.map(edit::read_edits).transpose()?;
    let mut raw = start(spec)?;
    let (mut applied, mut warnings) = (Vec::new(), Vec::new());
    if let Some(commands) = commands.filter(|c| !c.edits.is_empty() || !c.options.is_default()) {
        if commands.options.project.is_some() {
            return Err(CliError::new(
                "invalid-edit",
                "the edits name a project, and a new schedule has only its own",
                "take `project` out of the edits file's options",
            ));
        }
        let label = output.display().to_string();
        let options = edit::Options::from_document(&commands.options);
        let loaded = Loaded::new(raw);
        let done = edit::report(&label, &loaded, &commands.edits, options)?;
        (applied, warnings) = done.report.notes();
        raw = done.raw;
    }
    create_new(&raw, output)?;
    let report = report(output, &Loaded::new(raw), applied, warnings);
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print_line(&human(&report));
    }
    Ok(exit::OK)
}
