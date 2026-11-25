//! Hand-typed assignment spreads (`TASKRSRC.target_crv`, `remain_crv`
//! and `actual_crv`, marked by a `curv_id` the file doesn't hold) in a
//! synthetic file, `fixtures/xer/spreads.xer`, written fresh from
//! `model-mapping.xer`'s first project with a typed spread added. The file is
//! read, edited and written back, and only what changed differs.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::Path;

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};
use totalfloat_xer::mapping::DiagnosticKind;
use totalfloat_xer::raw::{self, RawXer};
use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
use totalfloat_xer::{patch, to_model};

const FIXTURE: &str = "fixtures/xer/spreads.xer";

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Stretches of `units` over `hours` of work.
fn stretches(list: &[(f64, i64)]) -> Vec<m::SpreadSegment> {
    list.iter()
        .map(|&(units, hours)| m::SpreadSegment {
            units,
            work: m::Minutes::new(hours * 60),
        })
        .collect()
}

/// The typed spread: 6 units done in the first 4 hours, and 40 left over the
/// remaining week of 8-hour days (Monday 12, Tuesday 4, Wednesday nothing,
/// then 24 over Thursday and Friday). The planned spread is both.
fn typed() -> m::TypedSpread {
    m::TypedSpread {
        planned: stretches(&[(6.0, 4), (12.0, 8), (4.0, 8), (0.0, 8), (24.0, 16)]),
        remaining: stretches(&[(12.0, 8), (4.0, 8), (0.0, 8), (24.0, 16)]),
        actual: stretches(&[(6.0, 4)]),
    }
}

/// The model the fixture is written from: the project's assignment, on its
/// in-progress activity, with the typed spread and units to match.
fn authored() -> m::Schedule {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut schedule = to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule
        .relationships
        .retain(|_, r| r.predecessor_project == project && r.successor_project == project);
    let assignment = first(&mut schedule);
    assignment.amounts.budgeted_units = Some(46.0);
    assignment.amounts.actual_units = Some(6.0);
    assignment.amounts.remaining_units = Some(40.0);
    assignment.typed_spread = Some(typed());
    schedule
}

fn first(schedule: &mut m::Schedule) -> &mut m::Assignment {
    schedule.assignments.values_mut().next().unwrap()
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

fn fixture() -> RawXer {
    raw::read(std::fs::read(root().join(FIXTURE)).unwrap()).unwrap()
}

/// The fixture with one `TASKRSRC` cell of its assignment replaced.
fn with_cell(field: &str, value: &str) -> RawXer {
    let mut raw = fixture();
    let t = raw.table_index("TASKRSRC").unwrap();
    let column = raw.tables()[t].field_index(field).unwrap();
    raw.set_cell(t, 0, column, value).unwrap();
    raw::read(bytes(&raw)).unwrap()
}

fn cell(raw: &RawXer, field: &str) -> String {
    let t = raw.table_index("TASKRSRC").unwrap();
    let table = &raw.tables()[t];
    table.rows[0]
        .get(table.field_index(field).unwrap())
        .unwrap()
        .to_owned()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md.
#[test]
fn the_fixture_is_the_authored_model_written_fresh() {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let written = bytes(&write(&authored(), options).unwrap());
    let path = root().join(FIXTURE);
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == written,
        "{FIXTURE} is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

/// The fresh writer marks the spread with curve 9, as P6's files do, and
/// writes each part in P6's spelling.
#[test]
fn the_spread_is_written_with_the_marker_p6_files_use() {
    let raw = fixture();
    assert_eq!(cell(&raw, "curv_id"), "9");
    assert_eq!(cell(&raw, "target_crv"), "6:4;12:8;4:8;0:8;24:16");
    assert_eq!(cell(&raw, "remain_crv"), "12:8;4:8;0:8;24:16");
    assert_eq!(cell(&raw, "actual_crv"), "6:4");
    assert!(raw.table_index("RSRCCURVDATA").is_none());
}

#[test]
fn the_typed_spread_is_read_with_no_warning() {
    let (mut schedules, diagnostics) = to_model(&fixture());
    let schedule = schedules.swap_remove(0);
    let assignment = schedule.assignments.values().next().unwrap();
    assert_eq!(assignment.typed_spread, Some(typed()));
    assert_eq!(assignment.curve, None);
    assert!(
        diagnostics
            .details
            .iter()
            .all(|d| !d.field.contains("crv") && d.field != "curv_id"),
        "{:?}",
        diagnostics.details
    );
    assert_eq!(schedule.validate(), []);
}

#[test]
fn an_unchanged_schedule_writes_back_byte_for_byte() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
}

/// Changing one period's units changes only that part's cell, and
/// `curv_id` keeps its value.
#[test]
fn changing_one_period_changes_only_its_cell() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let spread = first(&mut schedule).typed_spread.as_mut().unwrap();
    spread.remaining[1].units = 5.5;
    let out = patch(&raw, &schedule).unwrap();
    let d = diff_lines(raw.source(), &bytes(&out));
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 1);
    assert_eq!(cell(&out, "remain_crv"), "12:8;5.5:8;0:8;24:16");
    assert_eq!(cell(&out, "curv_id"), "9");
    let read = to_model(&out).0.swap_remove(0);
    assert_eq!(
        read.assignments.values().next().unwrap().typed_spread,
        first(&mut schedule).typed_spread
    );
}

