//! Typed edits to a [`Schedule`]. Pure: no I/O, and no calculation.
//!
//! An [`Edit`] says what a scheduler would change in a schedule, by the names a
//! scheduler uses. [`apply`] makes the change or refuses it, leaving the
//! schedule as it was. Results stay stale until the engine runs again.
//!
//! Each edit is checked on its own, and a refused one leaves the schedule as it
//! was, so a caller can go on to the next and report every refusal at once.
//! [`Edit::creates`] and [`Edit::needs`] say which refusals follow from another.
//! Callers wanting all or nothing apply a sequence to a copy.

#![forbid(unsafe_code)]

mod commands;
mod expenses;
mod names;
mod notes;
mod obs;
mod project_codes;
mod resolve;
mod span;
mod spreads;
mod steps;
mod udfs;
mod units;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use totalfloat_model::{
    Activity, ActivityCode, ActivityCodeId, ActivityCodeType, ActivityCodeTypeId, ActivityId,
    ActivityStatus, ActivityType, Assignment, AssignmentId, Calendar, CalendarId, CalendarType,
    CodeScope, Constraint, ConstraintType, Date, DateTime, HoursPerPeriod, LagCalendar, Minutes,
    PercentCompleteType, PriceSource, Relationship, RelationshipId, RelationshipType, Resource,
    ResourceId, ResourceRate, ResourceRateId, ResourceType, Role, RoleId, RoleRate, RoleRateId,
    Schedule, TimeOfDay, Wbs, WbsId, Weekday, WorkInterval, rate_in_force,
};

pub use commands::{COMMAND_SCHEMA, COMMAND_SCHEMAS_READ, CommandOptions, Commands};
pub use expenses::{category_named, cost_account_named, cost_account_path, item_key, rest_of};
pub use names::{Name, holds, suggest};
pub use obs::obs_named;
pub use project_codes::DEFAULT_PROJECT_CODE_LENGTH;
pub use resolve::{Missing, NameKind, Resolved, rename, resolve};
pub use span::Span;
pub use spreads::{SpreadPeriod, curve_named};
pub use udfs::{UdfFor, UdfKind, udf_named};

/// One change to a schedule. As JSON, `op` names the edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    /// Change an activity's duration. On work not started, remaining follows
    /// original unless given. On work in progress, each changes only if given.
    SetDuration {
        /// The Activity ID.
        activity: String,
        /// Original duration, such as `15d` or `120h`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original: Option<Span>,
        /// Remaining duration, such as `15d` or `120h`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<Span>,
    },
    /// Set a constraint, primary unless `secondary` is set.
    SetConstraint {
        /// The Activity ID.
        activity: String,
        /// The kind of constraint.
        constraint: ConstraintKind,
        /// The date, as `YYYY-MM-DD HH:MM`. As late as possible takes none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        date: Option<String>,
        /// Set the secondary constraint.
        #[serde(default)]
        secondary: bool,
    },
    /// Remove a constraint, primary unless `secondary` is set.
    ClearConstraint {
        /// The Activity ID.
        activity: String,
        /// Clear the secondary constraint.
        #[serde(default)]
        secondary: bool,
    },
    /// Move an activity to another calendar.
    SetCalendar {
        /// The Activity ID.
        activity: String,
        /// The calendar's name or ID.
        calendar: String,
    },
    /// Rename an activity.
    SetName {
        /// The Activity ID.
        activity: String,
        /// The new name.
        name: String,
    },
    /// Change an activity's type. Only work not started can change, and level
    /// of effort and WBS summary activities can't change here. A milestone's
    /// durations become zero, so turning one into a task needs a duration.
    SetType {
        /// The Activity ID.
        activity: String,
        /// The new type.
        activity_type: Kind,
        /// The duration, when a milestone becomes a task.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<Span>,
    },
    /// Set or clear an expected finish.
    SetExpectedFinish {
        /// The Activity ID.
        activity: String,
        /// The date, as `YYYY-MM-DD HH:MM`. None clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        date: Option<String>,
    },
    /// Record progress: an actual start starts work, and an actual finish
    /// finishes it. Actuals can't be after the data date.
    SetProgress {
        /// The Activity ID.
        activity: String,
        /// Actual start, as `YYYY-MM-DD HH:MM`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actual_start: Option<String>,
        /// Actual finish, as `YYYY-MM-DD HH:MM`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actual_finish: Option<String>,
        /// Remaining duration, such as `5d`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<Span>,
        /// Physical percent complete, 0 to 100. On started, unfinished work
        /// measured by duration percent, given without `remaining`, it sets
        /// remaining duration too (TF-023).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        percent: Option<f64>,
    },
    /// Take away recorded progress, so work is not started again.
    ClearProgress {
        /// The Activity ID.
        activity: String,
    },
    /// Suspend an activity in progress: work stopped on the suspend date and
    /// its remaining work restarts on the resume date (TF-020).
    SetSuspend {
        /// The Activity ID.
        activity: String,
        /// When work stopped, as `YYYY-MM-DD HH:MM`; on or after the actual start.
        suspend: String,
        /// When remaining work restarts, as `YYYY-MM-DD HH:MM`; after the suspend date.
        resume: String,
    },
    /// Take away an activity's suspend and resume dates.
    ClearSuspend {
        /// The Activity ID.
        activity: String,
    },
    /// Add a WBS node under another.
    AddWbs {
        /// The new node's code.
        code: String,
        /// Its name.
        name: String,
        /// The parent node's code or ID.
        parent: String,
    },
    /// Rename a WBS node. Give a new code, a new name or both.
    RenameWbs {
        /// The node's code or ID.
        wbs: String,
        /// The new code.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        /// The new name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Move a WBS node under another.
    MoveWbs {
        /// The node's code or ID.
        wbs: String,
        /// The new parent's code or ID.
        parent: String,
    },
    /// Move an activity to another WBS node.
    SetWbs {
        /// The Activity ID.
        activity: String,
        /// The node's code or ID.
        wbs: String,
    },
    /// Add a resource to the file's resource dictionary.
    AddResource {
        /// Its short name, the Resource ID.
        resource: String,
        /// Its name.
        name: String,
        /// Its type. Default is labor.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource_type: Option<ResourceKind>,
        /// Units per hour a new assignment starts with. Default is 1, a full
        /// day's work each day for labor.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        units_per_hour: Option<f64>,
        /// Its calendar, a global or resource calendar. Default is the default
        /// global calendar.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        calendar: Option<String>,
        /// Its limit: the most units per hour it can work, so 2 is two people
        /// at once. Default is none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_units_per_hour: Option<f64>,
    },
    /// Set or clear a resource's limit, the most units per hour it can work.
    /// Without `from`, every rate the resource has takes it, and a resource
    /// with no rate gets one, with no prices, from the project's start. With `from`,
    /// a rate starts that day with the prices in force then; a rate starting
    /// that day already takes the limit instead. With no rate starting
    /// before that day, a rate from the project's start keeps what held
    /// before (TF-001).
    SetResourceLimit {
        /// The resource's short name or name.
        resource: String,
        /// Units per hour, so 2 is two people at once; null clears it.
        #[serde(deserialize_with = "nullable")]
        #[schemars(required, extend("type" = ["number", "null"]))]
        max_units_per_hour: Option<f64>,
        /// The day the limit starts, as `YYYY-MM-DD`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
    /// Put a resource on an activity. Without units, it takes the resource's
    /// default units per hour over the activity's durations.
    AssignResource {
        /// The Activity ID.
        activity: String,
        /// The resource's short name or name.
        resource: String,
        /// Units, in hours for labor and nonlabor and the resource's unit for
        /// material.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        units: Option<f64>,
    },
    /// Set an assignment's actual or remaining units. Its remaining units per
    /// hour and a cost linked to its units follow. On an activity measured by
    /// units percent, the percent follows (docs/rules/TF-004-units-percent-complete.md).
    SetAssignmentUnits {
        /// The Activity ID.
        activity: String,
        /// The resource's short name or name.
        resource: String,
        /// Actual units, regular time: hours for labor and nonlabor, the
        /// resource's unit for material.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actual_units: Option<f64>,
        /// Remaining units.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining_units: Option<f64>,
    },
    /// Add a role to the file's role dictionary.
    AddRole {
        /// Its short name, the Role ID.
        role: String,
        /// Its name.
        name: String,
        /// The role it sits under, by short name or name. Default is none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        /// Its limit: the most units per hour of it the work can use, so 2
        /// is two people at once. Default is none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_units_per_hour: Option<f64>,
    },
    /// Set or clear a role's limit, the most units per hour of it the work
    /// can use, as `set_resource_limit` does for a resource (TF-001).
    SetRoleLimit {
        /// The role's short name or name.
        role: String,
        /// Units per hour, so 2 is two people at once; null clears it.
        #[serde(deserialize_with = "nullable")]
        #[schemars(required, extend("type" = ["number", "null"]))]
        max_units_per_hour: Option<f64>,
        /// The day the limit starts, as `YYYY-MM-DD`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
    /// Put a role on an activity with no resource yet, so the
    /// work is staffed by kind before anyone is named.
    AssignRole {
        /// The Activity ID.
        activity: String,
        /// The role's short name or name.
        role: String,
        /// Units, in hours. Without them the assignment has none yet.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        units: Option<f64>,
    },
    /// Take a role with no resource off an activity.
    UnassignRole {
        /// The Activity ID.
        activity: String,
        /// The role's short name or name.
        role: String,
    },
    /// Add an expense to an activity: one unit at the budgeted cost, all of
    /// it still to spend.
    AddExpense {
        /// The Activity ID.
        activity: String,
        /// The expense item, unique on the activity, ignoring case and spaces
        /// at the ends.
        expense: String,
        /// Its budgeted cost, negative for a credit.
        cost: f64,
        /// Its cost account, by short name or name. Default is none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
        /// Its expense category, by name. Default is none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        category: Option<String>,
        /// When its cost falls over the activity. Default is uniform.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accrual: Option<ExpenseAccrual>,
    },
    /// Change an expense's costs or accrual. Those left out stay as they are,
    /// but a new budget on an expense with nothing spent moves its remaining
    /// cost too.
    SetExpense {
        /// The Activity ID.
        activity: String,
        /// The expense item.
        expense: String,
        /// Its budgeted cost.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        budgeted_cost: Option<f64>,
        /// Its actual cost.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actual_cost: Option<f64>,
        /// Its remaining cost.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining_cost: Option<f64>,
        /// When its cost falls over the activity.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accrual: Option<ExpenseAccrual>,
    },
    /// Remove an expense from an activity.
    DeleteExpense {
        /// The Activity ID.
        activity: String,
        /// The expense item.
        expense: String,
    },
    /// Book the costs of a resource's assignment, a role's assignment with
    /// no resource, or an expense on an activity to a cost account, or to
    /// none. Give one of `resource`, `role` or `expense`.
    SetCostAccount {
        /// The Activity ID.
        activity: String,
        /// The resource whose assignment it is, by short name or name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource: Option<String>,
        /// The role of an assignment with no resource, by short name or name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<String>,
        /// The expense item.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expense: Option<String>,
        /// The cost account, by its path (short names from the top of the
        /// tree down, joined by dots, as `SITE.PERMIT`), or by short name or
        /// name where one account has it. Without one, the costs are booked to
        /// none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
    /// Take a resource off an activity.
    UnassignResource {
        /// The Activity ID.
        activity: String,
        /// The resource's short name or name.
        resource: String,
    },
    /// Spread a resource's units on an activity by a resource curve, or
    /// evenly over its work without one. Clears any typed spread.
    SetCurve {
        /// The Activity ID.
        activity: String,
        /// The resource's short name or name.
        resource: String,
        /// The curve's name, one of the file's. Without it, units spread
        /// evenly.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        curve: Option<String>,
    },
    /// Type a resource's remaining units on an activity for each period, in
    /// place of a curve. The periods follow one another from `start`, must
    /// cover the remaining work between the activity's remaining dates, and
    /// must add up to the remaining units.
    SetSpread {
        /// The Activity ID.
        activity: String,
        /// The resource's short name or name.
        resource: String,
        /// Days, weeks or calendar months.
        period: SpreadPeriod,
        /// The first period's first day, as `YYYY-MM-DD`; a month's is the
        /// first of the month.
        start: String,
        /// The units in each period, in order: hours for labor and nonlabor.
        units: Vec<f64>,
    },
    /// Give an activity a code value, replacing any it holds of that type.
    SetCode {
        /// The Activity ID.
        activity: String,
        /// The code type's name.
        code_type: String,
        /// The value, or its description.
        value: String,
    },
    /// Take a code off an activity.
    ClearCode {
        /// The Activity ID.
        activity: String,
        /// The code type's name.
        code_type: String,
    },
    /// Add an activity code type.
    AddCodeType {
        /// Its name.
        name: String,
        /// Where it applies. Default is this project only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<Scope>,
        /// The longest value it holds, in characters. Default is 12, the
        /// length most code types in real files have.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
    },
    /// Add a value to a code type.
    AddCodeValue {
        /// The code type's name.
        code_type: String,
        /// The new value.
        value: String,
        /// What it means. Default is the value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Give an activity, WBS node, resource or assignment a user-defined
    /// field value, checked against the field's data type. Name an activity,
    /// a WBS node or a resource, or an activity and a resource for their
    /// assignment.
    SetUdf {
        /// The field's label or `udf_type_id`.
        field: String,
        /// The value as text: `12.5`, `2026-03-02 08:00`, `UDF_G2` (or `2`)
        /// for an indicator, or any text.
        value: String,
        /// The Activity ID, for an activity's or assignment's value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity: Option<String>,
        /// The WBS code or `wbs_id`, for a WBS node's value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wbs: Option<String>,
        /// The resource's short name or name, for a resource's or
        /// assignment's value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource: Option<String>,
    },
    /// Take a user-defined field value off a record, named as for `set_udf`.
    ClearUdf {
        /// The field's label or `udf_type_id`.
        field: String,
        /// The Activity ID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity: Option<String>,
        /// The WBS code or `wbs_id`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wbs: Option<String>,
        /// The resource's short name or name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resource: Option<String>,
    },
    /// Add a user-defined field. Fields belong to every project in the file.
    AddUdfType {
        /// Its label.
        field: String,
        /// What it applies to.
        #[serde(rename = "for")]
        applies_to: UdfFor,
        /// Its data type.
        data_type: UdfKind,
    },
    /// Add a step to an activity. Where the project measures physical percent
    /// by steps, the activity's physical percent follows (TF-042).
    AddStep {
        /// The Activity ID.
        activity: String,
        /// The step's name.
        step: String,
        /// Its weight against the activity's other steps. Default is 1.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weight: Option<f64>,
        /// Its percent complete, 0 to 100. Default is 0.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        percent: Option<f64>,
    },
    /// Change a step's name, weight or percent complete; 100 marks it
    /// complete, and less marks it not.
    SetStep {
        /// The Activity ID.
        activity: String,
        /// The step's name or `proc_id`.
        step: String,
        /// A new name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// A new weight.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weight: Option<f64>,
        /// A new percent complete, 0 to 100.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        percent: Option<f64>,
    },
    /// Mark a step complete: 100 percent.
    CompleteStep {
        /// The Activity ID.
        activity: String,
        /// The step's name or `proc_id`.
        step: String,
    },
    /// Delete a step.
    DeleteStep {
        /// The Activity ID.
        activity: String,
        /// The step's name or `proc_id`.
        step: String,
    },
    /// Add a note to an activity or a WBS node, under a notebook topic. The
    /// text becomes simple HTML, a paragraph a line, as P6 stores notes.
    AddNote {
        /// The Activity ID, for a note on an activity.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity: Option<String>,
        /// The WBS code or `wbs_id`, for a note on a WBS node.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wbs: Option<String>,
        /// The notebook topic's name or `memo_type_id`.
        topic: String,
        /// The note, as plain text.
        text: String,
    },
    /// Replace an activity's or WBS node's note under a topic.
    SetNote {
        /// The Activity ID, for a note on an activity.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity: Option<String>,
        /// The WBS code or `wbs_id`, for a note on a WBS node.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wbs: Option<String>,
        /// The notebook topic's name or `memo_type_id`.
        topic: String,
        /// The note, as plain text.
        text: String,
    },
    /// Delete an activity's or WBS node's note under a topic.
    DeleteNote {
        /// The Activity ID, for a note on an activity.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity: Option<String>,
        /// The WBS code or `wbs_id`, for a note on a WBS node.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wbs: Option<String>,
        /// The notebook topic's name or `memo_type_id`.
        topic: String,
    },
    /// Add a node to the OBS, the structure of who is responsible for work.
    /// The OBS belongs to every project in the file.
    AddObs {
        /// Its name.
        obs: String,
        /// The node it sits under, by name or `obs_id`. Default is the top.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        /// What it is. Default is none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Make an OBS node a WBS node's responsible manager.
    SetWbsObs {
        /// The WBS node's code or `wbs_id`.
        wbs: String,
        /// The OBS node's name or `obs_id`.
        obs: String,
    },
    /// Give the project a project code value, replacing any it holds of that
    /// type.
    SetProjectCode {
        /// The project code type's name.
        code_type: String,
        /// The value, or its description.
        value: String,
    },
    /// Take a project code off the project.
    ClearProjectCode {
        /// The project code type's name.
        code_type: String,
    },
    /// Add a project code type. Project codes belong to every project in the
    /// file.
    AddProjectCodeType {
        /// Its name.
        name: String,
        /// The longest value it holds, in characters. Default is 20, the
        /// length most project code types in real files have.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
    },
    /// Add a value to a project code type.
    AddProjectCodeValue {
        /// The project code type's name.
        code_type: String,
        /// The new value.
        value: String,
        /// What it means. Default is the value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// Add a project calendar, copying another's working time.
    AddCalendar {
        /// The new calendar's name.
        name: String,
        /// The calendar to copy. Default is the project's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        copy_of: Option<String>,
    },
    /// Make a date a holiday, or give it its own shifts.
    AddCalendarException {
        /// The calendar's name or ID.
        calendar: String,
        /// The date, as `YYYY-MM-DD`.
        date: String,
        /// Shifts such as `08:00-12:00`. None makes the date a non-work day.
        #[serde(default)]
        shifts: Vec<String>,
    },
    /// Put a date back to the normal week.
    RemoveCalendarException {
        /// The calendar's name or ID.
        calendar: String,
        /// The date, as `YYYY-MM-DD`.
        date: String,
    },
    /// Change one day of the work week.
    SetWorkWeek {
        /// The calendar's name or ID.
        calendar: String,
        /// The day of the week.
        day: Day,
        /// Shifts such as `08:00-12:00`. None makes the day non-work.
        #[serde(default)]
        shifts: Vec<String>,
    },
    /// Set how many hours a day is on a calendar, for spans given in days
    /// (P6 `day_hr_cnt`). Changing the work week leaves this alone
    /// (TF-033).
    SetHoursPerDay {
        /// The calendar's name or ID.
        calendar: String,
        /// Hours in a day, such as `8` or `7.5`, in whole minutes.
        hours: f64,
    },
    /// Set a calendar's hours in a week, month or year (P6 `week_hr_cnt`,
    /// `month_hr_cnt` and `year_hr_cnt`): what a span in those units means
    /// on it.
    SetHoursPerPeriod {
        /// The calendar's name or ID.
        calendar: String,
        /// Which period.
        period: Period,
        /// Hours in it, such as `40` or `172`, in whole minutes.
        hours: f64,
    },
    /// Add an activity. Work needs a duration, and a milestone has none.
    AddActivity {
        /// The new Activity ID.
        code: String,
        /// Its name.
        name: String,
        /// Its WBS, by the code `query` shows.
        wbs: String,
        /// Its type. Default is task.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activity_type: Option<Kind>,
        /// Its calendar. Default is the project's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        calendar: Option<String>,
        /// Its duration, such as `5d` or `40h`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<Span>,
    },
    /// Delete an activity, with its links and assignments.
    DeleteActivity {
        /// The Activity ID.
        activity: String,
    },
    /// Add a relationship.
    AddLink {
        /// The predecessor's Activity ID.
        predecessor: String,
        /// The successor's Activity ID.
        successor: String,
        /// The relationship type. Default is finish to start.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        link_type: Option<LinkType>,
        /// Lag, such as `5d` or `-8h`. Default is none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lag: Option<Span>,
    },
    /// Remove a relationship. `link_type` is needed only if two activities have
    /// more than one.
    RemoveLink {
        /// The predecessor's Activity ID.
        predecessor: String,
        /// The successor's Activity ID.
        successor: String,
        /// Which relationship, if there are several.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        link_type: Option<LinkType>,
    },
    /// Change a relationship's type or lag. `link_type` says which relationship
    /// if two activities have more than one.
    ChangeLink {
        /// The predecessor's Activity ID.
        predecessor: String,
        /// The successor's Activity ID.
        successor: String,
        /// Which relationship, if there are several.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        link_type: Option<LinkType>,
        /// The new type.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_type: Option<LinkType>,
        /// The new lag.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lag: Option<Span>,
    },
}

/// Days of the week.
#[allow(missing_docs, reason = "each name is the day itself")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Day {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl From<Day> for Weekday {
    fn from(day: Day) -> Self {
        match day {
            Day::Monday => Self::Monday,
            Day::Tuesday => Self::Tuesday,
            Day::Wednesday => Self::Wednesday,
            Day::Thursday => Self::Thursday,
            Day::Friday => Self::Friday,
            Day::Saturday => Self::Saturday,
            Day::Sunday => Self::Sunday,
        }
    }
}

/// Where a code type applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// One project. P6 `AS_Project`.
    Project,
    /// Every project. P6 `AS_Global`.
    Global,
}

/// When an expense's cost falls over its activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExpenseAccrual {
    /// All at the activity's start. P6 `CL_Start`.
    Start,
    /// All at its finish. P6 `CL_End`.
    End,
    /// Spread evenly over it. P6 `CL_Uniform`.
    Uniform,
}

impl ExpenseAccrual {
    /// Every accrual, with the word flags and text use for it.
    pub const WORDS: [(&'static str, Self); 3] = [
        ("start", Self::Start),
        ("end", Self::End),
        ("uniform", Self::Uniform),
    ];

    /// The word for it.
    #[must_use]
    pub fn word(self) -> &'static str {
        Self::WORDS
            .iter()
            .find(|(_, a)| *a == self)
            .map_or("uniform", |(w, _)| w)
    }

    /// The accrual a word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::WORDS
            .iter()
            .find(|(w, _)| *w == word)
            .map(|(_, a)| *a)
    }
}

/// Resource types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    /// People. P6 `RT_Labor`.
    Labor,
    /// Equipment. P6 `RT_Equip`.
    Nonlabor,
    /// Material. P6 `RT_Mat`.
    Material,
}

impl From<ResourceKind> for ResourceType {
    fn from(kind: ResourceKind) -> Self {
        match kind {
            ResourceKind::Labor => Self::Labor,
            ResourceKind::Nonlabor => Self::Nonlabor,
            ResourceKind::Material => Self::Material,
        }
    }
}

/// The kinds of constraint, as XER files name them.
#[allow(missing_docs, reason = "each name is the constraint itself")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintKind {
    StartOn,
    StartOnOrAfter,
    StartOnOrBefore,
    FinishOn,
    FinishOnOrAfter,
    FinishOnOrBefore,
    AsLateAsPossible,
    MandatoryStart,
    MandatoryFinish,
}

