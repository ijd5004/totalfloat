//! On every corpus file: a duration edit and its reverse leave each assignment's
//! units and costs where they were, and an edit to the same duration changes
//! nothing (TF-034, TF-038). Skips when `TOTALFLOAT_CORPUS` is unset.

#![allow(clippy::unwrap_used, reason = "test")]

use std::path::Path;

use totalfloat_edit::{Edit, NextIds, apply};
use totalfloat_model::{ActivityStatus, ActivityType, Amounts, Schedule};

/// Activities tried per project, which keeps the run short on large files.
const PER_PROJECT: usize = 40;

fn close(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            let difference = (a - b).abs();
            difference <= 0.01 || difference <= 1e-4 * a.abs().max(b.abs())
        }
        (a, b) => a.is_none() && b.is_none(),
    }
}

fn same(a: &Amounts, b: &Amounts) -> bool {
    [
        (a.budgeted_units, b.budgeted_units),
        (a.remaining_units, b.remaining_units),
        (a.actual_units, b.actual_units),
        (a.budgeted_cost, b.budgeted_cost),
        (a.remaining_cost, b.remaining_cost),
        (a.actual_cost, b.actual_cost),
    ]
    .into_iter()
    .all(|(x, y)| close(x, y))
}

#[allow(clippy::cast_precision_loss, reason = "minutes are far below 2^52")]
fn hours(minutes: i64) -> String {
    format!("{}h", minutes as f64 / 60.0)
}

fn edit(code: &str, status: ActivityStatus, minutes: i64) -> Edit {
    let span = serde_json::Value::from(hours(minutes));
    let key = if status == ActivityStatus::NotStarted {
        "original"
    } else {
        "remaining"
    };
    serde_json::from_value(serde_json::json!({
        "op": "set_duration", "activity": code, key: span
    }))
    .unwrap()
}

fn ids() -> NextIds {
    NextIds {
        relationship: u64::MAX / 2,
        activity: u64::MAX / 2,
        calendar: u64::MAX / 2,
        code: u64::MAX / 2,
        assignment: u64::MAX / 2,
        wbs: u64::MAX / 2,
        code_type: u64::MAX / 2,
        resource: u64::MAX / 2,
        resource_rate: u64::MAX / 2,
        role: u64::MAX / 2,
        role_rate: u64::MAX / 2,
        expense: u64::MAX / 2,
        project_code_type: u64::MAX / 2,
        project_code: u64::MAX / 2,
        obs: u64::MAX / 2,
        activity_note: u64::MAX / 2,
        wbs_note: u64::MAX / 2,
        step: u64::MAX / 2,
        udf_type: u64::MAX / 2,
    }
}

fn amounts_of(schedule: &Schedule, activity: totalfloat_model::ActivityId) -> Vec<Amounts> {
    schedule
        .assignments
        .values()
        .filter(|a| a.activity == activity)
        .map(|a| a.amounts)
        .collect()
}

/// Edits that keep a duration, then double and restore it, on one project.
/// Returns how many activities were tried.
fn try_project(schedule: &Schedule, label: &str) -> usize {
    let mut tried = 0;
    let candidates = schedule.activities.values().filter(|a| {
        a.status != ActivityStatus::Completed
            && a.activity_type == ActivityType::TaskDependent
            && a.remaining_duration.get() > 0
            && schedule.assignments.values().any(|s| s.activity == a.id)
    });
    for activity in candidates.take(PER_PROJECT) {
        let span = if activity.status == ActivityStatus::NotStarted {
            activity.original_duration.get()
        } else {
            activity.remaining_duration.get()
        };
        // An edit gives hours to three decimals, so only those spans are exact.
        if span % 3 != 0 {
            continue;
        }
        let before = amounts_of(schedule, activity.id);
        let mut kept = schedule.clone();
        let mut next = ids();
        let same_span = edit(&activity.code, activity.status, span);
        if apply(&mut kept, &same_span, &mut next).is_err() {
            continue;
        }
        let code = &activity.code;
        assert_eq!(
            amounts_of(&kept, activity.id),
            before,
            "{label} {code}: the same duration moved units"
        );
        let mut there = schedule.clone();
        let longer = edit(code, activity.status, span * 2);
        apply(&mut there, &longer, &mut next).unwrap();
        apply(&mut there, &same_span, &mut next).unwrap();
        for (was, is) in before.iter().zip(amounts_of(&there, activity.id)) {
            assert!(
                same(was, &is),
                "{label} {code}: {was:?} came back as {is:?}"
            );
        }
        tried += 1;
    }
    tried
}

#[test]
fn a_duration_edit_and_its_reverse_leave_units_and_costs_on_every_corpus_file() {
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS unset; skipping units corpus test");
        return;
    };
    let mut files = Vec::new();
    for entry in std::fs::read_dir(Path::new(&root).join("files")).unwrap() {
        for file in std::fs::read_dir(entry.unwrap().path()).unwrap() {
            files.push(file.unwrap().path());
        }
    }
    files.sort();
    let mut tried = 0;
    for (index, file) in files.iter().enumerate() {
        let Ok(raw) = totalfloat_xer::raw::read(std::fs::read(file).unwrap()) else {
            continue;
        };
        for schedule in totalfloat_xer::to_model(&raw).0 {
            tried += try_project(&schedule, &format!("file {index}"));
        }
    }
    eprintln!("tried {tried} activities with assignments");
    assert!(tried > 0, "the corpus has activities with assignments");
}
