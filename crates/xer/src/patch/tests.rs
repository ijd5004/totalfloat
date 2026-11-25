use super::*;
use crate::raw;
use crate::rawdiff::{changed_cells, diff_lines};

fn fixture() -> RawXer {
    raw::read(include_bytes!("../../../../fixtures/xer/model-mapping.xer").as_slice()).unwrap()
}

fn bytes(x: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    x.write(&mut out).unwrap();
    out
}

/// The fixture's project-1 schedule.
fn project_one(raw: &RawXer) -> Schedule {
    to_model(raw)
        .0
        .into_iter()
        .find(|s| s.project.id == m::ProjectId(1))
        .unwrap()
}

#[test]
fn unmodified_schedule_writes_back_byte_identical() {
    let raw = fixture();
    for schedule in to_model(&raw).0 {
        let out = patch(&raw, &schedule).unwrap();
        assert!(!out.is_edited());
        assert_eq!(bytes(&out), raw.source());
    }
}

#[test]
fn changing_original_duration_changes_exactly_one_cell() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.activities
        .get_mut(&m::ActivityId(101))
        .unwrap()
        .original_duration = m::Minutes::new(16 * 60);
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    assert_eq!(d.removed.len(), 1, "{d:?}");
    assert_eq!(d.added.len(), 1, "{d:?}");
    assert_eq!(d.removed[0].0, d.added[0].0);
    let cells = changed_cells(&d.removed[0].1, &d.added[0].1);
    assert_eq!(cells.len(), 1);
    let task = raw.table("TASK").unwrap();
    // +1: cell 0 of the line is the "%R" marker.
    assert_eq!(
        cells[0],
        task.field_index("target_drtn_hr_cnt").unwrap() + 1
    );
    assert_eq!(d.added[0].1.split('\t').nth(cells[0]), Some("16"));
}

/// A fixture whose assignment row has every units and cost column.
fn with_assignments() -> RawXer {
    raw::read(include_bytes!("../../../../fixtures/xer/fresh-writer.xer").as_slice()).unwrap()
}

/// The first assignment of the project that has one.
fn assignment_of(s: &mut Schedule) -> &mut m::Assignment {
    s.assignments.values_mut().next().unwrap()
}

fn project_two(raw: &RawXer) -> Schedule {
    let mut schedules = to_model(raw).0;
    let at = schedules
        .iter()
        .position(|s| !s.assignments.is_empty())
        .unwrap();
    schedules.swap_remove(at)
}

#[test]
fn changing_remaining_units_rewrites_that_cell_and_the_activity_s_total() {
    let raw = with_assignments();
    let mut s = project_two(&raw);
    assignment_of(&mut s).amounts.remaining_units = Some(12.5);
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    // The assignment's row, and its activity's labor total, which moves with it.
    assert_eq!((d.removed.len(), d.added.len()), (2, 2), "{d:?}");
    let column =
        |table: &str, field: &str| raw.table(table).unwrap().field_index(field).unwrap() + 1;
    let (task, assignment) = (&d.added[0].1, &d.added[1].1);
    assert_eq!(
        changed_cells(&d.removed[0].1, task),
        [column("TASK", "remain_work_qty")]
    );
    assert_eq!(
        task.split('\t').nth(column("TASK", "remain_work_qty")),
        Some("12.5")
    );
    let cells = changed_cells(&d.removed[1].1, assignment);
    assert_eq!(cells, [column("TASKRSRC", "remain_qty")]);
    assert_eq!(assignment.split('\t').nth(cells[0]), Some("12.5"));
    let back = project_two(&raw::read(new).unwrap());
    let held = back.assignments.values().next().unwrap();
    assert_eq!(held.amounts.remaining_units, Some(12.5));
}

#[test]
fn changing_actual_units_rewrites_that_cell_and_the_activity_s_actual_total() {
    let raw = with_assignments();
    let mut s = project_two(&raw);
    assignment_of(&mut s).amounts.actual_units = Some(7.5);
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    assert_eq!((d.removed.len(), d.added.len()), (2, 2), "{d:?}");
    let column =
        |table: &str, field: &str| raw.table(table).unwrap().field_index(field).unwrap() + 1;
    let (task, assignment) = (&d.added[0].1, &d.added[1].1);
    assert_eq!(
        changed_cells(&d.removed[0].1, task),
        [column("TASK", "act_work_qty")]
    );
    assert_eq!(
        task.split('\t').nth(column("TASK", "act_work_qty")),
        Some("7.5")
    );
    let cells = changed_cells(&d.removed[1].1, assignment);
    assert_eq!(cells, [column("TASKRSRC", "act_reg_qty")]);
}

#[test]
fn a_wbs_node_s_estimate_method_rewrites_only_that_cell() {
    let raw = with_assignments();
    let mut s = project_two(&raw);
    let node = s.wbs.values_mut().next().unwrap();
    node.earned_value.estimate = Some(m::EstimateMethod::PerformanceFactorCpi);
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    let cells = changed_cells(&d.removed[0].1, &d.added[0].1);
    let column = raw
        .table("PROJWBS")
        .unwrap()
        .field_index("ev_etc_compute_type")
        .unwrap();
    assert_eq!(cells, [column + 1]);
    assert_eq!(d.added[0].1.split('\t').nth(cells[0]), Some("EE_PF_cpi"));
}

#[test]
fn a_new_assignment_writes_its_units_and_costs() {
    let raw = with_assignments();
    let base = project_two(&raw);
    let mut s = base.clone();
    let mut added = assignment_of(&mut s).clone();
    added.id = m::AssignmentId(9_000);
    added.source = None;
    added.amounts = m::Amounts {
        budgeted_units: Some(40.0),
        remaining_units: Some(40.0),
        price_per_unit: Some(55.25),
        rate_type: Some(m::RateType::Price2),
        cost_follows_units: false,
        ..m::Amounts::default()
    };
    s.assignments.insert(added.id, added.clone());
    let out = patch(&raw, &s).unwrap();
    let table = out.table("TASKRSRC").unwrap();
    let row = table.rows.last().unwrap();
    let cell = |field: &str| row.get(table.field_index(field).unwrap()).unwrap();
    assert_eq!(cell("target_qty"), "40");
    assert_eq!(cell("cost_per_qty"), "55.25");
    assert_eq!(cell("rate_type"), "COST_PER_QTY2");
    assert_eq!(cell("cost_qty_link_flag"), "N");
    // What wasn't given keeps the new row's defaults.
    assert_eq!(cell("act_reg_qty"), "0");
    let back = project_two(&raw::read(bytes(&out)).unwrap());
    let read = &back.assignments[&added.id].amounts;
    assert_eq!(read.budgeted_units, Some(40.0));
    assert_eq!(read.price_per_unit, Some(55.25));
    assert_eq!(read.rate_type, Some(m::RateType::Price2));
    assert_eq!(read.actual_units, Some(0.0));
}