impl From<ConstraintType> for ConstraintKind {
    fn from(kind: ConstraintType) -> Self {
        match kind {
            ConstraintType::StartOn => Self::StartOn,
            ConstraintType::StartOnOrAfter => Self::StartOnOrAfter,
            ConstraintType::StartOnOrBefore => Self::StartOnOrBefore,
            ConstraintType::FinishOn => Self::FinishOn,
            ConstraintType::FinishOnOrAfter => Self::FinishOnOrAfter,
            ConstraintType::FinishOnOrBefore => Self::FinishOnOrBefore,
            ConstraintType::AsLateAsPossible => Self::AsLateAsPossible,
            ConstraintType::MandatoryStart => Self::MandatoryStart,
            ConstraintType::MandatoryFinish => Self::MandatoryFinish,
        }
    }
}

impl From<ConstraintKind> for ConstraintType {
    fn from(kind: ConstraintKind) -> Self {
        match kind {
            ConstraintKind::StartOn => Self::StartOn,
            ConstraintKind::StartOnOrAfter => Self::StartOnOrAfter,
            ConstraintKind::StartOnOrBefore => Self::StartOnOrBefore,
            ConstraintKind::FinishOn => Self::FinishOn,
            ConstraintKind::FinishOnOrAfter => Self::FinishOnOrAfter,
            ConstraintKind::FinishOnOrBefore => Self::FinishOnOrBefore,
            ConstraintKind::AsLateAsPossible => Self::AsLateAsPossible,
            ConstraintKind::MandatoryStart => Self::MandatoryStart,
            ConstraintKind::MandatoryFinish => Self::MandatoryFinish,
        }
    }
}

/// The activity types an edit can give, as `query` names them.
#[allow(missing_docs, reason = "each name is the type itself")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Task,
    Resource,
    StartMilestone,
    FinishMilestone,
}

impl From<Kind> for ActivityType {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Task => Self::TaskDependent,
            Kind::Resource => Self::ResourceDependent,
            Kind::StartMilestone => Self::StartMilestone,
            Kind::FinishMilestone => Self::FinishMilestone,
        }
    }
}

/// Relationship types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LinkType {
    /// Finish to start.
    #[serde(alias = "fs")]
    FS,
    /// Start to start.
    #[serde(alias = "ss")]
    SS,
    /// Finish to finish.
    #[serde(alias = "ff")]
    FF,
    /// Start to finish.
    #[serde(alias = "sf")]
    SF,
}

impl From<LinkType> for RelationshipType {
    fn from(kind: LinkType) -> Self {
        match kind {
            LinkType::FS => Self::FinishToStart,
            LinkType::SS => Self::StartToStart,
            LinkType::FF => Self::FinishToFinish,
            LinkType::SF => Self::StartToFinish,
        }
    }
}

const fn link_label(kind: RelationshipType) -> &'static str {
    match kind {
        RelationshipType::FinishToStart => "FS",
        RelationshipType::StartToStart => "SS",
        RelationshipType::FinishToFinish => "FF",
        RelationshipType::StartToFinish => "SF",
    }
}

/// Why an edit was refused. The schedule is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct EditError {
    /// Stable code, such as `no-such-activity`.
    pub code: &'static str,
    /// What's wrong.
    pub message: String,
    /// What to do about it.
    pub hint: String,
}

/// A field that has to be given, as a value or null.
fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

fn refuse<T>(
    code: &'static str,
    message: impl Into<String>,
    hint: impl Into<String>,
) -> Result<T, EditError> {
    Err(EditError {
        code,
        message: message.into(),
        hint: hint.into(),
    })
}

/// IDs for new rows, which must be above every row in the file, including
/// rows of other projects and rows the model left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextIds {
    /// The next `TASKPRED.task_pred_id`.
    pub relationship: u64,
    /// The next `TASK.task_id`.
    pub activity: u64,
    /// The next `CALENDAR.clndr_id`.
    pub calendar: u64,
    /// The next `ACTVCODE.actv_code_id`.
    pub code: u64,
    /// The next `TASKRSRC.taskrsrc_id`.
    pub assignment: u64,
    /// The next `PROJWBS.wbs_id`.
    pub wbs: u64,
    /// The next `ACTVTYPE.actv_code_type_id`.
    pub code_type: u64,
    /// The next `RSRC.rsrc_id`.
    pub resource: u64,
    /// The next `RSRCRATE.rsrc_rate_id`.
    pub resource_rate: u64,
    /// The next `ROLES.role_id`.
    pub role: u64,
    /// The next `ROLERATE.role_rate_id`.
    pub role_rate: u64,
    /// The next `PROJCOST.cost_item_id`.
    pub expense: u64,
    /// The next `PCATTYPE.proj_catg_type_id`.
    pub project_code_type: u64,
    /// The next `PCATVAL.proj_catg_id`.
    pub project_code: u64,
    /// The next `OBS.obs_id`.
    pub obs: u64,
    /// The next `TASKMEMO.memo_id`.
    pub activity_note: u64,
    /// The next `WBSMEMO.wbs_memo_id`.
    pub wbs_note: u64,
    /// The next `TASKPROC.proc_id`.
    pub step: u64,
    /// The next `UDFTYPE.udf_type_id`.
    pub udf_type: u64,
}

/// The Activity IDs an edit names, so a caller can find its project.
#[must_use]
pub fn activities_named(edit: &Edit) -> Vec<&str> {
    match edit {
        Edit::SetDuration { activity, .. }
        | Edit::SetConstraint { activity, .. }
        | Edit::ClearConstraint { activity, .. }
        | Edit::SetCalendar { activity, .. }
        | Edit::SetName { activity, .. }
        | Edit::SetType { activity, .. }
        | Edit::SetExpectedFinish { activity, .. }
        | Edit::SetProgress { activity, .. }
        | Edit::ClearProgress { activity }
        | Edit::SetSuspend { activity, .. }
        | Edit::ClearSuspend { activity }
        | Edit::SetCode { activity, .. }
        | Edit::ClearCode { activity, .. }
        | Edit::AssignResource { activity, .. }
        | Edit::AssignRole { activity, .. }
        | Edit::UnassignRole { activity, .. }
        | Edit::AddExpense { activity, .. }
        | Edit::SetExpense { activity, .. }
        | Edit::DeleteExpense { activity, .. }
        | Edit::SetCostAccount { activity, .. }
        | Edit::UnassignResource { activity, .. }
        | Edit::SetAssignmentUnits { activity, .. }
        | Edit::SetCurve { activity, .. }
        | Edit::SetSpread { activity, .. }
        | Edit::SetWbs { activity, .. }
        | Edit::AddStep { activity, .. }
        | Edit::SetStep { activity, .. }
        | Edit::CompleteStep { activity, .. }
        | Edit::DeleteStep { activity, .. }
        | Edit::DeleteActivity { activity } => vec![activity],
        Edit::AddNote { activity, .. }
        | Edit::SetNote { activity, .. }
        | Edit::DeleteNote { activity, .. }
        | Edit::SetUdf { activity, .. }
        | Edit::ClearUdf { activity, .. } => activity.iter().map(String::as_str).collect(),
        Edit::AddUdfType { .. }
        | Edit::AddActivity { .. }
        | Edit::AddWbs { .. }
        | Edit::RenameWbs { .. }
        | Edit::MoveWbs { .. }
        | Edit::AddCodeType { .. }
        | Edit::AddCodeValue { .. }
        | Edit::AddObs { .. }
        | Edit::SetWbsObs { .. }
        | Edit::SetProjectCode { .. }
        | Edit::ClearProjectCode { .. }
        | Edit::AddProjectCodeType { .. }
        | Edit::AddProjectCodeValue { .. }
        | Edit::AddResource { .. }
        | Edit::SetResourceLimit { .. }
        | Edit::AddRole { .. }
        | Edit::SetRoleLimit { .. }
        | Edit::AddCalendar { .. }
        | Edit::AddCalendarException { .. }
        | Edit::RemoveCalendarException { .. }
        | Edit::SetWorkWeek { .. }
        | Edit::SetHoursPerDay { .. }
        | Edit::SetHoursPerPeriod { .. } => Vec::new(),
        Edit::AddLink {
            predecessor,
            successor,
            ..
        }
        | Edit::RemoveLink {
            predecessor,
            successor,
            ..
        }
        | Edit::ChangeLink {
            predecessor,
            successor,
            ..
        } => vec![predecessor, successor],
    }
}

/// The calendar an edit changes, if it changes one.
#[must_use]
pub fn calendar_changed(edit: &Edit) -> Option<&str> {
    match edit {
        Edit::AddCalendarException { calendar, .. }
        | Edit::RemoveCalendarException { calendar, .. }
        | Edit::SetWorkWeek { calendar, .. }
        | Edit::SetHoursPerDay { calendar, .. }
        | Edit::SetHoursPerPeriod { calendar, .. } => Some(calendar),
        _ => None,
    }
}

/// The calendars this name or ID could mean here, of any kind.
#[must_use]
pub fn calendars_named(schedule: &Schedule, wanted: &str) -> Vec<CalendarId> {
    let by_id = wanted
        .parse()
        .ok()
        .map(CalendarId)
        .filter(|id| schedule.calendars.contains_key(id));
    if let Some(id) = by_id {
        return vec![id];
    }
    let lower = wanted.to_lowercase();
    schedule
        .calendars
        .values()
        .filter(|c| c.name.to_lowercase() == lower)
        .map(|c| c.id)
        .collect()
}

fn calendar_named(schedule: &Schedule, wanted: &str) -> Option<CalendarId> {
    match calendars_named(schedule, wanted).as_slice() {
        [one] => Some(*one),
        _ => None,
    }
}

fn find(schedule: &Schedule, code: &str) -> Result<ActivityId, EditError> {
    find_ref(schedule, code).map(|activity| activity.id)
}

fn find_ref<'a>(schedule: &'a Schedule, code: &str) -> Result<&'a Activity, EditError> {
    match schedule.activity_by_code(code) {
        Some(activity) => Ok(activity),
        None => refuse(
            "no-such-activity",
            format!("no activity has the ID {code}"),
            "use the Activity ID, as `query` shows it in `code`; a link to another project names it PROJECT:CODE",
        ),
    }
}

fn find_mut<'a>(schedule: &'a mut Schedule, code: &str) -> Result<&'a mut Activity, EditError> {
    let id = find(schedule, code)?;
    match schedule.activities.get_mut(&id) {
        Some(activity) => Ok(activity),
        None => refuse(
            "no-such-activity",
            format!("no activity has the ID {code}"),
            "use the Activity ID, as `query` shows it in `code`",
        ),
    }
}

/// A span as a scheduler reads it.
#[allow(
    clippy::cast_precision_loss,
    reason = "display hours; minutes are far below 2^52"
)]
fn hours(span: Minutes) -> String {
    format!("{}h", (span.get() as f64 / 60.0 * 100.0).round() / 100.0)
}

fn parse_date(text: &str) -> Result<DateTime, EditError> {
    match text.parse() {
        Ok(date) => Ok(date),
        Err(_) => refuse(
            "invalid-date",
            format!("{text} isn't a valid date"),
            "use the form YYYY-MM-DD HH:MM",
        ),
    }
}

/// Minutes in a day on this calendar, for a span given in days: its hours
/// per day, or the length of its working days when they're all the same.
/// `None` if neither is known.
#[must_use]
pub fn day_length(schedule: &Schedule, calendar: CalendarId) -> Option<Minutes> {
    day_of(schedule, Some(calendar))
}

/// Minutes in a day for a lag given in days on a link between these two
/// activities of `schedule`, by the project's lag calendar option.
#[must_use]
pub fn lag_day_length(
    schedule: &Schedule,
    predecessor: &Activity,
    successor: &Activity,
) -> Option<Minutes> {
    lag_day(schedule, predecessor, successor)
}

/// The lengths a lag given in days, weeks, months or years is counted in on
/// a link between these two activities of `schedule`, by the project's lag
/// calendar option, as the link edits count it.
#[must_use]
pub fn lag_lengths_for(
    schedule: &Schedule,
    predecessor: &Activity,
    successor: &Activity,
) -> Option<HoursPerPeriod> {
    lag_lengths(schedule, predecessor, successor)
}

/// Hours in a day on this calendar, for a span given in days.
fn day_of(schedule: &Schedule, calendar: Option<CalendarId>) -> Option<Minutes> {
    lengths_of(schedule, calendar).and_then(|lengths| lengths.day)
}

