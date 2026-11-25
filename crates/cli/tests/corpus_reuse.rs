//! On every corpus file: recalculating only what an edit reaches gives the
//! full calculation's result, activity for activity, for edits of each
//! kind the engine reads. Counts only, never content. Skips when
//! `TOTALFLOAT_CORPUS` is unset.

#![allow(clippy::unwrap_used, reason = "test")]

use std::collections::BTreeMap;
use std::path::Path;

use totalfloat_engine::{Calculated, schedule_open_together, schedule_reusing};
use totalfloat_model::{ActivityId, ActivityStatus, Constraint, ConstraintType, Minutes, Schedule};

/// Activities edited per project, spread across it, which keeps the run short
/// on large files.
const PER_PROJECT: usize = 6;

type Edit = fn(&mut Schedule, ActivityId);

/// Each kind of edit, on one activity or on a link into it.
const EDITS: &[(&str, Edit)] = &[
    ("longer", |s, id| {
        let a = s.activities.get_mut(&id).unwrap();
        a.remaining_duration = Minutes(a.remaining_duration.get() + 480);
    }),
    ("shorter", |s, id| {
        let a = s.activities.get_mut(&id).unwrap();
        a.remaining_duration = Minutes(a.remaining_duration.get() / 2);
    }),
    ("constraint", |s, id| {
        let date = s.project.data_date;
        let a = s.activities.get_mut(&id).unwrap();
        if a.status == ActivityStatus::NotStarted {
            a.primary_constraint = Some(Constraint {
                kind: ConstraintType::StartOnOrAfter,
                date: date.and_then(|d| d.checked_add(Minutes(60 * 24 * 14))),
            });
            a.secondary_constraint = None;
        }
    }),
    ("as late as possible", |s, id| {
        let a = s.activities.get_mut(&id).unwrap();
        if a.status == ActivityStatus::NotStarted {
            a.primary_constraint = Some(Constraint {
                kind: ConstraintType::AsLateAsPossible,
                date: None,
            });
            a.secondary_constraint = None;
        }
    }),
    ("started", |s, id| {
        let date = s.project.data_date;
        let a = s.activities.get_mut(&id).unwrap();
        if a.status == ActivityStatus::NotStarted && date.is_some() {
            a.status = ActivityStatus::InProgress;
            a.actual_start = date;
        }
    }),
    ("link lag", |s, id| {
        if let Some(link) = s.relationships.values_mut().find(|l| l.successor == id) {
            link.lag = Minutes(link.lag.get() + 480);
        }
    }),
    ("link removed", |s, id| {
        let found = s.relationships.values().find(|l| l.successor == id);
        if let Some(link) = found.map(|l| l.id) {
            s.relationships.remove(&link);
        }
    }),
    ("deleted", |s, id| {
        s.activities.remove(&id);
        s.relationships
            .retain(|_, l| l.predecessor != id && l.successor != id);
    }),
];

#[derive(Default, Debug)]
struct Counts {
    projects: usize,
    refused: usize,
    edits: usize,
    reused: usize,
    refused_after: usize,
    kinds: BTreeMap<&'static str, usize>,
}

fn try_project(index: usize, all: &[Schedule], at: usize, counts: &mut Counts) {
    let schedule = &all[at];
    let options = &schedule.project.options;
    let Ok(before) = schedule_reusing(schedule, options, (all, None), None) else {
        counts.refused += 1;
        return;
    };
    counts.projects += 1;
    let ids: Vec<ActivityId> = schedule.activities.keys().copied().collect();
    let step = (ids.len() / PER_PROJECT).max(1);
    for id in ids.iter().step_by(step).take(PER_PROJECT) {
        for (kind, edit) in EDITS {
            let mut after = schedule.clone();
            edit(&mut after, *id);
            let reused = schedule_reusing(&after, options, (all, None), Some(&before));
            let full = schedule_open_together(&after, options, all, None);
            assert!(
                reused.as_ref().map(Calculated::result) == full.as_ref(),
                "file {index}, project {at}, {kind}"
            );
            counts.edits += 1;
            *counts.kinds.entry(kind).or_default() += 1;
            match reused {
                Ok(calculated) => {
                    if calculated
                        .worked_out()
                        .is_some_and(|worked| worked < after.activities.len())
                    {
                        counts.reused += 1;
                    }
                }
                Err(_) => counts.refused_after += 1,
            }
        }
    }
}

#[test]
fn reusing_a_calculation_gives_the_full_result_on_every_corpus_file() {
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS unset; skipping the reuse corpus test");
        return;
    };
    let mut files = Vec::new();
    for entry in std::fs::read_dir(Path::new(&root).join("files")).unwrap() {
        for file in std::fs::read_dir(entry.unwrap().path()).unwrap() {
            files.push(file.unwrap().path());
        }
    }
    files.sort();
    let mut counts = Counts::default();
    for (index, file) in files.iter().enumerate() {
        let Ok(raw) = totalfloat_xer::raw::read(std::fs::read(file).unwrap()) else {
            continue;
        };
        let schedules = totalfloat_xer::to_model(&raw).0;
        for at in 0..schedules.len() {
            try_project(index, &schedules, at, &mut counts);
        }
    }
    eprintln!("reuse: {counts:?}");
    assert!(counts.edits > 0, "the corpus has projects to edit");
    assert!(counts.reused > 0, "some edits reach only part of a project");
}
