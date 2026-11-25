//! `totalfloat textconv`: an XER as stable, sorted text, for git's `textconv`
//! (`.gitattributes`: `*.xer diff=totalfloat`). Rows come out in a fixed
//! order whatever order the file holds them in, so a diff shows what changed
//! rather than how P6 happened to write the file. Inputs come first and the
//! calculated dates last, so an input change reads first.

use std::fmt::Write as _;
use std::path::Path;

use totalfloat_model::{Activity, DateTime, Minutes, Schedule};

use crate::error::{CliError, exit};
use crate::loaded::Loaded;
use crate::output::{hours_of, print};
use crate::query::{constraint_label, link_label, type_label};

fn date(value: Option<DateTime>) -> String {
    value.map_or_else(|| "-".to_owned(), |d| d.to_string())
}

fn hours(value: Minutes) -> String {
    format!("{}h", hours_of(value))
}

fn activity_line(s: &mut String, schedule: &Schedule, a: &Activity) {
    let calendar = schedule
        .calendars
        .get(&a.calendar)
        .map_or_else(|| a.calendar.0.to_string(), |c| c.name.clone());
    let wbs = schedule
        .wbs
        .get(&a.wbs)
        .map_or_else(|| a.wbs.0.to_string(), |w| w.code.clone());
    let constraint = |c: Option<totalfloat_model::Constraint>| {
        c.map_or_else(
            || "-".to_owned(),
            |c| format!("{} {}", constraint_label(c.kind), date(c.date)),
        )
    };
    let _ = writeln!(
        s,
        "activity {} | {} | {} | wbs {wbs} | calendar {calendar} | original {} | remaining {} | constraint {} | secondary {} | actual {} to {}",
        a.code,
        a.name,
        type_label(a.activity_type),
        hours(a.original_duration),
        hours(a.remaining_duration),
        constraint(a.primary_constraint),
        constraint(a.secondary_constraint),
        date(a.actual_start),
        date(a.actual_finish),
    );
}

fn dates_line(s: &mut String, a: &Activity) {
    let r = &a.stored;
    let float = r.total_float.map_or_else(|| "-".to_owned(), hours);
    let _ = writeln!(
        s,
        "dates {} | early {} to {} | late {} to {} | total float {float}",
        a.code,
        date(r.early_start),
        date(r.early_finish),
        date(r.late_start),
        date(r.late_finish),
    );
}

/// One project as text: inputs, then links, calendars and resources, then
/// stored dates, each sorted.
fn project_text(s: &mut String, schedule: &Schedule, all: &[Schedule]) {
    let p = &schedule.project;
    let _ = writeln!(
        s,
        "project {} ({}) | data date {} | finish {}",
        p.short_name,
        p.id.0,
        date(p.data_date),
        date(p.stored.scheduled_finish)
    );
    let mut activities: Vec<&Activity> = schedule.activities.values().collect();
    activities.sort_by(|a, b| (&a.code, a.id).cmp(&(&b.code, b.id)));
    for a in &activities {
        activity_line(s, schedule, a);
    }
    // An activity in another project of the file is PROJECT:CODE.
    let code = |id| {
        if let Some(a) = schedule.activities.get(&id) {
            return a.code.clone();
        }
        all.iter()
            .find_map(|other| {
                let a = other.activities.get(&id)?;
                Some(format!("{}:{}", other.project.short_name, a.code))
            })
            .unwrap_or_else(|| format!("#{}", id.0))
    };
    let mut links: Vec<String> = schedule
        .relationships
        .values()
        .map(|r| {
            format!(
                "link {} {} {} lag {}",
                code(r.predecessor),
                link_label(r.kind),
                code(r.successor),
                hours(r.lag)
            )
        })
        .collect();
    links.sort();
    for link in links {
        let _ = writeln!(s, "{link}");
    }
    let mut calendars: Vec<&totalfloat_model::Calendar> = schedule.calendars.values().collect();
    calendars.sort_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)));
    for calendar in calendars {
        let _ = writeln!(s, "calendar {} ({})", calendar.name, calendar.id.0);
        // Exceptions hold the holidays, so a holiday edit shows as a line.
        for (day, shifts) in &calendar.exceptions {
            let work = if shifts.is_empty() {
                "non-work".to_owned()
            } else {
                shifts
                    .iter()
                    .map(|w| format!("{}-{}", w.start, w.finish))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = writeln!(s, "calendar {} exception {day} {work}", calendar.name);
        }
    }
    let mut resources: Vec<String> = schedule
        .resources
        .values()
        .map(|r| format!("resource {} | {}", r.short_name, r.name))
        .collect();
    resources.sort();
    for resource in resources {
        let _ = writeln!(s, "{resource}");
    }
    for a in &activities {
        dates_line(s, a);
    }
}

/// The text view of a file that's been read.
#[must_use]
pub fn text(loaded: &Loaded) -> String {
    let mut schedules: Vec<&Schedule> = loaded.schedules.iter().collect();
    schedules.sort_by(|a, b| {
        (&a.project.short_name, a.project.id).cmp(&(&b.project.short_name, b.project.id))
    });
    let mut s = String::new();
    for schedule in schedules {
        project_text(&mut s, schedule, &loaded.schedules);
        s.push('\n');
    }
    s
}

/// Runs the command. Problems mapping the file go to standard error, so a
/// readable XER always gives text for git.
///
/// # Errors
///
/// Only for a file that can't be read as an XER at all.
pub fn run(file: &Path) -> Result<u8, CliError> {
    let loaded = Loaded::new(crate::error::read_xer(file)?);
    let omitted = loaded.diagnostics.omitted.len();
    if omitted > 0 {
        eprintln!(
            "textconv: {omitted} rows of {} aren't shown; `totalfloat inspect` says why",
            file.display()
        );
    }
    print(&text(&loaded));
    Ok(exit::OK)
}