/// The length of a working day, if every working day in the week has the same.
#[must_use]
pub fn even_day(calendar: &Calendar) -> Option<Minutes> {
    let mut lengths = Weekday::ALL
        .iter()
        .map(|day| {
            let shifts = calendar.work_week.day(*day);
            shifts
                .iter()
                .map(|shift| shift.duration().get())
                .sum::<i64>()
        })
        .filter(|length| *length > 0);
    let first = lengths.next()?;
    lengths
        .all(|length| length == first)
        .then(|| Minutes::new(first))
}

/// Hours in a day for this activity. Resource-dependent work runs on its
/// resources' calendars, which an activity's own calendar doesn't speak for.
/// Minutes in a day for a duration of `activity` given in days: its
/// calendar's day, or `None` where days can't be used, as for resource
/// dependent work, whose duration comes from its resources' calendars.
#[must_use]
pub fn duration_day_length(schedule: &Schedule, activity: &Activity) -> Option<Minutes> {
    day_for(schedule, activity)
}

fn day_for(schedule: &Schedule, activity: &Activity) -> Option<Minutes> {
    lengths_for(schedule, activity).and_then(|lengths| lengths.day)
}

/// A calendar's hours in a day, week, month and year, for spans in those
/// units: its own, with a day of even working days where it gives none.
fn lengths_of(schedule: &Schedule, calendar: Option<CalendarId>) -> Option<HoursPerPeriod> {
    let calendar = schedule.calendars.get(&calendar?)?;
    let mut lengths = calendar.hours_per_period;
    lengths.day = lengths.day.or_else(|| even_day(calendar));
    Some(lengths)
}

/// [`lengths_of`] for a duration of `activity`; `None` for resource
/// dependent work, whose duration comes from its resources' calendars.
fn lengths_for(schedule: &Schedule, activity: &Activity) -> Option<HoursPerPeriod> {
    (activity.activity_type != ActivityType::ResourceDependent)
        .then(|| lengths_of(schedule, Some(activity.calendar)))
        .flatten()
}

/// The hours in a day, week, month and year for a duration of `activity`,
/// as a span in those units needs them.
#[must_use]
pub fn duration_lengths(schedule: &Schedule, activity: &Activity) -> Option<HoursPerPeriod> {
    lengths_for(schedule, activity)
}

/// Why a span can't be used for `code`: its calendar doesn't say how long
/// its unit is.
#[must_use]
pub fn no_length(code: &str, span: Span) -> EditError {
    match span.period() {
        None | Some("day") => no_day(code),
        Some(period) => EditError {
            code: "no-hours-per-period",
            message: format!("there's no number of hours in a {period} for {code}"),
            hint: format!(
                "give the span in hours or days, or set the calendar's hours per {period}"
            ),
        },
    }
}

fn no_day(code: &str) -> EditError {
    EditError {
        code: "no-hours-per-day",
        message: format!("there's no fixed length of day for {code}"),
        hint: "give the span in hours".to_owned(),
    }
}

fn set_duration(
    schedule: &mut Schedule,
    code: &str,
    original: Option<Span>,
    remaining: Option<Span>,
) -> Result<String, EditError> {
    let id = find(schedule, code)?;
    if original.is_none() && remaining.is_none() {
        return refuse(
            "nothing-to-change",
            format!("the edit to {code} gives no duration"),
            "give original, remaining or both",
        );
    }
    let a = &schedule.activities[&id];
    let (kind, status, lengths) = (a.activity_type, a.status, lengths_for(schedule, a));
    let (was_original, was_remaining) = (a.original_duration, a.remaining_duration);
    match kind {
        ActivityType::StartMilestone | ActivityType::FinishMilestone => {
            return refuse(
                "no-duration",
                format!("{code} is a milestone, which has no duration"),
                "change its type first, or move what it depends on",
            );
        }
        ActivityType::LevelOfEffort | ActivityType::WbsSummary => {
            return refuse(
                "no-duration",
                format!("{code}'s duration comes from the activities it spans"),
                "change those activities instead",
            );
        }
        ActivityType::TaskDependent | ActivityType::ResourceDependent => {}
    }
    if status == ActivityStatus::Completed && remaining.is_some() {
        return refuse(
            "finished-work",
            format!("{code} is finished, so it has no remaining duration"),
            "only its original duration can change",
        );
    }
    let span = |span: Span| -> Result<Minutes, EditError> {
        let minutes = span
            .minutes_in(lengths.as_ref())
            .ok_or_else(|| no_length(code, span))?;
        if minutes.is_negative() {
            return refuse(
                "negative-duration",
                format!("{code} can't have a negative duration"),
                "give zero or more",
            );
        }
        Ok(minutes)
    };
    let new_original = original.map(span).transpose()?;
    let new_remaining = remaining.map(span).transpose()?;
    if status == ActivityStatus::InProgress && new_remaining == Some(Minutes::ZERO) {
        return refuse(
            "finished-work",
            format!("{code} is in progress, so it needs remaining work"),
            "record an actual finish to finish it",
        );
    }
    let follows = status == ActivityStatus::NotStarted;
    let a = find_mut(schedule, code)?;
    if let Some(value) = new_original {
        a.original_duration = value;
        if follows && new_remaining.is_none() {
            a.remaining_duration = value;
        }
    }
    if let Some(value) = new_remaining {
        a.remaining_duration = value;
    }
    Ok(format!(
        "{code} duration: original {} to {}, remaining {} to {}",
        hours(was_original),
        hours(a.original_duration),
        hours(was_remaining),
        hours(a.remaining_duration)
    ))
}

fn set_constraint(
    schedule: &mut Schedule,
    code: &str,
    kind: ConstraintType,
    date: Option<&str>,
    secondary: bool,
) -> Result<String, EditError> {
    let date = date.map(parse_date).transpose()?;
    if kind.requires_date() && date.is_none() {
        return refuse(
            "constraint-needs-date",
            format!("that constraint on {code} needs a date"),
            "give a date as YYYY-MM-DD HH:MM",
        );
    }
    if !kind.requires_date() && date.is_some() {
        return refuse(
            "constraint-takes-no-date",
            "as late as possible takes no date",
            "leave the date out",
        );
    }
    let a = find_mut(schedule, code)?;
    if a.activity_type.is_span() {
        return refuse(
            "no-constraint-here",
            format!("{code} spans other activities, so a constraint would do nothing"),
            "constrain the activities it spans",
        );
    }
    if a.status == ActivityStatus::Completed {
        return refuse(
            "finished-work",
            format!("{code} is finished, so a constraint would do nothing"),
            "constrain the work still to come",
        );
    }
    let clash = |primary: ConstraintType, second: ConstraintType| {
        refuse(
            "constraints-clash",
            format!("{code} can't hold those two constraints together"),
            format!(
                "{primary:?} can't take a secondary constraint, or {second:?} can't be one; clear the other first"
            ),
        )
    };
    let constraint = Some(Constraint { kind, date });
    if secondary {
        let Some(primary) = a.primary_constraint else {
            return refuse(
                "no-primary-constraint",
                format!("{code} has no primary constraint"),
                "set a primary constraint first",
            );
        };
        if !primary.kind.allows_secondary() || !kind.can_be_secondary() {
            return clash(primary.kind, kind);
        }
        a.secondary_constraint = constraint;
    } else {
        if let Some(second) = a.secondary_constraint
            && !kind.allows_secondary()
        {
            return clash(kind, second.kind);
        }
        a.primary_constraint = constraint;
    }
    let which = if secondary { "secondary" } else { "primary" };
    Ok(format!("{code} {which} constraint set"))
}

fn clear_constraint(
    schedule: &mut Schedule,
    code: &str,
    secondary: bool,
) -> Result<String, EditError> {
    let a = find_mut(schedule, code)?;
    if !secondary && a.secondary_constraint.is_some() {
        return refuse(
            "secondary-constraint-remains",
            format!("{code} has a secondary constraint, which can't stand alone"),
            "clear the secondary first",
        );
    }
    let slot = if secondary {
        &mut a.secondary_constraint
    } else {
        &mut a.primary_constraint
    };
    if slot.is_none() {
        return refuse(
            "no-constraint",
            format!("{code} has no such constraint"),
            "`query --constrained` lists constraints",
        );
    }
    *slot = None;
    let which = if secondary { "secondary" } else { "primary" };
    Ok(format!("{code} {which} constraint cleared"))
}

fn set_calendar(schedule: &mut Schedule, code: &str, wanted: &str) -> Result<String, EditError> {
    let id = find(schedule, code)?;
    if schedule.activities[&id].activity_type == ActivityType::ResourceDependent {
        return refuse(
            "resource-calendar",
            format!("{code} runs on its resources' calendars"),
            "resource-dependent work ignores this calendar, so change the type or the resource",
        );
    }
    let calendar = find_calendar(schedule, wanted)?;
    let (calendar, name) = (calendar.id, calendar.name.clone());
    find_mut(schedule, code)?.calendar = calendar;
    Ok(format!("{code} calendar set to {name}"))
}

fn find_calendar<'a>(schedule: &'a Schedule, wanted: &str) -> Result<&'a Calendar, EditError> {
    let lower = wanted.to_lowercase();
    let usable = |c: &&Calendar| c.kind != CalendarType::Resource;
    let by_id = wanted
        .parse()
        .ok()
        .map(CalendarId)
        .and_then(|id| schedule.calendars.get(&id))
        .filter(usable);
    let named: Vec<_> = schedule
        .calendars
        .values()
        .filter(usable)
        .filter(|c| c.name.to_lowercase() == lower)
        .collect();
    match (by_id, named.as_slice()) {
        (Some(calendar), _) => Ok(calendar),
        (None, [calendar]) => Ok(calendar),
        (None, []) => {
            let known: Vec<_> = schedule
                .calendars
                .values()
                .filter(usable)
                .map(|c| c.name.as_str())
                .collect();
            refuse(
                "no-such-calendar",
                format!("no calendar matches {wanted}"),
                format!("calendars here: {}", known.join(", ")),
            )
        }
        (None, _) => refuse(
            "ambiguous-calendar",
            format!("{} calendars are named {wanted}", named.len()),
            "use the calendar's ID",
        ),
    }
}

/// As long as the longest activity name in the corpus (TF-034, item 8).
const MAX_NAME: usize = 120;

fn set_name(schedule: &mut Schedule, code: &str, name: &str) -> Result<String, EditError> {
    find(schedule, code)?;
    check_name(name)?;
    name.clone_into(&mut find_mut(schedule, code)?.name);
    Ok(format!("{code} renamed"))
}

fn check_name(name: &str) -> Result<(), EditError> {
    if name.trim().is_empty() {
        return refuse("empty-name", "a name can't be empty", "give it a name");
    }
    if name.chars().any(char::is_control) {
        return refuse(
            "unwritable-name",
            "a name can't hold tabs or line breaks",
            "write it on one line",
        );
    }
    if name.chars().count() > MAX_NAME {
        return refuse(
            "name-too-long",
            format!("a name can hold {MAX_NAME} characters"),
            "shorten it",
        );
    }
    Ok(())
}

fn set_type(
    schedule: &mut Schedule,
    code: &str,
    kind: ActivityType,
    duration: Option<Span>,
) -> Result<String, EditError> {
    let id = find(schedule, code)?;
    let a = &schedule.activities[&id];
    if a.status != ActivityStatus::NotStarted {
        return refuse(
            "started-work",
            format!("{code} has started, so its type can't change here"),
            "only work not started can change type",
        );
    }
    if a.activity_type.is_span() {
        return refuse(
            "span-type",
            format!("{code} spans other activities, so its type can't change here"),
            "add a new activity instead",
        );
    }
    let was_milestone = a.activity_type.is_milestone();
    let becomes_milestone = kind.is_milestone();
    let was = a.original_duration;
    if duration.is_some() && becomes_milestone {
        return refuse(
            "no-duration",
            format!("{code} would be a milestone, which has no duration"),
            "leave the duration out",
        );
    }
    // A milestone takes no resources, so its units would be lost for good.
    if becomes_milestone && schedule.assignments.values().any(|x| x.activity == id) {
        return refuse(
            "no-resources-here",
            format!("{code} has resources, and a milestone takes none"),
            "unassign them first",
        );
    }
    if was_milestone && !becomes_milestone && duration.is_none() {
        return refuse(
            "needs-duration",
            format!("{code} is a milestone, so it needs a duration to become a task"),
            "give a duration",
        );
    }
    let probe = Activity {
        activity_type: kind,
        ..a.clone()
    };
    let lengths = lengths_for(schedule, &probe);
    let minutes = duration
        .map(|span| {
            span.minutes_in(lengths.as_ref())
                .ok_or_else(|| no_length(code, span))
        })
        .transpose()?;
    if minutes.is_some_and(Minutes::is_negative) {
        return refuse(
            "negative-duration",
            format!("{code} can't have a negative duration"),
            "give zero or more",
        );
    }
    let a = find_mut(schedule, code)?;
    a.activity_type = kind;
    if becomes_milestone {
        a.original_duration = Minutes::ZERO;
        a.remaining_duration = Minutes::ZERO;
        return Ok(format!(
            "{code} type changed, and its {} duration is now zero",
            hours(was)
        ));
    }
    if let Some(minutes) = minutes {
        a.original_duration = minutes;
        a.remaining_duration = minutes;
        return Ok(format!(
            "{code} type changed, with a duration of {}",
            hours(minutes)
        ));
    }
    Ok(format!("{code} type changed"))
}

fn set_expected_finish(
    schedule: &mut Schedule,
    code: &str,
    date: Option<&str>,
) -> Result<String, EditError> {
    let date = date.map(parse_date).transpose()?;
    find_mut(schedule, code)?.expected_finish = date;
    Ok(match date {
        Some(date) => format!("{code} expected finish set to {date}"),
        None => format!("{code} expected finish cleared"),
    })
}

/// The lag's hours per day: the calendar lag runs on, for a span given in days.
fn lag_day(schedule: &Schedule, predecessor: &Activity, successor: &Activity) -> Option<Minutes> {
    lag_day_between((schedule, predecessor), (schedule, successor))
}

fn lag_lengths(
    schedule: &Schedule,
    predecessor: &Activity,
    successor: &Activity,
) -> Option<HoursPerPeriod> {
    lag_lengths_between((schedule, predecessor), (schedule, successor))
}

/// [`lag_day_between`], with the week, month and year the lag is counted in.
/// On 24 hours a day, a week is seven of them, and months and years have no
/// fixed length.
fn lag_lengths_between(
    (before, predecessor): (&Schedule, &Activity),
    (after, successor): (&Schedule, &Activity),
) -> Option<HoursPerPeriod> {
    match after.project.options.lag_calendar {
        LagCalendar::Predecessor => lengths_for(before, predecessor),
        LagCalendar::Successor => lengths_for(after, successor),
        LagCalendar::TwentyFourHour => Some(HoursPerPeriod {
            day: Some(Minutes::new(24 * 60)),
            week: Some(Minutes::new(7 * 24 * 60)),
            month: None,
            year: None,
        }),
        LagCalendar::ProjectDefault => lengths_of(after, after.project.default_calendar),
    }
}

/// The day a lag is counted in. A link's row belongs to its successor's project
/// (`TASKPRED.proj_id`), so that project's option says which calendar.
fn lag_day_between(
    before: (&Schedule, &Activity),
    after: (&Schedule, &Activity),
) -> Option<Minutes> {
    lag_lengths_between(before, after).and_then(|lengths| lengths.day)
}

/// Whichever route leads from `from` to `to` along the logic, as activities.
fn route<'a>(
    links: impl Iterator<Item = &'a Relationship>,
    from: ActivityId,
    to: ActivityId,
) -> Option<Vec<ActivityId>> {
    let mut next: BTreeMap<ActivityId, Vec<ActivityId>> = BTreeMap::new();
    for link in links {
        next.entry(link.predecessor)
            .or_default()
            .push(link.successor);
    }
    let mut came_from = BTreeMap::new();
    let mut seen = BTreeSet::from([from]);
    let mut pending = vec![from];
    while let Some(at) = pending.pop() {
        if at == to {
            let mut path = vec![to];
            let mut step = to;
            while let Some(previous) = came_from.get(&step) {
                path.push(*previous);
                step = *previous;
            }
            path.reverse();
            return Some(path);
        }
        for after in next.get(&at).into_iter().flatten() {
            if seen.insert(*after) {
                came_from.insert(*after, at);
                pending.push(*after);
            }
        }
    }
    None
}

/// The WBS node an edit acts on or under, if it's about WBS nodes.
#[must_use]
pub fn wbs_named(edit: &Edit) -> Option<&str> {
    match edit {
        Edit::AddWbs { parent, .. } => Some(parent),
        Edit::RenameWbs { wbs, .. } | Edit::MoveWbs { wbs, .. } | Edit::SetWbsObs { wbs, .. } => {
            Some(wbs)
        }
        Edit::AddNote { wbs, .. }
        | Edit::SetNote { wbs, .. }
        | Edit::DeleteNote { wbs, .. }
        | Edit::SetUdf { wbs, .. }
        | Edit::ClearUdf { wbs, .. } => wbs.as_deref(),
        _ => None,
    }
}

/// True if a node with this code or ID is here.
#[must_use]
pub fn holds_wbs(schedule: &Schedule, wanted: &str) -> bool {
    let lower = wanted.to_lowercase();
    schedule
        .wbs
        .values()
        .any(|w| w.code.to_lowercase() == lower)
        || wanted
            .parse()
            .is_ok_and(|id| schedule.wbs.contains_key(&WbsId(id)))
}

fn sibling_clash(
    schedule: &Schedule,
    parent: Option<WbsId>,
    code: &str,
    me: Option<WbsId>,
) -> bool {
    let lower = code.to_lowercase();
    schedule
        .wbs
        .values()
        .any(|w| w.parent == parent && Some(w.id) != me && w.code.to_lowercase() == lower)
}