#[test]
fn an_assignment_can_t_move_to_another_activity() {
    let raw = with_assignments();
    let mut s = project_two(&raw);
    let held = assignment_of(&mut s).activity;
    let other = *s.activities.keys().find(|id| **id != held).unwrap();
    assignment_of(&mut s).activity = other;
    assert!(matches!(patch(&raw, &s), Err(PatchError::Unsupported(_))));
}

#[test]
fn changing_lag_rewrites_only_the_lag_cell() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.relationships
        .get_mut(&m::RelationshipId(1001))
        .unwrap()
        .lag = m::Minutes::new(-90);
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1));
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 1);
    assert!(d.added[0].1.ends_with("\t-1.5"), "{}", d.added[0].1);
}

#[test]
fn adding_an_activity_and_relationship_only_appends_rows() {
    let raw = fixture();
    let mut s = project_one(&raw);
    let mut a = s.activities[&m::ActivityId(102)].clone();
    a.id = m::ActivityId(300);
    a.code = "A300".into();
    a.name = "Added".into();
    a.original_duration = m::Minutes::new(480);
    a.remaining_duration = m::Minutes::new(480);
    a.source = None;
    s.activities.insert(a.id, a);
    let mut r = s.relationships[&m::RelationshipId(1001)].clone();
    r.id = m::RelationshipId(2000);
    r.predecessor = m::ActivityId(102);
    r.successor = m::ActivityId(300);
    r.source = None;
    s.relationships.insert(r.id, r);
    let out = patch(&raw, &s).unwrap();
    let new = bytes(&out);
    let d = diff_lines(raw.source(), &new);
    assert_eq!(d.removed, []);
    assert_eq!(d.added.len(), 2, "{d:?}");
    assert!(d.added[0].1.starts_with("%R\t300\t1\t"), "{}", d.added[0].1);
    assert!(
        d.added[1].1.starts_with("%R\t2000\t1\t300\t1\t102\tPR_FS"),
        "{}",
        d.added[1].1
    );
    // The result reads back with the new entities in place.
    let again = project_one(&raw::read(new).unwrap());
    assert_eq!(
        again.activities[&m::ActivityId(300)].original_duration,
        m::Minutes::new(480)
    );
    assert!(again.relationships.contains_key(&m::RelationshipId(2000)));
}

#[test]
fn deleting_an_activity_removes_it_and_its_dependents_only() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.activities.remove(&m::ActivityId(101));
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    assert_eq!(d.added, []);
    let removed: Vec<&str> = d.removed.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(removed.len(), 7, "{removed:#?}");
    for prefix in [
        "%R\t101\t1\t12",           // TASK
        "%R\t1001\t1\t102\t1\t101", // its successor relationship
        "%R\t1002\t2\t201\t1\t101", // project 2's external relationship to it
        "%R\t801\t1\t101",          // its assignment
        "%R\t1\t101\t701",          // its activity code assignment
        "%R\t902\t101\t1",          // its UDF value
        "%R\t905\t801\t1",          // the removed assignment's UDF value
    ] {
        assert!(
            removed.iter().any(|l| l.starts_with(prefix)),
            "missing {prefix:?} in {removed:#?}"
        );
    }
}

#[test]
fn deleting_a_relationship_removes_its_row() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.relationships.remove(&m::RelationshipId(1001));
    let d = diff_lines(raw.source(), &bytes(&patch(&raw, &s).unwrap()));
    assert_eq!(d.added, []);
    assert_eq!(d.removed.len(), 1);
    assert!(d.removed[0].1.starts_with("%R\t1001\t"));
}

#[test]
fn project_dates_are_patched() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.project.data_date = "2026-03-02 08:00".parse().ok();
    let d = diff_lines(raw.source(), &bytes(&patch(&raw, &s).unwrap()));
    assert_eq!((d.removed.len(), d.added.len()), (1, 1));
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 1);
    assert!(d.added[0].1.contains("\t2026-03-02 08:00\t"));
}

#[test]
fn nonterminating_lag_is_written_and_reads_back_exactly() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.relationships
        .get_mut(&m::RelationshipId(1001))
        .unwrap()
        .lag = m::Minutes::new(20);
    let out = patch(&raw, &s).unwrap();
    let diff = diff_lines(raw.source(), &bytes(&out));
    assert_eq!(changed_cells(&diff.removed[0].1, &diff.added[0].1).len(), 1);
    assert_eq!(
        project_one(&out).relationships[&m::RelationshipId(1001)].lag,
        m::Minutes::new(20)
    );
}

