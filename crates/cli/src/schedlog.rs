//! `schedule --log`: a text log of a calculation, covering
//! the settings used, counts of what was calculated,
//! the problems found, each listed in full, and the results. The wording and
//! layout are TotalFloat's own. The same inputs always give the same bytes:
//! there's no clock time in it.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use totalfloat_model::{
    Activity, ActivityId, ActivityStatus, CriticalDefinition, FloatPathMode, FloatType,
    LagCalendar, OutOfSequenceMode, Schedule,
};

use crate::output::hours_of;
use crate::query::{constraint_label, link_label};

fn yes(flag: bool) -> &'static str {
    if flag { "yes" } else { "no" }
}

fn date(value: Option<totalfloat_model::DateTime>) -> String {
    value.map_or_else(|| "none".to_owned(), |d| d.to_string())
}

/// A list under its heading: the count, then each item on its own line, or
/// `none`.
fn list(out: &mut String, heading: &str, items: &[String]) {
    if items.is_empty() {
        let _ = writeln!(out, "  {heading}: none");
        return;
    }
    let _ = writeln!(out, "  {heading} ({}):", items.len());
    for item in items {
        let _ = writeln!(out, "    {item}");
    }
}

fn settings(out: &mut String, s: &Schedule, ignore_external: bool) {
    let o = &s.project.options;
    let _ = writeln!(out, "Settings");
    let _ = writeln!(out, "  Data date: {}", date(s.project.data_date));
    let _ = writeln!(
        out,
        "  Out-of-sequence progress: {}",
        match o.out_of_sequence {
            OutOfSequenceMode::RetainedLogic => "retained logic",
            OutOfSequenceMode::ProgressOverride => "progress override",
            OutOfSequenceMode::ActualDates => "actual dates",
        }
    );
    let _ = writeln!(
        out,
        "  Calendar for lags: {}",
        match o.lag_calendar {
            LagCalendar::Predecessor => "the predecessor's",
            LagCalendar::Successor => "the successor's",
            LagCalendar::TwentyFourHour => "24 hours a day",
            LagCalendar::ProjectDefault => "the project's default",
        }
    );
    let _ = writeln!(
        out,
        "  Total float: {}",
        match o.float_type {
            FloatType::StartFloat => "start float",
            FloatType::FinishFloat => "finish float",
            FloatType::SmallestFloat => "the smaller of start and finish float",
        }
    );
    let critical = match o.critical {
        CriticalDefinition::TotalFloatAtMost(limit) => {
            format!("total float of {}h or less", hours_of(limit))
        }
        CriticalDefinition::LongestPath => "on the longest path".to_owned(),
    };
    let _ = writeln!(out, "  Critical: {critical}");
    let _ = writeln!(
        out,
        "  Open-ended activities critical: {}",
        yes(o.make_open_ended_critical)
    );
    let _ = writeln!(
        out,
        "  Start-to-start lag counted from: {}",
        if o.ss_lag_from_early_start {
            "the predecessor's early start"
        } else {
            "the predecessor's actual start"
        }
    );
    let _ = writeln!(
        out,
        "  Expected finish dates used: {}",
        yes(o.use_expected_finish)
    );
    let _ = writeln!(
        out,
        "  Relationships to other projects: {}",
        if ignore_external || o.ignore_external_relationships {
            "ignored"
        } else {
            "counted"
        }
    );
    // As the file has them; what these two mean in P6 isn't confirmed (TF-008).
    let _ = writeln!(
        out,
        "  Float from each project's own finish: {}",
        yes(o.float_from_project_finish)
    );
    let _ = writeln!(
        out,
        "  Planned dates set to the forecast: {}",
        yes(o.set_planned_to_forecast)
    );
    let paths = &o.float_paths;
    if paths.enabled {
        let by = match paths.mode {
            FloatPathMode::TotalFloat => "total float",
            FloatPathMode::FreeFloat => "free float",
        };
        let most = paths
            .max_paths
            .map_or_else(|| "no limit".to_owned(), |n| n.to_string());
        let end = paths.end_activity.map_or_else(
            || "the project's finish".to_owned(),
            |id| {
                s.activities
                    .get(&id)
                    .map_or_else(|| format!("task {}", id.0), |a| a.code.clone())
            },
        );
        let _ = writeln!(
            out,
            "  Multiple float paths: yes, by {by}, at most {most}, ending at {end}"
        );
    } else {
        let _ = writeln!(out, "  Multiple float paths: no");
    }
}

