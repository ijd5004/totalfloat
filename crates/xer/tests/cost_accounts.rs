//! The cost account an assignment's costs are booked to
//! (`TASKRSRC.acct_id`), in `fixtures/xer/expenses.xer`, which has cost
//! accounts SITE and SITE.PERMIT and one assignment, 801.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::Path;

use totalfloat_model as m;
use totalfloat_xer::fresh::{FreshOptions, write};
use totalfloat_xer::raw::{self, RawXer};
use totalfloat_xer::rawdiff::{changed_cells, diff_lines};
use totalfloat_xer::{patch, to_model};

const ASSIGNMENT: m::AssignmentId = m::AssignmentId(801);

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

/// The fixture, with the assignment's `acct_id` set to `account`.
fn booked_to(account: &str) -> RawXer {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/xer/expenses.xer"
    ));
    let mut raw = raw::read(std::fs::read(path).unwrap()).unwrap();
    let t = raw.table_index("TASKRSRC").unwrap();
    let column = raw.tables()[t].field_index("acct_id").unwrap();
    raw.set_cell(t, 0, column, account).unwrap();
    raw
}

#[test]
fn an_assignment_s_cost_account_is_read() {
    let (schedules, diagnostics) = to_model(&booked_to("2"));
    let schedule = &schedules[0];
    assert_eq!(
        schedule.assignments[&ASSIGNMENT].cost_account,
        Some(m::CostAccountId(2))
    );
    assert!(diagnostics.details.iter().all(|d| d.field != "acct_id"));
    assert_eq!(schedule.validate(), []);
}

#[test]
fn an_assignment_s_account_missing_from_the_file_is_cleared_with_a_warning() {
    let (schedules, diagnostics) = to_model(&booked_to("99"));
    assert_eq!(schedules[0].assignments[&ASSIGNMENT].cost_account, None);
    let warned = diagnostics
        .details
        .iter()
        .filter(|d| d.field == "acct_id")
        .count();
    assert_eq!(warned, 1, "{:?}", diagnostics.details);
}

#[test]
fn booking_an_assignment_to_an_account_changes_one_cell() {
    let raw = booked_to("");
    let mut schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), bytes(&raw));
    schedule
        .assignments
        .get_mut(&ASSIGNMENT)
        .unwrap()
        .cost_account = Some(m::CostAccountId(1));
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(&bytes(&raw), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 1);
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(
        back.assignments[&ASSIGNMENT].cost_account,
        Some(m::CostAccountId(1))
    );

    // And back to none.
    let raw = booked_to("2");
    let mut schedule = to_model(&raw).0.swap_remove(0);
    schedule
        .assignments
        .get_mut(&ASSIGNMENT)
        .unwrap()
        .cost_account = None;
    let back = to_model(&patch(&raw, &schedule).unwrap()).0.swap_remove(0);
    assert_eq!(back.assignments[&ASSIGNMENT].cost_account, None);
}

#[test]
fn booking_an_assignment_to_an_account_not_in_the_file_is_refused() {
    let raw = booked_to("");
    let mut schedule = to_model(&raw).0.swap_remove(0);
    schedule
        .assignments
        .get_mut(&ASSIGNMENT)
        .unwrap()
        .cost_account = Some(m::CostAccountId(99));
    let refused = patch(&raw, &schedule).unwrap_err().to_string();
    assert!(refused.contains("cost account"), "{refused}");
}

#[test]
fn an_assignment_s_account_is_written_fresh() {
    let schedule = to_model(&booked_to("2")).0.swap_remove(0);
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let written = write(&schedule, options).unwrap();
    let back = to_model(&written).0.swap_remove(0);
    assert_eq!(
        back.assignments[&ASSIGNMENT].cost_account,
        Some(m::CostAccountId(2))
    );
}
