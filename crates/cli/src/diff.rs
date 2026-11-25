//! `totalfloat diff`: what changed between two versions of a schedule.

use std::fmt::Write as _;
use std::path::Path;

use serde::Serialize;
use totalfloat_diff::{Comparison, Counts, compare};
use totalfloat_model::Schedule;

use crate::error::{CliError, exit};
use crate::loaded::Loaded;
use crate::output::{print, print_line, rounded};

/// `totalfloat.diff/1`, documented in docs/schemas/diff-1.schema.json.
#[derive(Debug, Serialize)]
pub struct Report {
    schema: &'static str,
    from: String,
    to: String,
    /// Projects only in the earlier file, by `proj_short_name`.
    unmatched_from: Vec<String>,
    /// Projects only in the later file, by `proj_short_name`.
    unmatched_to: Vec<String>,
    /// Projects left out because a file lost their rows in mapping.
    incomplete: Vec<String>,
    projects: Vec<Comparison>,
}

/// Why a project is left out: a lost row would read as a deletion.
const INCOMPLETE: &str = "a file lost some of its rows, so missing work wouldn't be a real change";

fn summary(out: &mut String, c: &Counts) {
    let _ = writeln!(
        out,
        "  activities: {} added, {} deleted, {} changed",
        c.added, c.deleted, c.changed
    );
    let _ = writeln!(
        out,
        "  relationships: {} added, {} deleted, {} changed lag",
        c.links_added, c.links_deleted, c.links_changed
    );
    let _ = writeln!(
        out,
        "  critical: {} joined, {} left; milestones moved: {}; flagged: {}",
        c.became_critical, c.left_critical, c.milestones_moved, c.findings
    );
}

fn moved(comparison: &Comparison) -> String {
    let project = &comparison.project;
    let finish = |date: &Option<String>| date.clone().unwrap_or_else(|| "none".to_owned());
    match project.finish_moved_hours {
        Some(hours) if hours != 0.0 => format!(
            "finish {} to {} ({:+} hours)",
            finish(&project.from_finish),
            finish(&project.to_finish),
            rounded(hours)
        ),
        _ => format!("finish {}", finish(&project.to_finish)),
    }
}

/// The text form of a comparison.
pub fn human(report: &Report) -> String {
    let mut out = String::new();
    for name in &report.unmatched_from {
        let _ = writeln!(out, "Project {name} is only in {}", report.from);
    }
    for name in &report.unmatched_to {
        let _ = writeln!(out, "Project {name} is only in {}", report.to);
    }
    for name in &report.incomplete {
        let _ = writeln!(out, "Project {name} wasn't compared: {INCOMPLETE}");
    }
    for comparison in &report.projects {
        let _ = writeln!(
            out,
            "Project {}: {}",
            comparison.project.code,
            moved(comparison)
        );
        summary(&mut out, &comparison.counts);
        for finding in &comparison.findings {
            let _ = writeln!(out, "  look at {}: {}", finding.subject, finding.detail);
        }
    }
    out.trim_end().to_owned()
}

fn markdown(report: &Report) -> String {
    let mut out = format!("# Schedule changes: {} to {}\n", report.from, report.to);
    for name in &report.unmatched_from {
        let _ = writeln!(out, "\nProject {name} is only in {}.", report.from);
    }
    for name in &report.unmatched_to {
        let _ = writeln!(out, "\nProject {name} is only in {}.", report.to);
    }
    for name in &report.incomplete {
        let _ = writeln!(out, "\nProject {name} wasn't compared: {INCOMPLETE}.");
    }
    for comparison in &report.projects {
        let c = &comparison.counts;
        let _ = writeln!(
            out,
            "\n## {}\n\n{}\n\n| Change | Count |\n|---|---:|\n| Activities added | {} |\n| Activities deleted | {} |\n| Activities changed | {} |\n| Relationships added | {} |\n| Relationships deleted | {} |\n| Relationship lags changed | {} |\n| Milestones moved | {} |\n| Became critical | {} |\n| Left critical | {} |",
            comparison.project.code,
            moved(comparison),
            c.added,
            c.deleted,
            c.changed,
            c.links_added,
            c.links_deleted,
            c.links_changed,
            c.milestones_moved,
            c.became_critical,
            c.left_critical
        );
        if !comparison.findings.is_empty() {
            out.push_str("\n### To look at\n\n");
            for finding in &comparison.findings {
                let _ = writeln!(out, "- **{}**: {}", finding.subject, finding.detail);
            }
        }
        if !comparison.changed.is_empty() {
            out.push_str(
                "\n### Changed activities\n\n| Activity | Field | From | To |\n|---|---|---|---|\n",
            );
            for activity in &comparison.changed {
                for change in &activity.changes {
                    let _ = writeln!(
                        out,
                        "| {} | {} | {} | {} |",
                        activity.code, change.field, change.from, change.to
                    );
                }
            }
        }
    }
    out
}

