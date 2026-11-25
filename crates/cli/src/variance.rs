//! `totalfloat variance`: the facts behind a monthly narrative, measured
//! against a baseline. Every number comes from the files and the calendars,
//! so an agent can write the narrative without working out a date.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;
use totalfloat_diff::{Category, compare, milestone_date, pairs as activity_pairs};
use totalfloat_model::{Activity, CalendarId, DateTime, Minutes, Schedule};

use crate::check::project_heading;
use crate::diff::pairs;
use crate::error::{CliError, exit};
use crate::loaded::Loaded;
use crate::output::{create_new_text, hours_of, print, print_line, refuse_existing, rounded};
use crate::page::{Cell, Format, Page, Row};
use crate::query::{Calendars, compiled, finish_of, variance_minutes};

/// Items listed before the rest are only counted, so a report fits an
/// agent's context.
const MAX_LISTED: usize = 100;

/// `totalfloat.variance/1`, documented in docs/schemas/variance-1.schema.json.
#[derive(Debug, Serialize)]
pub struct Report {
    schema: &'static str,
    current: String,
    baseline: String,
    /// Projects in the current file with no project to compare in the baseline.
    unmatched_current: Vec<String>,
    /// Projects in the baseline with none in the current file.
    unmatched_baseline: Vec<String>,
    /// Projects left out because a side lost rows in mapping, so a lost row
    /// would read as a deletion.
    incomplete: Vec<String>,
    projects: Vec<ProjectVariance>,
}

/// One project against its baseline.
#[derive(Debug, Serialize)]
struct ProjectVariance {
    code: String,
    /// `PROJECT.proj_name`, for a page's heading; not in the JSON.
    #[serde(skip)]
    name: Option<String>,
    data_date: Option<String>,
    baseline_data_date: Option<String>,
    finish: Option<String>,
    baseline_finish: Option<String>,
    /// Baseline finish less current, in working hours on the project's
    /// calendar. Negative is late.
    finish_variance_hours: Option<f64>,
    /// Milestones in both, latest first.
    milestones: Vec<Moved>,
    milestones_total: usize,
    /// The largest finish slips, latest first.
    slips: Vec<Moved>,
    became_critical: Vec<Named>,
    left_critical: Vec<Named>,
    /// The largest losses of total float.
    float_erosion: Vec<Eroded>,
    counts: Counts,
}

/// An activity and how far a date moved against the baseline.
#[derive(Debug, Serialize)]
struct Moved {
    code: String,
    name: String,
    baseline: Option<String>,
    current: Option<String>,
    /// Baseline less current, in working hours. Negative is late.
    variance_hours: Option<f64>,
}

#[derive(Debug, Serialize)]
struct Named {
    code: String,
    name: String,
}

#[derive(Debug, Serialize)]
struct Eroded {
    code: String,
    name: String,
    baseline_float_hours: f64,
    float_hours: f64,
    lost_hours: f64,
}

/// What changed, counted in full.
#[derive(Debug, Serialize)]
struct Counts {
    activities_added: usize,
    activities_deleted: usize,
    /// Relationships added, deleted or changed.
    logic_changed: usize,
    /// Activities whose original or remaining duration changed.
    durations_changed: usize,
    /// Activities whose constraints changed.
    constraints_changed: usize,
    became_critical: usize,
    left_critical: usize,
    slipped: usize,
}

fn text(date: Option<DateTime>) -> Option<String> {
    date.map(|d| d.to_string())
}

fn hours(minutes: Minutes) -> f64 {
    rounded(hours_of(minutes))
}

/// Baseline less current on calendar `id` of `schedule`, in hours.
fn variance(
    (schedule, id): (&Schedule, Option<CalendarId>),
    (was, now): (Option<DateTime>, Option<DateTime>),
    calendars: &mut Calendars,
) -> Option<f64> {
    let (Some(was), Some(now)) = (was, now) else {
        return None;
    };
    let calendar = compiled(schedule, id?, calendars)?;
    Some(hours(variance_minutes(calendar, was, now)))
}

