//! Project codes in a synthetic file, `fixtures/xer/project-codes.xer`,
//! written fresh from `model-mapping.xer`'s first project with project codes
//! added. The file is read, edited and written back, and only what changed
//! differs.
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

const REGION: m::ProjectCodeTypeId = m::ProjectCodeTypeId(1);
const PHASE: m::ProjectCodeTypeId = m::ProjectCodeTypeId(2);

/// The model the fixture is written from.
fn authored() -> m::Schedule {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut schedule = to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule
        .relationships
        .retain(|_, r| r.predecessor_project == project && r.successor_project == project);
    for (id, name, max_length, sequence) in [(1, "Region", 20, 0), (2, "Phase", 10, 1)] {
        let id = m::ProjectCodeTypeId(id);
        schedule.project_code_types.insert(
            id,
            m::ProjectCodeType {
                id,
                name: name.to_owned(),
                max_length: Some(max_length),
                sequence,
                source: None,
            },
        );
    }
    for (id, kind, parent, value, description) in [
        (10, REGION, None, "WEST", "Western region"),
        (11, REGION, None, "EAST", "Eastern region"),
        (20, PHASE, None, "DESIGN", "Design"),
        (21, PHASE, Some(20), "DD", "Design development"),
    ] {
        let id = m::ProjectCodeId(id);
        schedule.project_codes.insert(
            id,
            m::ProjectCode {
                id,
                code_type: kind,
                parent: parent.map(m::ProjectCodeId),
                value: value.to_owned(),
                description: description.to_owned(),
                sequence: i64::try_from(id.0 % 10).unwrap(),
                source: None,
            },
        );
    }
    schedule.project.codes.insert(REGION, m::ProjectCodeId(10));
    schedule
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

fn fixture() -> RawXer {
    raw::read(std::fs::read(root().join("fixtures/xer/project-codes.xer")).unwrap()).unwrap()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md.
#[test]
fn the_fixture_is_the_authored_model_written_fresh() {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let written = bytes(&write(&authored(), options).unwrap());
    let path = root().join("fixtures/xer/project-codes.xer");
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == written,
        "fixtures/xer/project-codes.xer is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

#[test]
fn project_codes_are_read() {
    let schedule = to_model(&fixture()).0.swap_remove(0);
    assert_eq!(schedule.project_code_types[&REGION].name, "Region");
    assert_eq!(schedule.project_code_types[&PHASE].max_length, Some(10));
    let dd = &schedule.project_codes[&m::ProjectCodeId(21)];
    assert_eq!(
        (
            dd.code_type,
            dd.parent,
            dd.value.as_str(),
            dd.description.as_str()
        ),
        (
            PHASE,
            Some(m::ProjectCodeId(20)),
            "DD",
            "Design development"
        )
    );
    assert_eq!(
        schedule.project.codes.iter().collect::<Vec<_>>(),
        [(&REGION, &m::ProjectCodeId(10))]
    );
    assert_eq!(schedule.validate(), []);
}

#[test]
fn an_unchanged_schedule_writes_back_byte_for_byte() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
}

#[test]
fn changing_the_project_s_value_changes_one_cell() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    schedule.project.codes.insert(REGION, m::ProjectCodeId(11));
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 1);
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(back.project.codes[&REGION], m::ProjectCodeId(11));
}

#[test]
fn clearing_the_project_s_value_removes_its_row_only() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    schedule.project.codes.clear();
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 0), "{d:?}");
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert!(back.project.codes.is_empty());
}

