//! Roles in a synthetic file, `fixtures/xer/roles.xer`, written fresh
//! from `model-mapping.xer`'s first project with roles added. The file is
//! read, edited and written back, and only what changed differs.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::collections::BTreeMap;
use std::path::Path;

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};
use totalfloat_xer::raw::{self, RawXer};
use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
use totalfloat_xer::{patch, to_model};

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The model the fixture is written from.
fn authored() -> m::Schedule {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut schedule = to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule
        .relationships
        .retain(|_, r| r.predecessor_project == project && r.successor_project == project);
    let rate = |id, effective: &str, price| m::RoleRate {
        id: m::RoleRateId(id),
        effective: Some(effective.parse().unwrap()),
        prices: [Some(price), None, None, None, None],
        max_units_per_hour: Some(1.0),
        source: None,
    };
    for (id, code, name, parent, rates) in [
        (
            1,
            "SUPER",
            "Superintendent",
            None,
            vec![rate(1, "2026-01-01 00:00", 95.0)],
        ),
        (
            2,
            "FOREMAN",
            "Site foreman",
            Some(m::RoleId(1)),
            vec![
                rate(2, "2026-01-01 00:00", 70.0),
                rate(3, "2026-07-01 00:00", 74.5),
            ],
        ),
    ] {
        schedule.roles.insert(
            m::RoleId(id),
            m::Role {
                id: m::RoleId(id),
                short_name: code.to_owned(),
                name: name.to_owned(),
                parent,
                rates,
                default_cost_follows_units: true,
                source: None,
            },
        );
    }
    let resource = schedule.resources.values_mut().next().unwrap();
    resource.roles.push(m::ResourceRole {
        id: m::ResourceRoleId(1),
        role: m::RoleId(2),
        skill_level: Some(3),
        source: None,
    });
    let first = schedule.assignments.values_mut().next().unwrap();
    first.role = Some(m::RoleId(2));
    // An assignment by role alone beside it: more work no one is named for yet.
    let open = first.activity;
    let id = m::AssignmentId(schedule.assignments.keys().last().unwrap().0 + 1);
    schedule.assignments.insert(
        id,
        m::Assignment {
            id,
            activity: open,
            resource: None,
            role: Some(m::RoleId(1)),
            cost_account: None,
            project,
            curve: None,
            typed_spread: None,
            amounts: m::Amounts {
                budgeted_units: Some(16.0),
                remaining_units: Some(16.0),
                price_per_unit: Some(95.0),
                price_source: Some(m::PriceSource::Role),
                ..m::Amounts::default()
            },
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    schedule
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

fn fixture() -> RawXer {
    raw::read(std::fs::read(root().join("fixtures/xer/roles.xer")).unwrap()).unwrap()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md.
#[test]
fn the_fixture_is_the_authored_model_written_fresh() {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let written = bytes(&write(&authored(), options).unwrap());
    let path = root().join("fixtures/xer/roles.xer");
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == written,
        "fixtures/xer/roles.xer is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

#[test]
fn roles_and_an_assignment_by_role_alone_are_read() {
    let schedule = to_model(&fixture()).0.swap_remove(0);
    let foreman = &schedule.roles[&m::RoleId(2)];
    assert_eq!(foreman.short_name, "FOREMAN");
    assert_eq!(foreman.parent, Some(m::RoleId(1)));
    assert_eq!(foreman.rates.len(), 2);
    assert_eq!(
        foreman
            .rate_at("2026-08-01 08:00".parse().ok())
            .and_then(|r| r.prices[0]),
        Some(74.5)
    );
    let filled: Vec<_> = schedule
        .resources
        .values()
        .flat_map(|r| &r.roles)
        .map(|f| (f.role, f.skill_level))
        .collect();
    assert_eq!(filled, [(m::RoleId(2), Some(3))]);
    let alone: Vec<_> = schedule
        .assignments
        .values()
        .filter(|a| a.resource.is_none())
        .collect();
    assert_eq!(alone.len(), 1);
    assert_eq!(alone[0].role, Some(m::RoleId(1)));
    assert_eq!(alone[0].amounts.budgeted_units, Some(16.0));
    assert!(
        schedule
            .assignments
            .values()
            .any(|a| a.resource.is_some() && a.role == Some(m::RoleId(2)))
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
fn changing_a_role_assignment_s_units_changes_only_those_cells() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let alone = schedule
        .assignments
        .values_mut()
        .find(|a| a.resource.is_none())
        .unwrap();
    alone.amounts.budgeted_units = Some(24.0);
    alone.amounts.remaining_units = Some(24.0);
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    // The assignment's row, and the activity's labor totals, which a role
    // alone doesn't count in, so only the assignment's row.
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 2);
}

#[test]
fn a_new_role_and_its_assignment_are_added_rows_and_read_back() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let role = m::RoleId(3);
    schedule.roles.insert(
        role,
        m::Role {
            id: role,
            short_name: "RIGGER".to_owned(),
            name: "Rigger".to_owned(),
            parent: Some(m::RoleId(1)),
            rates: Vec::new(),
            default_cost_follows_units: true,
            source: None,
        },
    );
    let activity = *schedule.activities.keys().next().unwrap();
    let id = m::AssignmentId(schedule.assignments.keys().last().unwrap().0 + 1);
    schedule.assignments.insert(
        id,
        m::Assignment {
            id,
            activity,
            resource: None,
            role: Some(role),
            cost_account: None,
            project: schedule.project.id,
            curve: None,
            typed_spread: None,
            amounts: m::Amounts::default(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert!(d.removed.is_empty(), "{d:?}");
    assert_eq!(d.added.len(), 2, "{d:?}");
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(back.roles[&role].short_name, "RIGGER");
    assert_eq!(back.roles[&role].parent, Some(m::RoleId(1)));
    assert_eq!(
        (back.assignments[&id].resource, back.assignments[&id].role),
        (None, Some(role))
    );
}

#[test]
fn changing_an_assignment_s_role_or_a_role_is_refused() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    let mut moved = schedule.clone();
    let alone = moved
        .assignments
        .values_mut()
        .find(|a| a.resource.is_none())
        .unwrap();
    alone.role = Some(m::RoleId(2));
    assert!(patch(&raw, &moved).is_err());
    let mut renamed = schedule.clone();
    renamed.roles.get_mut(&m::RoleId(1)).unwrap().name = "Super".to_owned();
    assert!(patch(&raw, &renamed).is_err());
    let mut removed = schedule;
    removed.roles.remove(&m::RoleId(2));
    assert!(patch(&raw, &removed).is_err());
}

/// The fixture with the assignment by role alone given another `role_id`.
fn alone_with_role(value: &str) -> (RawXer, u64) {
    let mut raw = fixture();
    let t = raw.table_index("TASKRSRC").unwrap();
    let table = &raw.tables()[t];
    let (rsrc, role, id) = (
        table.field_index("rsrc_id").unwrap(),
        table.field_index("role_id").unwrap(),
        table.field_index("taskrsrc_id").unwrap(),
    );
    let (row, cells) = table
        .rows
        .iter()
        .enumerate()
        .find(|(_, r)| r.get(rsrc) == Some(""))
        .unwrap();
    let assignment = cells.get(id).unwrap().parse().unwrap();
    raw.set_cell(t, row, role, value).unwrap();
    (raw, assignment)
}

#[test]
fn an_assignment_with_neither_resource_nor_role_is_left_out_and_reported() {
    let (raw, id) = alone_with_role("");
    let (schedules, diagnostics) = to_model(&raw);
    assert!(!schedules[0].assignments.contains_key(&m::AssignmentId(id)));
    assert_eq!(diagnostics.skipped_fields.get("rsrc_id"), Some(&1));
}

#[test]
fn an_assignment_by_a_role_the_file_lacks_is_left_out_and_reported() {
    let (raw, id) = alone_with_role("99");
    let (schedules, diagnostics) = to_model(&raw);
    assert!(!schedules[0].assignments.contains_key(&m::AssignmentId(id)));
    assert_eq!(diagnostics.omitted.len(), 1);
    assert_eq!(schedules[0].validate(), []);
}

#[test]
fn a_role_whose_parent_the_file_lacks_loses_its_parent_with_a_warning() {
    let mut raw = fixture();
    let t = raw.table_index("ROLES").unwrap();
    let table = &raw.tables()[t];
    let (parent, id) = (
        table.field_index("parent_role_id").unwrap(),
        table.field_index("role_id").unwrap(),
    );
    let row = table
        .rows
        .iter()
        .position(|r| r.get(parent).is_some_and(|c| !c.is_empty()))
        .unwrap();
    let role = m::RoleId(table.rows[row].get(id).unwrap().parse().unwrap());
    raw.set_cell(t, row, parent, "999").unwrap();
    let (schedules, diagnostics) = to_model(&raw);
    assert_eq!(schedules[0].roles[&role].parent, None);
    assert_eq!(schedules[0].validate(), []);
    let cleared = diagnostics
        .details
        .iter()
        .filter(|d| d.field == "parent_role_id")
        .count();
    assert_eq!(cleared, 1, "{:?}", diagnostics.details);
}

#[test]
fn a_resource_s_assignment_keeps_its_resource_when_its_role_is_missing() {
    let mut raw = fixture();
    let t = raw.table_index("TASKRSRC").unwrap();
    let table = &raw.tables()[t];
    let (rsrc, role) = (
        table.field_index("rsrc_id").unwrap(),
        table.field_index("role_id").unwrap(),
    );
    let row = table
        .rows
        .iter()
        .position(|r| r.get(rsrc).is_some_and(|c| !c.is_empty()))
        .unwrap();
    raw.set_cell(t, row, role, "99").unwrap();
    let schedule = to_model(&raw).0.swap_remove(0);
    let held = schedule
        .assignments
        .values()
        .find(|a| a.resource.is_some())
        .unwrap();
    assert_eq!(held.role, None);
    assert_eq!(schedule.validate(), []);
}