/// A calendar's hours per day rewrite only `day_hr_cnt`, and its
/// hours per week only `week_hr_cnt`.
#[test]
fn a_calendar_s_hours_per_day_rewrite_only_that_cell() {
    let raw = fixture();
    let mut s = project_one(&raw);
    let calendar = s.calendars.values_mut().next().unwrap();
    calendar.hours_per_period.day = Some(m::Minutes::new(450));
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    let cells = changed_cells(&d.removed[0].1, &d.added[0].1);
    let column = raw
        .table("CALENDAR")
        .unwrap()
        .field_index("day_hr_cnt")
        .unwrap();
    assert_eq!(cells, [column + 1]);
    assert_eq!(d.added[0].1.split('\t').nth(cells[0]), Some("7.5"));
    let mut s = project_one(&raw);
    s.calendars
        .values_mut()
        .next()
        .unwrap()
        .hours_per_period
        .week = Some(m::Minutes::new(50 * 60));
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    let cells = changed_cells(&d.removed[0].1, &d.added[0].1);
    let column = raw
        .table("CALENDAR")
        .unwrap()
        .field_index("week_hr_cnt")
        .unwrap();
    assert_eq!(cells, [column + 1]);
    assert_eq!(d.added[0].1.split('\t').nth(cells[0]), Some("50"));
    // Month and year, each its own cell.
    for (field, set) in [
        ("month_hr_cnt", (Some(m::Minutes::new(172 * 60)), None)),
        ("year_hr_cnt", (None, Some(m::Minutes::new(2000 * 60)))),
    ] {
        let mut s = project_one(&raw);
        let hours = &mut s.calendars.values_mut().next().unwrap().hours_per_period;
        if let Some(month) = set.0 {
            hours.month = Some(month);
        }
        if let Some(year) = set.1 {
            hours.year = Some(year);
        }
        let new = bytes(&patch(&raw, &s).unwrap());
        let d = diff_lines(raw.source(), &new);
        let cells = changed_cells(&d.removed[0].1, &d.added[0].1);
        let column = raw.table("CALENDAR").unwrap().field_index(field).unwrap();
        assert_eq!(cells, [column + 1], "{field}");
    }
}

#[test]
fn unsupported_edits_are_errors_not_ignored() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.wbs.values_mut().next().unwrap().is_project_node ^= true;
    assert_eq!(patch(&raw, &s).unwrap_err(), PatchError::Unsupported("WBS"));
    let mut s = project_one(&raw);
    s.project.options.make_open_ended_critical = true;
    assert!(matches!(patch(&raw, &s), Err(PatchError::Unsupported(_))));
    // Activity UDF values are written; a project's still aren't.
    let mut s = project_one(&raw);
    s.project.udfs.clear();
    assert_eq!(
        patch(&raw, &s).unwrap_err(),
        PatchError::Unsupported("project fields other than its dates")
    );
}

#[test]
fn a_cleared_code_removes_its_row_and_nothing_else() {
    let raw = fixture();
    let rows = |x: &RawXer| x.table("TASKACTV").unwrap().rows.len();
    let mut s = project_one(&raw);
    let id = m::ActivityId(101);
    assert!(!s.activities[&id].codes.is_empty());
    s.activities.get_mut(&id).unwrap().codes.clear();
    let out = patch(&raw, &s).unwrap();
    let held = raw
        .table("TASKACTV")
        .unwrap()
        .rows
        .iter()
        .filter(|r| r.get(1) == Some("101"));
    assert_eq!(rows(&out) + held.count(), rows(&raw));
    assert!(project_one(&out).activities[&id].codes.is_empty());
}

#[test]
fn new_ids_must_be_above_the_tables_maximum() {
    let raw = fixture();
    let mut s = project_one(&raw);
    let mut a = s.activities[&m::ActivityId(102)].clone();
    a.id = m::ActivityId(150); // below project 2's activity 201
    a.source = None;
    s.activities.insert(a.id, a);
    assert_eq!(
        patch(&raw, &s).unwrap_err(),
        PatchError::IdNotAboveMaximum {
            table: "TASK",
            id: 150,
            max: 201
        }
    );
}

#[test]
fn unknown_project_is_an_error() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.project.id = m::ProjectId(77);
    assert_eq!(
        patch(&raw, &s).unwrap_err(),
        PatchError::NoSuchProject(m::ProjectId(77))
    );
}

#[test]
fn hours_formatting() {
    let h = |m| hours(m::Minutes::new(m));
    assert_eq!(h(0).as_deref(), Some("0"));
    assert_eq!(h(480).as_deref(), Some("8"));
    assert_eq!(h(30).as_deref(), Some("0.5"));
    assert_eq!(h(45).as_deref(), Some("0.75"));
    assert_eq!(h(3).as_deref(), Some("0.05"));
    assert_eq!(h(-90).as_deref(), Some("-1.5"));
    assert_eq!(h(-3).as_deref(), Some("-0.05"));
    assert_eq!(h(20).as_deref(), Some("0.333333"));
    assert_eq!(h(1).as_deref(), Some("0.016667"));
}

#[test]
fn every_minute_formatter_reads_back_including_model_limits() {
    for minute in (-10_000..=10_000).chain([i64::MIN, i64::MAX]) {
        let value = m::Minutes::new(minute);
        let text = hours(value).unwrap();
        assert_eq!(
            text.parse::<crate::typed::Decimal>()
                .unwrap()
                .export_minutes(),
            Some(value),
            "{text}"
        );
    }
}

proptest::proptest! {
    #[test]
    fn arbitrary_model_minutes_survive_export_formatting(minute in proptest::prelude::any::<i64>()) {
        let value = m::Minutes::new(minute);
        let text = hours(value).unwrap();
        proptest::prop_assert_eq!(text.parse::<crate::typed::Decimal>().unwrap().export_minutes(), Some(value));
    }
}

#[test]
fn precision_and_empty_lag_source_spellings_survive_unrelated_edits() {
    let mut raw = fixture();
    for (table, field, token) in [
        ("TASK", "target_drtn_hr_cnt", "7.816667"),
        ("TASKPRED", "lag_hr_cnt", ""),
    ] {
        let t = raw.table_index(table).unwrap();
        let c = raw.tables()[t].field_index(field).unwrap();
        raw.set_cell(t, 0, c, token).unwrap();
    }
    // Reparse to make these spellings the untouched source bytes.
    let raw = raw::read(bytes(&raw).as_slice()).unwrap();
    let mut schedule = project_one(&raw);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
    schedule.project.data_date = "2026-03-02 08:00".parse().ok();
    let out = patch(&raw, &schedule).unwrap();
    let diff = diff_lines(raw.source(), &bytes(&out));
    assert_eq!((diff.removed.len(), diff.added.len()), (1, 1));
    assert!(diff.added[0].1.starts_with("%R\t1\t"));
}