/// Pairs projects by `proj_short_name`, then by ID. Two files with one project
/// each are one project, whatever it was renamed to.
pub(crate) fn pairs<'a>(
    from: &'a [Schedule],
    to: &'a [Schedule],
) -> (Vec<(&'a Schedule, &'a Schedule)>, Vec<String>, Vec<String>) {
    if let ([a], [b]) = (from, to) {
        return (vec![(a, b)], Vec::new(), Vec::new());
    }
    // Names first, so an ID can't claim a project another name already matches.
    let mut taken = Vec::new();
    let mut matched = Vec::new();
    let mut left = Vec::new();
    for a in from {
        match to
            .iter()
            .find(|b| b.project.short_name == a.project.short_name)
        {
            Some(b) => {
                taken.push(b.project.id);
                matched.push((a, b));
            }
            None => left.push(a),
        }
    }
    let mut unmatched_from = Vec::new();
    for a in left {
        let free = |b: &&Schedule| b.project.id == a.project.id && !taken.contains(&b.project.id);
        match to.iter().find(free) {
            Some(b) => {
                taken.push(b.project.id);
                matched.push((a, b));
            }
            None => unmatched_from.push(a.project.short_name.clone()),
        }
    }
    let unmatched_to = to
        .iter()
        .filter(|b| !taken.contains(&b.project.id))
        .map(|b| b.project.short_name.clone())
        .collect();
    (matched, unmatched_from, unmatched_to)
}

/// Compares two files that have already been read.
///
/// # Errors
///
/// `no-projects` if a file has no project to compare.
pub fn report(
    from_file: &str,
    from: &Loaded,
    to_file: &str,
    to: &Loaded,
) -> Result<Report, CliError> {
    for (file, loaded) in [(from_file, from), (to_file, to)] {
        if loaded.schedules.is_empty() {
            return Err(CliError::new(
                "no-projects",
                format!("{file} has no project that can be compared"),
                "`inspect` reports what the file holds and what was left out",
            ));
        }
    }
    let (matched, unmatched_from, unmatched_to) = pairs(&from.schedules, &to.schedules);
    let (whole, lost): (Vec<_>, Vec<_>) = matched.iter().partition(|pair| {
        from.refuse_incomplete(pair.0).is_ok() && to.refuse_incomplete(pair.1).is_ok()
    });
    Ok(Report {
        schema: "totalfloat.diff/1",
        from: from_file.to_owned(),
        to: to_file.to_owned(),
        unmatched_from,
        unmatched_to,
        incomplete: lost
            .iter()
            .map(|(_, b)| b.project.short_name.clone())
            .collect(),
        projects: whole.iter().map(|(a, b)| compare(a, b)).collect(),
    })
}

impl Report {
    /// Keeps at most `limit` items in each list.
    pub fn limit(&mut self, limit: usize) {
        for comparison in &mut self.projects {
            comparison.limit(limit);
        }
    }
}

/// Runs the command. Changes flagged for a look exit 3, like other findings.
pub fn run(from: &Path, to: &Path, as_markdown: bool, json: bool) -> Result<u8, CliError> {
    let earlier = crate::location::read(from)?;
    let later = crate::location::read(to)?;
    let report = report(
        &from.display().to_string(),
        &Loaded::new(earlier),
        &to.display().to_string(),
        &Loaded::new(later),
    )?;
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else if as_markdown {
        print(&markdown(&report));
    } else {
        print_line(&human(&report));
    }
    let flagged = report.projects.iter().any(|c| c.counts.findings > 0)
        || !report.unmatched_from.is_empty()
        || !report.incomplete.is_empty();
    Ok(if flagged { exit::FINDINGS } else { exit::OK })
}

#[cfg(test)]
mod tests {
    use super::*;
    use totalfloat_model::{Project, ProjectId};

    fn project(id: u64, name: &str) -> Schedule {
        Schedule::new(Project::new(ProjectId(id), name))
    }

    #[test]
    fn a_name_match_keeps_an_id_from_claiming_the_same_project() {
        let from = [project(1, "X"), project(2, "Y")];
        let to = [project(1, "Y"), project(3, "Z")];
        let (matched, unmatched_from, unmatched_to) = pairs(&from, &to);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].0.project.short_name, "Y");
        assert_eq!(unmatched_from, ["X"]);
        assert_eq!(unmatched_to, ["Z"]);
    }

    #[test]
    fn single_projects_match_whatever_they_are_called() {
        let (from, to) = ([project(1, "X")], [project(2, "Y")]);
        let (matched, ..) = pairs(&from, &to);
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn markdown_names_projects_on_one_side_only() {
        let report = Report {
            schema: "totalfloat.diff/1",
            from: "a.xer".to_owned(),
            to: "b.xer".to_owned(),
            unmatched_from: vec!["X".to_owned()],
            unmatched_to: vec!["Z".to_owned()],
            incomplete: vec!["Y".to_owned()],
            projects: Vec::new(),
        };
        let text = markdown(&report);
        assert!(text.contains("Project X is only in a.xer."));
        assert!(text.contains("Project Z is only in b.xer."));
        assert!(text.contains("Project Y wasn't compared"));
    }
}
