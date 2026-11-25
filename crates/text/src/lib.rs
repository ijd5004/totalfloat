//! The TotalFloat schedule text format, `.tfs` (ARCHITECTURE.md, Schedule as text).
//!
//! A schedule written as text: one statement per line, with indented details
//! under it, and an activity's predecessors on its own line. [`parse`] reads a file into a [`Document`],
//! reporting every mistake in one pass with its line and column, and
//! [`format`] writes a document back in the canonical layout.
//!
//! The format holds what a planner decides. There are no calculated dates,
//! float or critical flags in it: those always come from the engine.

#![forbid(unsafe_code)]

mod compile;
mod diagnostic;
mod difference;
mod format;
mod lex;
mod parse;
mod render;
#[cfg(test)]
mod tests;

pub use compile::{Compiled, NewProject, Placed, STARTER_CALENDAR, compile};
pub use diagnostic::Diagnostic;
pub use difference::{Difference, Leaving, Missing, difference};
pub use format::format;
pub use parse::parse;
pub use render::{LeftOut, Rendered, render};
pub use totalfloat_edit::{ConstraintKind, Day, ExpenseAccrual, LinkType, ResourceKind, Span};
pub use totalfloat_model::{Date, DateTime};

/// The format version this crate reads and writes.
pub const VERSION: u32 = 1;

/// The first word of a schedule text file, before its version.
pub const HEADER: &str = "totalfloat-schedule";

/// Where something is in the source: a 1-based line and column, and a length,
/// all counted in characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct At {
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
    /// How many characters it covers.
    pub length: usize,
}

/// A value and where it was written.
#[derive(Debug, Clone, PartialEq)]
pub struct Value<T> {
    /// The value.
    pub value: T,
    /// Where it was written.
    pub at: At,
}

impl<T> Value<T> {
    /// A value with no place in a source, for documents built in code.
    pub fn new(value: T) -> Self {
        Self {
            value,
            at: At::default(),
        }
    }
}

/// A word: a code, a name or another identifier, as written (quotes and
/// escapes removed).
pub type Word = Value<String>;

/// An activity's `expense` line.
#[derive(Debug, Clone, PartialEq)]
pub struct Expense {
    /// The expense item (P6 `cost_name`).
    pub item: Word,
    /// Its budgeted cost (P6 `target_cost`).
    pub budget: Value<f64>,
    /// Its actual cost (P6 `act_cost`).
    pub actual: Option<Value<f64>>,
    /// Its remaining cost (P6 `remain_cost`).
    pub remaining: Option<Value<f64>>,
    /// When its cost falls (P6 `cost_load_type`).
    pub accrual: Option<Value<ExpenseAccrual>>,
}

impl Expense {
    /// Budget, actual and remaining cost and accrual, with what's left out
    /// filled in as the line means it.
    #[must_use]
    pub fn meant(&self) -> (f64, f64, f64, ExpenseAccrual) {
        let budget = self.budget.value;
        let actual = self.actual.as_ref().map_or(0.0, |a| a.value);
        let remaining = self
            .remaining
            .as_ref()
            .map_or_else(|| totalfloat_edit::rest_of(budget, actual), |r| r.value);
        let accrual = self
            .accrual
            .as_ref()
            .map_or(ExpenseAccrual::Uniform, |a| a.value);
        (budget, actual, remaining, accrual)
    }
}

/// A whole schedule text file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    /// A comment at the end of the version line.
    pub comment: Option<String>,
    /// The statements, comments and blank lines, in order.
    pub items: Vec<Item>,
}

/// A top-level line of the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// One or more blank lines, kept as one.
    Blank,
    /// A comment on a line of its own.
    Comment(Comment),
    /// `project CODE "Name"`.
    Project(Statement<Project, ProjectDetail>),
    /// `calendar "Name"`.
    Calendar(Statement<Calendar, CalendarDetail>),
    /// `code-type Name`.
    CodeType(Statement<CodeType, CodeValue>),
    /// `field "Name" TYPE`: a user-defined field for activities.
    Field(Statement<Field, Never>),
    /// `resource CODE "Name" [TYPE]`.
    Resource(Statement<Resource, ResourceDetail>),
    /// `role CODE "Name"`.
    Role(Statement<Role, RoleDetail>),
    /// `wbs PATH "Name"`.
    Wbs(Statement<Wbs, Never>),
    /// `activity`, `milestone` or `start-milestone`.
    Activity(Statement<Activity, ActivityDetail>),
}

/// A comment: the text after `#`, trimmed.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    /// The text.
    pub text: String,
    /// Where the `#` was.
    pub at: At,
}