#[test]
fn each_editable_project_date_is_written() {
    // Guards the editable-field list in check_unsupported against
    // PROJECT_FIELDS: every date it allows must reach the file.
    let raw = fixture();
    let when: m::DateTime = "2026-04-01 08:00".parse().unwrap();
    for set in [
        (|p: &mut m::Project, d| p.data_date = Some(d)) as fn(&mut m::Project, m::DateTime),
        |p, d| p.planned_start = Some(d),
        |p, d| p.must_finish_by = Some(d),
        |p, d| p.stored.scheduled_finish = Some(d),
    ] {
        let mut s = project_one(&raw);
        set(&mut s.project, when);
        let d = diff_lines(raw.source(), &bytes(&patch(&raw, &s).unwrap()));
        assert_eq!((d.removed.len(), d.added.len()), (1, 1));
        assert!(
            d.added[0].1.contains("2026-04-01 08:00"),
            "{}",
            d.added[0].1
        );
    }
}

#[test]
fn relationships_must_not_point_at_missing_activities() {
    let raw = fixture();
    // Repointing an existing relationship at a deleted activity.
    let mut s = project_one(&raw);
    s.activities.remove(&m::ActivityId(101));
    s.relationships
        .get_mut(&m::RelationshipId(1001))
        .unwrap()
        .successor = m::ActivityId(101);
    assert_eq!(
        patch(&raw, &s).unwrap_err(),
        PatchError::DanglingRelationship(m::RelationshipId(1001))
    );
    // A new relationship to an activity that doesn't exist.
    let mut s = project_one(&raw);
    let mut r = s.relationships[&m::RelationshipId(1001)].clone();
    r.id = m::RelationshipId(5000);
    r.predecessor = m::ActivityId(4242);
    r.source = None;
    s.relationships.insert(r.id, r);
    assert_eq!(
        patch(&raw, &s).unwrap_err(),
        PatchError::DanglingRelationship(m::RelationshipId(5000))
    );
}

#[test]
fn an_unspellable_value_is_kept_until_it_changes() {
    let mut raw = fixture();
    let t = raw.table_index("TASK").unwrap();
    let c = raw.tables()[t].field_index("phys_complete_pct").unwrap();
    raw.set_cell(t, 0, c, "33.33333").unwrap();
    let mut s = project_one(&raw);
    let id = *s
        .activities
        .iter()
        .find(|(_, a)| a.physical_percent_complete == Some(33.33333))
        .unwrap()
        .0;
    let a = s.activities.get_mut(&id).unwrap();
    a.stored.early_start = Some("2026-03-02 08:00".parse().unwrap());
    let patched = patch(&raw, &s).unwrap();
    assert_eq!(patched.tables()[t].rows[0].get(c), Some("33.33333"));

    let a = s.activities.get_mut(&id).unwrap();
    a.physical_percent_complete = Some(44.44444);
    assert!(matches!(
        patch(&raw, &s),
        Err(PatchError::Unrepresentable { .. })
    ));
}

#[test]
fn percent_complete_with_float_noise_is_refused() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.activities
        .get_mut(&m::ActivityId(101))
        .unwrap()
        .physical_percent_complete = Some(0.1 + 0.2);
    assert!(matches!(
        patch(&raw, &s),
        Err(PatchError::Unrepresentable {
            field: "phys_complete_pct",
            ..
        })
    ));
    s.activities
        .get_mut(&m::ActivityId(101))
        .unwrap()
        .physical_percent_complete = Some(62.5);
    let d = diff_lines(raw.source(), &bytes(&patch(&raw, &s).unwrap()));
    assert!(d.added[0].1.contains("\t62.5\t"));
}

fn longer(schedule: &Schedule) -> Schedule {
    let mut s = schedule.clone();
    let (_, activity) = s.activities.iter_mut().next().unwrap();
    activity.original_duration = m::Minutes(activity.original_duration.get() + 60);
    s
}

fn one_by_one(raw: &RawXer, edited: &[Schedule]) -> Vec<u8> {
    let mut out = raw.clone();
    for schedule in edited {
        out = patch(&out, schedule).unwrap();
    }
    bytes(&out)
}

#[test]
fn patching_together_gives_the_bytes_of_patching_one_after_another() {
    let raw = fixture();
    let (schedules, _) = to_model(&raw);
    assert!(schedules.len() > 1);
    let edited: Vec<_> = schedules.iter().map(longer).collect();
    let together = patch_all(&raw, &schedules, &edited).unwrap();
    assert_eq!(bytes(&together), one_by_one(&raw, &edited));
}

#[test]
fn a_deletion_alongside_other_projects_goes_one_at_a_time() {
    let raw = fixture();
    let (schedules, _) = to_model(&raw);
    let mut first = schedules[0].clone();
    let inside = first.relationships.values().find(|r| !first.is_external(r));
    let id = inside.unwrap().id;
    first.relationships.remove(&id);
    let edited = [first, longer(&schedules[1])];
    assert!(removes_rows(&schedules, &edited[0]));
    let together = patch_all(&raw, &schedules, &edited).unwrap();
    assert_eq!(bytes(&together), one_by_one(&raw, &edited));
}

/// A new role reaches every project, and a deletion sends them one at a time,
/// so the second project's copy meets the first's written row.
#[test]
fn a_role_shared_by_projects_patched_one_at_a_time_is_written_once() {
    let raw = fixture();
    let (schedules, _) = to_model(&raw);
    let role = m::RoleId(9001);
    let mut edited: Vec<_> = schedules.clone();
    for schedule in &mut edited {
        schedule.roles.insert(
            role,
            m::Role {
                id: role,
                short_name: "RIGGER".to_owned(),
                name: "Rigger".to_owned(),
                parent: None,
                rates: Vec::new(),
                default_cost_follows_units: true,
                source: None,
            },
        );
    }
    let first = &mut edited[0];
    let inside = first.relationships.values().find(|r| !first.is_external(r));
    let id = inside.unwrap().id;
    first.relationships.remove(&id);
    assert!(removes_rows(&schedules, &edited[0]));
    let together = patch_all(&raw, &schedules, &edited).unwrap();
    assert_eq!(bytes(&together), one_by_one(&raw, &edited));
    let (back, _) = to_model(&together);
    assert!(back.iter().all(|s| s.roles[&role].short_name == "RIGGER"));
    let t = together.table_index("ROLES").unwrap();
    assert_eq!(together.tables()[t].rows.len(), 1);
}