fn statistics(out: &mut String, s: &Schedule) {
    let count = |status| s.activities.values().filter(|a| a.status == status).count();
    let milestones = s
        .activities
        .values()
        .filter(|a| a.activity_type.is_milestone())
        .count();
    let spans = s
        .activities
        .values()
        .filter(|a| a.activity_type.is_span())
        .count();
    let external = s
        .relationships
        .values()
        .filter(|r| s.is_external(r))
        .count();
    let constrained = s
        .activities
        .values()
        .filter(|a| a.primary_constraint.is_some() || a.secondary_constraint.is_some())
        .count();
    let _ = writeln!(out, "Statistics");
    let _ = writeln!(
        out,
        "  Activities: {} (not started {}, in progress {}, completed {})",
        s.activities.len(),
        count(ActivityStatus::NotStarted),
        count(ActivityStatus::InProgress),
        count(ActivityStatus::Completed)
    );
    let _ = writeln!(out, "  Milestones: {milestones}");
    let _ = writeln!(out, "  Level of effort and WBS summary activities: {spans}");
    let _ = writeln!(
        out,
        "  Relationships: {} (to or from other projects: {external})",
        s.relationships.len()
    );
    let _ = writeln!(out, "  Activities with constraints: {constrained}");
}

/// Activities by Activity ID, then `task_id`, so the log reads in order.
fn by_code(s: &Schedule) -> Vec<&Activity> {
    let mut all: Vec<&Activity> = s.activities.values().collect();
    all.sort_by(|a, b| a.code.cmp(&b.code).then(a.id.cmp(&b.id)));
    all
}

fn named(a: &Activity) -> String {
    format!("{}  {}", a.code, a.name)
}