/// A statement line with its details.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement<H, D> {
    /// What the statement line says.
    pub head: H,
    /// Where its keyword was.
    pub at: At,
    /// A comment at the end of the statement line.
    pub comment: Option<String>,
    /// The indented lines under it.
    pub details: Vec<Detail<D>>,
}

impl<H, D> Statement<H, D> {
    /// A statement with no details or comment, for documents built in code.
    pub fn new(head: H) -> Self {
        Self {
            head,
            at: At::default(),
            comment: None,
            details: Vec::new(),
        }
    }
}

/// An indented line under a statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail<D> {
    /// A detail.
    Line {
        /// What it says.
        value: D,
        /// Where its keyword was.
        at: At,
        /// A comment at the end of the line.
        comment: Option<String>,
    },
    /// An indented comment on a line of its own.
    Comment(Comment),
}

impl<D> Detail<D> {
    /// A detail with no place or comment, for documents built in code.
    pub fn new(value: D) -> Self {
        Self::Line {
            value,
            at: At::default(),
            comment: None,
        }
    }
}

/// Details for statements that take none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Never {}

/// `project CODE "Name"`: the project's code (P6 `proj_short_name`) and name.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// The project code.
    pub code: Word,
    /// The project name.
    pub name: Word,
}

/// A project detail.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectDetail {
    /// `data-date DATE TIME` (P6 `last_recalc_date`).
    DataDate(Value<DateTime>),
    /// `must-finish DATE TIME` (P6 `plan_end_date`).
    MustFinish(Value<DateTime>),
    /// `calendar NAME`: the project's default calendar.
    Calendar(Word),
}

/// `calendar NAME [copy-of NAME]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Calendar {
    /// The calendar's name (P6 `clndr_name`).
    pub name: Word,
    /// The calendar it starts as a copy of.
    pub copy_of: Option<Word>,
}

/// A calendar detail.
#[derive(Debug, Clone, PartialEq)]
pub enum CalendarDetail {
    /// `week DAY[-DAY] SHIFT...` or `week DAY[-DAY] off`: the standard week.
    Week {
        /// The first day.
        from: Day,
        /// The last day, the same as `from` for one day.
        to: Day,
        /// The shifts, none for a day off.
        shifts: Vec<Value<String>>,
    },
    /// `holiday DATE`: a non-work day.
    Holiday(Value<Date>),
    /// `exception DATE SHIFT...`: a day with its own working time.
    Exception {
        /// The day.
        date: Value<Date>,
        /// Its shifts.
        shifts: Vec<Value<String>>,
    },
    /// `hours-per-day N`: how many hours a span of one day is on this
    /// calendar (P6 `day_hr_cnt`). Without it, a calendar whose week lines
    /// say every day it works has the same hours counts a day as those
    /// hours; otherwise it keeps what it was copied with.
    HoursPerDay(Value<f64>),
}

/// `code-type NAME`: an activity code type (P6 `actv_code_type`).
#[derive(Debug, Clone, PartialEq)]
pub struct CodeType {
    /// The code type's name.
    pub name: Word,
}

/// `field "Name" TYPE`: a user-defined field for activities, of one of the
/// data types an edit can add.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The field's label.
    pub name: Word,
    /// Its data type.
    pub kind: Value<totalfloat_edit::UdfKind>,
}

/// `value CODE ["Description"]`: one of a code type's values.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeValue {
    /// The value (P6 `short_name`).
    pub value: Word,
    /// Its description (P6 `actv_code_name`).
    pub description: Option<Word>,
}

/// `resource CODE "Name" [TYPE]`: a resource (P6 `RSRC`), labor unless the
/// type says nonlabor or material.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    /// Its short name, the Resource ID (P6 `rsrc_short_name`).
    pub code: Word,
    /// Its name (P6 `rsrc_name`).
    pub name: Word,
    /// `labor`, `nonlabor` or `material` (P6 `rsrc_type`).
    pub kind: Option<Value<ResourceKind>>,
}

/// A resource detail.
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceDetail {
    /// `calendar NAME`: a global or resource calendar (P6 `clndr_id`).
    Calendar(Word),
    /// `units-per-hour N`: the units a new assignment starts with, 1 when
    /// not given (P6 `def_qty_per_hr`).
    UnitsPerHour(Value<f64>),
    /// `max-units-per-hour N`: its limit, the most units per hour it can
    /// work, on every rate (P6 `RSRCRATE.max_qty_per_hr`).
    MaxUnitsPerHour(Value<f64>),
}

