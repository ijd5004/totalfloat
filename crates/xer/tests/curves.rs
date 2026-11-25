//! Resource curves (`RSRCCURVDATA`) and each assignment's curve
//! (`TASKRSRC.curv_id`), in a file written fresh from `model-mapping.xer`'s
//! first project with two curves added, kept as `fixtures/xer/curves.xer`.
//! The file is read, edited and written back, and only what changed differs.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::Path;

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};
use totalfloat_xer::mapping::DiagnosticKind;
use totalfloat_xer::raw::{self, RawXer};
use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
use totalfloat_xer::{patch, to_model};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn curve(id: u64, name: &str, default: bool, points: [f64; m::CURVE_POINTS]) -> m::ResourceCurve {
    m::ResourceCurve {
        id: m::ResourceCurveId(id),
        name: name.to_owned(),
        default,
        points,
        source: None,
    }
}

/// The model the file is written from: a linear curve, a user's curve that
/// takes the second half of the work, and the first assignment on that one,
/// with 40 hours still to do.
fn authored() -> m::Schedule {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut schedule = to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule
        .relationships
        .retain(|_, r| r.predecessor_project == project && r.successor_project == project);
    let mut linear = [5.0; m::CURVE_POINTS];
    linear[0] = 0.0;
    let mut late = [0.0; m::CURVE_POINTS];
    late[11..].fill(10.0);
    for c in [
        curve(1, "Linear", true, linear),
        curve(2, "Late", false, late),
    ] {
        schedule.resource_curves.insert(c.id, c);
    }
    let first = schedule.assignments.values_mut().next().unwrap();
    first.curve = Some(m::ResourceCurveId(2));
    first.amounts.budgeted_units = Some(40.0);
    first.amounts.remaining_units = Some(40.0);
    schedule
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

fn options() -> FreshOptions {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    options
}

fn file() -> RawXer {
    raw::read(std::fs::read(root().join("fixtures/xer/curves.xer")).unwrap()).unwrap()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md.
#[test]
fn the_fixture_is_the_authored_model_written_fresh() {
    let written = bytes(&write(&authored(), options()).unwrap());
    let path = root().join("fixtures/xer/curves.xer");
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == written,
        "fixtures/xer/curves.xer is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

fn first(schedule: &mut m::Schedule) -> &mut m::Assignment {
    schedule.assignments.values_mut().next().unwrap()
}

#[test]
fn curves_and_an_assignment_s_curve_are_read() {
    let schedule = to_model(&file()).0.swap_remove(0);
    let curves: Vec<_> = schedule
        .resource_curves
        .values()
        .map(|c| {
            (
                c.id.0,
                c.name.as_str(),
                c.default,
                c.points.iter().sum::<f64>(),
            )
        })
        .collect();
    assert_eq!(
        curves,
        [(1, "Linear", true, 100.0), (2, "Late", false, 100.0)]
    );
    assert_eq!(
        schedule.resource_curves[&m::ResourceCurveId(2)].points,
        authored().resource_curves[&m::ResourceCurveId(2)].points
    );
    let on_curves: Vec<_> = schedule
        .assignments
        .values()
        .filter_map(|a| a.curve)
        .collect();
    assert_eq!(on_curves, [m::ResourceCurveId(2)]);
    assert_eq!(schedule.validate(), []);
}

#[test]
fn an_unchanged_schedule_writes_back_byte_for_byte() {
    let raw = file();
    let schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
}

#[test]
fn changing_or_clearing_an_assignment_s_curve_changes_only_that_cell() {
    let raw = file();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    for curve in [Some(m::ResourceCurveId(1)), None] {
        first(&mut schedule).curve = curve;
        let out = patch(&raw, &schedule).unwrap();
        let d = diff_lines(raw.source(), &bytes(&out));
        assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
        assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 1);
        let read = to_model(&out).0.swap_remove(0);
        assert_eq!(read.assignments.values().next().unwrap().curve, curve);
    }
}

#[test]
fn a_curve_the_file_lacks_or_an_edited_curve_is_refused() {
    let raw = file();
    let schedule = to_model(&raw).0.swap_remove(0);
    let mut missing = schedule.clone();
    first(&mut missing).curve = Some(m::ResourceCurveId(9));
    assert!(patch(&raw, &missing).is_err());
    let mut reshaped = schedule.clone();
    let late = reshaped
        .resource_curves
        .get_mut(&m::ResourceCurveId(2))
        .unwrap();
    late.points[20] += 1.0;
    assert!(patch(&raw, &reshaped).is_err());
    let mut removed = schedule;
    removed.resource_curves.remove(&m::ResourceCurveId(1));
    assert!(patch(&raw, &removed).is_err());
}

/// The corpus has assignments naming a curve its file doesn't hold (TF-039).
#[test]
fn a_curve_the_file_lacks_is_cleared_with_a_warning_and_kept_in_the_file() {
    let mut raw = file();
    let t = raw.table_index("TASKRSRC").unwrap();
    let column = raw.tables()[t].field_index("curv_id").unwrap();
    let row = raw.tables()[t]
        .rows
        .iter()
        .position(|r| r.get(column) == Some("2"))
        .unwrap();
    raw.set_cell(t, row, column, "9").unwrap();
    let raw = raw::read(bytes(&raw)).unwrap();
    let (mut schedules, diagnostics) = to_model(&raw);
    let schedule = schedules.swap_remove(0);
    assert!(schedule.assignments.values().all(|a| a.curve.is_none()));
    assert!(
        diagnostics
            .details
            .iter()
            .any(|d| { d.kind == DiagnosticKind::UnrepresentedValue && d.field == "curv_id" })
    );
    assert_eq!(schedule.validate(), []);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
    // A curve can't replace it: the row's own spread would stay.
    let mut curved = schedule;
    first(&mut curved).curve = Some(m::ResourceCurveId(1));
    assert!(patch(&raw, &curved).is_err());
}

#[test]
fn a_curve_missing_a_point_is_left_out_and_reported() {
    let mut raw = file();
    let t = raw.table_index("RSRCCURVDATA").unwrap();
    let column = raw.tables()[t].field_index("pct_usage_7").unwrap();
    raw.set_cell(t, 0, column, "").unwrap();
    let (schedules, diagnostics) = to_model(&raw);
    assert!(
        !schedules[0]
            .resource_curves
            .contains_key(&m::ResourceCurveId(1))
    );
    assert_eq!(diagnostics.skipped_fields.get("pct_usage"), Some(&1));
    assert_eq!(diagnostics.omitted.len(), 1);
}