/// Latest first, then on time and early, with anything unmeasured last.
fn latest_first(a: &Moved, b: &Moved) -> std::cmp::Ordering {
    let key = |m: &Moved| m.variance_hours.unwrap_or(f64::INFINITY);
    key(a).total_cmp(&key(b)).then_with(|| a.code.cmp(&b.code))
}

/// Names for the codes `compare` lists, which are the current file's, at
/// most [`MAX_LISTED`].
fn named(names: &BTreeMap<&str, &str>, codes: &[String]) -> Vec<Named> {
    codes
        .iter()
        .take(MAX_LISTED)
        .map(|code| Named {
            code: code.clone(),
            name: names
                .get(code.as_str())
                .map_or_else(String::new, |n| (*n).to_owned()),
        })
        .collect()
}

/// Milestones and slips, latest first. Activities that span others, level of
/// effort and WBS summary, follow the work they span, so they aren't slips.
fn moves(
    matched: &[(&Activity, &Activity)],
    now: &Schedule,
    calendars: &mut Calendars,
) -> (Vec<Moved>, Vec<Moved>) {
    let (mut milestones, mut slips) = (Vec::new(), Vec::new());
    for (b, a) in matched {
        let milestone = a.activity_type.is_milestone();
        let (before, after) = if milestone {
            (milestone_date(b), milestone_date(a))
        } else {
            (finish_of(b), finish_of(a))
        };
        let moved = Moved {
            code: a.code.clone(),
            name: a.name.clone(),
            baseline: text(before),
            current: text(after),
            variance_hours: variance((now, Some(a.calendar)), (before, after), calendars),
        };
        if milestone {
            milestones.push(moved);
        } else if !a.activity_type.is_span() && moved.variance_hours.is_some_and(|v| v < 0.0) {
            slips.push(moved);
        }
    }
    milestones.sort_by(latest_first);
    slips.sort_by(latest_first);
    (milestones, slips)
}