/// `role CODE "Name"`: a role, a kind of work staffed before or instead of a
/// named resource (P6 `ROLES`).
#[derive(Debug, Clone, PartialEq)]
pub struct Role {
    /// Its short name, the Role ID (P6 `role_short_name`).
    pub code: Word,
    /// Its name (P6 `role_name`).
    pub name: Word,
}

/// A role detail.
#[derive(Debug, Clone, PartialEq)]
pub enum RoleDetail {
    /// `parent CODE`: the role it sits under, defined above it (P6
    /// `parent_role_id`).
    Parent(Word),
    /// `max-units-per-hour N`: its limit, the most units per hour of it the
    /// work can use, on every rate (P6 `ROLERATE.max_qty_per_hr`).
    MaxUnitsPerHour(Value<f64>),
}

/// `wbs PATH "Name"`: a WBS node, by its dotted path from the project code.
#[derive(Debug, Clone, PartialEq)]
pub struct Wbs {
    /// The dotted path, such as `PLANT.CIV`.
    pub path: Word,
    /// The node's name (P6 `wbs_name`).
    pub name: Word,
}

/// What kind of line an activity statement is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityKind {
    /// `activity`: a task.
    Activity,
    /// `milestone`: a finish milestone.
    Milestone,
    /// `start-milestone`.
    StartMilestone,
}

/// `activity CODE "Name" [SPAN] [after PRED, ...]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    /// Which keyword it used.
    pub kind: ActivityKind,
    /// The activity ID (P6 `task_code`).
    pub code: Word,
    /// The activity name (P6 `task_name`).
    pub name: Word,
    /// The original duration.
    pub duration: Option<Value<Span>>,
    /// Its predecessors.
    pub after: Vec<Predecessor>,
}

/// One predecessor: `A1020`, `A1020 ss`, `A1020 ss+2d` or `A1020 fs-1d`.
#[derive(Debug, Clone, PartialEq)]
pub struct Predecessor {
    /// The predecessor's activity ID, or `PROJECT:ID` in another project.
    pub code: Word,
    /// The link type; finish-to-start when not given.
    pub link: Option<LinkType>,
    /// The lag, negative for a lead.
    pub lag: Option<Span>,
    /// Where the link type and lag were written, if they were.
    pub link_at: Option<At>,
}

/// Whether an activity depends on resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    /// `type task`: task dependent.
    Task,
    /// `type resource`: resource dependent.
    Resource,
}

/// An activity detail.
#[derive(Debug, Clone, PartialEq)]
pub enum ActivityDetail {
    /// `calendar NAME`.
    Calendar(Word),
    /// `type task` or `type resource`.
    Type(TypeKind),
    /// `remaining SPAN`: the remaining duration.
    Remaining(Value<Span>),
    /// A constraint, such as `start-on-or-after 2026-04-06 08:00`, or
    /// `secondary finish-on-or-before ...` for the secondary one.
    Constraint {
        /// Whether it's the secondary constraint.
        secondary: bool,
        /// The kind.
        kind: ConstraintKind,
        /// Its date. Every kind but as late as possible needs one, which
        /// the edit checks.
        date: Option<Value<DateTime>>,
    },
    /// `code TYPE=VALUE`, or `code "Type name" VALUE`.
    Code {
        /// The code type.
        code_type: Word,
        /// The value.
        value: Word,
    },
    /// `udf FIELD=VALUE`, or `udf "Field name" VALUE`: a user-defined
    /// field value.
    Udf {
        /// The field's label.
        field: Word,
        /// The value, as text.
        value: Word,
    },
    /// `resource CODE [units N]`.
    Resource {
        /// The resource (P6 `rsrc_short_name`).
        resource: Word,
        /// Units, in hours for labor and nonlabor and the resource's unit
        /// for material; the resource's default when not given.
        units: Option<Value<f64>>,
    },
    /// `role CODE [units N]`: the role, with no resource yet.
    Role {
        /// The role (P6 `role_short_name`).
        role: Word,
        /// Units, in hours; none when not given.
        units: Option<Value<f64>>,
    },
    /// `expense "ITEM" COST [actual N] [remaining N] [accrual start|end|uniform]`:
    /// a cost no resource carries (P6 `PROJCOST`). Actual cost is 0 and
    /// remaining cost the rest of the budget when not given; accrual is
    /// uniform.
    Expense(Expense),
    /// `actual-start DATE TIME`.
    ActualStart(Value<DateTime>),
    /// `actual-finish DATE TIME`.
    ActualFinish(Value<DateTime>),
    /// `percent N`: physical percent complete, 0 to 100.
    Percent(Value<f64>),
    /// `expected-finish DATE TIME`.
    ExpectedFinish(Value<DateTime>),
}