/// Checks text that will be a WBS code, which a project's code also is.
///
/// # Errors
///
/// [`EditError`], saying what's wrong with it.
pub fn check_wbs_code(code: &str) -> Result<(), EditError> {
    let clean = !code.is_empty()
        && code == code.trim()
        && !code.chars().any(char::is_control)
        && !code.contains(['=', '>', ';'])
        && code.chars().count() <= MAX_CODE;
    if clean {
        return Ok(());
    }
    refuse(
        "invalid-wbs-code",
        format!("{code:?} can't be a WBS code"),
        format!(
            "use up to {MAX_CODE} characters on one line, without =, > or ;, and with no spaces at the ends"
        ),
    )
}

/// The place after a node's last child.
fn next_sequence(schedule: &Schedule, parent: WbsId, leaving: Option<WbsId>) -> i64 {
    schedule
        .wbs
        .values()
        .filter(|w| w.parent == Some(parent) && Some(w.id) != leaving)
        .map(|w| w.sequence)
        .max()
        .map_or(0, |last| last + 1)
}

/// As long as the longest WBS name in the corpus (TF-034, item 8).
const MAX_WBS_NAME: usize = 100;

/// Checks text that will be a WBS name, which a project's name also is.
///
/// # Errors
///
/// [`EditError`], saying what's wrong with it.
pub fn check_wbs_name(name: &str) -> Result<(), EditError> {
    check_name(name)?;
    if name.chars().count() > MAX_WBS_NAME {
        return refuse(
            "name-too-long",
            format!("a WBS name can hold {MAX_WBS_NAME} characters"),
            "shorten it",
        );
    }
    Ok(())
}

