//! Expenses in a synthetic file, `fixtures/xer/expenses.xer`, written
//! fresh from `model-mapping.xer`'s first project with cost accounts, an
//! expense category and expenses added. The file is read, edited and written
//! back, and only what changed differs.
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

fn expense(id: u64, activity: m::ActivityId, project: m::ProjectId, name: &str) -> m::Expense {
    m::Expense {
        id: m::ExpenseId(id),
        activity,
        project,
        name: name.to_owned(),
        cost_account: None,
        category: None,
        accrual: Some(m::Accrual::Uniform),
        budgeted_cost: Some(0.0),
        actual_cost: Some(0.0),
        remaining_cost: Some(0.0),
        price_per_unit: Some(0.0),
        budgeted_units: Some(1.0),
        unit_of_measure: String::new(),
        auto_compute_actuals: false,
        po_number: String::new(),
        vendor: String::new(),
        description: String::new(),
        source: None,
    }
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
    for (id, code, name, parent) in [
        (1, "SITE", "Site costs", None),
        (
            2,
            "SITE.PERMIT",
            "Permits and fees",
            Some(m::CostAccountId(1)),
        ),
    ] {
        schedule.cost_accounts.insert(
            m::CostAccountId(id),
            m::CostAccount {
                id: m::CostAccountId(id),
                short_name: code.to_owned(),
                name: name.to_owned(),
                parent,
                sequence: Some(i64::try_from(id).unwrap() * 10),
                description: String::new(),
                source: None,
            },
        );
    }
    schedule.expense_categories.insert(
        m::ExpenseCategoryId(1),
        m::ExpenseCategory {
            id: m::ExpenseCategoryId(1),
            name: "Fees".to_owned(),
            sequence: Some(0),
            source: None,
        },
    );
    let activities: Vec<_> = schedule.activities.keys().copied().collect();
    let mut permit = expense(1, activities[0], project, "Building permit");
    permit.cost_account = Some(m::CostAccountId(2));
    permit.category = Some(m::ExpenseCategoryId(1));
    permit.budgeted_cost = Some(1500.0);
    permit.remaining_cost = Some(1500.0);
    permit.price_per_unit = Some(1500.0);
    permit.accrual = Some(m::Accrual::Start);
    "PO-1001".clone_into(&mut permit.po_number);
    "City Hall".clone_into(&mut permit.vendor);
    let mut skip = expense(2, activities[1], project, "Skip hire");
    skip.budgeted_cost = Some(1200.0);
    skip.actual_cost = Some(400.0);
    skip.remaining_cost = Some(800.0);
    skip.price_per_unit = Some(300.0);
    skip.budgeted_units = Some(4.0);
    "week".clone_into(&mut skip.unit_of_measure);
    let mut survey = expense(3, activities[1], project, "Survey");
    survey.budgeted_cost = Some(650.0);
    survey.remaining_cost = Some(650.0);
    survey.price_per_unit = Some(650.0);
    survey.accrual = Some(m::Accrual::End);
    for x in [permit, skip, survey] {
        schedule.expenses.insert(x.id, x);
    }
    schedule
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut out = Vec::new();
    xer.write(&mut out).unwrap();
    out
}

fn fixture() -> RawXer {
    raw::read(std::fs::read(root().join("fixtures/xer/expenses.xer")).unwrap()).unwrap()
}

/// Set `TOTALFLOAT_UPDATE_CONTRACT=1` to rewrite the fixture after a
/// deliberate change, and record why in fixtures/xer/MANIFEST.md.
#[test]
fn the_fixture_is_the_authored_model_written_fresh() {
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let written = bytes(&write(&authored(), options).unwrap());
    let path = root().join("fixtures/xer/expenses.xer");
    if std::env::var_os("TOTALFLOAT_UPDATE_CONTRACT").is_some() {
        std::fs::write(&path, &written).unwrap();
        return;
    }
    assert!(
        std::fs::read(&path).unwrap() == written,
        "fixtures/xer/expenses.xer is out of date; rerun with TOTALFLOAT_UPDATE_CONTRACT=1"
    );
}

#[test]
fn expenses_accounts_and_categories_are_read() {
    let (schedules, diagnostics) = to_model(&fixture());
    assert!(diagnostics.omitted.is_empty());
    let schedule = &schedules[0];
    assert_eq!(schedule.expenses.len(), 3);
    assert_eq!(schedule.cost_accounts.len(), 2);
    assert_eq!(
        schedule.cost_accounts[&m::CostAccountId(2)].parent,
        Some(m::CostAccountId(1))
    );
    assert_eq!(
        schedule.expense_categories[&m::ExpenseCategoryId(1)].name,
        "Fees"
    );
    let permit = &schedule.expenses[&m::ExpenseId(1)];
    assert_eq!(permit.name, "Building permit");
    assert_eq!(permit.cost_account, Some(m::CostAccountId(2)));
    assert_eq!(permit.category, Some(m::ExpenseCategoryId(1)));
    assert_eq!(permit.accrual, Some(m::Accrual::Start));
    assert_eq!(permit.vendor, "City Hall");
    let skip = &schedule.expenses[&m::ExpenseId(2)];
    assert_eq!(skip.at_completion_cost(), 1200.0);
    assert_eq!(skip.unit_of_measure, "week");
    assert_eq!(schedule.validate(), []);
}