#[test]
fn a_project_edited_twice_ends_on_its_last_edit() {
    let raw = fixture();
    let (schedules, _) = to_model(&raw);
    let edited = [longer(&schedules[0]), schedules[0].clone()];
    let together = patch_all(&raw, &schedules, &edited).unwrap();
    assert_eq!(bytes(&together), one_by_one(&raw, &edited));
}

#[test]
fn patching_together_names_the_project_that_failed() {
    let raw = fixture();
    let (schedules, _) = to_model(&raw);
    let mut unknown = schedules[1].clone();
    unknown.project.id = m::ProjectId(999);
    let edited = [schedules[0].clone(), unknown];
    let (project, error) = patch_all(&raw, &schedules, &edited).unwrap_err();
    assert_eq!(project, m::ProjectId(999));
    assert!(matches!(error, PatchError::NoSuchProject(_)));
}

#[test]
fn a_row_spelling_its_id_with_a_leading_zero_goes_with_the_activity() {
    let mut raw = fixture();
    let table = raw.table_index("TASKRSRC").unwrap();
    let column = raw.tables()[table].field_index("task_id").unwrap();
    raw.set_cell(table, 0, column, "0101").unwrap();
    let mut s = project_one(&raw);
    s.activities.remove(&m::ActivityId(101));
    s.assignments
        .retain(|_, a| a.activity != m::ActivityId(101));
    s.relationships
        .retain(|_, r| r.predecessor != m::ActivityId(101) && r.successor != m::ActivityId(101));
    let out = patch(&raw, &s).unwrap();
    let table = out.table_index("TASKRSRC").unwrap();
    let rows = &out.tables()[table].rows;
    assert!(rows.iter().all(|row| row.get(column) != Some("0101")));
}

#[test]
fn an_activity_must_name_what_the_file_has() {
    let raw = fixture();
    let base = project_one(&raw);
    for change in [
        |a: &mut m::Activity| a.wbs = m::WbsId(99_999),
        |a: &mut m::Activity| a.calendar = m::CalendarId(99_999),
    ] {
        let mut s = base.clone();
        change(s.activities.values_mut().next().unwrap());
        assert!(matches!(
            patch(&raw, &s),
            Err(PatchError::DanglingActivity(_) | PatchError::Unsupported(_))
        ));
    }
}

#[test]
fn deleting_the_float_path_end_activity_is_refused() {
    let raw = fixture();
    let mut s = project_one(&raw);
    let end = *s.activities.keys().next().unwrap();
    s.project.options.float_paths.end_activity = Some(end);
    let base = [s.clone()];
    s.activities.remove(&end);
    s.relationships
        .retain(|_, r| r.predecessor != end && r.successor != end);
    s.assignments.retain(|_, a| a.activity != end);
    let mut patcher = Patcher::new(&raw, &base);
    assert!(matches!(
        patcher.apply(&s),
        Err(PatchError::Unsupported(
            "deleting the float-path end activity"
        ))
    ));
}

#[test]
fn a_changed_relationship_to_a_deleted_activity_is_refused() {
    let raw = fixture();
    let mut s = project_one(&raw);
    s.activities.remove(&m::ActivityId(101));
    s.assignments
        .retain(|_, a| a.activity != m::ActivityId(101));
    let link = s
        .relationships
        .values_mut()
        .find(|r| r.predecessor == m::ActivityId(101) && r.successor_project != m::ProjectId(1));
    let Some(link) = link else {
        panic!("the fixture links project 2 to activity 101");
    };
    link.lag = m::Minutes(link.lag.get() + 60);
    assert!(matches!(
        patch(&raw, &s),
        Err(PatchError::DanglingRelationship(_))
    ));
}

/// Deleting in one project moves another project's assignment rows, which isn't
/// an edit to its assignments.
#[test]
fn a_deletion_in_one_project_leaves_another_writable() {
    let raw = fixture();
    let schedules = to_model(&raw).0;
    let mut edited = schedules.clone();
    let first = edited
        .iter_mut()
        .find(|s| s.project.id == m::ProjectId(1))
        .unwrap();
    let gone = first.activity_by_code("A1").unwrap().id;
    first.activities.remove(&gone);
    first
        .relationships
        .retain(|_, r| r.predecessor != gone && r.successor != gone);
    first.assignments.retain(|_, a| a.activity != gone);
    let second = edited
        .iter_mut()
        .find(|s| s.project.id == m::ProjectId(2))
        .unwrap();
    second
        .relationships
        .retain(|_, r| r.predecessor != gone && r.successor != gone);
    let kept = second.activity_by_code("B1").unwrap().id;
    second.activities.get_mut(&kept).unwrap().name = "Renamed".to_owned();
    assert!(!second.assignments.is_empty());
    let together = patch_all(&raw, &schedules, &edited).unwrap();
    assert!(String::from_utf8_lossy(&bytes(&together)).contains("Renamed"));
}

/// A global type's new value is in each project holding that type, and the file
/// gets it once.
#[test]
fn a_new_code_value_shared_by_two_projects_is_written_once() {
    let raw = fixture();
    let mut schedules = to_model(&raw).0;
    let shared = schedules[0]
        .activity_code_types
        .values()
        .next()
        .unwrap()
        .clone();
    let kind = shared.id;
    schedules[1].activity_code_types.insert(kind, shared);
    let mut edited = schedules.clone();
    for s in &mut edited {
        s.activity_codes.insert(
            m::ActivityCodeId(9000),
            m::ActivityCode {
                id: m::ActivityCodeId(9000),
                code_type: kind,
                parent: None,
                value: "NEW".to_owned(),
                description: "New".to_owned(),
                sequence: 9,
                source: None,
            },
        );
    }
    let out = patch_all(&raw, &schedules, &edited).unwrap();
    let rows = |x: &RawXer| x.table("ACTVCODE").unwrap().rows.len();
    assert_eq!(rows(&out), rows(&raw) + 1);
}

#[test]
fn a_code_from_another_type_is_refused() {
    let raw = fixture();
    let mut s = project_one(&raw);
    let a = s.activities.get_mut(&m::ActivityId(101)).unwrap();
    let kind = *a.codes.keys().next().unwrap();
    a.codes.insert(kind, m::ActivityCodeId(99_999));
    assert_eq!(
        patch(&raw, &s).unwrap_err(),
        PatchError::DanglingActivity(m::ActivityId(101))
    );
}

