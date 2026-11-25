//! User-defined field values written back, on `model-mapping.xer`,
//! which holds a field for each kind of record. Only the changed `UDFVALUE`
//! rows differ.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::Path;

use totalfloat_model as m;
use totalfloat_xer::raw::{self, RawXer};
use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
use totalfloat_xer::{patch, to_model};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn fixture() -> RawXer {
    raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap()
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

const NUMBER: m::UdfTypeId = m::UdfTypeId(902);
const WBS_DATE: m::UdfTypeId = m::UdfTypeId(903);
const RESOURCE_COST: m::UdfTypeId = m::UdfTypeId(904);
const ASSIGNMENT_NOTE: m::UdfTypeId = m::UdfTypeId(905);

/// The edited schedule's bytes, and the lines removed and added.
fn patched(schedule: &m::Schedule) -> (Vec<u8>, usize, usize, usize) {
    let raw = fixture();
    let out = bytes(&patch(&raw, schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    let cells = if d.removed.len() == 1 && d.added.len() == 1 {
        changed_cells(&d.removed[0].1, &d.added[0].1).len()
    } else {
        0
    };
    (out, d.removed.len(), d.added.len(), cells)
}

/// Lines removed, lines added, and cells changed when one line changed.
fn counts(schedule: &m::Schedule) -> [usize; 3] {
    let (_, removed, added, cells) = patched(schedule);
    [removed, added, cells]
}

fn first() -> m::Schedule {
    to_model(&fixture()).0.swap_remove(0)
}

#[test]
fn a_changed_value_of_each_kind_of_record_changes_one_cell() {
    let mut s = first();
    s.activities
        .get_mut(&m::ActivityId(101))
        .unwrap()
        .udfs
        .insert(NUMBER, m::UdfValue::Number(7.5));
    assert_eq!(counts(&s), [1, 1, 1]);
    let mut s = first();
    s.wbs.get_mut(&m::WbsId(12)).unwrap().udfs.insert(
        WBS_DATE,
        m::UdfValue::Date("2026-02-02 08:00".parse().unwrap()),
    );
    assert_eq!(counts(&s), [1, 1, 1]);
    let mut s = first();
    s.resources
        .get_mut(&m::ResourceId(501))
        .unwrap()
        .udfs
        .insert(RESOURCE_COST, m::UdfValue::Number(99.95));
    let (out, ..) = patched(&s);
    assert_eq!(counts(&s), [1, 1, 1]);
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(
        back.resources[&m::ResourceId(501)].udfs[&RESOURCE_COST],
        m::UdfValue::Number(99.95)
    );
    let mut s = first();
    let assignment = s.assignments.values_mut().next().unwrap();
    assignment
        .udfs
        .insert(ASSIGNMENT_NOTE, m::UdfValue::Text("Night work".to_owned()));
    assert_eq!(counts(&s), [1, 1, 1]);
}

#[test]
fn a_cleared_value_removes_its_row_and_a_new_one_adds_one() {
    let mut s = first();
    s.activities
        .get_mut(&m::ActivityId(101))
        .unwrap()
        .udfs
        .clear();
    assert_eq!(counts(&s)[..2], [1, 0]);
    let mut s = first();
    s.activities
        .get_mut(&m::ActivityId(102))
        .unwrap()
        .udfs
        .insert(NUMBER, m::UdfValue::Number(3.0));
    let (out, removed, added, _) = patched(&s);
    assert_eq!((removed, added), (0, 1));
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(
        back.activities[&m::ActivityId(102)].udfs[&NUMBER],
        m::UdfValue::Number(3.0)
    );
}

#[test]
fn a_new_field_and_its_value_are_two_added_rows() {
    let mut s = first();
    let id = m::UdfTypeId(906);
    s.udf_types.insert(
        id,
        m::UdfType {
            id,
            subject: m::UdfSubject::Activity,
            label: "Commodity".to_owned(),
            data_type: m::UdfDataType::Indicator,
            source: None,
        },
    );
    s.activities
        .get_mut(&m::ActivityId(102))
        .unwrap()
        .udfs
        .insert(id, m::UdfValue::Text("UDF_G2".to_owned()));
    let (out, removed, added, _) = patched(&s);
    assert_eq!((removed, added), (0, 2));
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(back.udf_types[&id].label, "Commodity");
    assert_eq!(back.udf_types[&id].data_type, m::UdfDataType::Indicator);
    assert_eq!(
        back.activities[&m::ActivityId(102)].udfs[&id],
        m::UdfValue::Text("UDF_G2".to_owned())
    );
}

#[test]
fn changing_an_existing_field_is_refused() {
    let raw = fixture();
    let mut s = first();
    s.udf_types.get_mut(&NUMBER).unwrap().label = "Renamed".to_owned();
    assert!(patch(&raw, &s).is_err());
}

/// Sol round 1: a shared resource's cleared value stays cleared when every
/// project is written, though another project's copy still holds it.
#[test]
fn a_shared_resource_s_cleared_value_stays_cleared_across_projects() {
    // Give the second project an assignment of the first one's resource.
    let first = to_model(&fixture()).0;
    let mut second = first[1].clone();
    let resource = first[0].resources[&m::ResourceId(501)].clone();
    second.resources.insert(resource.id, resource);
    let assignment = first[0].assignments.values().next().unwrap().clone();
    let id = m::AssignmentId(9001);
    second.assignments.insert(
        id,
        m::Assignment {
            id,
            activity: *second.activities.keys().next().unwrap(),
            project: second.project.id,
            resource: Some(m::ResourceId(501)),
            udfs: std::collections::BTreeMap::new(),
            source: None,
            ..assignment
        },
    );
    let raw = patch(&fixture(), &second).unwrap();
    let base = to_model(&raw).0;
    let holders = base
        .iter()
        .filter(|s| {
            s.resources
                .get(&m::ResourceId(501))
                .is_some_and(|r| r.udfs.contains_key(&RESOURCE_COST))
        })
        .count();
    assert_eq!(holders, 2, "both projects hold the resource");
    let mut edited = base.clone();
    edited[0]
        .resources
        .get_mut(&m::ResourceId(501))
        .unwrap()
        .udfs
        .clear();
    let out = totalfloat_xer::patch_all(&raw, &base, &edited).unwrap();
    for schedule in to_model(&out).0 {
        let held = schedule
            .resources
            .get(&m::ResourceId(501))
            .map(|r| r.udfs.clone())
            .unwrap_or_default();
        assert!(held.is_empty(), "project {}: {held:?}", schedule.project.id);
    }
}