#[allow(
    clippy::too_many_lines,
    reason = "one list after another, in the log's order"
)]
fn problems(out: &mut String, s: &Schedule, projects: &[Schedule], ignore_external: bool) {
    let activities = by_code(s);
    let unfinished = || {
        activities
            .iter()
            .copied()
            .filter(|a| a.status != ActivityStatus::Completed)
    };
    let counted = |r: &&totalfloat_model::Relationship| {
        !(ignore_external || s.project.options.ignore_external_relationships) || !s.is_external(r)
    };
    let has_pred: BTreeSet<ActivityId> = s
        .relationships
        .values()
        .filter(counted)
        .map(|r| r.successor)
        .collect();
    let has_succ: BTreeSet<ActivityId> = s
        .relationships
        .values()
        .filter(counted)
        .map(|r| r.predecessor)
        .collect();
    let _ = writeln!(out, "Problems");
    let no_pred: Vec<String> = unfinished()
        .filter(|a| !has_pred.contains(&a.id))
        .map(named)
        .collect();
    list(out, "Unfinished activities without predecessors", &no_pred);
    let no_succ: Vec<String> = unfinished()
        .filter(|a| !has_succ.contains(&a.id))
        .map(named)
        .collect();
    list(out, "Unfinished activities without successors", &no_succ);

    let code_of = |id: ActivityId| {
        std::iter::once(s)
            .chain(projects)
            .find_map(|p| p.activities.get(&id))
            .map_or_else(|| format!("task {}", id.0), |a| a.code.clone())
    };
    // Links to other projects count unless they're ignored, for this run or
    // by the project's own option.
    let others = if ignore_external || s.project.options.ignore_external_relationships {
        &[][..]
    } else {
        projects
    };
    let mut broken: Vec<String> = totalfloat_engine::out_of_sequence_with_projects(s, others)
        .iter()
        .filter_map(|id| s.relationships.get(id))
        .map(|r| {
            format!(
                "{} {} {}: {} has progress before {} allows it",
                code_of(r.predecessor),
                link_label(r.kind),
                code_of(r.successor),
                code_of(r.successor),
                code_of(r.predecessor)
            )
        })
        .collect();
    broken.sort();
    list(out, "Out-of-sequence progress", &broken);

    let data_date = s.project.data_date;
    let after: Vec<String> = activities
        .iter()
        .filter_map(|a| {
            let late = |d: Option<totalfloat_model::DateTime>| {
                d.filter(|d| data_date.is_some_and(|data| *d > data))
            };
            let mut said = Vec::new();
            if let Some(d) = late(a.actual_start) {
                said.push(format!("actual start {d}"));
            }
            if let Some(d) = late(a.actual_finish) {
                said.push(format!("actual finish {d}"));
            }
            (!said.is_empty()).then(|| format!("{}: {}", a.code, said.join(", ")))
        })
        .collect();
    list(out, "Actual dates after the data date", &after);

    let constraints: Vec<String> = activities
        .iter()
        .flat_map(|a| {
            [
                ("primary", a.primary_constraint),
                ("secondary", a.secondary_constraint),
            ]
            .into_iter()
            .filter_map(move |(which, c)| {
                let c = c?;
                let on = c.date.map_or_else(String::new, |d| format!(" {d}"));
                Some(format!(
                    "{}: {which} {}{on}",
                    a.code,
                    constraint_label(c.kind)
                ))
            })
        })
        .collect();
    list(out, "Constraints", &constraints);

    let negative: Vec<String> = unfinished()
        .filter_map(|a| {
            let float = a.stored.total_float?;
            (float.get() < 0).then(|| format!("{}: total float {}h", a.code, hours_of(float)))
        })
        .collect();
    list(out, "Unfinished activities with negative float", &negative);

    let external: Vec<String> =
        if ignore_external || s.project.options.ignore_external_relationships {
            Vec::new()
        } else {
            unfinished()
                .filter_map(|a| {
                    let mut said = Vec::new();
                    if let Some(d) = a.external_early_start {
                        said.push(format!("external early start {d}"));
                    }
                    if let Some(d) = a.external_late_finish {
                        said.push(format!("external late finish {d}"));
                    }
                    (!said.is_empty()).then(|| format!("{}: {}", a.code, said.join(", ")))
                })
                .collect()
        };
    list(out, "External dates from other projects", &external);
}

fn results(out: &mut String, before: &Schedule, after: &Schedule, critical: &BTreeSet<ActivityId>) {
    let start = after
        .activities
        .values()
        .filter_map(|a| a.actual_start.or(a.stored.early_start))
        .min();
    let finish = after.project.stored.scheduled_finish;
    let _ = writeln!(out, "Results");
    let _ = writeln!(out, "  Start: {}", date(start));
    let _ = writeln!(
        out,
        "  Finish: {} (before this run: {})",
        date(finish),
        date(before.project.stored.scheduled_finish)
    );
    let count = after
        .activities
        .keys()
        .filter(|id| critical.contains(id))
        .count();
    let _ = writeln!(out, "  Critical activities: {count}");
    if let Some(must) = after.project.must_finish_by {
        let met = match finish {
            Some(f) if f <= must => "the finish meets it",
            Some(_) => "the finish is after it",
            None => "there's no finish to compare",
        };
        let _ = writeln!(out, "  Must finish by: {must}; {met}");
    }
}