/// Mapping reads numbers, so `0101` is activity 101, and rows it holds count.
#[test]
fn a_code_row_spelled_with_leading_zeros_is_found() {
    let text = String::from_utf8(raw_bytes())
        .unwrap()
        .replace("%R\t1\t101\t701\t711", "%R\t1\t0101\t701\t711");
    let raw = raw::read(text.into_bytes()).unwrap();
    let mut s = project_one(&raw);
    let id = m::ActivityId(101);
    assert!(!s.activities[&id].codes.is_empty(), "the row must map");
    s.activities.get_mut(&id).unwrap().codes.clear();
    let out = patch(&raw, &s).unwrap();
    assert!(project_one(&out).activities[&id].codes.is_empty());
}

fn raw_bytes() -> Vec<u8> {
    include_bytes!("../../../../fixtures/xer/model-mapping.xer").to_vec()
}

/// IDs are given in edit order, and projects are written in file order.
#[test]
fn new_ids_needn_t_rise_from_one_project_to_the_next() {
    let raw = fixture();
    let schedules = to_model(&raw).0;
    let mut edited = schedules.clone();
    for (index, id) in [(0, 900), (1, 899)] {
        let s = &mut edited[index];
        let activity = *s.activities.keys().next().unwrap();
        let resource = s.assignments.values().next().unwrap().resource;
        s.assignments.insert(
            m::AssignmentId(id),
            m::Assignment {
                id: m::AssignmentId(id),
                activity,
                resource,
                role: None,
                cost_account: None,
                project: s.project.id,
                curve: None,
                typed_spread: None,
                amounts: m::Amounts::default(),
                udfs: BTreeMap::new(),
                source: None,
            },
        );
    }
    let out = patch_all(&raw, &schedules, &edited).unwrap();
    let rows = |x: &RawXer| x.table("TASKRSRC").unwrap().rows.len();
    assert_eq!(rows(&out), rows(&raw) + 2);
}

#[test]
fn an_assignment_with_actuals_can_t_be_removed() {
    // The fixture's assignments hold no actuals, so its first one is given some.
    let text = String::from_utf8(raw_bytes()).unwrap();
    let mut inside = false;
    let mut first = true;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if line.starts_with("%T") {
                inside = line == "%T	TASKRSRC";
            }
            if inside && line.starts_with("%F") {
                return format!("{line}	act_reg_qty");
            }
            if inside && line.starts_with("%R") {
                let value = if first { "5" } else { "0" };
                first = false;
                return format!("{line}	{value}");
            }
            line.to_owned()
        })
        .collect();
    let raw = raw::read(
        lines
            .join(
                "
",
            )
            .into_bytes(),
    )
    .unwrap();
    let mut s = project_one(&raw);
    assert!(!s.assignments.is_empty());
    s.assignments.clear();
    assert_eq!(
        patch(&raw, &s).unwrap_err(),
        PatchError::Unsupported("removing an assignment with actual units or cost")
    );
}

/// A new node takes how the root is owned and earns value, and none of its money.
#[test]
fn a_new_wbs_node_copies_the_roots_settings_but_not_its_budget() {
    let text = String::from_utf8(raw_bytes()).unwrap();
    let mut inside = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if line.starts_with("%T") {
                inside = line == "%T\tPROJWBS";
            }
            if inside && line.starts_with("%F") {
                return format!("{line}\tobs_id\torig_cost");
            }
            if inside && line.starts_with("%R") {
                return format!("{line}\t77\t5000");
            }
            line.to_owned()
        })
        .collect();
    let raw = raw::read(lines.join("\n").into_bytes()).unwrap();
    let mut s = project_one(&raw);
    let root = s.wbs.values().find(|w| w.is_project_node).unwrap().clone();
    let id = m::WbsId(9000);
    s.wbs.insert(
        id,
        m::Wbs {
            id,
            parent: Some(root.id),
            code: "NEW".to_owned(),
            name: "New".to_owned(),
            is_project_node: false,
            source: None,
            ..root
        },
    );
    let out = patch(&raw, &s).unwrap();
    let table = out.table("PROJWBS").unwrap();
    let row = table.rows.last().unwrap();
    let cell = |name: &str| row.get(table.field_index(name).unwrap());
    assert_eq!(cell("wbs_short_name"), Some("NEW"));
    assert_eq!(cell("obs_id"), Some("77"));
    assert_eq!(cell("orig_cost"), Some("0"));
}

/// A new code type and resource go in the file once each, and read back in
/// every project: no activity uses them.
#[test]
fn a_new_code_type_and_resource_are_written_and_read_back() {
    let raw = fixture();
    let schedules = to_model(&raw).0;
    let mut edited = schedules.clone();
    let (kind, resource) = add_type_and_resource(&mut edited[0], None, false);
    let out = patch_all(&raw, &schedules, &edited).unwrap();
    let text = String::from_utf8_lossy(&bytes(&out)).into_owned();
    assert_eq!(text.matches("Synthetic new type").count(), 1);
    assert_eq!(text.matches("NEWCREW").count(), 1);
    let read = to_model(&out).0;
    assert_eq!(read.len(), 2);
    for schedule in &read {
        let id = schedule.project.id;
        assert_eq!(schedule.resources[&resource].short_name, "NEWCREW", "{id}");
        assert_eq!(
            schedule.resources[&resource].kind,
            m::ResourceType::Nonlabor
        );
        assert_eq!(
            schedule.activity_code_types[&kind].name,
            "Synthetic new type"
        );
    }
}

/// In a file with every resource column, a new resource holds what most
/// resources in the corpus hold.
#[test]
fn a_new_resource_holds_the_usual_settings() {
    let raw =
        raw::read(include_bytes!("../../../../fixtures/xer/fresh-writer.xer").as_slice()).unwrap();
    let schedules = to_model(&raw).0;
    let mut edited = schedules.clone();
    let (_, resource) = add_type_and_resource(&mut edited[0], Some(0.5), true);
    let out = patch_all(&raw, &schedules, &edited).unwrap();
    let text = String::from_utf8_lossy(&bytes(&out)).into_owned();
    let row = text.lines().find(|l| l.contains("NEWCREW")).unwrap();
    for cell in ["RT_Equip", "QT_Hour", "0.5", "Y", "N", "1"] {
        assert!(row.split('\t').any(|c| c == cell), "{cell}: {row}");
    }
    let read = to_model(&out).0;
    let held = &read[0].resources[&resource];
    assert_eq!(
        (held.default_units_per_hour, held.default_cost_follows_units),
        (Some(0.5), true)
    );
}