#[test]
fn a_new_type_value_and_assignment_are_added_rows_only() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let kind = m::ProjectCodeTypeId(3);
    schedule.project_code_types.insert(
        kind,
        m::ProjectCodeType {
            id: kind,
            name: "Sponsor".to_owned(),
            max_length: Some(20),
            sequence: 2,
            source: None,
        },
    );
    let value = m::ProjectCodeId(30);
    schedule.project_codes.insert(
        value,
        m::ProjectCode {
            id: value,
            code_type: kind,
            parent: None,
            value: "CITY".to_owned(),
            description: "City council".to_owned(),
            sequence: 0,
            source: None,
        },
    );
    schedule.project.codes.insert(kind, value);
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert!(d.removed.is_empty(), "{d:?}");
    assert_eq!(d.added.len(), 3, "{d:?}");
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(back.project_code_types[&kind].name, "Sponsor");
    assert_eq!(back.project_codes[&value].description, "City council");
    assert_eq!(back.project.codes[&kind], value);
    assert_eq!(back.validate(), []);
}

#[test]
fn a_file_without_project_codes_gains_their_tables() {
    let raw =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    assert!(raw.table_index("PCATTYPE").is_none());
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let codes = authored();
    schedule.project_code_types = codes.project_code_types;
    schedule.project_codes = codes.project_codes;
    schedule.project.codes = codes.project.codes;
    let out = patch(&raw, &schedule).unwrap();
    let back = to_model(&out).0.swap_remove(0);
    assert_eq!(back.project.codes[&REGION], m::ProjectCodeId(10));
    assert_eq!(back.project_codes.len(), 4);
}

#[test]
fn changing_or_removing_a_type_or_value_is_refused() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    let mut renamed = schedule.clone();
    renamed.project_code_types.get_mut(&REGION).unwrap().name = "Area".to_owned();
    assert!(patch(&raw, &renamed).is_err());
    let mut removed = schedule;
    removed.project_codes.remove(&m::ProjectCodeId(21));
    assert!(patch(&raw, &removed).is_err());
}

#[test]
fn an_assignment_of_a_value_the_file_lacks_is_left_out_and_reported() {
    let mut raw = fixture();
    let t = raw.table_index("PROJPCAT").unwrap();
    let column = raw.tables()[t].field_index("proj_catg_id").unwrap();
    raw.set_cell(t, 0, column, "99").unwrap();
    let (schedules, diagnostics) = to_model(&raw);
    assert!(schedules[0].project.codes.is_empty());
    assert_eq!(diagnostics.omitted.len(), 1);
    assert_eq!(schedules[0].validate(), []);
}

#[test]
fn a_value_whose_parent_the_file_lacks_loses_its_parent_with_a_warning() {
    let mut raw = fixture();
    let t = raw.table_index("PCATVAL").unwrap();
    let table = &raw.tables()[t];
    let parent = table.field_index("parent_proj_catg_id").unwrap();
    let row = table
        .rows
        .iter()
        .position(|r| r.get(parent).is_some_and(|c| !c.is_empty()))
        .unwrap();
    raw.set_cell(t, row, parent, "999").unwrap();
    let (schedules, diagnostics) = to_model(&raw);
    assert_eq!(
        schedules[0].project_codes[&m::ProjectCodeId(21)].parent,
        None
    );
    assert_eq!(schedules[0].validate(), []);
    let cleared = diagnostics
        .details
        .iter()
        .filter(|d| d.field == "parent_proj_catg_id")
        .count();
    assert_eq!(cleared, 1, "{:?}", diagnostics.details);
}

#[test]
fn setting_a_code_whose_row_was_left_out_rewrites_that_row() {
    let mut raw = fixture();
    let t = raw.table_index("PROJPCAT").unwrap();
    let column = raw.tables()[t].field_index("proj_catg_id").unwrap();
    raw.set_cell(t, 0, column, "99").unwrap();
    let mut bytes_before = Vec::new();
    raw.write(&mut bytes_before).unwrap();
    let raw = raw::read(bytes_before).unwrap();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    assert!(schedule.project.codes.is_empty());
    schedule.project.codes.insert(REGION, m::ProjectCodeId(11));
    let out = patch(&raw, &schedule).unwrap();
    assert_eq!(out.tables()[t].rows.len(), 1);
    let back = to_model(&out).0.swap_remove(0);
    assert_eq!(back.project.codes[&REGION], m::ProjectCodeId(11));
}