fn add_wbs(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    code: &str,
    name: &str,
    parent: &str,
) -> Result<String, EditError> {
    check_wbs_code(code)?;
    check_wbs_name(name)?;
    let parent = find_wbs(schedule, parent)?;
    if sibling_clash(schedule, Some(parent), code, None) {
        return refuse(
            "duplicate-wbs",
            format!("{code} is already under that node"),
            "choose an unused code",
        );
    }
    let sequence = next_sequence(schedule, parent, None);
    let free = schedule.wbs.keys().next_back().map_or(0, |last| last.0 + 1);
    let id = WbsId(ids.wbs.max(free));
    ids.wbs = id.0 + 1;
    let project = schedule.project.id;
    // A new node earns value and is owned as the project's root is, as the
    // file writes it.
    let root = schedule.wbs.values().find(|node| node.is_project_node);
    let earned_value = root
        .map(|root| root.earned_value.clone())
        .unwrap_or_default();
    let obs = root.and_then(|root| root.obs);
    schedule.wbs.insert(
        id,
        Wbs {
            id,
            project,
            parent: Some(parent),
            code: code.to_owned(),
            name: name.to_owned(),
            sequence,
            is_project_node: false,
            earned_value,
            obs,
            notes: std::collections::BTreeMap::new(),
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    Ok(format!("WBS {code} added"))
}

fn not_the_root(schedule: &Schedule, id: WbsId, wanted: &str) -> Result<(), EditError> {
    if schedule.wbs[&id].is_project_node {
        return refuse(
            "project-node",
            format!("{wanted} is the project's own node"),
            "it follows the project, so change that in P6",
        );
    }
    Ok(())
}

fn rename_wbs(
    schedule: &mut Schedule,
    wanted: &str,
    code: Option<&str>,
    name: Option<&str>,
) -> Result<String, EditError> {
    let id = find_wbs(schedule, wanted)?;
    not_the_root(schedule, id, wanted)?;
    if code.is_none() && name.is_none() {
        return refuse(
            "nothing-to-change",
            format!("the edit to {wanted} gives no change"),
            "give a new code, a new name or both",
        );
    }
    if let Some(code) = code {
        check_wbs_code(code)?;
        if sibling_clash(schedule, schedule.wbs[&id].parent, code, Some(id)) {
            return refuse(
                "duplicate-wbs",
                format!("{code} is already under that node"),
                "choose an unused code",
            );
        }
    }
    if let Some(name) = name {
        check_wbs_name(name)?;
    }
    let Some(node) = schedule.wbs.get_mut(&id) else {
        return refuse("no-such-wbs", "that node has gone", "list the nodes again");
    };
    if let Some(code) = code {
        code.clone_into(&mut node.code);
    }
    if let Some(name) = name {
        name.clone_into(&mut node.name);
    }
    Ok(format!("WBS {wanted} renamed"))
}

fn move_wbs(schedule: &mut Schedule, wanted: &str, parent: &str) -> Result<String, EditError> {
    let id = find_wbs(schedule, wanted)?;
    not_the_root(schedule, id, wanted)?;
    let parent = find_wbs(schedule, parent)?;
    if schedule.wbs[&id].parent == Some(parent) {
        return refuse(
            "nothing-to-change",
            format!("{wanted} is already there"),
            "choose another parent",
        );
    }
    // A damaged file can hold a loop of its own, so nothing is walked twice.
    let mut seen = BTreeSet::new();
    let mut at = Some(parent);
    while let Some(here) = at.filter(|here| seen.insert(*here)) {
        if here == id {
            return refuse(
                "circular-wbs",
                format!("{wanted} can't move under itself"),
                "choose a node outside it",
            );
        }
        at = schedule.wbs.get(&here).and_then(|w| w.parent);
    }
    let code = schedule.wbs[&id].code.clone();
    if sibling_clash(schedule, Some(parent), &code, Some(id)) {
        return refuse(
            "duplicate-wbs",
            format!("{code} is already under that node"),
            "rename one first",
        );
    }
    let sequence = next_sequence(schedule, parent, Some(id));
    let Some(node) = schedule.wbs.get_mut(&id) else {
        return refuse("no-such-wbs", "that node has gone", "list the nodes again");
    };
    node.parent = Some(parent);
    node.sequence = sequence;
    Ok(format!("WBS {wanted} moved"))
}

fn set_wbs(schedule: &mut Schedule, code: &str, wanted: &str) -> Result<String, EditError> {
    find(schedule, code)?;
    let id = find_wbs(schedule, wanted)?;
    find_mut(schedule, code)?.wbs = id;
    Ok(format!("{code} moved to WBS {wanted}"))
}

/// The resource a name means here: its short name, or else its name.
#[must_use]
pub fn resource_named(schedule: &Schedule, wanted: &str) -> Option<ResourceId> {
    one_named(&schedule.resources.values(), wanted, |r| {
        (&r.short_name, &r.name)
    })
    .map(|r| r.id)
}

/// The role a name means here: its short name, or else its name.
#[must_use]
pub fn role_named(schedule: &Schedule, wanted: &str) -> Option<RoleId> {
    one_named(&schedule.roles.values(), wanted, |r| {
        (&r.short_name, &r.name)
    })
    .map(|r| r.id)
}

/// The one item a name means: by short name, or else by name, ignoring case.
fn one_named<'a, T: 'a>(
    items: &(impl Iterator<Item = &'a T> + Clone),
    wanted: &str,
    names: fn(&T) -> (&str, &str),
) -> Option<&'a T> {
    let lower = wanted.to_lowercase();
    let by = |part: fn((&'a str, &'a str)) -> &'a str| {
        let mut found = items
            .clone()
            .filter(|item| part(names(item)).to_lowercase() == lower);
        let first = found.next()?;
        found.next().is_none().then_some(first)
    };
    by(|(short, _)| short).or_else(|| by(|(_, name)| name))
}

fn no_role(wanted: &str) -> EditError {
    EditError {
        code: "no-such-role",
        message: format!("no one role matches {wanted}"),
        hint: "`inspect` lists roles".to_owned(),
    }
}

fn add_role(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    (short_name, name): (&str, &str),
    (parent, limit): (Option<&str>, Option<f64>),
) -> Result<String, EditError> {
    if !clean(short_name) || !clean(name) {
        return refuse(
            "invalid-role",
            format!("{short_name:?} named {name:?} can't be a role"),
            "write each on one line, with no spaces at the ends",
        );
    }
    check_limit(limit)?;
    let lower = short_name.to_lowercase();
    if schedule
        .roles
        .values()
        .any(|r| r.short_name.to_lowercase() == lower)
    {
        return refuse(
            "duplicate-role",
            format!("there's already a role {short_name}"),
            "use assign_role to put it on an activity",
        );
    }
    let parent = match parent {
        None => None,
        Some(wanted) => Some(role_named(schedule, wanted).ok_or_else(|| no_role(wanted))?),
    };
    let free = schedule
        .roles
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = RoleId(ids.role.max(free));
    ids.role = id.0 + 1;
    let rates = match limit {
        Some(limit) => {
            let held = schedule.roles.values().flat_map(|r| &r.rates);
            let mut next = rate_ids(held.map(|r| r.id.0).collect(), &mut ids.role_rate);
            vec![RoleRate::made(
                next(),
                opening_day(schedule),
                [None; 5],
                Some(limit),
            )]
        }
        None => Vec::new(),
    };
    schedule.roles.insert(
        id,
        Role {
            id,
            short_name: short_name.to_owned(),
            name: name.to_owned(),
            parent,
            rates,
            // As every role in the corpus has it.
            default_cost_follows_units: true,
            source: None,
        },
    );
    let limit = limit.map_or_else(String::new, |l| format!(", at most {l} units per hour"));
    Ok(format!("added role {short_name}{limit}"))
}

fn assign_role(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    (code, wanted): (&str, &str),
    units: Option<f64>,
) -> Result<String, EditError> {
    if let Some(units) = units.filter(|u| !u.is_finite() || *u < 0.0) {
        return refuse(
            "invalid-units",
            format!("{units} units can't be assigned"),
            "give zero or more units",
        );
    }
    let id = find(schedule, code)?;
    let activity = &schedule.activities[&id];
    if activity.status == ActivityStatus::Completed {
        return refuse(
            "no-resources-here",
            format!("{code} is finished, so it takes no new roles"),
            "assign the role to work still to do",
        );
    }
    if activity.activity_type.is_milestone() {
        return refuse(
            "no-resources-here",
            format!("{code} is a milestone, which takes no time to staff"),
            "assign the role to the work before it",
        );
    }
    let role = role_named(schedule, wanted).ok_or_else(|| no_role(wanted))?;
    let held = schedule
        .assignments
        .values()
        .any(|a| a.activity == id && a.resource.is_none() && a.role == Some(role));
    if held {
        return refuse(
            "already-assigned",
            format!("{code} already has {wanted} with no resource"),
            "change its duration to move its units",
        );
    }
    let assignment = next_assignment(schedule, ids);
    // Priced at the role's first price in force when the activity starts, as
    // a resource's assignment is at the resource's.
    let starts = activity
        .planned_start
        .or(activity.stored.early_start)
        .or(schedule.project.data_date);
    let held = &schedule.roles[&role];
    let price = held.rate_at(starts).and_then(|rate| rate.prices[0]);
    let mut amounts = units::priced(
        units::new_amounts(activity, units, None, held.default_cost_follows_units),
        price,
    );
    // Its price is the role's, whether or not the role has one yet.
    amounts.price_source = Some(PriceSource::Role);
    let said = units::describe(&amounts);
    let project = schedule.project.id;
    schedule.assignments.insert(
        assignment,
        Assignment {
            id: assignment,
            activity: id,
            resource: None,
            role: Some(role),
            cost_account: None,
            project,
            curve: None,
            typed_spread: None,
            amounts,
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    Ok(format!("{code} assigned role {wanted}, with {said}"))
}

fn unassign_role(schedule: &mut Schedule, code: &str, wanted: &str) -> Result<String, EditError> {
    let id = find(schedule, code)?;
    let role = role_named(schedule, wanted).ok_or_else(|| no_role(wanted))?;
    let held = |a: &Assignment| a.activity == id && a.resource.is_none() && a.role == Some(role);
    if !schedule.assignments.values().any(held) {
        return refuse(
            "not-assigned",
            format!("{code} doesn't have {wanted} with no resource"),
            "`query --columns code,roles` lists roles, including those a resource fills, which go with `--unassign`",
        );
    }
    schedule.assignments.retain(|_, a| !held(a));
    Ok(format!("{code} unassigned role {wanted}"))
}

fn no_resource(wanted: &str) -> EditError {
    EditError {
        code: "no-such-resource",
        message: format!("no one resource matches {wanted}"),
        hint: "`inspect` lists resources".to_owned(),
    }
}

/// The ID a new assignment takes: past every ID in the file and every one
/// this batch of edits has given out.
fn next_assignment(schedule: &Schedule, ids: &mut NextIds) -> AssignmentId {
    let free = schedule
        .assignments
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let assignment = AssignmentId(ids.assignment.max(free));
    ids.assignment = assignment.0 + 1;
    assignment
}

fn assign_resource(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    (code, wanted): (&str, &str),
    units: Option<f64>,
) -> Result<String, EditError> {
    if let Some(units) = units.filter(|u| !u.is_finite() || *u < 0.0) {
        return refuse(
            "invalid-units",
            format!("{units} units can't be assigned"),
            "give zero or more units",
        );
    }
    let id = find(schedule, code)?;
    if schedule.activities[&id].status == ActivityStatus::Completed {
        return refuse(
            "no-resources-here",
            format!("{code} is finished, so it takes no new resources"),
            "assign the resource to work still to do",
        );
    }
    let resource = resource_named(schedule, wanted).ok_or_else(|| no_resource(wanted))?;
    if schedule.activities[&id].activity_type.is_milestone() {
        return refuse(
            "no-resources-here",
            format!("{code} is a milestone, which takes no time to staff"),
            "assign the resource to the work before it",
        );
    }
    let held = schedule
        .assignments
        .values()
        .any(|a| a.activity == id && a.resource == Some(resource));
    if held {
        return refuse(
            "already-assigned",
            format!("{code} already has {wanted}"),
            "change its duration to move its units, or unassign it first",
        );
    }
    if schedule.activities[&id].activity_type == ActivityType::ResourceDependent {
        let calendar_of = |resource: ResourceId| {
            schedule
                .resources
                .get(&resource)
                .and_then(|r| r.calendar)
                .unwrap_or(schedule.activities[&id].calendar)
        };
        // A role alone has no calendar to clash with.
        let clash = schedule
            .assignments
            .values()
            .filter(|a| a.activity == id)
            .filter_map(|a| a.resource)
            .any(|other| calendar_of(other) != calendar_of(resource));
        if clash {
            return refuse(
                "resource-calendar",
                format!("{code}'s resources would be on different calendars"),
                "resource-dependent work needs one calendar, so unassign the other resource first",
            );
        }
    }
    let assignment = next_assignment(schedule, ids);
    let project = schedule.project.id;
    // A new assignment starts from the resource's defaults, at its first
    // price in force when the activity starts.
    let held = schedule.resources.get(&resource);
    let activity = &schedule.activities[&id];
    // The start the assignment row is written with (`target_start_date`).
    let starts = activity
        .planned_start
        .or(activity.stored.early_start)
        .or(schedule.project.data_date);
    let price = held
        .and_then(|r| r.rate_at(starts))
        .and_then(|rate| rate.prices[0]);
    let amounts = units::priced(
        units::new_amounts(
            activity,
            units,
            held.and_then(|r| r.default_units_per_hour),
            held.is_some_and(|r| r.default_cost_follows_units),
        ),
        price,
    );
    let said = units::describe(&amounts);
    schedule.assignments.insert(
        assignment,
        Assignment {
            id: assignment,
            activity: id,
            resource: Some(resource),
            role: None,
            cost_account: None,
            project,
            curve: None,
            typed_spread: None,
            amounts,
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    Ok(format!("{code} assigned {wanted}, with {said}"))
}

fn unassign_resource(
    schedule: &mut Schedule,
    code: &str,
    wanted: &str,
) -> Result<String, EditError> {
    let id = find(schedule, code)?;
    let resource = resource_named(schedule, wanted).ok_or_else(|| no_resource(wanted))?;
    let held = |a: &Assignment| a.activity == id && a.resource == Some(resource);
    if !schedule.assignments.values().any(held) {
        return refuse(
            "not-assigned",
            format!("{code} doesn't have {wanted}"),
            "`query --columns code,resources` lists assignments",
        );
    }
    let others = schedule
        .assignments
        .values()
        .any(|a| a.activity == id && a.resource.is_some_and(|r| r != resource));
    if schedule.activities[&id].activity_type == ActivityType::ResourceDependent && !others {
        return refuse(
            "resource-calendar",
            format!("{code} runs on {wanted}'s calendar, and has no other resource"),
            "assign another resource first, or change the activity's type",
        );
    }
    schedule.assignments.retain(|_, a| !held(a));
    Ok(format!("{code} no longer has {wanted}"))
}

fn set_assignment_units(
    schedule: &mut Schedule,
    (code, wanted): (&str, &str),
    (actual, left): (Option<f64>, Option<f64>),
) -> Result<String, EditError> {
    if actual.is_none() && left.is_none() {
        return refuse(
            "nothing-to-change",
            format!("the edit to {code}'s {wanted} gives no units"),
            "give actual units, remaining units or both",
        );
    }
    if let Some(units) = [actual, left]
        .into_iter()
        .flatten()
        .find(|u| !u.is_finite() || *u < 0.0)
    {
        return refuse(
            "invalid-units",
            format!("{units} units can't be set"),
            "give zero or more units",
        );
    }
    let id = find(schedule, code)?;
    let resource = resource_named(schedule, wanted).ok_or_else(|| no_resource(wanted))?;
    let mut held = schedule
        .assignments
        .values()
        .filter(|a| a.activity == id && a.resource == Some(resource))
        .map(|a| a.id);
    let key = match (held.next(), held.next()) {
        (Some(key), None) => key,
        (Some(_), Some(_)) => {
            return refuse(
                "ambiguous-assignment",
                format!("{code} has {wanted} on it more than once"),
                "change the units in the source schedule, where each assignment has its own row",
            );
        }
        (None, _) => {
            return refuse(
                "not-assigned",
                format!("{code} doesn't have {wanted}"),
                "`query --columns code,resources` lists assignments",
            );
        }
    };
    let a = &schedule.activities[&id];
    if a.status == ActivityStatus::NotStarted && actual.is_some_and(|u| u > 0.0) {
        return refuse(
            "not-started",
            format!("{code} hasn't started, so it has no actual units"),
            "record an actual start first",
        );
    }
    if a.status == ActivityStatus::Completed && left.is_some_and(|u| u > 0.0) {
        return refuse(
            "finished-work",
            format!("{code} is finished, so it has no remaining units"),
            "clear its progress first, or give an actual finish later",
        );
    }
    let durations = (a.original_duration, a.remaining_duration);
    let not_started = a.status == ActivityStatus::NotStarted;
    let by_units = a.percent_complete_type == PercentCompleteType::Units;
    let before = schedule.assignments[&key].amounts;
    let after = units::with_units(before, (actual, left), durations, not_started);
    let role = schedule.assignments[&key].role;
    if let Some(held) = schedule.assignments.get_mut(&key) {
        held.amounts = after;
    }
    let mut line = units::said_moved(schedule, (Some(resource), role), (&before, &after))
        .map_or_else(
            || format!("{code}'s {wanted} keeps its units"),
            |m| format!("{code}'s {m}"),
        );
    if by_units && let Some(percent) = units::units_percent(schedule, id) {
        // The stored percent follows, so text and tables that carry it back
        // don't split the units again (docs/rules/TF-004-units-percent-complete.md).
        if let Some(a) = schedule.activities.get_mut(&id) {
            a.physical_percent_complete = Some((percent * 100.0).round() / 100.0);
        }
        let _ = write!(line, "; {code} is {percent:.2}% complete by units");
    }
    Ok(line)
}

fn code_type(schedule: &Schedule, wanted: &str) -> Result<ActivityCodeTypeId, EditError> {
    let lower = wanted.to_lowercase();
    let mut found = schedule
        .activity_code_types
        .values()
        .filter(|t| t.name.to_lowercase() == lower);
    match (found.next(), found.next()) {
        (Some(one), None) => Ok(one.id),
        (Some(_), Some(_)) => refuse(
            "ambiguous-code",
            format!("two code types here are named {wanted}"),
            "rename one in P6 first",
        ),
        (None, _) => {
            let known: Vec<&str> = schedule
                .activity_code_types
                .values()
                .map(|t| t.name.as_str())
                .collect();
            refuse(
                "no-such-code",
                format!("no code type here is named {wanted}"),
                format!(
                    "code types here: {}. add_code_type adds one",
                    known.join(", ")
                ),
            )
        }
    }
}

fn set_code(
    schedule: &mut Schedule,
    code: &str,
    wanted: &str,
    value: &str,
) -> Result<String, EditError> {
    find(schedule, code)?;
    let kind = code_type(schedule, wanted)?;
    let lower = value.to_lowercase();
    let values: Vec<_> = schedule
        .activity_codes
        .values()
        .filter(|c| c.code_type == kind)
        .collect();
    // A value is exact. Its description is only a fallback, as in `query`.
    let exact: Vec<ActivityCodeId> = values
        .iter()
        .filter(|c| c.value.to_lowercase() == lower)
        .map(|c| c.id)
        .collect();
    let named: Vec<ActivityCodeId> = values
        .iter()
        .filter(|c| c.description.to_lowercase() == lower)
        .map(|c| c.id)
        .collect();
    let matches = if exact.is_empty() { named } else { exact };
    let id = match matches.as_slice() {
        [one] => *one,
        [] => {
            let known: Vec<&str> = values.iter().map(|c| c.value.as_str()).collect();
            return refuse(
                "no-such-code",
                format!("{wanted} has no value {value}"),
                format!("{wanted} holds: {}", known.join(", ")),
            );
        }
        _ => {
            return refuse(
                "ambiguous-code",
                format!("{} values of {wanted} match {value}", matches.len()),
                "rename one in P6, or use a value that names one",
            );
        }
    };
    find_mut(schedule, code)?.codes.insert(kind, id);
    Ok(format!("{code} set to {wanted}={value}"))
}

fn clear_code(schedule: &mut Schedule, code: &str, wanted: &str) -> Result<String, EditError> {
    find(schedule, code)?;
    let kind = code_type(schedule, wanted)?;
    if find_mut(schedule, code)?.codes.remove(&kind).is_none() {
        return refuse(
            "no-code",
            format!("{code} has no {wanted} code"),
            "`query --columns code,codes` lists codes",
        );
    }
    Ok(format!("{code} no longer has a {wanted} code"))
}

fn add_code_value(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    wanted: &str,
    value: &str,
    description: Option<&str>,
) -> Result<String, EditError> {
    let kind = code_type(schedule, wanted)?;
    let description = description.unwrap_or(value);
    if !clean(value) || !clean(description) {
        return refuse(
            "invalid-code-value",
            format!("{value:?} can't be a value of {wanted}"),
            "write it on one line, with no spaces at the ends",
        );
    }
    let limit = schedule.activity_code_types[&kind]
        .max_length
        .and_then(|limit| usize::try_from(limit).ok());
    if limit.is_some_and(|limit| value.chars().count() > limit) {
        return refuse(
            "code-too-long",
            format!("{wanted} values hold {} characters", limit.unwrap_or(0)),
            "shorten it",
        );
    }
    let lower = value.to_lowercase();
    let sibling = |c: &&ActivityCode| c.code_type == kind;
    let values = || schedule.activity_codes.values().filter(sibling);
    if values().any(|c| c.value.to_lowercase() == lower) {
        return refuse(
            "duplicate-code",
            format!("{wanted} already has the value {value}"),
            "use set_code to give it to an activity",
        );
    }
    let sequence = values()
        .map(|c| c.sequence)
        .max()
        .map_or(0, |last| last + 1);
    let free = schedule
        .activity_codes
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = ActivityCodeId(ids.code.max(free));
    ids.code = id.0 + 1;
    schedule.activity_codes.insert(
        id,
        ActivityCode {
            id,
            code_type: kind,
            parent: None,
            value: value.to_owned(),
            description: description.to_owned(),
            sequence,
            source: None,
        },
    );
    Ok(format!("{wanted} gained the value {value}"))
}

/// Text fit for a name: on one line, with no spaces at the ends.
fn clean(text: &str) -> bool {
    !text.is_empty() && text == text.trim() && !text.chars().any(char::is_control)
}

fn add_code_type(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    name: &str,
    scope: Option<Scope>,
    max_length: Option<u32>,
) -> Result<String, EditError> {
    if !clean(name) {
        return refuse(
            "invalid-code-type",
            format!("{name:?} can't name a code type"),
            "write it on one line, with no spaces at the ends",
        );
    }
    if max_length == Some(0) {
        return refuse(
            "invalid-code-type",
            "a code type's values need at least one character",
            "give a max_length of 1 or more",
        );
    }
    let lower = name.to_lowercase();
    if schedule
        .activity_code_types
        .values()
        .any(|t| t.name.to_lowercase() == lower)
    {
        return refuse(
            "duplicate-code-type",
            format!("there's already a code type named {name}"),
            "use add_code_value to give it values",
        );
    }
    let scope = scope.unwrap_or(Scope::Project);
    let sequence = schedule
        .activity_code_types
        .values()
        .map(|t| t.sequence)
        .max()
        .map_or(0, |last| last + 1);
    let free = schedule
        .activity_code_types
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = ActivityCodeTypeId(ids.code_type.max(free));
    ids.code_type = id.0 + 1;
    schedule.activity_code_types.insert(
        id,
        ActivityCodeType {
            id,
            name: name.to_owned(),
            scope: match scope {
                Scope::Project => CodeScope::Project,
                Scope::Global => CodeScope::Global,
            },
            project: (scope == Scope::Project).then_some(schedule.project.id),
            max_length: Some(max_length.unwrap_or(DEFAULT_CODE_LENGTH)),
            sequence,
            source: None,
        },
    );
    let reach = match scope {
        Scope::Project => "this project",
        Scope::Global => "every project",
    };
    Ok(format!("Added code type {name}, for {reach}"))
}

/// A new code type's longest value: the length most code types in real files
/// have.
pub const DEFAULT_CODE_LENGTH: u32 = 12;

/// The calendar a new resource gets when none is named: the default global
/// calendar.
fn default_resource_calendar(schedule: &Schedule) -> Option<CalendarId> {
    let global = |c: &&Calendar| c.kind == CalendarType::Global;
    schedule
        .calendars
        .values()
        .filter(global)
        .find(|c| c.is_default)
        .map(|c| c.id)
        .or_else(|| {
            schedule
                .project
                .default_calendar
                .filter(|id| schedule.calendars.get(id).is_some_and(|c| global(&c)))
        })
}

fn add_resource(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    (short_name, name): (&str, &str),
    kind: ResourceType,
    (units, calendar, limit): (Option<f64>, Option<&str>, Option<f64>),
) -> Result<String, EditError> {
    if !clean(short_name) || !clean(name) {
        return refuse(
            "invalid-resource",
            format!("{short_name:?} named {name:?} can't be a resource"),
            "write each on one line, with no spaces at the ends",
        );
    }
    if let Some(units) = units.filter(|u| !u.is_finite() || *u < 0.0) {
        return refuse(
            "invalid-units",
            format!("{units} units per hour can't be a resource's default"),
            "give zero or more units per hour",
        );
    }
    check_limit(limit)?;
    let lower = short_name.to_lowercase();
    if schedule
        .resources
        .values()
        .any(|r| r.short_name.to_lowercase() == lower)
    {
        return refuse(
            "duplicate-resource",
            format!("there's already a resource {short_name}"),
            "use assign_resource to put it on an activity",
        );
    }
    let calendar = match calendar {
        None => default_resource_calendar(schedule),
        Some(wanted) => {
            let id = calendar_named(schedule, wanted).ok_or_else(|| no_calendar(wanted))?;
            if schedule.calendars[&id].kind == CalendarType::Project {
                return refuse(
                    "resource-calendar",
                    format!("{wanted} is a project calendar, which a resource can't use"),
                    "use a global or resource calendar",
                );
            }
            Some(id)
        }
    };
    let free = schedule
        .resources
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = ResourceId(ids.resource.max(free));
    ids.resource = id.0 + 1;
    let rates = match limit {
        Some(limit) => vec![new_rate(schedule, ids, opening_day(schedule), Some(limit))],
        None => Vec::new(),
    };
    schedule.resources.insert(
        id,
        Resource {
            id,
            short_name: short_name.to_owned(),
            name: name.to_owned(),
            kind,
            calendar,
            parent: None,
            default_units_per_hour: Some(units.unwrap_or(1.0)),
            rates,
            roles: Vec::new(),
            default_cost_follows_units: true,
            udfs: BTreeMap::new(),
            source: None,
        },
    );
    let kind = match kind {
        ResourceType::Labor => "labor",
        ResourceType::Nonlabor => "nonlabor",
        ResourceType::Material => "material",
    };
    let limit = limit.map_or_else(String::new, |l| format!(", at most {l} units per hour"));
    Ok(format!(
        "Added resource {short_name} ({name}, {kind}{limit})"
    ))
}

/// A limit, if given, is a number of units per hour, zero or more.
fn check_limit(limit: Option<f64>) -> Result<(), EditError> {
    match limit.filter(|l| !l.is_finite() || *l < 0.0) {
        Some(limit) => refuse(
            "invalid-units",
            format!("{limit} units per hour can't be a limit"),
            "give zero or more units per hour, or null to clear the limit",
        ),
        None => Ok(()),
    }
}

/// When a rate made with no day given starts: midnight on the project's
/// planned start, or else its data date (TF-001). Before a resource's first
/// rate, that rate is used, so the day changes no result in the project.
fn opening_day(schedule: &Schedule) -> Option<DateTime> {
    let project = &schedule.project;
    project
        .planned_start
        .or(project.data_date)
        .and_then(|d| d.date().at(TimeOfDay::MIDNIGHT))
}

/// A rate holding prices and a limit: a resource's or a role's (TF-001).
trait Rate: Sized {
    fn made(
        id: u64,
        effective: Option<DateTime>,
        prices: [Option<f64>; 5],
        limit: Option<f64>,
    ) -> Self;
    fn id(&self) -> u64;
    fn effective(&self) -> Option<DateTime>;
    fn prices(&self) -> [Option<f64>; 5];
    fn limit(&self) -> Option<f64>;
    fn set_limit(&mut self, limit: Option<f64>);
    fn in_force(rates: &[Self], when: Option<DateTime>) -> Option<&Self>;
}

impl Rate for ResourceRate {
    fn made(
        id: u64,
        effective: Option<DateTime>,
        prices: [Option<f64>; 5],
        limit: Option<f64>,
    ) -> Self {
        ResourceRate {
            id: ResourceRateId(id),
            effective,
            prices,
            max_units_per_hour: limit,
            source: None,
        }
    }
    fn id(&self) -> u64 {
        self.id.0
    }
    fn effective(&self) -> Option<DateTime> {
        self.effective
    }
    fn prices(&self) -> [Option<f64>; 5] {
        self.prices
    }
    fn limit(&self) -> Option<f64> {
        self.max_units_per_hour
    }
    fn set_limit(&mut self, limit: Option<f64>) {
        self.max_units_per_hour = limit;
    }
    fn in_force(rates: &[Self], when: Option<DateTime>) -> Option<&Self> {
        rate_in_force(rates, when, |r| (r.effective, r.id.0))
    }
}

impl Rate for RoleRate {
    fn made(
        id: u64,
        effective: Option<DateTime>,
        prices: [Option<f64>; 5],
        limit: Option<f64>,
    ) -> Self {
        RoleRate {
            id: RoleRateId(id),
            effective,
            prices,
            max_units_per_hour: limit,
            source: None,
        }
    }
    fn id(&self) -> u64 {
        self.id.0
    }
    fn effective(&self) -> Option<DateTime> {
        self.effective
    }
    fn prices(&self) -> [Option<f64>; 5] {
        self.prices
    }
    fn limit(&self) -> Option<f64> {
        self.max_units_per_hour
    }
    fn set_limit(&mut self, limit: Option<f64>) {
        self.max_units_per_hour = limit;
    }
    fn in_force(rates: &[Self], when: Option<DateTime>) -> Option<&Self> {
        rate_in_force(rates, when, |r| (r.effective, r.id.0))
    }
}

/// Rate IDs, one at a time: past every rate ID in `held` and every one this
/// batch of edits has given out in `next`.
fn rate_ids(held: Vec<u64>, next: &mut u64) -> impl FnMut() -> u64 {
    *next = held
        .into_iter()
        .map(|id| id + 1)
        .max()
        .unwrap_or(0)
        .max(*next);
    move || {
        *next += 1;
        *next - 1
    }
}

/// A rate with no prices: past every rate ID in the file and every one this
/// batch of edits has given out.
fn new_rate(
    schedule: &Schedule,
    ids: &mut NextIds,
    effective: Option<DateTime>,
    limit: Option<f64>,
) -> ResourceRate {
    let held = schedule.resources.values().flat_map(|r| &r.rates);
    let mut next = rate_ids(held.map(|r| r.id.0).collect(), &mut ids.resource_rate);
    ResourceRate::made(next(), effective, [None; 5], limit)
}

/// Sets or clears a resource's limit on every rate, or from a day on.
fn set_resource_limit(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    wanted: &str,
    (limit, from): (Option<f64>, Option<&str>),
) -> Result<String, EditError> {
    check_limit(limit)?;
    let id = resource_named(schedule, wanted).ok_or_else(|| no_resource(wanted))?;
    let from = from.map(parse_day).transpose()?;
    let opening = opening_day(schedule);
    let held = schedule.resources.values().flat_map(|r| &r.rates);
    let new_id = rate_ids(held.map(|r| r.id.0).collect(), &mut ids.resource_rate);
    let resource = schedule
        .resources
        .get_mut(&id)
        .ok_or_else(|| no_resource(wanted))?;
    set_limit(
        &mut resource.rates,
        (new_id, opening),
        &resource.short_name,
        (limit, from),
    )
}

/// Sets or clears a role's limit on every rate, or from a day on, as
/// [`set_resource_limit`] does for a resource.
fn set_role_limit(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    wanted: &str,
    (limit, from): (Option<f64>, Option<&str>),
) -> Result<String, EditError> {
    check_limit(limit)?;
    let id = role_named(schedule, wanted).ok_or_else(|| no_role(wanted))?;
    let from = from.map(parse_day).transpose()?;
    let opening = opening_day(schedule);
    let held = schedule.roles.values().flat_map(|r| &r.rates);
    let new_id = rate_ids(held.map(|r| r.id.0).collect(), &mut ids.role_rate);
    let role = schedule.roles.get_mut(&id).ok_or_else(|| no_role(wanted))?;
    set_limit(
        &mut role.rates,
        (new_id, opening),
        &role.short_name,
        (limit, from),
    )
}

/// Sets or clears a limit on every rate, or from `day` on (TF-001). New
/// rates take IDs from `new_id`, and one made with no day given starts on
/// `opening`.
fn set_limit<R: Rate>(
    rates: &mut Vec<R>,
    (mut new_id, opening): (impl FnMut() -> u64, Option<DateTime>),
    short_name: &str,
    (limit, from): (Option<f64>, Option<Date>),
) -> Result<String, EditError> {
    let said = limit.map_or_else(
        || "no limit".to_owned(),
        |l| format!("at most {l} units per hour"),
    );
    let Some(day) = from else {
        if limit.is_none() && rates.iter().all(|r| r.limit().is_none()) {
            return refuse(
                "nothing-to-change",
                format!("{short_name} has no limit"),
                "only a limit that is set can be cleared",
            );
        }
        if rates.is_empty() {
            rates.push(R::made(new_id(), opening, [None; 5], limit));
        }
        for rate in rates.iter_mut() {
            rate.set_limit(limit);
        }
        return Ok(format!("{short_name} now has {said}"));
    };
    let starts = day.at(TimeOfDay::MIDNIGHT).ok_or_else(|| EditError {
        code: "invalid-date",
        message: format!("{day} isn't a valid date"),
        hint: "use the form YYYY-MM-DD".to_owned(),
    })?;
    // Of rates starting that day, the one in force from it (TF-001 item 1).
    let on_the_day = rates
        .iter()
        .enumerate()
        .filter(|(_, r)| r.effective().is_some_and(|e| e.date() == day))
        .max_by_key(|(_, r)| (r.effective(), r.id()))
        .map(|(at, _)| at);
    let in_force = R::in_force(rates, Some(starts));
    if on_the_day.is_none() && limit.is_none() && in_force.is_none_or(|r| r.limit().is_none()) {
        return refuse(
            "nothing-to-change",
            format!("{short_name} has no limit on {day}"),
            "only a limit that is set can be cleared",
        );
    }
    let prices = in_force.map_or([None; 5], Rate::prices);
    let before_limit = in_force.and_then(Rate::limit);
    // With no rate started before that day, the earliest rate is used before
    // it starts too, so changing or adding one would reach back. A rate from
    // the project's start, or else the day before, keeps what held before;
    // as the earliest rate, it covers any time before it too.
    let started = rates
        .iter()
        .any(|r| r.effective().is_none_or(|e| e < starts));
    let day_before = day
        .checked_add_days(-1)
        .and_then(|d| d.at(TimeOfDay::MIDNIGHT));
    let opening = opening.filter(|d| *d < starts).or(day_before);
    if !started {
        rates.push(R::made(new_id(), opening, prices, before_limit));
    }
    match on_the_day {
        Some(at) => rates[at].set_limit(limit),
        None => rates.push(R::made(new_id(), Some(starts), prices, limit)),
    }
    Ok(format!("{short_name} has {said} from {day}"))
}

fn no_calendar(wanted: &str) -> EditError {
    EditError {
        code: "no-such-calendar",
        message: format!("no one calendar matches {wanted}"),
        hint: "`inspect` lists calendars; use an ID if names repeat".to_owned(),
    }
}

fn parse_day(text: &str) -> Result<Date, EditError> {
    match text.parse() {
        Ok(date) => Ok(date),
        Err(_) => refuse(
            "invalid-date",
            format!("{text} isn't a valid date"),
            "use the form YYYY-MM-DD",
        ),
    }
}

fn parse_time(text: &str) -> Option<TimeOfDay> {
    let (hour, minute) = text.split_once(':')?;
    let (hour, minute): (u16, u16) = (hour.parse().ok()?, minute.parse().ok()?);
    (minute < 60)
        .then(|| TimeOfDay::from_minutes(hour.checked_mul(60)?.checked_add(minute)?))
        .flatten()
}

fn parse_shifts(shifts: &[String]) -> Result<Vec<WorkInterval>, EditError> {
    let mut parsed = Vec::with_capacity(shifts.len());
    for text in shifts {
        let interval = text
            .split_once('-')
            .and_then(|(start, finish)| WorkInterval::new(parse_time(start)?, parse_time(finish)?));
        let Some(interval) = interval else {
            return refuse(
                "invalid-shift",
                format!("{text} isn't a shift"),
                "use the form 08:00-12:00, with the end after the start",
            );
        };
        parsed.push(interval);
    }
    parsed.sort();
    if parsed.windows(2).any(|w| w[1].start < w[0].finish) {
        return refuse(
            "invalid-shift",
            "those shifts overlap",
            "give shifts that don't overlap",
        );
    }
    Ok(parsed)
}

fn calendar_mut<'a>(
    schedule: &'a mut Schedule,
    wanted: &str,
) -> Result<&'a mut Calendar, EditError> {
    let id = calendar_named(schedule, wanted).ok_or_else(|| no_calendar(wanted))?;
    schedule
        .calendars
        .get_mut(&id)
        .ok_or_else(|| no_calendar(wanted))
}

fn add_calendar(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    name: &str,
    copy_of: Option<&str>,
) -> Result<String, EditError> {
    check_name(name)?;
    if !calendars_named(schedule, name).is_empty() {
        return refuse(
            "duplicate-calendar",
            format!("{name} is already a calendar here"),
            "choose an unused name",
        );
    }
    let source = match copy_of {
        Some(wanted) => calendar_named(schedule, wanted).ok_or_else(|| no_calendar(wanted))?,
        None => match schedule.project.default_calendar {
            Some(calendar) => calendar,
            None => {
                return refuse(
                    "needs-calendar",
                    "this project has no default calendar to copy",
                    "say which calendar to copy",
                );
            }
        },
    };
    let free = schedule
        .calendars
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = CalendarId(ids.calendar.max(free));
    ids.calendar = id.0 + 1;
    let mut calendar = schedule
        .calendars
        .get(&source)
        .cloned()
        .ok_or_else(|| no_calendar(copy_of.unwrap_or_default()))?;
    let copied = calendar.name.clone();
    calendar.id = id;
    name.clone_into(&mut calendar.name);
    calendar.kind = CalendarType::Project;
    calendar.project = Some(schedule.project.id);
    calendar.base = None;
    calendar.is_default = false;
    calendar.source = None;
    schedule.calendars.insert(id, calendar);
    Ok(format!("{name} added, copied from {copied}"))
}

fn add_exception(
    schedule: &mut Schedule,
    wanted: &str,
    date: &str,
    shifts: &[String],
) -> Result<String, EditError> {
    let date = parse_day(date)?;
    let shifts = parse_shifts(shifts)?;
    let calendar = calendar_mut(schedule, wanted)?;
    let kind = if shifts.is_empty() {
        "non-work"
    } else {
        "worked"
    };
    calendar.exceptions.insert(date, shifts);
    Ok(format!("{} is {kind} on {date}", calendar.name))
}

fn remove_exception(
    schedule: &mut Schedule,
    wanted: &str,
    date: &str,
) -> Result<String, EditError> {
    let date = parse_day(date)?;
    let id = calendar_named(schedule, wanted).ok_or_else(|| no_calendar(wanted))?;
    let base = schedule.calendars[&id]
        .base
        .and_then(|base| schedule.calendars.get(&base));
    let own = schedule.calendars[&id].exceptions.get(&date);
    if base.is_some_and(|base| base.exceptions.get(&date) == own && own.is_some()) {
        return refuse(
            "inherited-exception",
            format!("{date} comes from the calendar this one is based on"),
            "change the base calendar",
        );
    }
    let calendar = calendar_mut(schedule, wanted)?;
    if calendar.exceptions.remove(&date).is_none() {
        return refuse(
            "no-such-exception",
            format!("{} has no exception on {date}", calendar.name),
            "`calendar` lists a calendar's exceptions",
        );
    }
    Ok(format!(
        "{} follows its normal week on {date}",
        calendar.name
    ))
}

fn set_work_week(
    schedule: &mut Schedule,
    wanted: &str,
    day: Weekday,
    shifts: &[String],
) -> Result<String, EditError> {
    let shifts = parse_shifts(shifts)?;
    let calendar = calendar_mut(schedule, wanted)?;
    let others_work = Weekday::ALL
        .iter()
        .any(|other| *other != day && !calendar.work_week.day(*other).is_empty());
    if shifts.is_empty() && !others_work {
        return refuse(
            "no-working-time",
            format!("{} would have no working days", calendar.name),
            "keep at least one working day",
        );
    }
    let name = calendar.name.clone();
    *calendar.work_week.day_mut(day) = shifts;
    let mut report = format!("{name} work week changed on {day:?}");
    // Hours per day are kept apart from the week (TF-033), so say when they part.
    if let (Some(day), Some(working)) = (calendar.hours_per_period.day, even_day(calendar))
        && day != working
    {
        let _ = write!(
            report,
            "; its working days are {} hours, but a day still counts as {} (set its hours per day to change that)",
            hours_text(working),
            hours_text(day)
        );
    }
    Ok(report)
}

/// Minutes as hours, such as `10` or `7.5`.
fn hours_text(minutes: Minutes) -> String {
    #[allow(clippy::cast_precision_loss, reason = "a day's minutes are small")]
    let hours = minutes.get() as f64 / 60.0;
    format!("{hours}")
}

/// `hours` as whole minutes from one up to `most` hours, or `None`.
fn whole_minutes(hours: f64, most: i64) -> Option<Minutes> {
    let minutes = hours * 60.0;
    #[allow(clippy::cast_precision_loss, reason = "a few thousand hours")]
    let fits =
        (1.0..=(most * 60) as f64).contains(&minutes) && (minutes - minutes.round()).abs() < 1e-6;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "checked to be a whole number of minutes in range"
    )]
    fits.then(|| Minutes::new(minutes.round() as i64))
}

fn set_hours_per_day(
    schedule: &mut Schedule,
    wanted: &str,
    hours: f64,
) -> Result<String, EditError> {
    let Some(minutes) = whole_minutes(hours, 24) else {
        return refuse(
            "bad-hours-per-day",
            format!("{hours} isn't a number of hours in a day"),
            "give more than 0 and at most 24 hours, in whole minutes, such as 8 or 7.5",
        );
    };
    let calendar = calendar_mut(schedule, wanted)?;
    calendar.hours_per_period.day = Some(minutes);
    Ok(format!(
        "{} counts a day as {} hours",
        calendar.name,
        hours_text(minutes)
    ))
}

/// A period longer than a day, whose hours a calendar holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    /// A week: `CALENDAR.week_hr_cnt`.
    Week,
    /// A month: `CALENDAR.month_hr_cnt`.
    Month,
    /// A year: `CALENDAR.year_hr_cnt`.
    Year,
}