fn add_type_and_resource(
    schedule: &mut Schedule,
    units: Option<f64>,
    cost_follows_units: bool,
) -> (m::ActivityCodeTypeId, m::ResourceId) {
    let kind = m::ActivityCodeTypeId(9100);
    schedule.activity_code_types.insert(
        kind,
        m::ActivityCodeType {
            id: kind,
            name: "Synthetic new type".to_owned(),
            scope: m::CodeScope::Global,
            project: None,
            max_length: Some(12),
            sequence: 5,
            source: None,
        },
    );
    let resource = m::ResourceId(9200);
    schedule.resources.insert(
        resource,
        m::Resource {
            id: resource,
            short_name: "NEWCREW".to_owned(),
            name: "Synthetic new crew".to_owned(),
            kind: m::ResourceType::Nonlabor,
            calendar: None,
            parent: None,
            default_units_per_hour: units,
            rates: Vec::new(),
            roles: Vec::new(),
            default_cost_follows_units: cost_follows_units,
            udfs: std::collections::BTreeMap::new(),
            source: None,
        },
    );
    (kind, resource)
}

/// A resource the file already holds isn't written again when another
/// project takes it up.
#[test]
fn a_resource_from_another_project_isn_t_written_twice() {
    let raw = fixture();
    let schedules = to_model(&raw).0;
    let mut edited = schedules.clone();
    let (taken, held) = schedules[0]
        .resources
        .iter()
        .find(|(id, _)| !schedules[1].resources.contains_key(id))
        .map(|(id, r)| (*id, r.clone()))
        .unwrap();
    edited[1].resources.insert(taken, held.clone());
    let out = patch_all(&raw, &schedules, &edited).unwrap();
    let text = String::from_utf8_lossy(&bytes(&out)).into_owned();
    let rows = text
        .lines()
        .filter(|l| l.starts_with("%R") && l.contains(&held.short_name) && l.contains(&held.name))
        .count();
    assert_eq!(rows, 1);
}

/// Nothing deletes a resource or code value, so a schedule without one it
/// started with is refused, not written as if it were still there.
#[test]
fn a_dropped_resource_or_code_value_is_refused() {
    let raw = fixture();
    let schedules = to_model(&raw).0;
    let mut edited = schedules[0].clone();
    edited.assignments.clear();
    edited.resources.clear();
    assert!(matches!(
        patch(&raw, &edited),
        Err(PatchError::Unsupported("resources"))
    ));
    let mut edited = schedules[0].clone();
    for activity in edited.activities.values_mut() {
        activity.codes.clear();
    }
    edited.activity_codes.clear();
    assert!(matches!(
        patch(&raw, &edited),
        Err(PatchError::Unsupported("existing activity code values"))
    ));
}

/// The fixture without the tables a new resource, code type, code value,
/// code and assignment go in, as about a third of corpus files are.
fn without_dictionaries() -> RawXer {
    const GONE: [&str; 5] = ["RSRC", "TASKRSRC", "ACTVTYPE", "ACTVCODE", "TASKACTV"];
    let text = String::from_utf8(raw_bytes()).unwrap();
    let mut inside = false;
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| {
            if let Some(name) = line.strip_prefix("%T\t") {
                inside = GONE.contains(&name);
            } else if line.starts_with("%E") {
                inside = false;
            }
            !inside
        })
        .collect();
    let raw = raw::read(lines.join("\n").into_bytes()).unwrap();
    for name in GONE {
        assert!(raw.table(name).is_none(), "{name}");
    }
    raw
}