#[test]
fn an_unchanged_schedule_writes_back_byte_for_byte() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    assert_eq!(bytes(&patch(&raw, &schedule).unwrap()), raw.source());
}

#[test]
fn changing_an_expense_s_cost_changes_only_those_cells() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let skip = schedule.expenses.get_mut(&m::ExpenseId(2)).unwrap();
    skip.actual_cost = Some(600.0);
    skip.remaining_cost = Some(600.0);
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert_eq!((d.removed.len(), d.added.len()), (1, 1), "{d:?}");
    assert_eq!(changed_cells(&d.removed[0].1, &d.added[0].1).len(), 2);
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    assert_eq!(back.expenses[&m::ExpenseId(2)].actual_cost, Some(600.0));
}

#[test]
fn a_new_expense_is_one_added_row_and_reads_back() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let activity = *schedule.activities.keys().next().unwrap();
    let mut added = expense(4, activity, schedule.project.id, "Crane");
    added.cost_account = Some(m::CostAccountId(1));
    added.budgeted_cost = Some(2400.0);
    added.remaining_cost = Some(2400.0);
    added.price_per_unit = Some(2400.0);
    schedule.expenses.insert(added.id, added.clone());
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert!(d.removed.is_empty(), "{d:?}");
    assert_eq!(d.added.len(), 1, "{d:?}");
    let back = to_model(&raw::read(out).unwrap()).0.swap_remove(0);
    let mut read = back.expenses[&added.id].clone();
    read.source = None;
    assert_eq!(read, added);
}

#[test]
fn a_removed_expense_is_one_removed_row() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    schedule.expenses.remove(&m::ExpenseId(3));
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(raw.source(), &out);
    assert!(d.added.is_empty(), "{d:?}");
    assert_eq!(d.removed.len(), 1, "{d:?}");
    assert!(d.removed[0].1.contains("Survey"), "{d:?}");
}

#[test]
fn moving_an_expense_or_changing_an_account_or_category_is_refused() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    let mut moved = schedule.clone();
    let other = *moved.activities.keys().nth(1).unwrap();
    moved.expenses.get_mut(&m::ExpenseId(1)).unwrap().activity = other;
    assert!(patch(&raw, &moved).is_err());
    let mut renamed = schedule.clone();
    renamed
        .cost_accounts
        .get_mut(&m::CostAccountId(1))
        .unwrap()
        .name = "Site".to_owned();
    assert!(patch(&raw, &renamed).is_err());
    let mut added = schedule.clone();
    let mut account = added.cost_accounts[&m::CostAccountId(1)].clone();
    account.id = m::CostAccountId(9);
    account.source = None;
    added.cost_accounts.insert(account.id, account);
    assert!(patch(&raw, &added).is_err());
    let mut dropped = schedule;
    dropped.expense_categories.clear();
    dropped
        .expenses
        .values_mut()
        .for_each(|x| x.category = None);
    assert!(patch(&raw, &dropped).is_err());
}

#[test]
fn moving_an_expense_off_an_activity_being_deleted_is_refused() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let activity = schedule.expenses[&m::ExpenseId(1)].activity;
    let other = *schedule
        .activities
        .keys()
        .find(|id| **id != activity)
        .unwrap();
    schedule.activities.remove(&activity);
    schedule
        .relationships
        .retain(|_, r| r.predecessor != activity && r.successor != activity);
    schedule.assignments.retain(|_, a| a.activity != activity);
    schedule
        .expenses
        .retain(|id, x| x.activity != activity || id.0 == 1);
    schedule
        .expenses
        .get_mut(&m::ExpenseId(1))
        .unwrap()
        .activity = other;
    let refused = patch(&raw, &schedule).unwrap_err().to_string();
    assert!(refused.contains("moving an expense"), "{refused}");
}

#[test]
fn an_expense_given_an_account_or_category_not_in_the_file_is_refused() {
    let raw = fixture();
    let schedule = to_model(&raw).0.swap_remove(0);
    let mut accounted = schedule.clone();
    accounted
        .expenses
        .get_mut(&m::ExpenseId(1))
        .unwrap()
        .cost_account = Some(m::CostAccountId(99));
    let refused = patch(&raw, &accounted).unwrap_err().to_string();
    assert!(refused.contains("cost account"), "{refused}");
    let mut categorized = schedule;
    categorized
        .expenses
        .get_mut(&m::ExpenseId(1))
        .unwrap()
        .category = Some(m::ExpenseCategoryId(99));
    let refused = patch(&raw, &categorized).unwrap_err().to_string();
    assert!(refused.contains("category"), "{refused}");
}

