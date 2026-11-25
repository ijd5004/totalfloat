//! Activity steps in a synthetic file, `fixtures/xer/steps.xer`,
//! written fresh from `model-mapping.xer`'s first project with steps added
//! and physical percent measured by steps. The file is read, edited and
//! written back, and only what changed differs.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::Path;

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};
use totalfloat_xer::raw::{self, RawXer};
use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
use totalfloat_xer::{patch, to_model};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

const A101: m::ActivityId = m::ActivityId(101);

fn step(id: u64, name: &str, weight: f64, percent: f64) -> m::Step {
    m::Step {
        id: m::StepId(id),
        name: name.to_owned(),
        sequence: i64::try_from(id).unwrap(),
        weight: Some(weight),
        percent: Some(percent),
        completed: percent >= 100.0,
        description: String::new(),
        source: None,
    }
}

/// The model the fixture is written from: steps weighted 1, 2 and 1 at
/// 100, 50 and 0 percent, so physical percent is 50.
fn authored() -> m::Schedule {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut schedule = to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule
        .relationships
        .retain(|_, r| r.predecessor_project == project && r.successor_project == project);
    schedule.project.percent_from_steps = true;
    let a = schedule.activities.get_mut(&A101).unwrap();
    a.percent_complete_type = m::PercentCompleteType::Physical;
    for s in [
        step(1, "Excavate", 1.0, 100.0),
        step(2, "Form and pour", 2.0, 50.0),
        step(3, "Backfill", 1.0, 0.0),
    ] {
        a.steps.insert(s.id, s);
    }
    a.physical_percent_complete = a.steps_percent();
    schedule
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

fn fixture() -> RawXer {
    raw::read(std::fs::read(root().join("fixtures/xer/steps.xer")).unwrap()).unwrap()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md.
#[test]
fn the_fixture_is_the_authored_model_written_fresh() {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let written = bytes(&write(&authored(), options).unwrap());
    let path = root().join("fixtures/xer/steps.xer");
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == written,
        "fixtures/xer/steps.xer is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

#[test]
fn steps_are_read_and_give_the_physical_percent() {
    let schedule = to_model(&fixture()).0.swap_remove(0);
    assert!(schedule.project.percent_from_steps);
    let a = &schedule.activities[&A101];
    assert_eq!(a.steps.len(), 3);
    let pour = &a.steps[&m::StepId(2)];
    assert_eq!(
        (
            pour.name.as_str(),
            pour.weight,
            pour.percent,
            pour.completed
        ),
        ("Form and pour", Some(2.0), Some(50.0), false)
    );
    assert!(a.steps[&m::StepId(1)].completed);
    // (1 x 100 + 2 x 50 + 1 x 0) / 4
    assert_eq!(a.steps_percent(), Some(50.0));
    assert_eq!(a.physical_percent_complete, Some(50.0));
    assert_eq!(schedule.validate(), []);
}

#[test]
fn an_unchanged_schedule_writes_back_byte_for_byte() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
}

#[test]
fn a_step_s_progress_changes_its_row_only() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let s = schedule
        .activities
        .get_mut(&A101)
        .unwrap()
        .steps
        .get_mut(&m::StepId(3))
        .unwrap();
    s.percent = Some(100.0);
    s.completed = true;
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 2);
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(back.activities[&A101].steps_percent(), Some(75.0));
}

#[test]
fn new_and_deleted_steps_are_rows_added_and_removed() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let a = schedule.activities.get_mut(&A101).unwrap();
    a.steps.remove(&m::StepId(3));
    a.steps.insert(m::StepId(4), step(4, "Cure", 1.0, 0.0));
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    let names: Vec<&str> = back.activities[&A101]
        .steps
        .values()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, ["Excavate", "Form and pour", "Cure"]);
}

#[test]
fn a_step_whose_activity_is_missing_is_left_out_and_reported() {
    let mut raw = fixture();
    let t = raw.table_index("TASKPROC").unwrap();
    let column = raw.tables()[t].field_index("task_id").unwrap();
    raw.set_cell(t, 0, column, "999").unwrap();
    let (schedules, diagnostics) = to_model(&raw);
    assert_eq!(schedules[0].activities[&A101].steps.len(), 2);
    assert_eq!(diagnostics.omitted.len(), 1);
}

#[test]
fn a_description_with_quotes_and_lines_reads_back_as_written() {
    let mut schedule = authored();
    let s = schedule
        .activities
        .get_mut(&A101)
        .unwrap()
        .steps
        .get_mut(&m::StepId(1))
        .unwrap();
    s.description = "<p class=\"x\">Dig</p>\n<p>to 2 m</p>".to_owned();
    let out = write(&schedule, FreshOptions::new("2026-01-01".parse().unwrap())).unwrap();
    let back = to_model(&out).0.swap_remove(0);
    assert_eq!(
        back.activities[&A101].steps[&m::StepId(1)].description,
        "<p class=\"x\">Dig</p>\n<p>to 2 m</p>"
    );
}