/// The log of a calculation: each project calculated, before and after, with
/// the activities the engine found critical.
pub fn text(
    file: &str,
    (before, after): (&[Schedule], &[Schedule]),
    critical: &BTreeSet<ActivityId>,
    ignore_external: bool,
) -> String {
    let mut out = format!("TotalFloat scheduling log\nFile: {file}\n");
    for project in after {
        let Some(was) = before.iter().find(|b| b.project.id == project.project.id) else {
            continue;
        };
        let _ = writeln!(
            out,
            "\nProject {} ({}): {}\n",
            project.project.short_name,
            project.project.id.0,
            project.project.name.as_deref().unwrap_or("")
        );
        settings(&mut out, project, ignore_external);
        out.push('\n');
        statistics(&mut out, project);
        out.push('\n');
        problems(&mut out, project, before, ignore_external);
        out.push('\n');
        results(&mut out, was, project, critical);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use totalfloat_model::{
        Activity, ActivityId, ActivityStatus, CalendarId, FloatPathMode, Minutes, Project,
        ProjectId, Relationship, RelationshipId, RelationshipType, Schedule, WbsId,
    };

    /// Project 1's unfinished A2 leads, across projects, to project 2's B1,
    /// which has started.
    fn across() -> (Schedule, Schedule) {
        let task = |id, project, code: &str| {
            Activity::new(
                ActivityId(id),
                ProjectId(project),
                WbsId(1),
                CalendarId(1),
                code,
                "Authored",
            )
        };
        let mut one = Schedule::new(Project::new(ProjectId(1), "ONE"));
        one.activities.insert(ActivityId(2), task(2, 1, "A2"));
        let mut two = Schedule::new(Project::new(ProjectId(2), "TWO"));
        let mut started = task(11, 2, "B1");
        started.status = ActivityStatus::InProgress;
        started.actual_start = "2026-01-05 08:00".parse().ok();
        two.activities.insert(ActivityId(11), started);
        two.relationships.insert(
            RelationshipId(20),
            Relationship {
                id: RelationshipId(20),
                predecessor: ActivityId(2),
                successor: ActivityId(11),
                predecessor_project: ProjectId(1),
                successor_project: ProjectId(2),
                kind: RelationshipType::FinishToStart,
                lag: Minutes::ZERO,
                source: None,
            },
        );
        (one, two)
    }

    #[test]
    fn out_of_sequence_progress_across_projects_is_listed_unless_ignored() {
        let (one, two) = across();
        let projects = [one, two.clone()];
        let listed = |ignore: bool, two: &Schedule| {
            let mut out = String::new();
            super::problems(&mut out, two, &projects, ignore);
            out.contains("B1 has progress before A2 allows it")
        };
        assert!(listed(false, &two));
        assert!(!listed(true, &two));
        // A run on project 2 alone still finds project 1 in the file.
        let log = super::text(
            "plan.xer",
            (&projects, std::slice::from_ref(&two)),
            &BTreeSet::new(),
            false,
        );
        assert!(log.contains("B1 has progress before A2 allows it"), "{log}");
        let mut ignoring = two;
        ignoring.project.options.ignore_external_relationships = true;
        assert!(!listed(false, &ignoring));
    }

    #[test]
    fn settings_name_every_option_including_float_paths() {
        let mut schedule = Schedule::new(Project::new(ProjectId(1), "P"));
        let options = &mut schedule.project.options;
        options.float_paths.enabled = true;
        options.float_paths.mode = FloatPathMode::FreeFloat;
        options.float_paths.max_paths = Some(5);
        options.set_planned_to_forecast = true;
        let mut out = String::new();
        super::settings(&mut out, &schedule, false);
        assert!(
            out.contains("  Multiple float paths: yes, by free float, at most 5, ending at the project's finish\n"),
            "{out}"
        );
        assert!(
            out.contains("  Planned dates set to the forecast: yes\n"),
            "{out}"
        );
        assert!(
            out.contains("  Float from each project's own finish: yes\n"),
            "{out}"
        );
    }
}
