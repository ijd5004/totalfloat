//! `totalfloat calendar`: what a calendar works, without scheduling anything.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;
use totalfloat_model::{Calendar, Date, Minutes, Weekday, WorkInterval};

use crate::error::{CliError, exit, parse_day};
use crate::inspect::calendar_kind;
use crate::loaded::Loaded;
use crate::output::{hours_of, print, print_line, rounded, same_text};

/// Days a date range can list.
const MAX_DAYS: usize = 366;
/// Exceptions listed. The count is always complete.
const MAX_EXCEPTIONS: usize = 200;

/// `totalfloat.calendar/1`.
#[derive(Debug, Serialize)]
pub struct Report {
    schema: &'static str,
    file: String,
    id: u64,
    name: String,
    kind: &'static str,
    project: Option<u64>,
    hours_per_day: Option<f64>,
    hours_per_week: Option<f64>,
    week: Vec<Day>,
    /// Dates that differ from the work week, within the range if there is one.
    exceptions_total: usize,
    exceptions: Vec<Day>,
    /// Every date in the range, when both ends are given.
    days: Vec<Day>,
}

#[derive(Debug, Serialize)]
struct Day {
    /// The date, or nothing for a day of the standard week.
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    weekday: &'static str,
    hours: f64,
    shifts: Vec<String>,
}

const fn day_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Sunday => "sunday",
        Weekday::Monday => "monday",
        Weekday::Tuesday => "tuesday",
        Weekday::Wednesday => "wednesday",
        Weekday::Thursday => "thursday",
        Weekday::Friday => "friday",
        Weekday::Saturday => "saturday",
    }
}

fn day(date: Option<Date>, weekday: Weekday, shifts: &[WorkInterval]) -> Day {
    let minutes = shifts.iter().map(|s| s.duration().get()).sum();
    Day {
        date: date.map(|d| d.to_string()),
        weekday: day_name(weekday),
        hours: hours_of(Minutes::new(minutes)),
        shifts: shifts
            .iter()
            .map(|s| format!("{}-{}", s.start, s.finish))
            .collect(),
    }
}

fn on(calendar: &Calendar, date: Date) -> Day {
    let shifts = calendar
        .exceptions
        .get(&date)
        .map_or_else(|| calendar.work_week.day(date.weekday()), Vec::as_slice);
    day(Some(date), date.weekday(), shifts)
}

fn all_calendars(loaded: &Loaded) -> BTreeMap<u64, &Calendar> {
    loaded
        .schedules
        .iter()
        .flat_map(|s| s.calendars.values())
        .map(|c| (c.id.0, c))
        .collect()
}

fn listing(calendars: &BTreeMap<u64, &Calendar>) -> String {
    let names: Vec<String> = calendars
        .values()
        .map(|c| format!("{} ({})", c.name, c.id.0))
        .collect();
    format!("calendars in this file: {}", names.join(", "))
}

fn find<'a>(loaded: &'a Loaded, wanted: &str) -> Result<&'a Calendar, CliError> {
    let calendars = all_calendars(loaded);
    let by_id = wanted.parse().ok().and_then(|id| calendars.get(&id));
    if let Some(calendar) = by_id {
        return Ok(calendar);
    }
    let named: Vec<&&Calendar> = calendars
        .values()
        .filter(|c| same_text(&c.name, wanted))
        .collect();
    match named.as_slice() {
        [one] => Ok(one),
        [] => Err(CliError::new(
            "no-such-calendar",
            format!("no calendar matches {wanted}"),
            listing(&calendars),
        )),
        _ => Err(CliError::new(
            "ambiguous-calendar",
            format!("{} calendars are named {wanted}", named.len()),
            format!("use the ID: {}", listing(&calendars)),
        )),
    }
}

/// What to list. An open end lists exceptions that way. `days` needs both ends.
#[derive(Debug, Default)]
pub struct Range<'a> {
    /// First date, as `YYYY-MM-DD`.
    pub from: Option<&'a str>,
    /// Last date, as `YYYY-MM-DD`.
    pub to: Option<&'a str>,
}