/// The fixture with one `PROJCOST` cell replaced, on the first expense's row.
fn with_cell(field: &str, value: &str) -> RawXer {
    let mut raw = fixture();
    let t = raw.table_index("PROJCOST").unwrap();
    let column = raw.tables()[t].field_index(field).unwrap();
    raw.set_cell(t, 0, column, value).unwrap();
    raw
}

#[test]
fn an_expense_whose_account_or_category_is_missing_keeps_its_cost_with_a_warning() {
    for field in ["acct_id", "cost_type_id"] {
        let (schedules, diagnostics) = to_model(&with_cell(field, "99"));
        let permit = &schedules[0].expenses[&m::ExpenseId(1)];
        assert_eq!(permit.cost_account.is_none(), field == "acct_id");
        assert_eq!(permit.category.is_none(), field == "cost_type_id");
        assert_eq!(permit.budgeted_cost, Some(1500.0));
        assert_eq!(schedules[0].validate(), []);
        let warned = diagnostics
            .details
            .iter()
            .filter(|d| d.field == field)
            .count();
        assert_eq!(warned, 1, "{field}: {:?}", diagnostics.details);
    }
}

#[test]
fn an_expense_on_a_missing_activity_is_left_out_and_reported() {
    let (schedules, diagnostics) = to_model(&with_cell("task_id", "99999"));
    assert!(!schedules[0].expenses.contains_key(&m::ExpenseId(1)));
    assert_eq!(diagnostics.omitted.len(), 1);
}

#[test]
fn an_unknown_accrual_code_reads_as_none_with_a_warning() {
    let (schedules, diagnostics) = to_model(&with_cell("cost_load_type", "CL_Later"));
    assert_eq!(schedules[0].expenses[&m::ExpenseId(1)].accrual, None);
    assert!(
        diagnostics
            .details
            .iter()
            .any(|d| d.field == "cost_load_type")
    );
}

#[test]
fn deleting_an_activity_takes_its_expenses() {
    let raw = fixture();
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let activity = schedule.expenses[&m::ExpenseId(1)].activity;
    schedule.activities.remove(&activity);
    schedule
        .relationships
        .retain(|_, r| r.predecessor != activity && r.successor != activity);
    schedule.assignments.retain(|_, a| a.activity != activity);
    schedule.expenses.retain(|_, x| x.activity != activity);
    let out = patch(&raw, &schedule).unwrap();
    let back = to_model(&out).0.swap_remove(0);
    assert!(!back.expenses.contains_key(&m::ExpenseId(1)));
    assert_eq!(back.expenses.len(), 2);
}

/// The fixture with a text UDF on expenses, and a value on `expense`.
fn with_expense_udf(expense: u64) -> RawXer {
    let mut raw = fixture();
    let row = |raw: &RawXer, table: &str, cells: &[(&str, &str)]| {
        let t = raw.table_index(table).unwrap();
        let fields = &raw.tables()[t].fields;
        let mut row = vec![String::new(); fields.len()];
        for (name, value) in cells {
            let at = fields.iter().position(|f| f == name).unwrap();
            (*value).clone_into(&mut row[at]);
        }
        (t, row)
    };
    let (t, cells) = row(
        &raw,
        "UDFTYPE",
        &[
            ("udf_type_id", "900"),
            ("table_name", "PROJCOST"),
            ("udf_type_name", "user_field_900"),
            ("udf_type_label", "Invoice"),
            ("logical_data_type", "FT_TEXT"),
        ],
    );
    raw.push_row(t, &cells).unwrap();
    let id = expense.to_string();
    let (t, cells) = row(
        &raw,
        "UDFVALUE",
        &[
            ("udf_type_id", "900"),
            ("fk_id", &id),
            ("proj_id", "1"),
            ("udf_text", "INV-7"),
        ],
    );
    raw.push_row(t, &cells).unwrap();
    raw
}

#[test]
fn a_removed_expense_takes_its_udf_values() {
    let raw = with_expense_udf(3);
    let mut schedule = to_model(&raw).0.swap_remove(0);
    schedule.expenses.remove(&m::ExpenseId(3));
    let out = bytes(&patch(&raw, &schedule).unwrap());
    let d = diff_lines(&bytes(&raw), &out);
    assert!(d.added.is_empty(), "{d:?}");
    assert_eq!(d.removed.len(), 2, "{d:?}");
    assert!(
        d.removed.iter().any(|(_, line)| line.contains("INV-7")),
        "{d:?}"
    );
}

#[test]
fn deleting_an_activity_takes_its_expenses_udf_values() {
    let raw = with_expense_udf(1);
    let mut schedule = to_model(&raw).0.swap_remove(0);
    let activity = schedule.expenses[&m::ExpenseId(1)].activity;
    schedule.activities.remove(&activity);
    schedule
        .relationships
        .retain(|_, r| r.predecessor != activity && r.successor != activity);
    schedule.assignments.retain(|_, a| a.activity != activity);
    schedule.expenses.retain(|_, x| x.activity != activity);
    let out = bytes(&patch(&raw, &schedule).unwrap());
    assert!(!String::from_utf8_lossy(&out).contains("INV-7"));
}