impl Period {
    /// Its name, and the most hours it can hold.
    const fn limits(self) -> (&'static str, i64) {
        match self {
            Self::Week => ("week", 7 * 24),
            Self::Month => ("month", 31 * 24),
            Self::Year => ("year", 366 * 24),
        }
    }
}

fn set_hours_per_period(
    schedule: &mut Schedule,
    wanted: &str,
    period: Period,
    hours: f64,
) -> Result<String, EditError> {
    let (name, most) = period.limits();
    let Some(minutes) = whole_minutes(hours, most) else {
        return refuse(
            "bad-hours-per-period",
            format!("{hours} isn't a number of hours in a {name}"),
            format!("give more than 0 and at most {most} hours, in whole minutes"),
        );
    };
    let calendar = calendar_mut(schedule, wanted)?;
    let slot = match period {
        Period::Week => &mut calendar.hours_per_period.week,
        Period::Month => &mut calendar.hours_per_period.month,
        Period::Year => &mut calendar.hours_per_period.year,
    };
    *slot = Some(minutes);
    Ok(format!(
        "{} counts a {name} as {} hours",
        calendar.name,
        hours_text(minutes)
    ))
}

/// What progress to record.
#[derive(Clone, Copy)]
struct Progress<'a> {
    start: Option<&'a str>,
    finish: Option<&'a str>,
    remaining: Option<Span>,
    percent: Option<f64>,
}

fn after_data_date(code: &str, what: &str, date: DateTime, data_date: DateTime) -> EditError {
    EditError {
        code: "actual-after-data-date",
        message: format!("{code}'s actual {what} of {date} is after the data date of {data_date}"),
        hint: "an actual has happened, so move the data date or correct the date".to_owned(),
    }
}

/// The actuals an edit leaves, checked against each other and the data date.
/// With no date given, nothing here can have changed, so nothing is checked.
fn progress_dates(
    schedule: &Schedule,
    a: &Activity,
    progress: Progress<'_>,
) -> Result<(Option<DateTime>, Option<DateTime>), EditError> {
    if progress.start.is_none() && progress.finish.is_none() {
        return Ok((a.actual_start, a.actual_finish));
    }
    let code = a.code.as_str();
    let mut start = progress
        .start
        .map(parse_date)
        .transpose()?
        .or(a.actual_start);
    let mut finish = progress
        .finish
        .map(parse_date)
        .transpose()?
        .or(a.actual_finish);
    if a.activity_type.is_milestone() {
        // A milestone happens at one moment.
        if start.is_some() && finish.is_some() && start != finish {
            return refuse(
                "milestone-dates",
                format!("{code} is a milestone, so its two actuals must match"),
                "give one date",
            );
        }
        start = start.or(finish);
        finish = finish.or(start);
    }
    let Some(data_date) = schedule.project.data_date else {
        return refuse(
            "no-data-date",
            "this project has no data date to measure actuals against",
            "give a data date too",
        );
    };
    for (what, date) in [("start", start), ("finish", finish)] {
        if let Some(date) = date.filter(|date| *date > data_date) {
            return Err(after_data_date(code, what, date, data_date));
        }
    }
    match (start, finish) {
        (None, Some(_)) => refuse(
            "finish-needs-start",
            format!("{code} can't finish without having started"),
            "give an actual start too",
        ),
        (Some(start), Some(finish)) if finish < start => refuse(
            "finish-before-start",
            format!("{code} finishes at {finish}, before it started at {start}"),
            "correct one of the dates",
        ),
        _ => Ok((start, finish)),
    }
}

/// The remaining duration an edit leaves, given the actuals it leaves.
fn progress_remaining(
    schedule: &Schedule,
    a: &Activity,
    progress: Progress<'_>,
    finish: Option<DateTime>,
    started: bool,
) -> Result<Option<Minutes>, EditError> {
    let code = a.code.as_str();
    if progress.remaining.is_some() && a.activity_type.is_milestone() {
        return refuse(
            "no-duration",
            format!("{code} is a milestone, which has no duration"),
            "leave remaining out",
        );
    }
    let remaining = progress
        .remaining
        .map(|span| {
            span.minutes_in(lengths_for(schedule, a).as_ref())
                .ok_or_else(|| no_length(code, span))
        })
        .transpose()?;
    if remaining.is_some_and(Minutes::is_negative) {
        return refuse(
            "negative-duration",
            format!("{code} can't have negative remaining work"),
            "give zero or more",
        );
    }
    if finish.is_some() && remaining.is_some_and(|r| r != Minutes::ZERO) {
        return refuse(
            "finished-work",
            format!("{code} is finished, so it has no remaining work"),
            "leave remaining out",
        );
    }
    let left = remaining.unwrap_or(a.remaining_duration);
    let working = finish.is_none() && started && !a.activity_type.is_milestone();
    if working && left == Minutes::ZERO && (progress.start.is_some() || remaining.is_some()) {
        return refuse(
            "finished-work",
            format!("{code} is in progress, so it needs remaining work"),
            "give an actual finish to finish it",
        );
    }
    Ok(remaining)
}

/// Remaining duration from a duration percent complete: original less that
/// share of it, to the minute (TF-020). Only on started, unfinished work
/// measured by duration percent, given a percent and no remaining; physical
/// and units percent don't touch remaining.
fn linked_remaining(
    a: &Activity,
    progress: Progress<'_>,
    started: bool,
    finish: Option<DateTime>,
) -> Option<Minutes> {
    let percent = progress.percent?;
    let linked = a.percent_complete_type == PercentCompleteType::Duration
        && progress.remaining.is_none()
        && started
        && finish.is_none()
        && !a.activity_type.is_milestone();
    if !linked {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "durations are far below 2^52 minutes"
    )]
    let left = (a.original_duration.get() as f64 * (100.0 - percent) / 100.0).round() as i64;
    Some(Minutes::new(left))
}

/// A suspension stays when progress changes, so it has to fit the new actual
/// start: a suspended activity keeps one, on or before its suspend date.
fn fits_suspension(a: &Activity, code: &str, start: Option<DateTime>) -> Result<(), EditError> {
    let Some(suspend) = a.suspend.or(a.resume) else {
        return Ok(());
    };
    match start {
        None => refuse(
            "suspended",
            format!("{code} is suspended, so it needs an actual start"),
            "clear its suspend and resume dates first",
        ),
        Some(start) if a.suspend.is_some_and(|s| s < start) => refuse(
            "suspend-before-start",
            format!("{code} was suspended {suspend}, before an actual start of {start}"),
            "clear or move its suspend dates first",
        ),
        Some(_) => Ok(()),
    }
}

fn set_progress(
    schedule: &mut Schedule,
    code: &str,
    progress: Progress<'_>,
) -> Result<String, EditError> {
    let id = find(schedule, code)?;
    let a = &schedule.activities[&id];
    if progress.start.is_none()
        && progress.finish.is_none()
        && progress.remaining.is_none()
        && progress.percent.is_none()
    {
        return refuse(
            "nothing-to-change",
            format!("the edit to {code} gives no progress"),
            "give an actual date, remaining duration or percent",
        );
    }
    if a.activity_type.is_span() {
        return refuse(
            "span-progress",
            format!("{code}'s progress comes from the activities it spans"),
            "record progress on those activities",
        );
    }
    if progress
        .percent
        .is_some_and(|p| !(0.0..=100.0).contains(&p))
    {
        return refuse(
            "invalid-percent",
            format!("{code} needs a percent from 0 to 100"),
            "give a number from 0 to 100",
        );
    }
    let (start, finish) = progress_dates(schedule, a, progress)?;
    if finish.is_some()
        && progress
            .percent
            .is_some_and(|p| (p - 100.0).abs() > f64::EPSILON)
    {
        return refuse(
            "finished-work",
            format!("{code} is finished, so it's 100% complete"),
            "leave percent out",
        );
    }
    let linked = linked_remaining(a, progress, start.is_some(), finish);
    if linked == Some(Minutes::ZERO) {
        return refuse(
            "finished-work",
            format!(
                "{code} measures percent by duration, and {}% leaves no remaining work",
                progress.percent.unwrap_or(100.0)
            ),
            "give an actual finish to finish it",
        );
    }
    fits_suspension(a, code, start)?;
    let remaining = progress_remaining(schedule, a, progress, finish, start.is_some())?.or(linked);
    let a = find_mut(schedule, code)?;
    a.actual_start = start;
    a.actual_finish = finish;
    if let Some(remaining) = remaining {
        a.remaining_duration = remaining;
    }
    if progress.percent.is_some() {
        a.physical_percent_complete = progress.percent;
    }
    a.status = match (start, finish) {
        (_, Some(_)) => ActivityStatus::Completed,
        (Some(_), None) => ActivityStatus::InProgress,
        (None, None) => ActivityStatus::NotStarted,
    };
    if a.status == ActivityStatus::Completed {
        a.remaining_duration = Minutes::ZERO;
        a.physical_percent_complete = Some(100.0);
        // Results finished work doesn't have, which recalculating wouldn't clear.
        a.stored.total_float = None;
        a.stored.free_float = None;
        a.stored.remaining_early_start = None;
        a.stored.remaining_early_finish = None;
        a.stored.remaining_late_start = None;
        a.stored.remaining_late_finish = None;
    }
    let status = match a.status {
        ActivityStatus::NotStarted => "not started",
        ActivityStatus::InProgress => "in progress",
        ActivityStatus::Completed => "completed",
    };
    Ok(format!(
        "{code} is {status}, with {} remaining",
        hours(a.remaining_duration)
    ))
}