/// Reports one calendar of a file that's already been read.
pub fn report(
    file: &str,
    loaded: &Loaded,
    wanted: &str,
    range: &Range<'_>,
) -> Result<Report, CliError> {
    let calendar = find(loaded, wanted)?;
    let from = range.from.map(parse_day).transpose()?;
    let to = range.to.map(parse_day).transpose()?;
    if let (Some(first), Some(last)) = (from, to)
        && first > last
    {
        return Err(CliError::new(
            "invalid-range",
            format!("{first} is after {last}"),
            "give the earlier date as --from",
        ));
    }
    let exceptions: Vec<Date> = calendar
        .exceptions
        .keys()
        .filter(|d| from.is_none_or(|f| **d >= f) && to.is_none_or(|t| **d <= t))
        .copied()
        .collect();
    let mut days = Vec::new();
    if let (Some(first), Some(last)) = (from, to) {
        let mut date = first;
        while date <= last {
            if days.len() == MAX_DAYS {
                return Err(CliError::new(
                    "range-too-long",
                    format!("a range can list at most {MAX_DAYS} days"),
                    "use a shorter range, or no range to list every exception",
                ));
            }
            days.push(on(calendar, date));
            match date.checked_add_days(1) {
                Some(next) => date = next,
                None => break,
            }
        }
    }
    Ok(Report {
        schema: "totalfloat.calendar/1",
        file: file.to_owned(),
        id: calendar.id.0,
        name: calendar.name.clone(),
        kind: calendar_kind(calendar.kind),
        project: calendar.project.map(|p| p.0),
        hours_per_day: calendar.hours_per_period.day.map(hours_of),
        hours_per_week: calendar.hours_per_period.week.map(hours_of),
        week: [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
            Weekday::Saturday,
            Weekday::Sunday,
        ]
        .into_iter()
        .map(|d| day(None, d, calendar.work_week.day(d)))
        .collect(),
        exceptions_total: exceptions.len(),
        exceptions: exceptions
            .iter()
            .take(MAX_EXCEPTIONS)
            .map(|d| on(calendar, *d))
            .collect(),
        days,
    })
}

fn line(s: &mut String, name: &str, day: &Day) {
    let shifts = if day.shifts.is_empty() {
        "no work".to_owned()
    } else {
        day.shifts.join(", ")
    };
    let _ = writeln!(s, "  {name:<20} {:>5}h  {shifts}", rounded(day.hours));
}

fn human(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "Calendar {} ({}, {})", r.name, r.id, r.kind);
    if let Some(hours) = r.hours_per_day {
        let _ = writeln!(s, "  hours per day: {}", rounded(hours));
    }
    let _ = writeln!(s, "week:");
    for d in &r.week {
        line(&mut s, d.weekday, d);
    }
    let _ = writeln!(s, "exceptions ({}):", r.exceptions_total);
    for d in &r.exceptions {
        line(
            &mut s,
            &format!("{} {}", d.date.as_deref().unwrap_or(""), &d.weekday[..3]),
            d,
        );
    }
    if r.exceptions.len() < r.exceptions_total {
        let _ = writeln!(s, "  and {} more", r.exceptions_total - r.exceptions.len());
    }
    if !r.days.is_empty() {
        let _ = writeln!(s, "days:");
        for d in &r.days {
            line(
                &mut s,
                &format!("{} {}", d.date.as_deref().unwrap_or(""), &d.weekday[..3]),
                d,
            );
        }
    }
    s
}

/// Runs the command.
pub fn run(file: &Path, wanted: &str, range: &Range<'_>, json: bool) -> Result<u8, CliError> {
    let raw = crate::location::read(file)?;
    let report = report(
        &file.display().to_string(),
        &Loaded::new(raw),
        wanted,
        range,
    )?;
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        print(&human(&report));
    }
    Ok(exit::OK)
}