fn erosion(matched: &[(&Activity, &Activity)]) -> Vec<Eroded> {
    let mut out: Vec<Eroded> = matched
        .iter()
        .filter_map(|(b, a)| {
            let (before, after) = (b.stored.total_float?, a.stored.total_float?);
            (after < before).then(|| Eroded {
                code: a.code.clone(),
                name: a.name.clone(),
                baseline_float_hours: hours(before),
                float_hours: hours(after),
                lost_hours: hours(Minutes::new(before.get() - after.get())),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.lost_hours
            .total_cmp(&a.lost_hours)
            .then_with(|| a.code.cmp(&b.code))
    });
    out
}

fn project((was, now): (&Schedule, &Schedule), top: usize) -> ProjectVariance {
    let comparison = compare(was, now);
    let mut calendars = Calendars::new();
    // Paired as `diff` pairs them, so every list here agrees with its counts.
    let matched = activity_pairs(was, now);
    let (finish, baseline_finish) = (
        now.project.stored.scheduled_finish,
        was.project.stored.scheduled_finish,
    );
    let finish_variance_hours = variance(
        (now, now.project.default_calendar),
        (baseline_finish, finish),
        &mut calendars,
    );
    let (milestones, mut slips) = moves(&matched, now, &mut calendars);
    let mut float_erosion = erosion(&matched);
    let names: BTreeMap<&str, &str> = now
        .activities
        .values()
        .chain(was.activities.values())
        .map(|a| (a.code.as_str(), a.name.as_str()))
        .rev()
        .collect();
    let changed = |category: Category| {
        comparison
            .changed
            .iter()
            .filter(|c| c.changes.iter().any(|f| f.category == category))
            .count()
    };
    let counts = Counts {
        activities_added: comparison.counts.added,
        activities_deleted: comparison.counts.deleted,
        logic_changed: comparison.counts.links_added
            + comparison.counts.links_deleted
            + comparison.counts.links_changed,
        durations_changed: changed(Category::Duration),
        constraints_changed: changed(Category::Constraint),
        became_critical: comparison.counts.became_critical,
        left_critical: comparison.counts.left_critical,
        slipped: slips.len(),
    };
    let top = top.min(MAX_LISTED);
    slips.truncate(top);
    float_erosion.truncate(top);
    let milestones_total = milestones.len();
    ProjectVariance {
        code: now.project.short_name.clone(),
        name: now.project.name.clone(),
        data_date: text(now.project.data_date),
        baseline_data_date: text(was.project.data_date),
        finish: text(finish),
        baseline_finish: text(baseline_finish),
        finish_variance_hours,
        milestones: milestones.into_iter().take(MAX_LISTED).collect(),
        milestones_total,
        slips,
        became_critical: named(&names, &comparison.became_critical),
        left_critical: named(&names, &comparison.left_critical),
        float_erosion,
        counts,
    }
}

/// Measures each project of `current` against its project in `baseline`.
///
/// # Errors
///
/// `no-projects` if either file has none that can be compared.
pub fn report(
    (current_file, current): (&str, &Loaded),
    (baseline_file, baseline): (&str, &Loaded),
    top: usize,
) -> Result<Report, CliError> {
    for (file, loaded) in [(current_file, current), (baseline_file, baseline)] {
        if loaded.schedules.is_empty() {
            return Err(CliError::new(
                "no-projects",
                format!("{file} has no project that can be compared"),
                "`inspect` reports what the file holds and what was left out",
            ));
        }
    }
    let (matched, unmatched_baseline, unmatched_current) =
        pairs(&baseline.schedules, &current.schedules);
    let (whole, lost): (Vec<_>, Vec<_>) = matched.iter().partition(|(was, now)| {
        baseline.refuse_incomplete(was).is_ok() && current.refuse_incomplete(now).is_ok()
    });
    Ok(Report {
        schema: "totalfloat.variance/1",
        current: current_file.to_owned(),
        baseline: baseline_file.to_owned(),
        unmatched_current,
        unmatched_baseline,
        incomplete: lost
            .iter()
            .map(|(_, now)| now.project.short_name.clone())
            .collect(),
        projects: whole.iter().map(|pair| project(*pair, top)).collect(),
    })
}

fn hours_text(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| format!("{v}h"))
}

fn moved_lines(out: &mut String, title: &str, items: &[Moved], total: usize) {
    if items.is_empty() {
        return;
    }
    let _ = writeln!(out, "  {title} ({total}):");
    for m in items {
        let _ = writeln!(
            out,
            "    {} {}: {} (baseline {}), {}",
            m.code,
            m.name,
            m.current.as_deref().unwrap_or("-"),
            m.baseline.as_deref().unwrap_or("-"),
            hours_text(m.variance_hours)
        );
    }
}

fn human(report: &Report) -> String {
    let mut out = String::new();
    for p in &report.projects {
        let _ = writeln!(
            out,
            "Project {}: finish {} (baseline {}), variance {}",
            p.code,
            p.finish.as_deref().unwrap_or("-"),
            p.baseline_finish.as_deref().unwrap_or("-"),
            hours_text(p.finish_variance_hours)
        );
        let _ = writeln!(
            out,
            "  data date {} (baseline {})",
            p.data_date.as_deref().unwrap_or("-"),
            p.baseline_data_date.as_deref().unwrap_or("-")
        );
        let c = &p.counts;
        let _ = writeln!(
            out,
            "  {} became critical, {} left critical",
            c.became_critical, c.left_critical
        );
        let _ = writeln!(
            out,
            "  {} added, {} deleted; logic changed {}, durations {}, constraints {}",
            c.activities_added,
            c.activities_deleted,
            c.logic_changed,
            c.durations_changed,
            c.constraints_changed
        );
        moved_lines(&mut out, "milestones", &p.milestones, p.milestones_total);
        moved_lines(&mut out, "largest slips", &p.slips, c.slipped);
        for (title, list) in [
            ("became critical", &p.became_critical),
            ("left critical", &p.left_critical),
        ] {
            if !list.is_empty() {
                let shown: Vec<String> = list
                    .iter()
                    .map(|n| format!("{} {}", n.code, n.name))
                    .collect();
                let _ = writeln!(out, "  {title}: {}", shown.join(", "));
            }
        }
        if !p.float_erosion.is_empty() {
            let _ = writeln!(out, "  largest float erosion:");
            for e in &p.float_erosion {
                let _ = writeln!(
                    out,
                    "    {} {}: {}h to {}h, lost {}h",
                    e.code, e.name, e.baseline_float_hours, e.float_hours, e.lost_hours
                );
            }
        }
    }
    for (what, list) in [
        ("only in the current file", &report.unmatched_current),
        ("only in the baseline", &report.unmatched_baseline),
        ("left out, rows lost when read", &report.incomplete),
    ] {
        if !list.is_empty() {
            let _ = writeln!(out, "Projects {what}: {}", list.join(", "));
        }
    }
    if report.projects.is_empty() {
        let _ = writeln!(out, "No project could be compared with its baseline.");
    }
    out.trim_end().to_owned()
}

/// A variance's cell: marked late when negative.
fn variance_cell(value: Option<f64>) -> Cell {
    Cell::marked(
        hours_text(value),
        value.is_some_and(|v| v < 0.0).then_some("late"),
    )
}

fn moved_table(page: &mut Page, items: &[Moved]) {
    let rows = items
        .iter()
        .map(|m| {
            Row::marked(
                vec![
                    Cell::from(m.code.as_str()),
                    Cell::from(m.name.as_str()),
                    Cell::from(m.current.as_deref().unwrap_or("-")),
                    Cell::from(m.baseline.as_deref().unwrap_or("-")),
                    variance_cell(m.variance_hours),
                ],
                None,
            )
        })
        .collect();
    page.table(
        [
            "Activity ID",
            "Activity name",
            "Current",
            "Baseline",
            "Variance",
        ]
        .map(str::to_owned)
        .to_vec(),
        rows,
        None,
    );
}

fn named_table(page: &mut Page, items: &[Named]) {
    let rows = items
        .iter()
        .map(|n| Row::from(vec![n.code.as_str(), n.name.as_str()]))
        .collect();
    page.table(
        vec!["Activity ID".to_owned(), "Activity name".to_owned()],
        rows,
        None,
    );
}

/// One project's section of the document.
fn project_section(page: &mut Page, baseline: &str, p: &ProjectVariance) {
    page.heading(2, project_heading(&p.code, p.name.as_deref()));
    page.paragraph(format!(
        "Data date {} · baseline {baseline}, data date {}",
        p.data_date.as_deref().unwrap_or("-"),
        p.baseline_data_date.as_deref().unwrap_or("-")
    ));
    page.table(
        ["Date", "Current", "Baseline", "Variance"]
            .map(str::to_owned)
            .to_vec(),
        vec![Row::marked(
            vec![
                Cell::from("Project finish"),
                Cell::from(p.finish.as_deref().unwrap_or("-")),
                Cell::from(p.baseline_finish.as_deref().unwrap_or("-")),
                variance_cell(p.finish_variance_hours),
            ],
            None,
        )],
        None,
    );
    let c = &p.counts;
    page.heading(3, "Changes");
    page.table(
        vec!["Change".to_owned(), "Count".to_owned()],
        [
            ("Activities added", c.activities_added),
            ("Activities deleted", c.activities_deleted),
            ("Relationships changed", c.logic_changed),
            ("Durations changed", c.durations_changed),
            ("Constraints changed", c.constraints_changed),
            ("Became critical", c.became_critical),
            ("Left critical", c.left_critical),
        ]
        .into_iter()
        .map(|(what, count)| Row::from(vec![what.to_owned(), count.to_string()]))
        .collect(),
        None,
    );
    if !p.milestones.is_empty() {
        page.heading(3, format!("Milestones ({})", p.milestones_total));
        moved_table(page, &p.milestones);
    }
    page.heading(3, format!("Largest slips ({})", c.slipped));
    if c.slipped == 0 {
        page.paragraph("No activity finishes later than in the baseline.");
    } else if p.slips.is_empty() {
        page.paragraph("None listed.");
    } else {
        moved_table(page, &p.slips);
    }
    for (title, list, count) in [
        ("Became critical", &p.became_critical, c.became_critical),
        ("Left critical", &p.left_critical, c.left_critical),
    ] {
        if !list.is_empty() {
            page.heading(3, title);
            named_table(page, list);
            let more = count.saturating_sub(list.len());
            if more > 0 {
                page.paragraph(format!("And {more} more, not listed."));
            }
        }
    }
    erosion_section(page, &p.float_erosion);
}

fn erosion_section(page: &mut Page, eroded: &[Eroded]) {
    page.heading(3, "Largest float erosion");
    if eroded.is_empty() {
        // The list is cut to `--top`, so an empty one may still have had some.
        page.paragraph("None listed.");
    } else {
        let rows = eroded
            .iter()
            .map(|e| {
                Row::marked(
                    vec![
                        Cell::from(e.code.as_str()),
                        Cell::from(e.name.as_str()),
                        Cell::from(format!("{}h", e.baseline_float_hours)),
                        Cell::from(format!("{}h", e.float_hours)),
                        Cell::marked(format!("{}h", e.lost_hours), Some("late")),
                    ],
                    None,
                )
            })
            .collect();
        page.table(
            [
                "Activity ID",
                "Activity name",
                "Baseline float",
                "Float",
                "Lost",
            ]
            .map(str::to_owned)
            .to_vec(),
            rows,
            None,
        );
    }
}

/// The report as a document: per project, the finish against the baseline,
/// then what changed, as tables.
fn document(report: &Report) -> Page {
    let mut page = Page::marked(format!(
        "Variance: {} against {}",
        report.current, report.baseline
    ));
    page.paragraph(
        "Variances are the baseline less the current date, in working hours, so negative is late.",
    );
    for p in &report.projects {
        project_section(&mut page, &report.baseline, p);
    }
    for (what, list) in [
        ("only in the current file", &report.unmatched_current),
        ("only in the baseline", &report.unmatched_baseline),
        ("left out, rows lost when read", &report.incomplete),
    ] {
        if !list.is_empty() {
            page.paragraph(format!("Projects {what}: {}", list.join(", ")));
        }
    }
    if report.projects.is_empty() {
        page.paragraph("No project could be compared with its baseline.");
    }
    page
}

/// The report as Markdown, or as one HTML page.
pub fn render(report: &Report, format: Format) -> String {
    document(report).render(format)
}

/// Runs the command. `format` is a Markdown or HTML document instead of the
/// text report; `output` is a new file to write it to instead of stdout.
pub fn run(
    (current, baseline): (&Path, &Path),
    top: usize,
    (format, json): (Option<Format>, bool),
    output: Option<&Path>,
) -> Result<u8, CliError> {
    if let Some(output) = output {
        refuse_existing(output)?;
    }
    let now = Loaded::new(crate::location::read(current)?);
    let was = Loaded::new(crate::location::read(baseline)?);
    let report = report(
        (&current.display().to_string(), &now),
        (&baseline.display().to_string(), &was),
        top,
    )?;
    let text = if json {
        serde_json::to_string_pretty(&report).unwrap_or_default() + "\n"
    } else if let Some(format) = format {
        render(&report, format)
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
    Ok(exit::OK)
}