/// A marker other than 9 is kept when the spread changes.
#[test]
fn another_missing_curve_as_the_marker_is_kept() {
    let raw = with_cell("curv_id", "7");
    let mut schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(first(&mut schedule).typed_spread, Some(typed()));
    first(&mut schedule).typed_spread.as_mut().unwrap().actual[0].units = 7.0;
    let out = patch(&raw, &schedule).unwrap();
    assert_eq!(cell(&out, "curv_id"), "7");
    assert_eq!(cell(&out, "actual_crv"), "7:4");
}

/// Going back to an even spread, or to a curve, clears the marker and every
/// stored spread; typing the spread again writes them back as they were.
#[test]
fn clearing_the_typed_spread_clears_its_cells() {
    let raw = fixture();
    let typed_schedule = to_model(&raw).0.swap_remove(0);
    let mut even = typed_schedule.clone();
    first(&mut even).typed_spread = None;
    let out = patch(&raw, &even).unwrap();
    let d = diff_lines(raw.source(), &bytes(&out));
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 4);
    for field in ["curv_id", "target_crv", "remain_crv", "actual_crv"] {
        assert_eq!(cell(&out, field), "", "{field}");
    }
    let read = to_model(&out).0.swap_remove(0);
    let assignment = read.assignments.values().next().unwrap();
    assert_eq!((assignment.curve, &assignment.typed_spread), (None, &None));
    // Typed again, the file is the fixture again.
    let out = raw::read(bytes(&out)).unwrap();
    assert_eq!(
        bytes(&patch(&out, &typed_schedule).unwrap()),
        raw.source(),
        "typing the spread again restores the cells"
    );
}

/// Units that don't add up to the assignment's are kept and reported; the
/// spread is still read.
#[test]
fn units_that_differ_from_the_assignment_s_are_reported() {
    let raw = with_cell("remain_crv", "50:20;50:20");
    let (mut schedules, diagnostics) = to_model(&raw);
    let assignment = first(&mut schedules[0]).clone();
    assert_eq!(
        assignment.typed_spread.unwrap().remaining,
        stretches(&[(50.0, 20), (50.0, 20)])
    );
    let found: Vec<_> = diagnostics
        .details
        .iter()
        .filter(|d| d.kind == DiagnosticKind::InconsistentValues)
        .map(|d| d.field.as_str())
        .collect();
    assert_eq!(found, ["remain_crv"]);
    assert_eq!(bytes(&patch(&raw, &schedules[0]).unwrap()), raw.source());
}

/// A spread that can't be read leaves the assignment as if it held no typed spread: its
/// curve cleared with a warning, its cells as they are, and its spread not
/// open to change.
#[test]
fn an_unreadable_spread_is_reported_and_kept_raw() {
    let raw = with_cell("remain_crv", "12:8;4:x");
    let (mut schedules, diagnostics) = to_model(&raw);
    let schedule = schedules.swap_remove(0);
    let assignment = schedule.assignments.values().next().unwrap();
    assert_eq!((assignment.curve, &assignment.typed_spread), (None, &None));
    let warned: Vec<_> = diagnostics
        .details
        .iter()
        .filter(|d| d.kind == DiagnosticKind::UnrepresentedValue)
        .map(|d| d.field.as_str())
        .collect();
    assert_eq!(warned, ["remain_crv", "curv_id"]);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
    let mut retyped = schedule;
    first(&mut retyped).typed_spread = Some(typed());
    assert!(patch(&raw, &retyped).is_err());
}