/// A file with no table for what an edit adds gets one, with P6's columns,
/// after every table it had, which keep their bytes.
#[test]
fn a_missing_table_is_added_when_an_edit_needs_it() {
    let raw = without_dictionaries();
    let mut s = project_one(&raw);
    assert!(s.resources.is_empty() && s.activity_code_types.is_empty());
    let (kind, resource) = add_type_and_resource(&mut s, Some(2.0), true);
    let code = m::ActivityCodeId(9300);
    s.activity_codes.insert(
        code,
        m::ActivityCode {
            id: code,
            code_type: kind,
            parent: None,
            value: "NEW".to_owned(),
            description: "Synthetic new value".to_owned(),
            sequence: 1,
            source: None,
        },
    );
    let activity = *s.activities.keys().next().unwrap();
    s.activities
        .get_mut(&activity)
        .unwrap()
        .codes
        .insert(kind, code);
    let assignment = m::AssignmentId(9400);
    s.assignments.insert(
        assignment,
        m::Assignment {
            id: assignment,
            activity,
            resource: Some(resource),
            role: None,
            cost_account: None,
            project: s.project.id,
            curve: None,
            typed_spread: None,
            amounts: m::Amounts::default(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    let out = patch(&raw, &s).unwrap();
    // Everything the file held is still there, byte for byte, in order.
    let new = bytes(&out);
    let d = diff_lines(raw.source(), &new);
    assert!(d.removed.is_empty(), "{d:?}");
    let source = String::from_utf8(raw.source().to_vec()).unwrap();
    let end = source.find("%E").unwrap();
    assert!(new.starts_with(&raw.source()[..end]));
    for (name, columns) in [
        ("RSRC", crate::fresh::columns("RSRC").unwrap()),
        ("ACTVTYPE", crate::fresh::columns("ACTVTYPE").unwrap()),
        ("ACTVCODE", crate::fresh::columns("ACTVCODE").unwrap()),
        ("TASKACTV", crate::fresh::columns("TASKACTV").unwrap()),
        ("TASKRSRC", crate::fresh::columns("TASKRSRC").unwrap()),
    ] {
        let table = out.table(name).unwrap();
        assert_eq!(table.fields, columns, "{name}");
        assert_eq!(table.rows.len(), 1, "{name}");
    }
    let read = project_one(&out);
    assert_eq!(read.resources[&resource].short_name, "NEWCREW");
    assert_eq!(read.activity_code_types[&kind].name, "Synthetic new type");
    assert_eq!(read.activity_codes[&code].value, "NEW");
    assert_eq!(read.activities[&activity].codes[&kind], code);
    assert_eq!(read.assignments[&assignment].resource, Some(resource));
    // Written back unchanged, the patched file stays as it is.
    assert_eq!(bytes(&patch(&out, &read).unwrap()), new);
}

/// The fixture with one column taken out of one table, as some corpus files
/// are exported.
fn without_column(table: &str, field: &str) -> RawXer {
    let text = String::from_utf8(raw_bytes()).unwrap();
    let mut inside = false;
    let mut at = None;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if let Some(name) = line.strip_prefix("%T\t") {
                inside = name == table;
                at = None;
            }
            let mut cells: Vec<&str> = line.split('\t').collect();
            if inside && cells.first() == Some(&"%F") {
                at = cells.iter().position(|cell| *cell == field);
            }
            if inside
                && let Some(at) = at
                && (cells[0] == "%F" || cells[0] == "%R")
                && at < cells.len()
            {
                cells.remove(at);
            }
            cells.join("\t")
        })
        .collect();
    let raw = raw::read(lines.join("\n").into_bytes()).unwrap();
    let read = raw.table(table).unwrap();
    assert!(
        read.field_index(field).is_none() && !read.rows.is_empty(),
        "{table}.{field}"
    );
    raw
}

/// A file whose resource table has no units-per-hour column takes a new
/// resource at the default 1 an hour, which reads back as not stated, as an
/// empty cell does. Units asked for that the file can't hold are refused
///.
#[test]
fn a_resource_table_without_units_per_hour_takes_the_default() {
    let raw = without_column("RSRC", "def_qty_per_hr");
    let mut s = project_one(&raw);
    let (_, resource) = add_type_and_resource(&mut s, Some(1.0), false);
    let out = patch(&raw, &s).unwrap();
    let read = project_one(&out);
    assert_eq!(read.resources[&resource].short_name, "NEWCREW");
    assert_eq!(read.resources[&resource].default_units_per_hour, None);
    assert!(
        out.table("RSRC")
            .unwrap()
            .field_index("def_qty_per_hr")
            .is_none()
    );

    let mut s = project_one(&raw);
    add_type_and_resource(&mut s, Some(2.0), false);
    assert!(matches!(
        patch(&raw, &s),
        Err(PatchError::MissingColumn {
            table: "RSRC",
            field: "def_qty_per_hr"
        })
    ));
}

/// A resource's limit changes in its rate's `max_qty_per_hr` cell and
/// nowhere else, and a new rate is a new `RSRCRATE` row that reads back.
#[test]
fn a_resource_s_limit_changes_its_rate_and_a_new_rate_is_appended() {
    let raw = raw::read(include_bytes!("../../../../fixtures/xer/usage.xer").as_slice()).unwrap();
    let mut s = to_model(&raw).0.swap_remove(0);
    let (&id, crew) = s
        .resources
        .iter_mut()
        .find(|(_, r)| !r.rates.is_empty())
        .unwrap();
    crew.rates[0].max_units_per_hour = Some(2.0);
    let new = bytes(&patch(&raw, &s).unwrap());
    let d = diff_lines(raw.source(), &new);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    let cells = changed_cells(&d.removed[0].1, &d.added[0].1);
    let rates = raw.table("RSRCRATE").unwrap();
    assert_eq!(cells, [rates.field_index("max_qty_per_hr").unwrap() + 1]);

    let crew = s.resources.get_mut(&id).unwrap();
    let mut later = crew.rates[0].clone();
    later.id = m::ResourceRateId(2);
    later.effective = "2026-01-12 00:00".parse().ok();
    later.max_units_per_hour = Some(3.0);
    later.source = None;
    crew.rates.push(later.clone());
    let out = patch(&raw, &s).unwrap();
    let read = to_model(&out).0.swap_remove(0);
    let held = &read.resources[&id].rates;
    assert_eq!(held.len(), 2);
    assert_eq!(
        (
            held[1].id,
            held[1].effective,
            held[1].prices,
            held[1].max_units_per_hour
        ),
        (later.id, later.effective, later.prices, Some(3.0))
    );
    // Taking a rate away isn't an edit TotalFloat makes, so it's refused.
    let mut dropped = to_model(&raw).0.swap_remove(0);
    dropped.resources.get_mut(&id).unwrap().rates.clear();
    assert!(matches!(
        patch(&raw, &dropped),
        Err(PatchError::Unsupported("resources"))
    ));
}

/// A new rate on a resource two projects hold goes in the file once,
/// whether the projects are patched together or one at a time.
#[test]
fn a_new_rate_two_projects_hold_is_written_once() {
    let raw = fixture();
    let schedules = to_model(&raw).0;
    let (taken, mut held) = schedules[0]
        .resources
        .iter()
        .find(|(id, _)| !schedules[1].resources.contains_key(id))
        .map(|(id, r)| (*id, r.clone()))
        .unwrap();
    held.rates.push(m::ResourceRate {
        id: m::ResourceRateId(9001),
        effective: "2027-01-04 00:00".parse().ok(),
        prices: [None; 5],
        max_units_per_hour: Some(3.0),
        source: None,
    });
    for one_at_a_time in [false, true] {
        let mut edited = schedules.clone();
        edited[0].resources.insert(taken, held.clone());
        edited[1].resources.insert(taken, held.clone());
        if one_at_a_time {
            // Removing a link makes the projects go one at a time.
            let link = *edited[0].relationships.keys().next().unwrap();
            edited[0].relationships.remove(&link);
        }
        let out = patch_all(&raw, &schedules, &edited).unwrap();
        let rates = out.table("RSRCRATE").unwrap();
        let ids: Vec<_> = rates.rows.iter().map(|r| r.get(0).unwrap()).collect();
        assert_eq!(ids, ["9001"], "one at a time: {one_at_a_time}");
        let read = to_model(&out).0;
        assert_eq!(
            read[0].resources[&taken].rates[0].max_units_per_hour,
            Some(3.0)
        );
    }
}
