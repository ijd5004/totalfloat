//! The OBS and each WBS node's responsible manager in a synthetic file,
//! `fixtures/xer/obs.xer`, written fresh from `model-mapping.xer`'s first
//! project with an OBS added. The file is read, edited and written back, and
//! only what changed differs.
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

const OWNER: m::ObsId = m::ObsId(1);
const CONTRACTOR: m::ObsId = m::ObsId(2);
const DESIGN: m::ObsId = m::ObsId(3);
const ROOT: m::WbsId = m::WbsId(11);
const PACKAGE: m::WbsId = m::WbsId(12);

/// The model the fixture is written from.
fn authored() -> m::Schedule {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut schedule = to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule
        .relationships
        .retain(|_, r| r.predecessor_project == project && r.successor_project == project);
    for (id, parent, name, description, sequence) in [
        (OWNER, None, "Owner", "<p>The owner's team</p>", 0),
        (CONTRACTOR, Some(OWNER), "Contractor", "", 0),
        (DESIGN, Some(OWNER), "Design", "", 1),
    ] {
        schedule.obs.insert(
            id,
            m::Obs {
                id,
                parent,
                name: name.to_owned(),
                description: description.to_owned(),
                sequence,
                source: None,
            },
        );
    }
    schedule.wbs.get_mut(&ROOT).unwrap().obs = Some(OWNER);
    schedule.wbs.get_mut(&PACKAGE).unwrap().obs = Some(CONTRACTOR);
    schedule
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

fn fixture() -> RawXer {
    raw::read(std::fs::read(root().join("fixtures/xer/obs.xer")).unwrap()).unwrap()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md.
#[test]
fn the_fixture_is_the_authored_model_written_fresh() {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let written = bytes(&write(&authored(), options).unwrap());
    let path = root().join("fixtures/xer/obs.xer");
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == written,
        "fixtures/xer/obs.xer is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

#[test]
fn the_obs_and_each_node_s_manager_are_read() {
    let schedule = to_model(&fixture()).0.swap_remove(0);
    assert_eq!(schedule.obs.len(), 3);
    let contractor = &schedule.obs[&CONTRACTOR];
    assert_eq!(
        (contractor.parent, contractor.name.as_str()),
        (Some(OWNER), "Contractor")
    );
    assert_eq!(schedule.obs[&OWNER].description, "<p>The owner's team</p>");
    assert_eq!(schedule.wbs[&ROOT].obs, Some(OWNER));
    assert_eq!(schedule.wbs[&PACKAGE].obs, Some(CONTRACTOR));
    assert_eq!(schedule.validate(), []);
}

#[test]
fn a_model_without_an_obs_is_written_with_one_owning_every_node() {
    let input =
        raw::read(std::fs::read(root().join("fixtures/xer/model-mapping.xer")).unwrap()).unwrap();
    let mut schedule = to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule
        .relationships
        .retain(|_, r| r.predecessor_project == project && r.successor_project == project);
    assert!(schedule.obs.is_empty());
    let out = write(&schedule, FreshOptions::new("2026-01-01".parse().unwrap())).unwrap();
    let back = to_model(&out).0.swap_remove(0);
    assert_eq!(back.obs.len(), 1);
    assert!(back.wbs.values().all(|node| node.obs == Some(m::ObsId(1))));
}

#[test]
fn an_unchanged_schedule_writes_back_byte_for_byte() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
}

#[test]
fn a_new_manager_changes_one_cell() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    schedule.wbs.get_mut(&PACKAGE).unwrap().obs = Some(DESIGN);
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 1);
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(back.wbs[&PACKAGE].obs, Some(DESIGN));
}

#[test]
fn a_new_node_is_one_added_row_with_a_guid() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let id = m::ObsId(4);
    schedule.obs.insert(
        id,
        m::Obs {
            id,
            parent: Some(CONTRACTOR),
            name: "Electrical".to_owned(),
            description: String::new(),
            sequence: 0,
            source: None,
        },
    );
    schedule.wbs.get_mut(&PACKAGE).unwrap().obs = Some(id);
    let out = patch(&raw, &schedule).unwrap();
    let d = diff_lines(raw.source(), &bytes(&out));
    assert_eq!((d.removed.len(), d.added.len()), (1, 2), "{d:?}");
    let t = out.table_index("OBS").unwrap();
    let guid = out.tables()[t].field_index("guid").unwrap();
    assert!(
        out.tables()[t]
            .rows
            .last()
            .unwrap()
            .get(guid)
            .is_some_and(|g| !g.is_empty())
    );
    let back = to_model(&out).0.swap_remove(0);
    assert_eq!(back.obs[&id].parent, Some(CONTRACTOR));
    assert_eq!(back.wbs[&PACKAGE].obs, Some(id));
    assert_eq!(back.validate(), []);
}

#[test]
fn a_new_wbs_node_keeps_the_manager_the_model_gives_it() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let mut node = schedule.wbs[&PACKAGE].clone();
    node.id = m::WbsId(13);
    node.code = "WP2".to_owned();
    node.sequence = 2;
    node.obs = Some(DESIGN);
    node.udfs.clear();
    node.source = None;
    schedule.wbs.insert(node.id, node);
    let out = patch(&raw, &schedule).unwrap();
    let back = to_model(&out).0.swap_remove(0);
    assert_eq!(back.wbs[&m::WbsId(13)].obs, Some(DESIGN));
}

#[test]
fn changing_or_removing_a_node_is_refused() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    let mut renamed = schedule.clone();
    renamed.obs.get_mut(&DESIGN).unwrap().name = "Designers".to_owned();
    assert!(patch(&raw, &renamed).is_err());
    let mut removed = schedule;
    removed.obs.remove(&DESIGN);
    assert!(patch(&raw, &removed).is_err());
}

#[test]
fn a_manager_the_file_lacks_is_cleared_with_a_warning() {
    let mut raw = fixture();
    let t = raw.table_index("PROJWBS").unwrap();
    let column = raw.tables()[t].field_index("obs_id").unwrap();
    raw.set_cell(t, 0, column, "99").unwrap();
    let (schedules, diagnostics) = to_model(&raw);
    assert!(schedules[0].wbs.values().any(|node| node.obs.is_none()));
    assert_eq!(schedules[0].validate(), []);
    let cleared = diagnostics
        .details
        .iter()
        .filter(|d| d.field == "obs_id")
        .count();
    assert_eq!(cleared, 1, "{:?}", diagnostics.details);
}

#[test]
fn a_wbs_node_with_no_manager_is_written_fresh_with_the_top_node() {
    let mut schedule = authored();
    schedule.wbs.get_mut(&PACKAGE).unwrap().obs = None;
    let out = write(&schedule, FreshOptions::new("2026-01-01".parse().unwrap())).unwrap();
    let back = to_model(&out).0.swap_remove(0);
    assert_eq!(back.wbs[&PACKAGE].obs, Some(OWNER));
}