/// The authored model beside a linear curve, curve 1, written fresh.
fn with_a_curve() -> RawXer {
    let mut schedule = authored();
    let mut points = [5.0; m::CURVE_POINTS];
    points[0] = 0.0;
    schedule.resource_curves.insert(
        m::ResourceCurveId(1),
        m::ResourceCurve {
            id: m::ResourceCurveId(1),
            name: "Linear".to_owned(),
            default: true,
            points,
            source: None,
        },
    );
    let written = write(&schedule, FreshOptions::new("2026-01-01".parse().unwrap())).unwrap();
    raw::read(bytes(&written)).unwrap()
}

/// Moving a typed assignment onto a curve the file holds clears its stored
/// spreads; it can't have both.
#[test]
fn a_typed_spread_can_give_way_to_a_held_curve_but_not_sit_beside_it() {
    let raw = with_a_curve();
    let schedule = to_model(&raw).0.swap_remove(0);
    assert!(first(&mut schedule.clone()).typed_spread.is_some());
    let mut both = schedule.clone();
    first(&mut both).curve = Some(m::ResourceCurveId(1));
    assert!(patch(&raw, &both).is_err());
    let mut curved = both;
    first(&mut curved).typed_spread = None;
    let out = patch(&raw, &curved).unwrap();
    assert_eq!(cell(&out, "curv_id"), "1");
    for field in ["target_crv", "remain_crv", "actual_crv"] {
        assert_eq!(cell(&out, field), "", "{field}");
    }
    let read = to_model(&out).0.swap_remove(0);
    let assignment = read.assignments.values().next().unwrap();
    assert_eq!(
        (assignment.curve, &assignment.typed_spread),
        (Some(m::ResourceCurveId(1)), &None)
    );
}

/// An assignment on a curve the file holds isn't read as typed, even with
/// a stored spread beside it.
#[test]
fn a_stored_spread_beside_a_held_curve_is_not_read() {
    let mut raw = with_a_curve();
    let t = raw.table_index("TASKRSRC").unwrap();
    let column = raw.tables()[t].field_index("curv_id").unwrap();
    raw.set_cell(t, 0, column, "1").unwrap();
    let raw = raw::read(bytes(&raw)).unwrap();
    let read = to_model(&raw).0.swap_remove(0);
    let assignment = read.assignments.values().next().unwrap();
    assert_eq!(assignment.curve, Some(m::ResourceCurveId(1)));
    assert_eq!(assignment.typed_spread, None);
    assert_eq!(bytes(&patch(&raw, &read).unwrap()), raw.source());
}

/// The writers refuse a spread they can't mark or spell.
#[test]
fn spreads_the_writers_cannot_write_are_refused() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    let mut negative = schedule;
    first(&mut negative)
        .typed_spread
        .as_mut()
        .unwrap()
        .remaining[0]
        .units = -1.0;
    assert!(patch(&raw, &negative).is_err());
    let mut empty = negative;
    first(&mut empty).typed_spread = Some(m::TypedSpread::default());
    assert!(patch(&raw, &empty).is_err());
    let mut marker_held = authored();
    marker_held.resource_curves.insert(
        m::ResourceCurveId(9),
        m::ResourceCurve {
            id: m::ResourceCurveId(9),
            name: "Nine".to_owned(),
            default: false,
            points: [5.0; m::CURVE_POINTS],
            source: None,
        },
    );
    assert!(
        write(
            &marker_held,
            FreshOptions::new("2026-01-01".parse().unwrap())
        )
        .is_err()
    );
}