/// Suspends an activity in progress until its resume date. Both dates are
/// needed: the engine can't calculate a suspended activity without a resume
/// date (TF-020).
fn set_suspend(
    schedule: &mut Schedule,
    code: &str,
    (suspend, resume): (&str, &str),
) -> Result<String, EditError> {
    let (suspend, resume) = (parse_date(suspend)?, parse_date(resume)?);
    let a = find_mut(schedule, code)?;
    if a.status != ActivityStatus::InProgress {
        let state = if a.status == ActivityStatus::NotStarted {
            "hasn't started"
        } else {
            "is finished"
        };
        return refuse(
            "not-in-progress",
            format!("{code} {state}, and only work in progress can be suspended"),
            "record its actual start first, or clear its finish",
        );
    }
    if a.actual_start.is_some_and(|start| suspend < start) {
        return refuse(
            "suspend-before-start",
            format!(
                "{code} started {}, after the suspend date {suspend}",
                a.actual_start.map(|d| d.to_string()).unwrap_or_default()
            ),
            "suspend it on or after its actual start",
        );
    }
    if resume <= suspend {
        return refuse(
            "resume-not-after-suspend",
            format!("{code}'s resume date {resume} isn't after its suspend date {suspend}"),
            "resume it later than it was suspended",
        );
    }
    a.suspend = Some(suspend);
    a.resume = Some(resume);
    Ok(format!("{code} suspended {suspend}, resuming {resume}"))
}

fn clear_suspend(schedule: &mut Schedule, code: &str) -> Result<String, EditError> {
    let a = find_mut(schedule, code)?;
    if a.suspend.is_none() && a.resume.is_none() {
        return refuse(
            "nothing-to-change",
            format!("{code} isn't suspended"),
            "only a suspended activity's dates can be cleared",
        );
    }
    a.suspend = None;
    a.resume = None;
    Ok(format!("{code} no longer suspended"))
}

fn clear_progress(schedule: &mut Schedule, code: &str) -> Result<String, EditError> {
    let a = find_mut(schedule, code)?;
    if a.status == ActivityStatus::NotStarted {
        return refuse(
            "nothing-to-change",
            format!("{code} hasn't started"),
            "only recorded progress can be cleared",
        );
    }
    if a.activity_type.is_span() {
        return refuse(
            "span-progress",
            format!("{code}'s progress comes from the activities it spans"),
            "clear progress on those activities",
        );
    }
    a.status = ActivityStatus::NotStarted;
    a.actual_start = None;
    a.actual_finish = None;
    a.suspend = None;
    a.resume = None;
    a.remaining_duration = a.original_duration;
    if a.physical_percent_complete.is_some() {
        a.physical_percent_complete = Some(0.0);
    }
    Ok(format!(
        "{code} is not started, with {} remaining",
        hours(a.remaining_duration)
    ))
}

pub(crate) fn find_wbs(schedule: &Schedule, wanted: &str) -> Result<WbsId, EditError> {
    let lower = wanted.to_lowercase();
    let same: Vec<_> = schedule
        .wbs
        .values()
        .filter(|node| node.code.to_lowercase() == lower)
        .collect();
    let exact: Vec<_> = same.iter().filter(|node| node.code == wanted).collect();
    match (exact.as_slice(), same.as_slice()) {
        ([one], _) => Ok(one.id),
        (_, [one]) => Ok(one.id),
        (_, []) => match wanted.parse().map(WbsId) {
            Ok(id) if schedule.wbs.contains_key(&id) => Ok(id),
            _ => refuse(
                "no-such-wbs",
                format!("no WBS node has the code {wanted}"),
                "`query --columns code,wbs` shows WBS codes",
            ),
        },
        _ => {
            let ids: Vec<String> = same.iter().map(|node| node.id.0.to_string()).collect();
            refuse(
                "ambiguous-wbs",
                format!("{} WBS nodes have the code {wanted}", same.len()),
                format!("use one's `wbs_id`: {}", ids.join(", ")),
            )
        }
    }
}

/// Twice the longest Activity ID in the corpus (TF-034, item 8).
const MAX_CODE: usize = 40;

/// What a new activity is.
struct New<'a> {
    code: &'a str,
    name: &'a str,
    wbs: &'a str,
    kind: ActivityType,
    calendar: Option<&'a str>,
    duration: Option<Span>,
}

fn add_activity(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    new: &New<'_>,
) -> Result<String, EditError> {
    let New { code, name, .. } = *new;
    if code.trim().is_empty()
        || code.chars().any(char::is_control)
        || code.chars().count() > MAX_CODE
        || code != code.trim()
        || code.contains(['=', '>', ';'])
    {
        return refuse(
            "invalid-activity-id",
            format!("{code:?} can't be an Activity ID"),
            format!(
                "use up to {MAX_CODE} characters on one line, without =, > or ;, and with no spaces at the ends"
            ),
        );
    }
    if schedule.activity_by_code(code).is_some() {
        return refuse(
            "duplicate-activity",
            format!("{code} is already in this project"),
            "choose an unused Activity ID",
        );
    }
    check_name(name)?;
    match (new.kind.is_milestone(), new.duration) {
        (true, Some(_)) => {
            return refuse(
                "no-duration",
                format!("{code} is a milestone, which has no duration"),
                "leave the duration out",
            );
        }
        (false, None) => {
            return refuse(
                "needs-duration",
                format!("{code} is work, so it needs a duration"),
                "give a duration such as 5d",
            );
        }
        _ => {}
    }
    let wbs = find_wbs(schedule, new.wbs)?;
    let calendar = match new.calendar {
        Some(wanted) => find_calendar(schedule, wanted)?.id,
        None => match schedule.project.default_calendar {
            Some(calendar) => calendar,
            None => {
                return refuse(
                    "needs-calendar",
                    format!("this project has no default calendar for {code}"),
                    "name a calendar",
                );
            }
        },
    };
    let lengths = if new.kind == ActivityType::ResourceDependent {
        None
    } else {
        lengths_of(schedule, Some(calendar))
    };
    let duration = new
        .duration
        .map(|span| {
            span.minutes_in(lengths.as_ref())
                .ok_or_else(|| no_length(code, span))
        })
        .transpose()?
        .unwrap_or(Minutes::ZERO);
    if duration.is_negative() {
        return refuse(
            "negative-duration",
            format!("{code} can't have a negative duration"),
            "give zero or more",
        );
    }
    let free = schedule
        .activities
        .keys()
        .next_back()
        .map_or(0, |last| last.0 + 1);
    let id = ActivityId(ids.activity.max(free));
    ids.activity = id.0 + 1;
    let mut activity = Activity::new(id, schedule.project.id, wbs, calendar, code, name);
    activity.activity_type = new.kind;
    activity.original_duration = duration;
    activity.remaining_duration = duration;
    schedule.activities.insert(id, activity);
    Ok(format!(
        "{code} added, {} long, with no links yet",
        hours(duration)
    ))
}

fn delete_activity(schedule: &mut Schedule, code: &str) -> Result<String, EditError> {
    let id = find(schedule, code)?;
    if schedule.project.options.float_paths.end_activity == Some(id) {
        return refuse(
            "float-path-end",
            format!("float paths end at {code}"),
            "choose another end activity in P6 first",
        );
    }
    let links: Vec<Relationship> = schedule
        .relationships
        .values()
        .filter(|r| r.predecessor == id || r.successor == id)
        .cloned()
        .collect();
    if links.iter().any(|link| schedule.is_external(link)) {
        return refuse(
            "other-project",
            format!("{code} links to another project"),
            "remove that link first, naming its ends PROJECT:CODE",
        );
    }
    let held = schedule.assignments.len();
    schedule
        .relationships
        .retain(|_, r| r.predecessor != id && r.successor != id);
    schedule.assignments.retain(|_, a| a.activity != id);
    let assignments = held - schedule.assignments.len();
    let held = schedule.expenses.len();
    schedule.expenses.retain(|_, x| x.activity != id);
    let expenses = held - schedule.expenses.len();
    let actuals = schedule
        .activities
        .remove(&id)
        .is_some_and(|a| a.actual_start.is_some() || a.actual_finish.is_some());
    let opened = |near: fn(&Relationship) -> ActivityId, far: fn(&Relationship) -> ActivityId| {
        let others: BTreeSet<ActivityId> = links
            .iter()
            .filter(|link| near(link) == id)
            .map(far)
            .collect();
        others
            .into_iter()
            .filter(|other| !schedule.relationships.values().any(|r| far(r) == *other))
            .filter_map(|other| schedule.activities.get(&other))
            .map(|a| a.code.as_str())
            .collect::<Vec<_>>()
    };
    let no_predecessor = opened(|r| r.predecessor, |r| r.successor);
    let no_successor = opened(|r| r.successor, |r| r.predecessor);
    let count = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
    let mut line = format!(
        "{code} deleted, with {} and {}",
        count(links.len(), "link"),
        count(assignments, "assignment")
    );
    if expenses > 0 {
        let _ = write!(line, "; {} gone with it", count(expenses, "expense"));
    }
    if actuals {
        line.push_str("; its actual dates are gone");
    }
    if !no_predecessor.is_empty() {
        let _ = write!(line, "; no predecessor now: {}", no_predecessor.join(", "));
    }
    if !no_successor.is_empty() {
        let _ = write!(line, "; no successor now: {}", no_successor.join(", "));
    }
    Ok(line)
}

fn repeat_refusal<T>(from: &str, to: &str, kind: RelationshipType) -> Result<T, EditError> {
    refuse(
        "duplicate-link",
        format!("{from} to {to} already has a {} link", link_label(kind)),
        "use change_link to change its lag",
    )
}

fn loop_refusal<T>(from: &str, to: &str, codes: &[&str]) -> Result<T, EditError> {
    refuse(
        "circular-logic",
        format!(
            "{from} to {to} would close a loop: {} back to {to}",
            codes.join(" to ")
        ),
        "remove a link in that loop, or link the other way",
    )
}

/// An ID above the caller's mark and above everything in these projects, so
/// nothing is replaced.
fn new_link_id(ids: &mut NextIds, schedules: &[&Schedule]) -> RelationshipId {
    let free = schedules
        .iter()
        .filter_map(|s| s.relationships.keys().next_back())
        .map(|last| last.0 + 1)
        .max()
        .unwrap_or(0);
    let id = RelationshipId(ids.relationship.max(free));
    ids.relationship = id.0 + 1;
    id
}

fn add_link(
    schedule: &mut Schedule,
    ids: &mut NextIds,
    (from, to): (&str, &str),
    kind: Option<LinkType>,
    lag: Option<Span>,
) -> Result<String, EditError> {
    let predecessor = find(schedule, from)?;
    let successor = find(schedule, to)?;
    if predecessor == successor {
        return refuse(
            "self-link",
            format!("{from} can't depend on itself"),
            "link two different activities",
        );
    }
    let kind: RelationshipType = kind.unwrap_or(LinkType::FS).into();
    let repeat = schedule
        .relationships
        .values()
        .any(|r| r.predecessor == predecessor && r.successor == successor && r.kind == kind);
    if repeat {
        return repeat_refusal(from, to, kind);
    }
    if let Some(path) = route(schedule.relationships.values(), successor, predecessor) {
        let codes: Vec<&str> = path
            .iter()
            .filter_map(|id| schedule.activities.get(id))
            .map(|a| a.code.as_str())
            .collect();
        return loop_refusal(from, to, &codes);
    }
    let lengths = lag_lengths(
        schedule,
        &schedule.activities[&predecessor],
        &schedule.activities[&successor],
    );
    let lag = lag
        .map(|lag| {
            lag.minutes_in(lengths.as_ref())
                .ok_or_else(|| no_length(from, lag))
        })
        .transpose()?
        .unwrap_or(Minutes::ZERO);
    let id = new_link_id(ids, &[schedule]);
    let project = schedule.project.id;
    schedule.relationships.insert(
        id,
        Relationship {
            id,
            predecessor,
            successor,
            predecessor_project: project,
            successor_project: project,
            kind,
            lag,
            source: None,
        },
    );
    Ok(format!(
        "{from} to {to} {} added, lag {}",
        link_label(kind),
        hours(lag)
    ))
}

fn link_between(
    schedule: &Schedule,
    (predecessor, successor): (ActivityId, ActivityId),
    (from, to): (&str, &str),
    kind: Option<LinkType>,
) -> Result<RelationshipId, EditError> {
    let kind: Option<RelationshipType> = kind.map(Into::into);
    let found: Vec<&Relationship> = schedule
        .relationships
        .values()
        .filter(|r| r.predecessor == predecessor && r.successor == successor)
        .filter(|r| kind.is_none_or(|kind| kind == r.kind))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.id),
        [] => refuse(
            "no-such-link",
            format!("{from} doesn't lead to {to} by such a link"),
            format!("`query --succs {from}` lists its links"),
        ),
        _ => refuse(
            "ambiguous-link",
            format!("{from} to {to} has {} links", found.len()),
            "give link_type to say which",
        ),
    }
}

fn the_link(
    schedule: &Schedule,
    ends: (&str, &str),
    kind: Option<LinkType>,
) -> Result<RelationshipId, EditError> {
    let (from, to) = ends;
    let held = (find(schedule, from)?, find(schedule, to)?);
    let id = link_between(schedule, held, ends, kind)?;
    if schedule.is_external(&schedule.relationships[&id]) {
        return refuse(
            "other-project",
            "that link reaches another project",
            "name its ends PROJECT:CODE, so both projects' copies change",
        );
    }
    Ok(id)
}

/// The link as it should now read. `lengths` are the day, week, month and
/// year its lag is counted in.
fn revised(
    schedule: &Schedule,
    id: RelationshipId,
    (from, to): (&str, &str),
    (new_type, lag): (Option<LinkType>, Option<Span>),
    lengths: Option<HoursPerPeriod>,
) -> Result<Relationship, EditError> {
    if new_type.is_none() && lag.is_none() {
        return refuse(
            "nothing-to-change",
            format!("the edit to {from} to {to} gives no change"),
            "give new_type, lag or both",
        );
    }
    let mut link = schedule.relationships[&id].clone();
    let new_type: RelationshipType = new_type.map_or(link.kind, Into::into);
    let repeat = schedule.relationships.values().any(|r| {
        r.id != id
            && r.predecessor == link.predecessor
            && r.successor == link.successor
            && r.kind == new_type
    });
    if repeat {
        return refuse(
            "duplicate-link",
            format!("{from} to {to} already has a {} link", link_label(new_type)),
            "remove that link first",
        );
    }
    let lag = lag
        .map(|lag| {
            lag.minutes_in(lengths.as_ref())
                .ok_or_else(|| no_length(from, lag))
        })
        .transpose()?;
    link.kind = new_type;
    link.lag = lag.unwrap_or(link.lag);
    Ok(link)
}

fn change_link(
    schedule: &mut Schedule,
    ends: (&str, &str),
    kind: Option<LinkType>,
    change: (Option<LinkType>, Option<Span>),
) -> Result<String, EditError> {
    let (from, to) = ends;
    let id = the_link(schedule, ends, kind)?;
    let link = &schedule.relationships[&id];
    let lengths = lag_lengths(
        schedule,
        &schedule.activities[&link.predecessor],
        &schedule.activities[&link.successor],
    );
    let link = revised(schedule, id, ends, change, lengths)?;
    let summary = format!(
        "{from} to {to} is now {}, lag {}",
        link_label(link.kind),
        hours(link.lag)
    );
    schedule.relationships.insert(id, link);
    Ok(summary)
}

/// Refuses a link that would close a loop, which can run through any project.
fn no_loop_across(
    schedules: &[Schedule],
    (predecessor, successor): (ActivityId, ActivityId),
    (from, to): (&str, &str),
) -> Result<(), EditError> {
    let links = schedules.iter().flat_map(|s| s.relationships.values());
    let Some(path) = route(links, successor, predecessor) else {
        return Ok(());
    };
    let code = |id: &ActivityId| {
        let mut holders = schedules.iter().filter_map(|s| s.activities.get(id));
        holders.next().map(|a| a.code.as_str())
    };
    let codes: Vec<&str> = path.iter().filter_map(code).collect();
    loop_refusal(from, to, &codes)
}

/// Refuses a link added inside one project that would close a loop through
/// another. [`apply`] sees only its own project, so the caller holding the
/// others asks this first.
///
/// # Errors
///
/// `circular-logic`, naming the loop.
pub fn check_loop_across(
    schedules: &[Schedule],
    index: usize,
    edit: &Edit,
) -> Result<(), EditError> {
    let Edit::AddLink {
        predecessor,
        successor,
        ..
    } = edit
    else {
        return Ok(());
    };
    let Some(schedule) = schedules.get(index) else {
        return Ok(());
    };
    // An end that isn't there is for `apply` to refuse.
    let (Some(before), Some(after)) = (
        schedule.activity_by_code(predecessor),
        schedule.activity_by_code(successor),
    ) else {
        return Ok(());
    };
    no_loop_across(schedules, (before.id, after.id), (predecessor, successor))
}

