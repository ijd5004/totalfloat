//! Expenses: costs on an activity that no resource carries.

use crate::{ActivityId, CostAccountId, ExpenseCategoryId, ExpenseId, ProjectId, SourceRef};

/// A cost on an activity that no resource carries, such as a permit or a
/// subcontract. XER: a `PROJCOST` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Expense {
    /// XER: `PROJCOST.cost_item_id`.
    pub id: ExpenseId,
    /// XER: `PROJCOST.task_id`.
    pub activity: ActivityId,
    /// XER: `PROJCOST.proj_id`.
    pub project: ProjectId,
    /// Expense item. XER: `PROJCOST.cost_name`.
    pub name: String,
    /// XER: `PROJCOST.acct_id`.
    pub cost_account: Option<CostAccountId>,
    /// XER: `PROJCOST.cost_type_id`.
    pub category: Option<ExpenseCategoryId>,
    /// How its cost is spread over the activity; unknown codes read as none.
    /// XER: `PROJCOST.cost_load_type`.
    pub accrual: Option<Accrual>,
    /// XER: `PROJCOST.target_cost`.
    pub budgeted_cost: Option<f64>,
    /// XER: `PROJCOST.act_cost`.
    pub actual_cost: Option<f64>,
    /// XER: `PROJCOST.remain_cost`.
    pub remaining_cost: Option<f64>,
    /// XER: `PROJCOST.cost_per_qty`.
    pub price_per_unit: Option<f64>,
    /// XER: `PROJCOST.target_qty`.
    pub budgeted_units: Option<f64>,
    /// XER: `PROJCOST.qty_name`.
    pub unit_of_measure: String,
    /// Whether its actual cost is worked out from the activity's progress.
    /// XER: `PROJCOST.auto_compute_act_flag`.
    pub auto_compute_actuals: bool,
    /// XER: `PROJCOST.po_number`.
    pub po_number: String,
    /// XER: `PROJCOST.vendor_name`.
    pub vendor: String,
    /// XER: `PROJCOST.cost_descr`.
    pub description: String,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

impl Expense {
    /// Actual plus remaining cost: the expense's cost at completion.
    #[must_use]
    pub fn at_completion_cost(&self) -> f64 {
        self.actual_cost.unwrap_or(0.0) + self.remaining_cost.unwrap_or(0.0)
    }
}

/// When an expense's cost falls over its activity. XER: `PROJCOST.cost_load_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Accrual {
    /// All at the activity's start. `CL_Start`.
    Start,
    /// All at its finish. `CL_End`.
    End,
    /// Spread evenly over it. `CL_Uniform`.
    #[default]
    Uniform,
}

/// An account costs are booked to. XER: an `ACCOUNT` row.
#[derive(Debug, Clone, PartialEq)]
pub struct CostAccount {
    /// XER: `ACCOUNT.acct_id`.
    pub id: CostAccountId,
    /// Cost account ID shown to users. XER: `ACCOUNT.acct_short_name`.
    pub short_name: String,
    /// XER: `ACCOUNT.acct_name`.
    pub name: String,
    /// Parent in the account hierarchy. XER: `ACCOUNT.parent_acct_id`.
    pub parent: Option<CostAccountId>,
    /// Sort order among siblings. XER: `ACCOUNT.acct_seq_num`.
    pub sequence: Option<i64>,
    /// XER: `ACCOUNT.acct_descr`.
    pub description: String,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A kind of expense, such as travel. XER: a `COSTTYPE` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpenseCategory {
    /// XER: `COSTTYPE.cost_type_id`.
    pub id: ExpenseCategoryId,
    /// XER: `COSTTYPE.cost_type`.
    pub name: String,
    /// Sort order. XER: `COSTTYPE.seq_num`.
    pub sequence: Option<i64>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}