/// In a file that holds a real curve 9, a spread marked by another missing
/// curve still reads, writes back and changes; only a newly typed spread,
/// which would be marked 9, is refused.
#[test]
fn a_file_holding_curve_9_keeps_its_own_marker() {
    let mut raw = with_a_curve();
    let curves = raw.table_index("RSRCCURVDATA").unwrap();
    let id = raw.tables()[curves].field_index("curv_id").unwrap();
    raw.set_cell(curves, 0, id, "9").unwrap();
    let t = raw.table_index("TASKRSRC").unwrap();
    let column = raw.tables()[t].field_index("curv_id").unwrap();
    raw.set_cell(t, 0, column, "7").unwrap();
    let raw = raw::read(bytes(&raw)).unwrap();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    assert!(
        schedule
            .resource_curves
            .contains_key(&m::ResourceCurveId(9))
    );
    assert_eq!(first(&mut schedule).typed_spread, Some(typed()));
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
    first(&mut schedule)
        .typed_spread
        .as_mut()
        .unwrap()
        .remaining[0]
        .units = 13.0;
    let out = patch(&raw, &schedule).unwrap();
    assert_eq!(cell(&out, "curv_id"), "7");
    // Moving to the real curve 9 writes it, though the marker spells the
    // same in the model.
    let mut on_nine = schedule.clone();
    first(&mut on_nine).typed_spread = None;
    first(&mut on_nine).curve = Some(m::ResourceCurveId(9));
    let out = patch(&raw, &on_nine).unwrap();
    assert_eq!(cell(&out, "curv_id"), "9");
    let read = to_model(&out).0.swap_remove(0);
    assert_eq!(
        read.assignments.values().next().unwrap().curve,
        Some(m::ResourceCurveId(9))
    );
    first(&mut schedule).typed_spread = None;
    let even = raw::read(bytes(&patch(&raw, &schedule).unwrap())).unwrap();
    let mut retyped = to_model(&even).0.swap_remove(0);
    first(&mut retyped).typed_spread = Some(typed());
    assert!(patch(&even, &retyped).is_err());
    // Also when the file's curve 9 can't be read.
    let mut unread = even.clone();
    let point = unread.tables()[curves].field_index("pct_usage_7").unwrap();
    unread.set_cell(curves, 0, point, "").unwrap();
    let unread = raw::read(bytes(&unread)).unwrap();
    let mut retyped = to_model(&unread).0.swap_remove(0);
    assert!(retyped.resource_curves.is_empty());
    first(&mut retyped).typed_spread = Some(typed());
    assert!(patch(&unread, &retyped).is_err());
}

/// An assignment that isn't typed can still hold old spreads, as 80 in the
/// corpus do. Typing it replaces all of them, including the parts it leaves
/// empty.
#[test]
fn typing_a_spread_replaces_old_cells_without_the_marker() {
    let raw = with_cell("curv_id", "");
    let mut schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(first(&mut schedule).typed_spread, None);
    let only_planned = m::TypedSpread {
        planned: stretches(&[(46.0, 44)]),
        ..m::TypedSpread::default()
    };
    first(&mut schedule).typed_spread = Some(only_planned.clone());
    let out = patch(&raw, &schedule).unwrap();
    assert_eq!(cell(&out, "curv_id"), "9");
    assert_eq!(cell(&out, "remain_crv"), "");
    assert_eq!(cell(&out, "actual_crv"), "");
    let read = to_model(&out).0.swap_remove(0);
    assert_eq!(
        read.assignments.values().next().unwrap().typed_spread,
        Some(only_planned)
    );
}

/// A curve the file lists but couldn't read isn't the marker: the
/// assignment keeps the old warning, not a typed spread.
#[test]
fn an_unreadable_curve_is_not_the_marker() {
    let mut raw = with_a_curve();
    let curves = raw.table_index("RSRCCURVDATA").unwrap();
    let point = raw.tables()[curves].field_index("pct_usage_7").unwrap();
    raw.set_cell(curves, 0, point, "").unwrap();
    let t = raw.table_index("TASKRSRC").unwrap();
    let column = raw.tables()[t].field_index("curv_id").unwrap();
    raw.set_cell(t, 0, column, "1").unwrap();
    let raw = raw::read(bytes(&raw)).unwrap();
    let (mut schedules, diagnostics) = to_model(&raw);
    let assignment = first(&mut schedules[0]).clone();
    assert_eq!((assignment.curve, assignment.typed_spread), (None, None));
    assert!(
        diagnostics
            .details
            .iter()
            .any(|d| d.kind == DiagnosticKind::UnrepresentedValue && d.field == "curv_id")
    );
}