/// The two projects a link runs between, and the activities at its ends.
struct Between<'a> {
    before: &'a Schedule,
    after: &'a Schedule,
    predecessor: &'a Activity,
    successor: &'a Activity,
    /// The ends as `PROJECT:CODE`.
    labels: (String, String),
}

impl<'a> Between<'a> {
    fn new(
        schedules: &'a [Schedule],
        (first, second): (usize, usize),
        (from, to): (&str, &str),
    ) -> Result<Self, EditError> {
        let (before, after) = (&schedules[first], &schedules[second]);
        Ok(Self {
            before,
            after,
            predecessor: find_ref(before, from)?,
            successor: find_ref(after, to)?,
            labels: (
                format!("{}:{from}", before.project.short_name),
                format!("{}:{to}", after.project.short_name),
            ),
        })
    }

    fn labels(&self) -> (&str, &str) {
        (&self.labels.0, &self.labels.1)
    }

    fn the_link(&self, kind: Option<LinkType>) -> Result<RelationshipId, EditError> {
        let held = (self.predecessor.id, self.successor.id);
        link_between(self.before, held, self.labels(), kind)
    }

    fn lengths(&self) -> Option<HoursPerPeriod> {
        lag_lengths_between(
            (self.before, self.predecessor),
            (self.after, self.successor),
        )
    }

    /// A new link, or a refusal saying why it can't be. Its loop can run through
    /// any project in the file.
    fn added(
        &self,
        schedules: &[Schedule],
        kind: Option<LinkType>,
        lag: Option<Span>,
        ids: &mut NextIds,
    ) -> Result<Relationship, EditError> {
        let (from, to) = self.labels();
        let (predecessor, successor) = (self.predecessor.id, self.successor.id);
        let kind: RelationshipType = kind.unwrap_or(LinkType::FS).into();
        let repeat =
            self.before.relationships.values().any(|r| {
                r.predecessor == predecessor && r.successor == successor && r.kind == kind
            });
        if repeat {
            return repeat_refusal(from, to, kind);
        }
        no_loop_across(schedules, (predecessor, successor), (from, to))?;
        let lag = lag
            .map(|lag| {
                lag.minutes_in(self.lengths().as_ref())
                    .ok_or_else(|| no_length(from, lag))
            })
            .transpose()?
            .unwrap_or(Minutes::ZERO);
        Ok(Relationship {
            id: new_link_id(ids, &[self.before, self.after]),
            predecessor,
            successor,
            predecessor_project: self.before.project.id,
            successor_project: self.after.project.id,
            kind,
            lag,
            source: None,
        })
    }
}

/// Makes a link edit between two projects, in both their copies of the link.
/// `projects` is where in `schedules` the predecessor and successor are, and the
/// edit names them by plain Activity ID. Returns a line saying what changed, and
/// changes nothing if it refuses.
///
/// # Errors
///
/// [`EditError`], saying what's wrong and what to try.
pub fn apply_between(
    schedules: &mut [Schedule],
    edit: &Edit,
    projects: (usize, usize),
    ids: &mut NextIds,
) -> Result<String, EditError> {
    let (first, second) = projects;
    let (link, summary, gone) = match edit {
        Edit::AddLink {
            predecessor,
            successor,
            link_type,
            lag,
        } => {
            let between = Between::new(schedules, projects, (predecessor, successor))?;
            let link = between.added(schedules, *link_type, *lag, ids)?;
            let (from, to) = between.labels();
            let summary = format!(
                "{from} to {to} {} added, lag {}",
                link_label(link.kind),
                hours(link.lag)
            );
            (Some(link), summary, None)
        }
        Edit::RemoveLink {
            predecessor,
            successor,
            link_type,
        } => {
            let between = Between::new(schedules, projects, (predecessor, successor))?;
            let id = between.the_link(*link_type)?;
            let (from, to) = between.labels();
            (None, format!("{from} to {to} removed"), Some(id))
        }
        Edit::ChangeLink {
            predecessor,
            successor,
            link_type,
            new_type,
            lag,
        } => {
            let between = Between::new(schedules, projects, (predecessor, successor))?;
            let id = between.the_link(*link_type)?;
            let change = (*new_type, *lag);
            let link = revised(
                between.before,
                id,
                between.labels(),
                change,
                between.lengths(),
            )?;
            let (from, to) = between.labels();
            let summary = format!(
                "{from} to {to} is now {}, lag {}",
                link_label(link.kind),
                hours(link.lag)
            );
            (Some(link), summary, None)
        }
        _ => {
            return refuse(
                "not-a-link",
                "only link edits can span two projects",
                "name one project's activities",
            );
        }
    };
    for index in [first, second] {
        let relationships = &mut schedules[index].relationships;
        if let Some(link) = &link {
            relationships.insert(link.id, link.clone());
        }
        if let Some(id) = gone {
            relationships.remove(&id);
        }
    }
    Ok(summary)
}

/// Makes one edit, or refuses it and leaves `schedule` as it was. Returns a
/// line saying what changed.
///
/// # Errors
///
/// [`EditError`], saying what's wrong and what to try. An edit that changes a
/// duration also moves its assignments' units and costs, by the activity's
/// duration type (TF-038), and the line says what moved.
pub fn apply(schedule: &mut Schedule, edit: &Edit, ids: &mut NextIds) -> Result<String, EditError> {
    let changes_duration = matches!(
        edit,
        Edit::SetDuration { .. }
            | Edit::SetType { .. }
            | Edit::SetProgress { .. }
            | Edit::ClearProgress { .. }
    );
    let named = if changes_duration {
        activities_named(edit)
    } else {
        Vec::new()
    };
    let held: Vec<(ActivityId, (Minutes, Minutes))> = named
        .into_iter()
        .filter_map(|code| schedule.activity_by_code(code))
        .map(|a| (a.id, (a.original_duration, a.remaining_duration)))
        .collect();
    // A new percent, compared with the one stored before the edit: text and
    // progress tables carry the stored one back, which changes nothing.
    let new_percent = match edit {
        Edit::SetProgress {
            activity,
            percent: Some(percent),
            ..
        } => schedule
            .activity_by_code(activity)
            .filter(|a| {
                a.physical_percent_complete
                    .is_none_or(|was| (was - percent).abs() >= 0.005)
            })
            .map(|a| (a.id, *percent)),
        _ => None,
    };
    let mut line = apply_one(schedule, edit, ids)?;
    let split = new_percent.filter(|(id, _)| schedule.activities.get(id).is_some_and(splits_units));
    for (id, was) in held {
        // The assignments the percent splits are set at the new durations
        // below; the rest follow them as usual.
        let counted_split = split.is_some_and(|(at, _)| at == id);
        for moved in units::follow_durations(schedule, id, was, counted_split) {
            let _ = write!(line, "; {moved}");
        }
    }
    if let Some((id, percent)) = split {
        for moved in units::follow_units_percent(schedule, id, percent) {
            let _ = write!(line, "; {moved}");
        }
    }
    Ok(line)
}

/// True where a new progress percent splits an activity's units: it's
/// measured by units percent and under way (docs/rules/TF-004-units-percent-complete.md).
#[must_use]
pub fn splits_units(a: &Activity) -> bool {
    a.percent_complete_type == PercentCompleteType::Units
        && a.status == ActivityStatus::InProgress
        && !a.activity_type.is_milestone()
}

#[allow(
    clippy::too_many_lines,
    reason = "one short arm per edit, and each does its work in its own function"
)]
fn apply_one(schedule: &mut Schedule, edit: &Edit, ids: &mut NextIds) -> Result<String, EditError> {
    match edit {
        Edit::SetDuration {
            activity,
            original,
            remaining,
        } => set_duration(schedule, activity, *original, *remaining),
        Edit::SetConstraint {
            activity,
            constraint,
            date,
            secondary,
        } => set_constraint(
            schedule,
            activity,
            (*constraint).into(),
            date.as_deref(),
            *secondary,
        ),
        Edit::ClearConstraint {
            activity,
            secondary,
        } => clear_constraint(schedule, activity, *secondary),
        Edit::SetCalendar { activity, calendar } => set_calendar(schedule, activity, calendar),
        Edit::SetName { activity, name } => set_name(schedule, activity, name),
        Edit::SetType {
            activity,
            activity_type,
            duration,
        } => set_type(schedule, activity, (*activity_type).into(), *duration),
        Edit::SetExpectedFinish { activity, date } => {
            set_expected_finish(schedule, activity, date.as_deref())
        }
        Edit::SetProgress {
            activity,
            actual_start,
            actual_finish,
            remaining,
            percent,
        } => set_progress(
            schedule,
            activity,
            Progress {
                start: actual_start.as_deref(),
                finish: actual_finish.as_deref(),
                remaining: *remaining,
                percent: *percent,
            },
        ),
        Edit::ClearProgress { activity } => clear_progress(schedule, activity),
        Edit::SetSuspend {
            activity,
            suspend,
            resume,
        } => set_suspend(schedule, activity, (suspend, resume)),
        Edit::ClearSuspend { activity } => clear_suspend(schedule, activity),
        Edit::AddWbs { code, name, parent } => add_wbs(schedule, ids, code, name, parent),
        Edit::RenameWbs { wbs, code, name } => {
            rename_wbs(schedule, wbs, code.as_deref(), name.as_deref())
        }
        Edit::MoveWbs { wbs, parent } => move_wbs(schedule, wbs, parent),
        Edit::SetWbs { activity, wbs } => set_wbs(schedule, activity, wbs),
        Edit::AddResource {
            resource,
            name,
            resource_type,
            units_per_hour,
            calendar,
            max_units_per_hour,
        } => add_resource(
            schedule,
            ids,
            (resource, name),
            resource_type.map_or(ResourceType::Labor, Into::into),
            (*units_per_hour, calendar.as_deref(), *max_units_per_hour),
        ),
        Edit::SetResourceLimit {
            resource,
            max_units_per_hour,
            from,
        } => set_resource_limit(
            schedule,
            ids,
            resource,
            (*max_units_per_hour, from.as_deref()),
        ),
        Edit::AssignResource {
            activity,
            resource,
            units,
        } => assign_resource(schedule, ids, (activity, resource), *units),
        Edit::UnassignResource { activity, resource } => {
            unassign_resource(schedule, activity, resource)
        }
        Edit::SetAssignmentUnits {
            activity,
            resource,
            actual_units,
            remaining_units,
        } => set_assignment_units(
            schedule,
            (activity, resource),
            (*actual_units, *remaining_units),
        ),
        Edit::SetCurve {
            activity,
            resource,
            curve,
        } => spreads::set_curve(schedule, (activity, resource), curve.as_deref()),
        Edit::SetSpread {
            activity,
            resource,
            period,
            start,
            units,
        } => spreads::set_spread(
            schedule,
            (activity, resource),
            &spreads::Typed {
                period: *period,
                start,
                units,
            },
        ),
        Edit::AddRole {
            role,
            name,
            parent,
            max_units_per_hour,
        } => add_role(
            schedule,
            ids,
            (role, name),
            (parent.as_deref(), *max_units_per_hour),
        ),
        Edit::SetRoleLimit {
            role,
            max_units_per_hour,
            from,
        } => set_role_limit(schedule, ids, role, (*max_units_per_hour, from.as_deref())),
        Edit::AssignRole {
            activity,
            role,
            units,
        } => assign_role(schedule, ids, (activity, role), *units),
        Edit::UnassignRole { activity, role } => unassign_role(schedule, activity, role),
        Edit::AddExpense {
            activity,
            expense,
            cost,
            account,
            category,
            accrual,
        } => expenses::add_expense(
            schedule,
            ids,
            &expenses::NewExpense {
                activity,
                name: expense,
                cost: *cost,
                account: account.as_deref(),
                category: category.as_deref(),
                accrual: *accrual,
            },
        ),
        Edit::SetExpense {
            activity,
            expense,
            budgeted_cost,
            actual_cost,
            remaining_cost,
            accrual,
        } => expenses::set_expense(
            schedule,
            (activity, expense),
            expenses::Costs {
                budgeted: *budgeted_cost,
                actual: *actual_cost,
                remaining: *remaining_cost,
                accrual: *accrual,
            },
        ),
        Edit::DeleteExpense { activity, expense } => {
            expenses::delete_expense(schedule, (activity, expense))
        }
        Edit::SetCostAccount {
            activity,
            resource,
            role,
            expense,
            account,
        } => expenses::set_cost_account(
            schedule,
            activity,
            (resource.as_deref(), role.as_deref(), expense.as_deref()),
            account.as_deref(),
        ),
        Edit::SetCode {
            activity,
            code_type,
            value,
        } => set_code(schedule, activity, code_type, value),
        Edit::ClearCode {
            activity,
            code_type,
        } => clear_code(schedule, activity, code_type),
        Edit::AddCodeType {
            name,
            scope,
            max_length,
        } => add_code_type(schedule, ids, name, *scope, *max_length),
        Edit::AddCodeValue {
            code_type,
            value,
            description,
        } => add_code_value(schedule, ids, code_type, value, description.as_deref()),
        Edit::SetUdf {
            field,
            value,
            activity,
            wbs,
            resource,
        } => udfs::set_udf(
            schedule,
            udfs::Target {
                activity: activity.as_deref(),
                wbs: wbs.as_deref(),
                resource: resource.as_deref(),
            },
            field,
            value,
        ),
        Edit::ClearUdf {
            field,
            activity,
            wbs,
            resource,
        } => udfs::clear_udf(
            schedule,
            udfs::Target {
                activity: activity.as_deref(),
                wbs: wbs.as_deref(),
                resource: resource.as_deref(),
            },
            field,
        ),
        Edit::AddUdfType {
            field,
            applies_to,
            data_type,
        } => udfs::add_udf_type(schedule, ids, field, *applies_to, *data_type),
        Edit::AddStep {
            activity,
            step,
            weight,
            percent,
        } => steps::add_step(
            schedule,
            ids,
            activity,
            &steps::Change {
                name: Some(step),
                weight: *weight,
                percent: *percent,
            },
        ),
        Edit::SetStep {
            activity,
            step,
            name,
            weight,
            percent,
        } => steps::set_step(
            schedule,
            activity,
            step,
            &steps::Change {
                name: name.as_deref(),
                weight: *weight,
                percent: *percent,
            },
        ),
        Edit::CompleteStep { activity, step } => steps::complete_step(schedule, activity, step),
        Edit::DeleteStep { activity, step } => steps::delete_step(schedule, activity, step),
        Edit::AddNote {
            activity,
            wbs,
            topic,
            text,
        } => notes::change_note(
            schedule,
            ids,
            notes::Target {
                activity: activity.as_deref(),
                wbs: wbs.as_deref(),
            },
            topic,
            &notes::Change::Add(text),
        ),
        Edit::SetNote {
            activity,
            wbs,
            topic,
            text,
        } => notes::change_note(
            schedule,
            ids,
            notes::Target {
                activity: activity.as_deref(),
                wbs: wbs.as_deref(),
            },
            topic,
            &notes::Change::Set(text),
        ),
        Edit::DeleteNote {
            activity,
            wbs,
            topic,
        } => notes::change_note(
            schedule,
            ids,
            notes::Target {
                activity: activity.as_deref(),
                wbs: wbs.as_deref(),
            },
            topic,
            &notes::Change::Delete,
        ),
        Edit::AddObs {
            obs,
            parent,
            description,
        } => obs::add_obs(
            schedule,
            ids,
            obs,
            parent.as_deref(),
            description.as_deref(),
        ),
        Edit::SetWbsObs { wbs, obs } => obs::set_wbs_obs(schedule, wbs, obs),
        Edit::SetProjectCode { code_type, value } => {
            project_codes::set_project_code(schedule, code_type, value)
        }
        Edit::ClearProjectCode { code_type } => {
            project_codes::clear_project_code(schedule, code_type)
        }
        Edit::AddProjectCodeType { name, max_length } => {
            project_codes::add_project_code_type(schedule, ids, name, *max_length)
        }
        Edit::AddProjectCodeValue {
            code_type,
            value,
            description,
        } => project_codes::add_project_code_value(
            schedule,
            ids,
            code_type,
            value,
            description.as_deref(),
        ),
        Edit::AddCalendar { name, copy_of } => {
            add_calendar(schedule, ids, name, copy_of.as_deref())
        }
        Edit::AddCalendarException {
            calendar,
            date,
            shifts,
        } => add_exception(schedule, calendar, date, shifts),
        Edit::RemoveCalendarException { calendar, date } => {
            remove_exception(schedule, calendar, date)
        }
        Edit::SetWorkWeek {
            calendar,
            day,
            shifts,
        } => set_work_week(schedule, calendar, (*day).into(), shifts),
        Edit::SetHoursPerDay { calendar, hours } => set_hours_per_day(schedule, calendar, *hours),
        Edit::SetHoursPerPeriod {
            calendar,
            period,
            hours,
        } => set_hours_per_period(schedule, calendar, *period, *hours),
        Edit::AddActivity {
            code,
            name,
            wbs,
            activity_type,
            calendar,
            duration,
        } => add_activity(
            schedule,
            ids,
            &New {
                code,
                name,
                wbs,
                kind: activity_type.map_or(ActivityType::TaskDependent, Into::into),
                calendar: calendar.as_deref(),
                duration: *duration,
            },
        ),
        Edit::DeleteActivity { activity } => delete_activity(schedule, activity),
        Edit::AddLink {
            predecessor,
            successor,
            link_type,
            lag,
        } => add_link(schedule, ids, (predecessor, successor), *link_type, *lag),
        Edit::RemoveLink {
            predecessor,
            successor,
            link_type,
        } => {
            let id = the_link(schedule, (predecessor, successor), *link_type)?;
            schedule.relationships.remove(&id);
            Ok(format!("{predecessor} to {successor} removed"))
        }
        Edit::ChangeLink {
            predecessor,
            successor,
            link_type,
            new_type,
            lag,
        } => change_link(
            schedule,
            (predecessor, successor),
            *link_type,
            (*new_type, *lag),
        ),
    }
}

#[cfg(test)]
mod tests;
