//! Patch writer: apply an edited [`Schedule`] to the XER it came from,
//! changing only what was edited (ARCHITECTURE.md, Lossless round trips).
//!
//! [`patch`] compares an edited schedule with the one mapped from the original
//! file, and edits a copy of the [`RawXer`]. [`patch_all`] does several at once:
//!
//! - **Changed fields** of activities, relationships and the project's dates:
//!   only cells whose formatted value differs are rewritten; every other byte
//!   of the row is kept.
//! - **Added** activities and relationships are appended to their table. Their
//!   IDs must be above the table's current maximum, so they can't collide with
//!   rows the model omitted.
//! - **Deleted** activities are removed together with every row that depends on
//!   them: rows in any table whose `task_id` matches, relationships whose
//!   predecessor matches, and UDF values of the removed activities and
//!   assignments. Deleted relationships are removed.
//!
//! Values are written the way P6 writes them: dates as `YYYY-MM-DD HH:MM`,
//! durations, lags and float as decimal hours, flags as `Y`/`N`. Nonterminating
//! hours use six decimal places, recognized exactly on read-back (TF-012).
//! Unchanged source spellings are preserved. A calendar's working time is
//! written to `clndr_data`, only for a calendar that changed. Edits to anything
//! this writer doesn't handle yet (deleting WBS nodes, other calendar fields, resources,
//! changes to an assignment, code types, UDFs, schedule options) are reported as
//! [`PatchError::Unsupported`], never ignored.

use std::collections::{BTreeMap, BTreeSet};

use totalfloat_model::{self as m, Schedule};

use crate::mapping::enums;
use crate::raw::{EditError, RawXer};
use crate::spread_data;
use crate::to_model;
use crate::typed::RowRef;

/// Why a schedule couldn't be written back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatchError {
    /// The original file has no representable project with this ID.
    #[error("the original file has no project {0}")]
    NoSuchProject(m::ProjectId),
    /// The edit touches something the patch writer doesn't handle yet.
    #[error("changes to {0} are not supported by the patch writer yet")]
    Unsupported(&'static str),
    /// A value can't be written exactly in XER form.
    #[error("{table}.{field} of {entity} can't be written exactly: {reason}")]
    Unrepresentable {
        /// XER table.
        table: &'static str,
        /// XER field.
        field: &'static str,
        /// The entity, e.g. `activity 101`.
        entity: String,
        /// Why.
        reason: &'static str,
    },
    /// An added entity's ID isn't above every existing ID in its table.
    #[error("new {table} ID {id} must be above the table's maximum ({max})")]
    IdNotAboveMaximum {
        /// XER table.
        table: &'static str,
        /// The new ID.
        id: u64,
        /// The largest ID already in the file.
        max: u64,
    },
    /// The file has no table to append to.
    #[error("the file has no {0} table")]
    MissingTable(&'static str),
    /// A value needs a column the file's table doesn't have.
    #[error("the file's {table} table has no {field} column")]
    MissingColumn {
        /// XER table.
        table: &'static str,
        /// XER field.
        field: &'static str,
    },
    /// The model entity has no usable link to its source row.
    #[error("{0} has no source row")]
    NoSourceRow(String),
    /// A changed or new relationship points at an activity that isn't in the
    /// edited schedule.
    #[error("relationship {0} points at an activity that isn't in the schedule")]
    DanglingRelationship(m::RelationshipId),
    /// An activity names a project, WBS or calendar that isn't in the file.
    #[error("activity {0} names a project, WBS or calendar that isn't in the file")]
    DanglingActivity(m::ActivityId),
    /// The raw editor rejected a value.
    #[error(transparent)]
    Edit(#[from] EditError),
}

/// Applies `edited` to a copy of `raw` and returns it. Write it with
/// [`RawXer::write`]. With no changes, the result writes back byte-identical.
///
/// # Errors
///
/// [`PatchError`]; on error nothing is returned and `raw` is untouched.
pub fn patch(raw: &RawXer, edited: &Schedule) -> Result<RawXer, PatchError> {
    let (schedules, _) = to_model(raw);
    let mut patcher = Patcher::new(raw, &schedules);
    patcher.apply(edited)?;
    patcher.finish()
}

/// [`patch`], with new IDs measured against `original`, the file before this run
/// of edits. Projects are patched in file order but IDs come in edit order, so a
/// later project can hold a lower new ID than one patched before it.
fn patch_over(
    raw: &RawXer,
    (original, first): (&RawXer, &[Schedule]),
    edited: &Schedule,
) -> Result<RawXer, PatchError> {
    let (schedules, _) = to_model(raw);
    let mut patcher = Patcher::new(raw, &schedules);
    patcher.original = original;
    patcher.first = first;
    patcher.apply(edited)?;
    patcher.finish()
}

/// Whether `raw` has a column for every field the writer keeps for projects
/// and activities, so writing any change to them, such as a calculation's
/// results, can't be refused for a missing column.
#[must_use]
pub fn holds_every_project_and_activity_column(raw: &RawXer) -> bool {
    let all = |table: &str, fields: &[&str]| {
        raw.tables()
            .iter()
            .filter(|t| t.name == table)
            .all(|t| fields.iter().all(|field| t.field_index(field).is_some()))
    };
    all("PROJECT", &PROJECT_FIELDS.map(|(field, _)| field))
        && all("TASK", &TASK_FIELDS.map(|(field, _)| field))
}

/// Applies edited schedules to one copy of `raw`, giving the bytes of patching
/// them one after another. `base` must be what [`to_model`] gives for `raw`; a
/// caller holding edited schedules has usually mapped the file already.
///
/// Edits that only change rows share one copy and one mapping. A later edit can
/// depend on an earlier one's removed rows, or on an earlier edit of the same
/// project, so those go one at a time.
///
/// # Errors
///
/// The project and its [`PatchError`]; `raw` is untouched.
pub fn patch_all(
    raw: &RawXer,
    base: &[Schedule],
    edited: &[Schedule],
) -> Result<RawXer, (m::ProjectId, PatchError)> {
    let mut seen = BTreeSet::new();
    let shared = edited
        .iter()
        .all(|s| seen.insert(s.project.id) && !removes_rows(base, s));
    if !shared {
        let mut out = raw.clone();
        for s in edited {
            out = patch_over(&out, (raw, base), s).map_err(|e| (s.project.id, e))?;
        }
        return Ok(out);
    }
    let mut patcher = Patcher::new(raw, base);
    for s in edited {
        patcher.apply(s).map_err(|e| (s.project.id, e))?;
    }
    Ok(patcher.out)
}

fn removes_rows(base: &[Schedule], edited: &Schedule) -> bool {
    base.iter()
        .find(|s| s.project.id == edited.project.id)
        .is_some_and(|base| {
            let mut activities = base.activities.keys();
            let mut relationships = base.relationships.keys();
            activities.any(|id| !edited.activities.contains_key(id))
                || relationships.any(|id| !edited.relationships.contains_key(id))
                || base
                    .assignments
                    .keys()
                    .any(|id| !edited.assignments.contains_key(id))
                || base
                    .expenses
                    .keys()
                    .any(|id| !edited.expenses.contains_key(id))
                || base
                    .project
                    .codes
                    .keys()
                    .any(|kind| !edited.project.codes.contains_key(kind))
                || base.activities.iter().any(|(id, a)| {
                    edited.activities.get(id).is_some_and(|is| {
                        a.notes
                            .iter()
                            .any(|(topic, n)| is.notes.get(topic).is_none_or(|m| m.id != n.id))
                    })
                })
                || base.activities.iter().any(|(id, a)| {
                    edited
                        .activities
                        .get(id)
                        .is_some_and(|is| a.steps.keys().any(|s| !is.steps.contains_key(s)))
                })
                || removes_udfs(base, edited)
                || base.wbs.iter().any(|(id, w)| {
                    edited.wbs.get(id).is_some_and(|is| {
                        w.notes
                            .iter()
                            .any(|(topic, n)| is.notes.get(topic).is_none_or(|m| m.id != n.id))
                    })
                })
                || base.activities.iter().any(|(id, a)| {
                    edited
                        .activities
                        .get(id)
                        .is_some_and(|is| a.codes.keys().any(|kind| !is.codes.contains_key(kind)))
                })
        })
}

/// Whether any record that stays loses a UDF value, which removes its row.
fn removes_udfs(base: &Schedule, edited: &Schedule) -> bool {
    fn lost<K: Ord, T>(
        was: &BTreeMap<K, T>,
        is: &BTreeMap<K, T>,
        udfs: impl Fn(&T) -> &BTreeMap<m::UdfTypeId, m::UdfValue>,
    ) -> bool {
        was.iter().any(|(id, w)| {
            is.get(id)
                .is_some_and(|i| udfs(w).keys().any(|k| !udfs(i).contains_key(k)))
        })
    }
    lost(&base.activities, &edited.activities, |a| &a.udfs)
        || lost(&base.wbs, &edited.wbs, |w| &w.udfs)
        || lost(&base.resources, &edited.resources, |r| &r.udfs)
        || lost(&base.assignments, &edited.assignments, |a| &a.udfs)
}

/// One copy of a file being edited against the schedules mapped from it.
struct Patcher<'a> {
    raw: &'a RawXer,
    /// The file new IDs have to clear.
    original: &'a RawXer,
    /// The schedules `original` maps to, which the edits started from.
    first: &'a [Schedule],
    base: &'a [Schedule],
    out: RawXer,
    /// Rows to remove. They go last, because source rows are addressed by the
    /// positions they had in `raw`.
    remove: BTreeSet<RowRef>,
    /// The largest ID in each table appended to.
    highest: BTreeMap<&'static str, u64>,
    /// Each raw table's columns for the fields written to it.
    columns: BTreeMap<(&'static str, usize), Vec<Option<usize>>>,
    /// Follows the source file, for new rows' GUIDs. Worked out when first needed.
    seed: Option<u64>,
    /// `TASKACTV` rows by activity and code type, built when first needed.
    code_rows: Option<BTreeMap<(u64, u64), Vec<RowRef>>>,
    /// New code values written so far. A global type's value is in each project
    /// holding that type, and goes in the file once.
    new_codes: BTreeSet<m::ActivityCodeId>,
    /// New links written so far. A link between projects is in both, and goes
    /// in the file once.
    new_links: BTreeSet<m::RelationshipId>,
    /// New resources and code types written so far. Each is the file's, and
    /// goes in it once.
    new_dictionary: BTreeSet<(&'static str, u64)>,
}

impl<'a> Patcher<'a> {
    fn new(raw: &'a RawXer, base: &'a [Schedule]) -> Self {
        Self {
            raw,
            original: raw,
            first: base,
            base,
            out: raw.clone(),
            remove: BTreeSet::new(),
            highest: BTreeMap::new(),
            columns: BTreeMap::new(),
            seed: None,
            code_rows: None,
            new_codes: BTreeSet::new(),
            new_links: BTreeSet::new(),
            new_dictionary: BTreeSet::new(),
        }
    }

    /// Applies one project's edits, comparing them with the mapped file.
    fn apply(&mut self, edited: &Schedule) -> Result<(), PatchError> {
        let pid = edited.project.id;
        let base = self
            .base
            .iter()
            .find(|s| s.project.id == pid)
            .ok_or(PatchError::NoSuchProject(pid))?;
        let deleted: BTreeSet<m::ActivityId> = base
            .activities
            .keys()
            .filter(|id| !edited.activities.contains_key(id))
            .copied()
            .collect();
        let first = self
            .first
            .iter()
            .find(|s| s.project.id == pid)
            .unwrap_or(base);
        check_unsupported((base, first), edited, &deleted)?;
        check_marker(self.raw, base, edited)?;
        check_references(base, edited)?;
        if edited
            .project
            .options
            .float_paths
            .end_activity
            .is_some_and(|id| deleted.contains(&id))
        {
            return Err(PatchError::Unsupported(
                "deleting the float-path end activity",
            ));
        }
        self.update(
            row_of(base.project.source, &format!("project {pid}"))?,
            "PROJECT",
            &format!("project {pid}"),
            &PROJECT_FIELDS,
            &base.project,
            &edited.project,
        )?;
        self.new_obs(base, edited)?;
        self.dictionaries(base, edited)?;
        self.udf_dictionary_and_owners((base, first), edited)?;
        self.project_codes(base, edited)?;
        self.activities(base, edited)?;
        self.assignments(base, edited, &deleted)?;
        self.expenses(base, edited, &deleted)?;
        self.activity_totals(base, edited, &deleted)?;
        check_relationship_ends(base, edited, &deleted)?;
        // Relationships touching deleted activities go with them.
        let live = |r: &&m::Relationship| {
            !deleted.contains(&r.predecessor) && !deleted.contains(&r.successor)
        };
        let before: BTreeMap<_, _> = base
            .relationships
            .values()
            .filter(live)
            .map(|r| (r.id, r))
            .collect();
        for r in edited.relationships.values().filter(live) {
            let entity = format!("relationship {}", r.id);
            match before.get(&r.id) {
                Some(b) => self.update(
                    row_of(b.source, &entity)?,
                    "TASKPRED",
                    &entity,
                    &PRED_FIELDS,
                    b,
                    r,
                )?,
                None if !self.new_links.insert(r.id) => {}
                None => {
                    self.append("TASKPRED", "task_pred_id", r.id.0, &entity, &PRED_FIELDS, r)?;
                }
            }
        }
        for r in edited.relationships.values() {
            relationship_fields_covered(r);
        }
        for r in before
            .values()
            .filter(|r| !edited.relationships.contains_key(&r.id))
        {
            let at = row_of(r.source, &format!("relationship {}", r.id))?;
            self.remove.insert(at);
        }
        if !deleted.is_empty() {
            self.remove.extend(dependents(self.raw, &deleted));
        }
        Ok(())
    }

    fn activities(&mut self, base: &Schedule, edited: &Schedule) -> Result<(), PatchError> {
        for (id, activity) in &edited.activities {
            let entity = format!("activity {id}");
            if let Some(before) = base.activities.get(id) {
                let (codes, udfs, notes, steps) = activity_extras(activity);
                self.code_assignments(activity, &before.codes, codes)?;
                self.steps(activity, &before.steps, steps)?;
                self.notes(
                    &ACTIVITY_NOTES,
                    (activity.id.0, activity.project),
                    &before.notes,
                    notes,
                )?;
                self.udf_values((id.0, Some(activity.project)), &before.udfs, udfs)?;
                if before.physical_percent_complete != activity.physical_percent_complete
                    && percent(activity.physical_percent_complete).is_none()
                {
                    return Err(unrepresentable("TASK", "phys_complete_pct", &entity));
                }
                let at = row_of(before.source, &entity)?;
                self.update(at, "TASK", &entity, &TASK_FIELDS, before, activity)?;
            } else {
                self.append("TASK", "task_id", id.0, &entity, &TASK_FIELDS, activity)?;
                self.udf_values(
                    (id.0, Some(activity.project)),
                    &BTreeMap::new(),
                    &activity.udfs,
                )?;
                self.code_assignments(activity, &BTreeMap::new(), &activity.codes)?;
                let none = BTreeMap::new();
                self.notes(
                    &ACTIVITY_NOTES,
                    (id.0, activity.project),
                    &none,
                    &activity.notes,
                )?;
                self.steps(activity, &BTreeMap::new(), &activity.steps)?;
            }
        }
        Ok(())
    }

    /// Writes new assignments and queues removed ones.
    fn assignments(
        &mut self,
        base: &Schedule,
        edited: &Schedule,
        deleted: &BTreeSet<m::ActivityId>,
    ) -> Result<(), PatchError> {
        let mut gone = BTreeSet::new();
        for (id, was) in &base.assignments {
            if edited.assignments.contains_key(id) || deleted.contains(&was.activity) {
                continue;
            }
            let at = row_of(was.source, &format!("assignment {id}"))?;
            if holds_actuals(self.raw, at) {
                return Err(PatchError::Unsupported(
                    "removing an assignment with actual units or cost",
                ));
            }
            if !was.udfs.is_empty() {
                return Err(PatchError::Unsupported("removing an assignment with UDFs"));
            }
            self.remove.insert(at);
            gone.insert(id.0);
        }
        self.remove.extend(rows_naming_assignments(self.raw, &gone));
        for (id, assignment) in &edited.assignments {
            let account_known = assignment
                .cost_account
                .is_none_or(|c| edited.cost_accounts.contains_key(&c));
            if let Some(was) = base.assignments.get(id) {
                if (was.resource, was.role) != (assignment.resource, assignment.role) {
                    return Err(PatchError::Unsupported(
                        "changing an assignment's resource or role",
                    ));
                }
                self.assignment_rows(assignment, Some(was))?;
                let same_shape =
                    was.curve == assignment.curve && was.typed_spread == assignment.typed_spread;
                if was.amounts == assignment.amounts
                    && same_shape
                    && was.cost_account == assignment.cost_account
                {
                    continue;
                }
                let entity = format!("assignment {id}");
                if !account_known {
                    return Err(PatchError::Unrepresentable {
                        table: "TASKRSRC",
                        field: "acct_id",
                        entity,
                        reason: "it names a cost account that isn't in the schedule",
                    });
                }
                let at = row_of(was.source, &entity)?;
                if !same_shape && names_a_cleared_curve(self.raw, at, was) {
                    return Err(PatchError::Unsupported(
                        "changing the curve of an assignment whose curve the file lacks",
                    ));
                }
                self.update(at, "TASKRSRC", &entity, &ASSIGNMENT_FIELDS, was, assignment)?;
                if was.typed_spread.is_some() != assignment.typed_spread.is_some() {
                    self.typed_or_not(at, assignment)?;
                }
                continue;
            }
            let entity = format!("assignment {id}");
            let activity = edited.activities.get(&assignment.activity);
            let resource = assignment.resource.map(|r| edited.resources.get(&r));
            let role_known = assignment
                .role
                .is_none_or(|r| edited.roles.contains_key(&r));
            let (Some(activity), Some(resource), true) = (
                activity,
                resource.map_or(Some(None), |found| found.map(Some)),
                role_known
                    && account_known
                    && (assignment.resource.is_some() || assignment.role.is_some()),
            ) else {
                return Err(PatchError::Unrepresentable {
                    table: "TASKRSRC",
                    field: "rsrc_id",
                    entity,
                    reason: "it names an activity, resource, role or cost account that isn't in the schedule",
                });
            };
            let when = |planned: Option<m::DateTime>, early: Option<m::DateTime>| {
                planned.or(early).or(edited.project.data_date)
            };
            let row = NewAssignment {
                assignment: assignment.clone(),
                kind: resource.map(|r| r.kind),
                start: when(activity.planned_start, activity.stored.early_start),
                finish: when(activity.planned_finish, activity.stored.early_finish),
            };
            self.append(
                "TASKRSRC",
                "taskrsrc_id",
                id.0,
                &entity,
                &NEW_ASSIGNMENT_FIELDS,
                &row,
            )?;
            self.assignment_rows(assignment, None)?;
            self.fill_last("TASKRSRC", &entity, &ASSIGNMENT_FIELDS, assignment)?;
        }
        Ok(())
    }

    /// Brings `curv_id` and the spread cells of an assignment that became
    /// typed, or stopped being typed, in line with it, where `update`
    /// compared formatted values that agree while the row doesn't (TF-040):
    /// a row that wasn't typed can still hold old spreads, which the marker
    /// would make part of the new one, and the file's own marker can differ
    /// from a curve 9 the assignment moves to.
    fn typed_or_not(&mut self, at: RowRef, assignment: &m::Assignment) -> Result<(), PatchError> {
        let empty = Vec::new();
        let typed = assignment.typed_spread.as_ref();
        let part = |f: fn(&m::TypedSpread) -> &Vec<m::SpreadSegment>| typed.map_or(&empty, f);
        let table = &self.raw.tables()[at.table];
        let mut changes = Vec::new();
        if let (Some(column), Some(curve)) =
            (table.field_index("curv_id"), assignment_curve(assignment))
            && table.rows[at.row].get(column) != Some(curve.as_str())
        {
            changes.push((column, curve));
        }
        for (field, segments) in [
            ("target_crv", part(|t| &t.planned)),
            ("remain_crv", part(|t| &t.remaining)),
            ("actual_crv", part(|t| &t.actual)),
        ] {
            if let Some(column) = table.field_index(field)
                && segments.is_empty()
                && table.rows[at.row]
                    .get(column)
                    .is_some_and(|c| !c.is_empty())
            {
                changes.push((column, String::new()));
            }
        }
        if !changes.is_empty() {
            self.out.set_cells(at.table, at.row, &changes)?;
        }
        Ok(())
    }

    /// Writes changed and new expenses, and queues removed ones with their
    /// UDF values. Those of deleted activities go with them.
    fn expenses(
        &mut self,
        base: &Schedule,
        edited: &Schedule,
        deleted: &BTreeSet<m::ActivityId>,
    ) -> Result<(), PatchError> {
        let mut gone = BTreeSet::new();
        for (id, was) in &base.expenses {
            if edited.expenses.contains_key(id) || deleted.contains(&was.activity) {
                continue;
            }
            self.remove
                .insert(row_of(was.source, &format!("expense {id}"))?);
            gone.insert(id.0);
        }
        if !gone.is_empty() {
            let removed = BTreeMap::from([("PROJCOST", gone)]);
            self.remove.extend(udf_rows(self.raw, &removed));
        }
        for (id, expense) in &edited.expenses {
            let entity = format!("expense {id}");
            let was = base.expenses.get(id);
            // Those of deleted activities go with them, unchanged or not.
            if was.is_some_and(|was| was == expense || deleted.contains(&was.activity)) {
                continue;
            }
            let known = edited.activities.contains_key(&expense.activity)
                && expense
                    .cost_account
                    .is_none_or(|a| edited.cost_accounts.contains_key(&a))
                && expense
                    .category
                    .is_none_or(|c| edited.expense_categories.contains_key(&c));
            if !known {
                return Err(PatchError::Unrepresentable {
                    table: "PROJCOST",
                    field: "task_id",
                    entity,
                    reason: "it names an activity, cost account or category that isn't in the schedule",
                });
            }
            if let Some(was) = was {
                let at = row_of(was.source, &entity)?;
                self.update(at, "PROJCOST", &entity, &EXPENSE_FIELDS, was, expense)?;
                continue;
            }
            self.append(
                "PROJCOST",
                "cost_item_id",
                id.0,
                &entity,
                &EXPENSE_FIELDS,
                expense,
            )?;
        }
        Ok(())
    }

    /// Moves each activity's labor and nonlabor unit totals by what its
    /// assignments' units moved. A total that already differed from its
    /// assignments' sum keeps that difference, as some corpus files hold (TF-004).
    fn activity_totals(
        &mut self,
        base: &Schedule,
        edited: &Schedule,
        deleted: &BTreeSet<m::ActivityId>,
    ) -> Result<(), PatchError> {
        type Totals = BTreeMap<(m::ActivityId, &'static str), f64>;
        let sums = |s: &Schedule, sign: f64, totals: &mut Totals| {
            for a in s
                .assignments
                .values()
                .filter(|a| !deleted.contains(&a.activity))
            {
                // A role alone has no resource type, so it isn't counted.
                let resource = a.resource.and_then(|r| s.resources.get(&r));
                let columns = match resource.map(|r| r.kind) {
                    Some(m::ResourceType::Labor) => {
                        ["target_work_qty", "act_work_qty", "remain_work_qty"]
                    }
                    Some(m::ResourceType::Nonlabor) => {
                        ["target_equip_qty", "act_equip_qty", "remain_equip_qty"]
                    }
                    _ => continue,
                };
                let actual = a.amounts.actual_units.unwrap_or(0.0)
                    + a.amounts.actual_overtime_units.unwrap_or(0.0);
                for (column, units) in columns.into_iter().zip([
                    a.amounts.budgeted_units,
                    Some(actual),
                    a.amounts.remaining_units,
                ]) {
                    *totals.entry((a.activity, column)).or_default() += sign * units.unwrap_or(0.0);
                }
            }
        };
        let mut moved = Totals::new();
        sums(edited, 1.0, &mut moved);
        sums(base, -1.0, &mut moved);
        moved.retain(|_, delta| delta.abs() > 1e-9);
        if moved.is_empty() {
            return Ok(());
        }
        let t = self
            .out
            .table_index("TASK")
            .ok_or(PatchError::MissingTable("TASK"))?;
        let table = &self.out.tables()[t];
        let id_column = table
            .field_index("task_id")
            .ok_or(PatchError::MissingColumn {
                table: "TASK",
                field: "task_id",
            })?;
        let rows: BTreeMap<u64, usize> = table
            .rows
            .iter()
            .enumerate()
            .filter_map(|(at, row)| Some((row.get(id_column)?.parse().ok()?, at)))
            .collect();
        for ((activity, column), delta) in moved {
            let table = &self.out.tables()[t];
            let (Some(&row), Some(at)) = (rows.get(&activity.0), table.field_index(column)) else {
                continue;
            };
            let held = table.rows[row].get(at).and_then(|v| v.parse::<f64>().ok());
            let total = (held.unwrap_or(0.0) + delta).max(0.0);
            let text = decimal(Some((total * 1e6).round() / 1e6))
                .ok_or_else(|| unrepresentable("TASK", column, &format!("activity {activity}")))?;
            self.out.set_cells(t, row, &[(at, text)])?;
        }
        Ok(())
    }

    /// Writes the values `fields` give into the row just appended, over its
    /// defaults. Empty values leave the default.
    fn fill_last<T>(
        &mut self,
        table: &'static str,
        entity: &str,
        fields: &[Field<T>],
        value: &T,
    ) -> Result<(), PatchError> {
        let t = self
            .out
            .table_index(table)
            .ok_or(PatchError::MissingTable(table))?;
        let row = self.out.tables()[t].rows.len() - 1;
        let mut changes = Vec::new();
        for &(field, format) in fields {
            let text = format(value).ok_or_else(|| unrepresentable(table, field, entity))?;
            match self.out.tables()[t].field_index(field) {
                Some(column) if !text.is_empty() => changes.push((column, text)),
                Some(_) => {}
                // A missing column reads as nothing, or as `N` for a flag, so
                // writing that to it loses nothing.
                None if text.is_empty() || text == "N" => {}
                None => return Err(PatchError::MissingColumn { table, field }),
            }
        }
        if !changes.is_empty() {
            self.out.set_cells(t, row, &changes)?;
        }
        Ok(())
    }

    /// Gives the `PROJWBS` row just added the settings of the project's root
    /// that the model left empty.
    fn fill_from_root(&mut self, base: &Schedule, entity: &str) -> Result<(), PatchError> {
        let root = base.wbs.values().find(|node| node.is_project_node);
        let at = row_of(root.and_then(|node| node.source), entity)?;
        let table = self
            .out
            .table_index("PROJWBS")
            .ok_or(PatchError::MissingTable("PROJWBS"))?;
        let row = self.out.tables()[table].rows.len() - 1;
        let source = &self.raw.tables()[at.table];
        for field in WBS_SETTINGS {
            let value = source
                .field_index(field)
                .and_then(|column| source.rows[at.row].get(column));
            let column = self.out.tables()[table].field_index(field);
            // The node's own OBS, from the model, stands.
            let given = column.and_then(|c| self.out.tables()[table].rows[row].get(c));
            if field == "obs_id" && given.is_some_and(|given| !given.is_empty()) {
                continue;
            }
            if let (Some(value), Some(column)) = (value, column) {
                self.out.set_cell(table, row, column, value.to_owned())?;
            }
        }
        Ok(())
    }

    /// Gives the `RSRC` row just added the file's first currency.
    fn fill_currency(&mut self) -> Result<(), PatchError> {
        let currency = self.raw.table("CURRTYPE").and_then(|table| {
            let column = table.field_index("curr_id")?;
            table.rows.first()?.get(column).map(str::to_owned)
        });
        let t = self
            .out
            .table_index("RSRC")
            .ok_or(PatchError::MissingTable("RSRC"))?;
        let row = self.out.tables()[t].rows.len() - 1;
        if let (Some(currency), Some(column)) =
            (currency, self.out.tables()[t].field_index("curr_id"))
        {
            self.out.set_cell(t, row, column, currency)?;
        }
        Ok(())
    }

    /// New OBS nodes, once each: the OBS is the file's.
    fn new_obs(&mut self, base: &Schedule, edited: &Schedule) -> Result<(), PatchError> {
        for (id, node) in &edited.obs {
            if !base.obs.contains_key(id)
                && node.source.is_none()
                && self.new_dictionary.insert(("OBS", id.0))
            {
                let entity = format!("OBS node {id}");
                self.append("OBS", "obs_id", id.0, &entity, &NEW_OBS_FIELDS, node)?;
            }
        }
        Ok(())
    }

    /// Writes changed calendars, and new calendars, resources, code types and
    /// code values.
    fn dictionaries(&mut self, base: &Schedule, edited: &Schedule) -> Result<(), PatchError> {
        for (id, was) in &base.calendars {
            if let Some(is) = edited.calendars.get(id).filter(|is| *is != was) {
                let entity = format!("calendar {id}");
                let at = row_of(was.source, &entity)?;
                self.update(at, "CALENDAR", &entity, &CALENDAR_FIELDS, was, is)?;
            }
        }
        for (id, calendar) in &edited.calendars {
            if !base.calendars.contains_key(id) {
                let entity = format!("calendar {id}");
                self.append(
                    "CALENDAR",
                    "clndr_id",
                    id.0,
                    &entity,
                    &NEW_CALENDAR_FIELDS,
                    calendar,
                )?;
            }
        }
        for (id, was) in &base.wbs {
            if let Some(is) = edited.wbs.get(id).filter(|is| *is != was) {
                let entity = format!("WBS {id}");
                let at = row_of(was.source, &entity)?;
                self.update(at, "PROJWBS", &entity, &WBS_FIELDS, was, is)?;
            }
        }
        for (id, node) in &edited.wbs {
            if !base.wbs.contains_key(id) {
                let entity = format!("WBS {id}");
                self.append("PROJWBS", "wbs_id", id.0, &entity, &NEW_WBS_FIELDS, node)?;
                self.fill_from_root(base, &entity)?;
                self.fill_last("PROJWBS", &entity, &WBS_EARNED_VALUE, node)?;
            }
        }
        let none = BTreeMap::new();
        for (id, node) in &edited.wbs {
            let was = base.wbs.get(id).map_or(&none, |was| &was.notes);
            self.notes(&WBS_NOTES, (id.0, node.project), was, &node.notes)?;
        }
        for (id, kind) in &edited.activity_code_types {
            // One another project in the file holds comes with its source row.
            if !base.activity_code_types.contains_key(id)
                && kind.source.is_none()
                && self.new_dictionary.insert(("ACTVTYPE", id.0))
            {
                let entity = format!("activity code type {id}");
                self.append(
                    "ACTVTYPE",
                    "actv_code_type_id",
                    id.0,
                    &entity,
                    &NEW_CODE_TYPE_FIELDS,
                    kind,
                )?;
            }
        }
        for (id, resource) in &edited.resources {
            if !base.resources.contains_key(id)
                && resource.source.is_none()
                && self.new_dictionary.insert(("RSRC", id.0))
            {
                let entity = format!("resource {id}");
                self.append(
                    "RSRC",
                    "rsrc_id",
                    id.0,
                    &entity,
                    &NEW_RESOURCE_FIELDS,
                    resource,
                )?;
                self.fill_currency()?;
            }
        }
        self.resource_rates(base, edited)?;
        // `check_dictionaries` refused changed and removed roles, but for
        // their limits.
        for (id, role) in &edited.roles {
            if !base.roles.contains_key(id) && self.new_dictionary.insert(("ROLES", id.0)) {
                let entity = format!("role {id}");
                self.append("ROLES", "role_id", id.0, &entity, &NEW_ROLE_FIELDS, role)?;
            }
        }
        self.role_rates(base, edited)?;
        for (id, code) in &edited.activity_codes {
            if !base.activity_codes.contains_key(id) && self.new_codes.insert(*id) {
                let entity = format!("activity code {id}");
                self.append(
                    "ACTVCODE",
                    "actv_code_id",
                    id.0,
                    &entity,
                    &NEW_CODE_FIELDS,
                    code,
                )?;
            }
        }
        Ok(())
    }

    /// Resources' limits: a changed one on a rate the file holds, and new
    /// rates, each once. A resource is the file's, so every project holding
    /// it carries the same rates. `check_dictionaries` refused other changes.
    fn resource_rates(&mut self, base: &Schedule, edited: &Schedule) -> Result<(), PatchError> {
        // A rate the file holds through any project, this one's first: a
        // resource borrowed in this run isn't in this project's base.
        let all = self.base;
        let held = |id: &m::ResourceId, rate: m::ResourceRateId| {
            std::iter::once(base)
                .chain(all)
                .filter_map(|s| s.resources.get(id))
                .flat_map(|r| &r.rates)
                .find(|r| r.id == rate)
        };
        for (id, resource) in &edited.resources {
            for rate in &resource.rates {
                let entity = format!("resource {id} rate {}", rate.id);
                match held(id, rate.id) {
                    Some(was) => {
                        let at = row_of(was.source, &entity)?;
                        self.update(at, "RSRCRATE", &entity, &RATE_LIMIT_FIELDS, was, rate)?;
                    }
                    // A rate another project's copy brought along is in the file.
                    None if rate.source.is_some() => {}
                    None if self.new_dictionary.insert(("RSRCRATE", rate.id.0)) => {
                        self.append(
                            "RSRCRATE",
                            "rsrc_rate_id",
                            rate.id.0,
                            &entity,
                            &NEW_RATE_FIELDS,
                            &(*id, rate.clone()),
                        )?;
                    }
                    None => {}
                }
            }
        }
        Ok(())
    }

    /// Roles' limits, as [`Self::resource_rates`] writes resources': a
    /// changed one on a rate the file holds, and new rates, each once.
    fn role_rates(&mut self, base: &Schedule, edited: &Schedule) -> Result<(), PatchError> {
        let all = self.base;
        let held = |id: &m::RoleId, rate: m::RoleRateId| {
            std::iter::once(base)
                .chain(all)
                .filter_map(|s| s.roles.get(id))
                .flat_map(|r| &r.rates)
                .find(|r| r.id == rate)
        };
        for (id, role) in &edited.roles {
            for rate in &role.rates {
                let entity = format!("role {id} rate {}", rate.id);
                match held(id, rate.id) {
                    Some(was) => {
                        let at = row_of(was.source, &entity)?;
                        self.update(at, "ROLERATE", &entity, &ROLE_RATE_LIMIT_FIELDS, was, rate)?;
                    }
                    // A rate another project's copy brought along is in the file.
                    None if rate.source.is_some() => {}
                    None if self.new_dictionary.insert(("ROLERATE", rate.id.0)) => {
                        self.append(
                            "ROLERATE",
                            "role_rate_id",
                            rate.id.0,
                            &entity,
                            &NEW_ROLE_RATE_FIELDS,
                            &(*id, rate.clone()),
                        )?;
                    }
                    None => {}
                }
            }
        }
        Ok(())
    }

    /// New project code types and values, once each, and the differences in
    /// the project's own codes, in `PROJPCAT`. Every row for a type moves
    /// together, because mapping reads whichever comes first.
    fn project_codes(&mut self, base: &Schedule, edited: &Schedule) -> Result<(), PatchError> {
        for (id, kind) in &edited.project_code_types {
            if !base.project_code_types.contains_key(id)
                && kind.source.is_none()
                && self.new_dictionary.insert(("PCATTYPE", id.0))
            {
                let entity = format!("project code type {id}");
                self.append(
                    "PCATTYPE",
                    "proj_catg_type_id",
                    id.0,
                    &entity,
                    &NEW_PROJECT_CODE_TYPE_FIELDS,
                    kind,
                )?;
            }
        }
        for (id, code) in &edited.project_codes {
            if !base.project_codes.contains_key(id)
                && code.source.is_none()
                && self.new_dictionary.insert(("PCATVAL", id.0))
            {
                let entity = format!("project code {id}");
                self.append(
                    "PCATVAL",
                    "proj_catg_id",
                    id.0,
                    &entity,
                    &NEW_PROJECT_CODE_FIELDS,
                    code,
                )?;
            }
        }
        let (was, is) = (&base.project.codes, &edited.project.codes);
        if was == is {
            return Ok(());
        }
        let project = edited.project.id;
        let rows_of = |raw: &RawXer, kind: m::ProjectCodeTypeId| {
            let rows = project_code_rows(raw, project, kind);
            if rows.is_empty() {
                Err(PatchError::NoSourceRow(format!(
                    "project {project} code {kind}"
                )))
            } else {
                Ok(rows)
            }
        };
        for kind in was.keys().filter(|kind| !is.contains_key(*kind)) {
            let rows = rows_of(self.raw, *kind)?;
            self.remove.extend(rows);
        }
        for (kind, code) in is {
            if was.get(kind) == Some(code) {
                continue;
            }
            // A row mapping left out, for a value the file lacks, is
            // rewritten rather than joined by a second row for the type.
            let held = project_code_rows(self.raw, project, *kind);
            if was.contains_key(kind) || !held.is_empty() {
                for at in held {
                    let column = self.out.tables()[at.table]
                        .field_index("proj_catg_id")
                        .ok_or(PatchError::MissingColumn {
                            table: "PROJPCAT",
                            field: "proj_catg_id",
                        })?;
                    self.out
                        .set_cell(at.table, at.row, column, code.to_string())?;
                }
            } else {
                let table = self.table_or_add("PROJPCAT")?;
                let fields = &self.out.tables()[table].fields;
                let mut cells = vec![String::new(); fields.len()];
                for (name, value) in [
                    ("proj_id", project.to_string()),
                    ("proj_catg_type_id", kind.to_string()),
                    ("proj_catg_id", code.to_string()),
                ] {
                    let column =
                        fields
                            .iter()
                            .position(|f| f == name)
                            .ok_or(PatchError::MissingColumn {
                                table: "PROJPCAT",
                                field: name,
                            })?;
                    cells[column] = value;
                }
                self.out.push_row(table, &cells)?;
            }
        }
        Ok(())
    }

    /// Writes the differences in an activity's or WBS node's notes: a changed
    /// note's text cell, new notes' rows, and removed notes' rows queued.
    fn notes(
        &mut self,
        kind: &NoteTable,
        (owner, project): (u64, m::ProjectId),
        was: &BTreeMap<m::NotebookTopicId, m::Note>,
        is: &BTreeMap<m::NotebookTopicId, m::Note>,
    ) -> Result<(), PatchError> {
        if was == is {
            return Ok(());
        }
        let entity = |note: &m::Note| format!("{} {}", kind.table, note.id);
        for (topic, note) in was {
            if is.get(topic).is_none_or(|now| now.id != note.id) {
                let at = row_of(note.source, &entity(note))?;
                self.remove.insert(at);
            }
        }
        for (topic, note) in is {
            match was.get(topic).filter(|before| before.id == note.id) {
                Some(before) if before.html == note.html => {}
                Some(before) => {
                    let at = row_of(before.source, &entity(note))?;
                    let column = self.raw.tables()[at.table].field_index(kind.text).ok_or(
                        PatchError::MissingColumn {
                            table: kind.table,
                            field: kind.text,
                        },
                    )?;
                    let cell = crate::mapping::notes::encode(&note.html);
                    self.out.set_cell(at.table, at.row, column, cell)?;
                }
                None => {
                    let row = NoteRow {
                        id: note.id,
                        owner,
                        topic: *topic,
                        project,
                        html: note.html.clone(),
                    };
                    self.append(
                        kind.table,
                        kind.id,
                        note.id,
                        &entity(note),
                        &kind.fields,
                        &row,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Writes the differences in an activity's steps: changed cells, new
    /// rows, and removed rows queued.
    fn steps(
        &mut self,
        activity: &m::Activity,
        was: &BTreeMap<m::StepId, m::Step>,
        is: &BTreeMap<m::StepId, m::Step>,
    ) -> Result<(), PatchError> {
        if was == is {
            return Ok(());
        }
        for (id, step) in was {
            if !is.contains_key(id) {
                let at = row_of(step.source, &format!("step {id}"))?;
                self.remove.insert(at);
            }
        }
        for (id, step) in is {
            let entity = format!("step {id}");
            if let Some(before) = was.get(id) {
                let at = row_of(before.source, &entity)?;
                self.update(at, "TASKPROC", &entity, &STEP_FIELDS, before, step)?;
            } else {
                let row = StepRow {
                    step: step.clone(),
                    activity: activity.id,
                    project: activity.project,
                };
                self.append("TASKPROC", "proc_id", id.0, &entity, &NEW_STEP_FIELDS, &row)?;
            }
        }
        Ok(())
    }

    /// New UDF definitions, once each, and the UDF values of WBS nodes and
    /// resources.
    fn udf_dictionary_and_owners(
        &mut self,
        (base, first): (&Schedule, &Schedule),
        edited: &Schedule,
    ) -> Result<(), PatchError> {
        for (id, kind) in &edited.udf_types {
            if !base.udf_types.contains_key(id)
                && kind.source.is_none()
                && self.new_dictionary.insert(("UDFTYPE", id.0))
            {
                let entity = format!("UDF definition {id}");
                self.append(
                    "UDFTYPE",
                    "udf_type_id",
                    id.0,
                    &entity,
                    &NEW_UDF_TYPE_FIELDS,
                    kind,
                )?;
            }
        }
        let none = BTreeMap::new();
        for (id, node) in &edited.wbs {
            let was = base.wbs.get(id).map_or(&none, |was| &was.udfs);
            self.udf_values((id.0, Some(node.project)), was, &node.udfs)?;
        }
        for (id, resource) in &edited.resources {
            let Some(was) = base.resources.get(id).map(|was| &was.udfs) else {
                // A new resource another project added is that project's to write.
                if resource.source.is_none() && self.new_dictionary.insert(("RSRC UDFVALUE", id.0))
                {
                    self.udf_values((id.0, None), &none, &resource.udfs)?;
                }
                continue;
            };
            // A resource is the file's, and every project holds a copy. This
            // project's changes are against the file before this run, so a
            // copy another project's edit already moved on isn't put back.
            let start = first.resources.get(id).map_or(was, |r| &r.udfs);
            let mut is = was.clone();
            for kind in start.keys().filter(|k| !resource.udfs.contains_key(*k)) {
                is.remove(kind);
            }
            for (kind, value) in &resource.udfs {
                if start.get(kind) != Some(value) {
                    is.insert(*kind, value.clone());
                }
            }
            self.udf_values((id.0, None), was, &is)?;
        }
        Ok(())
    }

    /// Writes the differences in one record's UDF values to `UDFVALUE`: a
    /// changed value's cell, new values' rows, and removed values' rows
    /// queued. `project` is `None` for a resource, whose values name none.
    fn udf_values(
        &mut self,
        (owner, project): (u64, Option<m::ProjectId>),
        was: &BTreeMap<m::UdfTypeId, m::UdfValue>,
        is: &BTreeMap<m::UdfTypeId, m::UdfValue>,
    ) -> Result<(), PatchError> {
        if was == is {
            return Ok(());
        }
        let rows_of = |raw: &RawXer, kind: m::UdfTypeId| {
            let rows = udf_value_rows(raw, kind, owner, project);
            if rows.is_empty() {
                Err(PatchError::NoSourceRow(format!("UDF {kind} of {owner}")))
            } else {
                Ok(rows)
            }
        };
        for kind in was.keys().filter(|kind| !is.contains_key(*kind)) {
            let rows = rows_of(self.raw, *kind)?;
            self.remove.extend(rows);
        }
        for (kind, value) in is {
            let entity = format!("UDF {kind} of {owner}");
            let (column, text) = udf_cell(value).ok_or_else(|| PatchError::Unrepresentable {
                table: "UDFVALUE",
                field: "udf_number",
                entity: entity.clone(),
                reason: "the number can't be written exactly",
            })?;
            match was.get(kind) {
                Some(before) if before == value => {}
                Some(before) => {
                    let old = udf_cell(before).map(|(c, _)| c);
                    for at in rows_of(self.raw, *kind)? {
                        let table = &self.out.tables()[at.table];
                        let mut changes = Vec::new();
                        for (field, cell) in [(column, text.as_str())]
                            .into_iter()
                            .chain(old.filter(|c| *c != column).map(|c| (c, "")))
                        {
                            let index =
                                table.field_index(field).ok_or(PatchError::MissingColumn {
                                    table: "UDFVALUE",
                                    field,
                                })?;
                            changes.push((index, cell.to_owned()));
                        }
                        self.out.set_cells(at.table, at.row, &changes)?;
                    }
                }
                None => {
                    let table = self.table_or_add("UDFVALUE")?;
                    let fields = &self.out.tables()[table].fields;
                    let mut cells = vec![String::new(); fields.len()];
                    let project = project.map(|p| p.to_string()).unwrap_or_default();
                    for (name, cell) in [
                        ("udf_type_id", kind.to_string()),
                        ("fk_id", owner.to_string()),
                        ("proj_id", project),
                        (column, text.clone()),
                    ] {
                        match fields.iter().position(|f| f == name) {
                            Some(at) => cells[at] = cell,
                            None if cell.is_empty() => {}
                            None => {
                                return Err(PatchError::MissingColumn {
                                    table: "UDFVALUE",
                                    field: name,
                                });
                            }
                        }
                    }
                    self.out.push_row(table, &cells)?;
                }
            }
        }
        Ok(())
    }

    /// Writes an assignment's UDF values, against what it held, or nothing
    /// for a new one.
    fn assignment_rows(
        &mut self,
        is: &m::Assignment,
        was: Option<&m::Assignment>,
    ) -> Result<(), PatchError> {
        let none = BTreeMap::new();
        let udfs = was.map_or(&none, |w| &w.udfs);
        self.udf_values((is.id.0, Some(is.project)), udfs, &is.udfs)
    }

    /// Every `TASKACTV` row, by the numbers it holds, as mapping reads them.
    fn code_rows(&mut self) -> &BTreeMap<(u64, u64), Vec<RowRef>> {
        let raw = self.raw;
        self.code_rows.get_or_insert_with(|| {
            let mut rows: BTreeMap<(u64, u64), Vec<RowRef>> = BTreeMap::new();
            for (table, t) in raw.tables().iter().enumerate() {
                if t.name != "TASKACTV" {
                    continue;
                }
                let (Some(task), Some(kind)) =
                    (t.field_index("task_id"), t.field_index("actv_code_type_id"))
                else {
                    continue;
                };
                for (row, cells) in t.rows.iter().enumerate() {
                    let number = |column| cells.get(column)?.parse::<u64>().ok();
                    if let (Some(task), Some(kind)) = (number(task), number(kind)) {
                        rows.entry((task, kind))
                            .or_default()
                            .push(RowRef { table, row });
                    }
                }
            }
            rows
        })
    }

    /// Writes the differences in an activity's codes to `TASKACTV`. Every row for
    /// a type moves together, because mapping reads whichever comes first.
    fn code_assignments(
        &mut self,
        activity: &m::Activity,
        was: &BTreeMap<m::ActivityCodeTypeId, m::ActivityCodeId>,
        is: &BTreeMap<m::ActivityCodeTypeId, m::ActivityCodeId>,
    ) -> Result<(), PatchError> {
        if was == is {
            return Ok(());
        }
        let rows_of = |patcher: &mut Self, kind: &m::ActivityCodeTypeId| {
            let found = patcher.code_rows().get(&(activity.id.0, kind.0)).cloned();
            found.ok_or_else(|| {
                PatchError::NoSourceRow(format!("activity {} code {kind}", activity.id))
            })
        };
        for kind in was.keys().filter(|kind| !is.contains_key(*kind)) {
            let rows = rows_of(self, kind)?;
            self.remove.extend(rows);
        }
        for (kind, code) in is {
            if was.get(kind) == Some(code) {
                continue;
            }
            if was.contains_key(kind) {
                for at in rows_of(self, kind)? {
                    let column = self.out.tables()[at.table]
                        .field_index("actv_code_id")
                        .ok_or(PatchError::MissingColumn {
                            table: "TASKACTV",
                            field: "actv_code_id",
                        })?;
                    self.out
                        .set_cell(at.table, at.row, column, code.to_string())?;
                }
            } else {
                self.new_code_row(activity, *kind, *code)?;
            }
        }
        Ok(())
    }

    fn new_code_row(
        &mut self,
        activity: &m::Activity,
        kind: m::ActivityCodeTypeId,
        code: m::ActivityCodeId,
    ) -> Result<(), PatchError> {
        let table = self.table_or_add("TASKACTV")?;
        let fields = &self.out.tables()[table].fields;
        let mut cells = vec![String::new(); fields.len()];
        for (name, value) in [
            ("task_id", activity.id.to_string()),
            ("actv_code_type_id", kind.to_string()),
            ("actv_code_id", code.to_string()),
            ("proj_id", activity.project.to_string()),
        ] {
            let column =
                fields
                    .iter()
                    .position(|f| f == name)
                    .ok_or(PatchError::MissingColumn {
                        table: "TASKACTV",
                        field: name,
                    })?;
            cells[column] = value;
        }
        self.out.push_row(table, &cells)?;
        Ok(())
    }

    /// The output's `table`, added at the end of the file with P6's columns
    /// if the file has none. Tables go last so every existing row keeps its
    /// place. XER Tools accepts tables in this order.
    fn table_or_add(&mut self, table: &'static str) -> Result<usize, PatchError> {
        if let Some(t) = self.out.table_index(table) {
            return Ok(t);
        }
        let columns = crate::fresh::columns(table).ok_or(PatchError::MissingTable(table))?;
        let columns = columns.iter().map(|c| (*c).to_owned()).collect();
        Ok(self.out.push_table(table, columns)?)
    }

    /// Removes the rows queued for removal and gives the copy back.
    fn finish(mut self) -> Result<RawXer, PatchError> {
        let rows = self.remove.into_iter().map(|at| (at.table, at.row));
        self.out.remove_rows(rows)?;
        Ok(self.out)
    }

    /// Rewrites the cells whose formatted value differs between `before` and `after`.
    fn update<T>(
        &mut self,
        at: RowRef,
        table: &'static str,
        entity: &str,
        fields: &[Field<T>],
        before: &T,
        after: &T,
    ) -> Result<(), PatchError>
    where
        T: PartialEq,
    {
        if before == after {
            return Ok(());
        }
        let columns = self.columns.entry((table, at.table)).or_insert_with(|| {
            let raw = &self.raw.tables()[at.table];
            let columns = fields.iter().map(|&(field, _)| raw.field_index(field));
            columns.collect()
        });
        let mut changes = Vec::new();
        for (&(field, format), &column) in fields.iter().zip(columns.iter()) {
            let (old, new) = (format(before), format(after));
            // A source value the writer couldn't spell is fine if it isn't changing.
            if old == new {
                continue;
            }
            let new = new.ok_or_else(|| unrepresentable(table, field, entity))?;
            match column {
                Some(column) => changes.push((column, new)),
                None if new.is_empty() => {}
                None => return Err(PatchError::MissingColumn { table, field }),
            }
        }
        if !changes.is_empty() {
            self.out.set_cells(at.table, at.row, &changes)?;
        }
        Ok(())
    }

    /// Appends a row for a new entity: known fields formatted, the rest empty.
    fn append<T>(
        &mut self,
        table: &'static str,
        id_field: &'static str,
        id: u64,
        entity: &str,
        fields: &[Field<T>],
        value: &T,
    ) -> Result<(), PatchError> {
        let t = self.table_or_add(table)?;
        let out = &mut self.out;
        let original = self.original;
        let max = *self.highest.entry(table).or_insert_with(|| {
            original
                .tables()
                .iter()
                .filter(|x| x.name == table)
                .filter_map(|x| x.field_index(id_field).map(|c| (x, c)))
                .flat_map(|(x, c)| {
                    x.rows
                        .iter()
                        .filter_map(move |r| r.get(c)?.parse::<u64>().ok())
                })
                .max()
                .unwrap_or(0)
        });
        if id <= max {
            return Err(PatchError::IdNotAboveMaximum { table, id, max });
        }
        let mut cells = vec![String::new(); out.tables()[t].fields.len()];
        for &(field, format) in fields {
            let text = format(value).ok_or_else(|| unrepresentable(table, field, entity))?;
            match out.tables()[t].field_index(field) {
                Some(column) => cells[column] = text,
                // A missing column reads as nothing, or as `N` for a flag, so
                // writing that to it loses nothing.
                None if text.is_empty()
                    || (text == "N" && field.ends_with("_flag"))
                    || OPTIONAL_COLUMNS.contains(&(table, field))
                    || UNSTATED_DEFAULTS.contains(&(table, field, text.as_str())) => {}
                None => return Err(PatchError::MissingColumn { table, field }),
            }
        }
        let defaults = new_row_defaults(table);
        if !defaults.is_empty() || table == "OBS" {
            let seed = *self.seed.get_or_insert_with(|| source_seed(self.raw));
            let fields = &out.tables()[t].fields;
            for (field, value) in defaults {
                if let Some(column) = fields.iter().position(|name| name == field)
                    && cells[column].is_empty()
                {
                    (*value).clone_into(&mut cells[column]);
                }
            }
            if let Some(column) = fields.iter().position(|name| name == "guid") {
                cells[column] = crate::fresh::guid(seed, table, id);
            }
        }
        out.push_row(t, &cells)?;
        Ok(())
    }
}

/// What a new row of `table` holds that the model doesn't give: for a
/// setting, the value most rows in the corpus hold; for a quantity or cost,
/// zero (TF-034, TF-036). The patch writer and the fresh writer both fill
/// them, into cells the model left empty only. The corpus test
/// `new_row_defaults_are_what_corpus_rows_hold` checks the settings.
#[must_use]
pub(crate) fn new_row_defaults(table: &str) -> &'static [(&'static str, &'static str)] {
    match table {
        "TASK" => &NEW_ACTIVITY_CELLS,
        "TASKRSRC" => &NEW_ASSIGNMENT_CELLS,
        "PROJWBS" => &NEW_WBS_CELLS,
        "RSRC" => &NEW_RESOURCE_CELLS,
        "ROLES" => &NEW_ROLE_CELLS,
        _ => &[],
    }
}

/// What a new role holds that the model doesn't give: what nearly every role
/// in the corpus holds.
const NEW_ROLE_CELLS: [(&str, &str); 1] = [("cost_qty_type", "QT_Hour")];

/// What a new assignment holds that the model doesn't give: zero for any unit
/// or cost left empty, and the settings nearly every corpus row holds.
const NEW_ASSIGNMENT_CELLS: [(&str, &str); 19] = [
    ("target_lag_drtn_hr_cnt", "0"),
    ("relag_drtn_hr_cnt", "0"),
    ("act_this_per_qty", "0"),
    ("act_this_per_cost", "0"),
    ("remain_qty", "0"),
    ("target_qty", "0"),
    ("target_qty_per_hr", "0"),
    ("remain_qty_per_hr", "0"),
    ("act_reg_qty", "0"),
    ("act_ot_qty", "0"),
    ("target_cost", "0"),
    ("remain_cost", "0"),
    ("act_reg_cost", "0"),
    ("act_ot_cost", "0"),
    ("cost_qty_link_flag", "Y"),
    ("rate_type", "COST_PER_QTY"),
    ("cost_per_qty_source_type", "ST_Rsrc"),
    ("rollup_dates_flag", "Y"),
    ("ts_pend_act_end_flag", "N"),
];

const NEW_ACTIVITY_CELLS: [(&str, &str); 15] = [
    ("phys_complete_pct", "0"),
    ("rev_fdbk_flag", "N"),
    ("lock_plan_flag", "N"),
    ("auto_compute_act_flag", "N"),
    ("priority_type", "PT_Normal"),
    ("review_type", "RV_OK"),
    ("est_wt", "1"),
    ("act_work_qty", "0"),
    ("remain_work_qty", "0"),
    ("target_work_qty", "0"),
    ("act_equip_qty", "0"),
    ("remain_equip_qty", "0"),
    ("target_equip_qty", "0"),
    ("act_this_per_work_qty", "0"),
    ("act_this_per_equip_qty", "0"),
];

/// A number that follows the file, so a new row's GUID differs between files.
fn source_seed(raw: &RawXer) -> u64 {
    crate::fresh::seed_of(raw.source())
}
/// An activity edited to name a project, WBS or calendar has to name one the file
/// has. Real files can already hold rows pointing elsewhere, which aren't ours to
/// refuse.
fn check_references(base: &Schedule, edited: &Schedule) -> Result<(), PatchError> {
    for (id, a) in &edited.activities {
        let before = base.activities.get(id);
        let same = |read: fn(&m::Activity) -> u64| before.is_some_and(|b| read(b) == read(a));
        let known = (same(|a| a.project.0) || a.project == edited.project.id)
            && (same(|a| a.wbs.0) || edited.wbs.contains_key(&a.wbs))
            && (same(|a| a.calendar.0) || edited.calendars.contains_key(&a.calendar));
        let unchanged = before.is_some_and(|b| b.codes == a.codes);
        let coded = unchanged
            || a.codes.iter().all(|(kind, code)| {
                edited
                    .activity_codes
                    .get(code)
                    .is_some_and(|c| c.code_type == *kind)
            });
        if !known || !coded {
            return Err(PatchError::DanglingActivity(*id));
        }
    }
    let new_codes = edited
        .activity_codes
        .iter()
        .filter(|(id, _)| !base.activity_codes.contains_key(id));
    for (id, code) in new_codes {
        let parent_known = code
            .parent
            .is_none_or(|parent| edited.activity_codes.contains_key(&parent));
        if !edited.activity_code_types.contains_key(&code.code_type) || !parent_known {
            return Err(PatchError::Unrepresentable {
                table: "ACTVCODE",
                field: "actv_code_type_id",
                entity: format!("activity code {id}"),
                reason: "it names a code type or parent that isn't in the file",
            });
        }
    }
    Ok(())
}

/// A changed or new relationship must not point at an activity that isn't in
/// the edited schedule. (Unchanged relationships of deleted activities are
/// removed with them.)
fn check_relationship_ends(
    base: &Schedule,
    edited: &Schedule,
    deleted: &BTreeSet<m::ActivityId>,
) -> Result<(), PatchError> {
    let pid = edited.project.id;
    for r in edited.relationships.values() {
        if base.relationships.get(&r.id) == Some(r)
            && (deleted.contains(&r.predecessor) || deleted.contains(&r.successor))
        {
            continue;
        }
        let missing = |project: m::ProjectId, activity: m::ActivityId| {
            deleted.contains(&activity)
                || (project == pid && !edited.activities.contains_key(&activity))
        };
        if missing(r.predecessor_project, r.predecessor)
            || missing(r.successor_project, r.successor)
        {
            return Err(PatchError::DanglingRelationship(r.id));
        }
    }
    Ok(())
}

/// An activity's codes, UDF values and notes, which other tables hold.
type ActivityExtras<'a> = (
    &'a BTreeMap<m::ActivityCodeTypeId, m::ActivityCodeId>,
    &'a BTreeMap<m::UdfTypeId, m::UdfValue>,
    &'a BTreeMap<m::NotebookTopicId, m::Note>,
    &'a BTreeMap<m::StepId, m::Step>,
);

/// Every `Activity` field is either written through `TASK_FIELDS` or returned
/// here for an explicit check. The exhaustive patterns make adding a model
/// field a compile error until the patch writer handles it.
fn activity_extras(a: &m::Activity) -> ActivityExtras<'_> {
    let m::Activity {
        id: _,
        project: _,
        wbs: _,
        calendar: _,
        code: _,
        name: _,
        activity_type: _,
        duration_type: _,
        percent_complete_type: _,
        status: _,
        original_duration: _,
        remaining_duration: _,
        physical_percent_complete: _,
        planned_start: _,
        planned_finish: _,
        actual_start: _,
        actual_finish: _,
        suspend: _,
        resume: _,
        expected_finish: _,
        primary_constraint: _,
        secondary_constraint: _,
        external_early_start: _,
        external_late_finish: _,
        codes,
        udfs,
        notes,
        steps,
        stored,
        source: _,
    } = a;
    let m::StoredResults {
        early_start: _,
        early_finish: _,
        late_start: _,
        late_finish: _,
        remaining_early_start: _,
        remaining_early_finish: _,
        remaining_late_start: _,
        remaining_late_finish: _,
        total_float: _,
        free_float: _,
        on_longest_path: _,
        float_path: _,
        float_path_order: _,
    } = stored;
    (codes, udfs, notes, steps)
}

/// Every `Relationship` field is written through `PRED_FIELDS` (see
/// [`activity_extras`] for why this pattern is exhaustive).
fn relationship_fields_covered(r: &m::Relationship) {
    let m::Relationship {
        id: _,
        predecessor: _,
        successor: _,
        predecessor_project: _,
        successor_project: _,
        kind: _,
        lag: _,
        source: _,
    } = r;
}

/// Rows to remove with `deleted` activities: their `TASK` rows, any row whose
/// `task_id` matches, relationships whose `pred_task_id` matches, and UDF
/// values of removed activities and assignments.
fn dependents(raw: &RawXer, deleted: &BTreeSet<m::ActivityId>) -> BTreeSet<RowRef> {
    // As numbers, because a cell can spell an ID with leading zeros.
    let ids: BTreeSet<u64> = deleted.iter().map(|id| id.0).collect();
    let number = |cell: Option<&str>| cell.and_then(|cell| cell.parse::<u64>().ok());
    let mut rows = BTreeSet::new();
    // Removed entity IDs by the table their UDFs name.
    let mut removed: BTreeMap<&str, BTreeSet<u64>> = BTreeMap::new();
    removed.insert("TASK", ids.clone());
    for (t, table) in raw.tables().iter().enumerate() {
        let task = table.field_index("task_id");
        let pred = (table.name == "TASKPRED")
            .then(|| table.field_index("pred_task_id"))
            .flatten();
        // Rows with UDF values of their own, by their own ID column.
        let own = match table.name.as_str() {
            "TASKRSRC" => Some(("TASKRSRC", "taskrsrc_id")),
            "PROJCOST" => Some(("PROJCOST", "cost_item_id")),
            _ => None,
        };
        let own_id = own.and_then(|(_, column)| table.field_index(column));
        for (r, row) in table.rows.iter().enumerate() {
            let hit = |c: Option<usize>| {
                number(c.and_then(|c| row.get(c))).is_some_and(|id| ids.contains(&id))
            };
            if hit(task) || hit(pred) {
                rows.insert(RowRef { table: t, row: r });
                if let (Some((name, _)), Some(id)) = (own, number(own_id.and_then(|c| row.get(c))))
                {
                    removed.entry(name).or_default().insert(id);
                }
            }
        }
    }
    rows.extend(udf_rows(raw, &removed));
    rows
}

/// The `UDFVALUE` rows of removed entities, by the table their UDFs name.
fn udf_rows(raw: &RawXer, removed: &BTreeMap<&str, BTreeSet<u64>>) -> BTreeSet<RowRef> {
    let number = |cell: Option<&str>| cell.and_then(|cell| cell.parse::<u64>().ok());
    let mut rows = BTreeSet::new();
    let mut subject: BTreeMap<String, String> = BTreeMap::new();
    for table in raw.tables().iter().filter(|t| t.name == "UDFTYPE") {
        let (Some(id), Some(name)) = (
            table.field_index("udf_type_id"),
            table.field_index("table_name"),
        ) else {
            continue;
        };
        for row in &table.rows {
            if let (Some(id), Some(name)) = (row.get(id), row.get(name)) {
                subject.insert(id.to_owned(), name.to_owned());
            }
        }
    }
    for (t, table) in raw
        .tables()
        .iter()
        .enumerate()
        .filter(|(_, t)| t.name == "UDFVALUE")
    {
        let (Some(kind), Some(fk)) = (table.field_index("udf_type_id"), table.field_index("fk_id"))
        else {
            continue;
        };
        for (r, row) in table.rows.iter().enumerate() {
            let target = row.get(kind).and_then(|k| subject.get(k));
            let gone = target
                .and_then(|name| removed.get(name.as_str()))
                .is_some_and(|ids| number(row.get(fk)).is_some_and(|fk| ids.contains(&fk)));
            if gone {
                rows.insert(RowRef { table: t, row: r });
            }
        }
    }
    rows
}

/// The `PROJPCAT` rows giving `project` a value of `kind`, as mapping reads them.
fn project_code_rows(
    raw: &RawXer,
    project: m::ProjectId,
    kind: m::ProjectCodeTypeId,
) -> Vec<RowRef> {
    let mut found = Vec::new();
    for (table, t) in raw.tables().iter().enumerate() {
        if t.name != "PROJPCAT" {
            continue;
        }
        let (Some(owner), Some(of)) =
            (t.field_index("proj_id"), t.field_index("proj_catg_type_id"))
        else {
            continue;
        };
        for (row, cells) in t.rows.iter().enumerate() {
            let number = |column| cells.get(column)?.parse::<u64>().ok();
            if number(owner) == Some(project.0) && number(of) == Some(kind.0) {
                found.push(RowRef { table, row });
            }
        }
    }
    found
}

/// Whether a resource is as it was, but for what the writer can change: its
/// UDF values, which `udf_values` writes, and its limits and new rates, which
/// `resource_rates` writes.
fn same_resource(was: &m::Resource, is: &m::Resource) -> bool {
    let kept = |w: &m::ResourceRate| {
        is.rates
            .iter()
            .any(|i| (i.id, i.effective, i.prices) == (w.id, w.effective, w.prices))
    };
    let mut copy = is.clone();
    copy.udfs.clone_from(&was.udfs);
    copy.rates.clone_from(&was.rates);
    copy == *was && was.rates.iter().all(kept)
}

/// Edits outside this writer's scope must not be silently dropped.
/// Resources, but for their UDF values and limits, and code types and code
/// values stay as they are. One the edited
/// project doesn't hold is fine only if it didn't hold it to start with: an
/// earlier project's edit left it unused, so every project holds it now. A
/// project can take up a resource another project in the file holds, and new
/// ones can come.
fn check_dictionaries(
    (base, first): (&Schedule, &Schedule),
    edited: &Schedule,
) -> Result<(), PatchError> {
    let same = |a: bool, what| {
        if a {
            Ok(())
        } else {
            Err(PatchError::Unsupported(what))
        }
    };
    let kept = |held: bool, is: Option<bool>| is.unwrap_or(!held);
    same(
        base.resources.iter().all(|(id, resource)| {
            let held = first.resources.contains_key(id);
            kept(
                held,
                edited
                    .resources
                    .get(id)
                    .map(|is| same_resource(resource, is)),
            )
        }),
        "resources",
    )?;
    same(
        base.activity_code_types.iter().all(|(id, kind)| {
            let held = first.activity_code_types.contains_key(id);
            kept(
                held,
                edited.activity_code_types.get(id).map(|is| is == kind),
            )
        }),
        "activity code types",
    )?;
    // A code value this call wrote through another project reads back with a
    // source row, so `source` doesn't count.
    let same_code = |was: &m::ActivityCode, is: &m::ActivityCode| {
        (
            was.code_type,
            was.parent,
            &was.value,
            &was.description,
            was.sequence,
        ) == (
            is.code_type,
            is.parent,
            &is.value,
            &is.description,
            is.sequence,
        )
    };
    same(
        base.activity_codes.iter().all(|(id, code)| {
            let held = first.activity_codes.contains_key(id);
            kept(
                held,
                edited.activity_codes.get(id).map(|is| same_code(code, is)),
            )
        }),
        "existing activity code values",
    )?;
    // Roles as code values: one written through another project reads back
    // with source rows. A rate's limit can change, and rates can be added.
    let same_role = |was: &m::Role, is: &m::Role| {
        let kept = |w: &m::RoleRate| {
            is.rates
                .iter()
                .any(|i| (i.id, i.effective, i.prices) == (w.id, w.effective, w.prices))
        };
        (
            &was.short_name,
            &was.name,
            was.parent,
            was.default_cost_follows_units,
        ) == (
            &is.short_name,
            &is.name,
            is.parent,
            is.default_cost_follows_units,
        ) && was.rates.iter().all(kept)
    };
    same(
        base.roles.iter().all(|(id, role)| {
            let held = first.roles.contains_key(id);
            kept(held, edited.roles.get(id).map(|is| same_role(role, is)))
        }),
        "existing roles",
    )?;
    check_project_code_dictionaries((base, first), edited)?;
    // OBS nodes, as roles: one written through another project reads back
    // with a source row.
    let same_obs = |was: &m::Obs, is: &m::Obs| {
        (was.parent, &was.name, &was.description, was.sequence)
            == (is.parent, &is.name, &is.description, is.sequence)
    };
    same(
        base.obs.iter().all(|(id, node)| {
            let held = first.obs.contains_key(id);
            kept(held, edited.obs.get(id).map(|is| same_obs(node, is)))
        }),
        "existing OBS nodes",
    )?;
    check_cost_dictionaries((base, first), edited)
}

/// Project code types and values stay as they are, held as
/// [`check_dictionaries`] holds activity codes.
fn check_project_code_dictionaries(
    (base, first): (&Schedule, &Schedule),
    edited: &Schedule,
) -> Result<(), PatchError> {
    let same = |a: bool, what| {
        if a {
            Ok(())
        } else {
            Err(PatchError::Unsupported(what))
        }
    };
    let kept = |held: bool, is: Option<bool>| is.unwrap_or(!held);
    // Project code types and values: one this call wrote through another
    // project reads back with a source row, so `source` doesn't count.
    let same_type = |was: &m::ProjectCodeType, is: &m::ProjectCodeType| {
        (&was.name, was.max_length, was.sequence) == (&is.name, is.max_length, is.sequence)
    };
    same(
        base.project_code_types.iter().all(|(id, kind)| {
            let held = first.project_code_types.contains_key(id);
            kept(
                held,
                edited
                    .project_code_types
                    .get(id)
                    .map(|is| same_type(kind, is)),
            )
        }),
        "existing project code types",
    )?;
    let same_value = |was: &m::ProjectCode, is: &m::ProjectCode| {
        (
            was.code_type,
            was.parent,
            &was.value,
            &was.description,
            was.sequence,
        ) == (
            is.code_type,
            is.parent,
            &is.value,
            &is.description,
            is.sequence,
        )
    };
    same(
        base.project_codes.iter().all(|(id, code)| {
            let held = first.project_codes.contains_key(id);
            kept(
                held,
                edited.project_codes.get(id).map(|is| same_value(code, is)),
            )
        }),
        "existing project code values",
    )?;
    Ok(())
}

/// Cost accounts and expense categories stay as they are, held as
/// [`check_dictionaries`] holds resources, and new ones can't come yet.
fn check_cost_dictionaries(
    (base, first): (&Schedule, &Schedule),
    edited: &Schedule,
) -> Result<(), PatchError> {
    let same = |a: bool, what| {
        if a {
            Ok(())
        } else {
            Err(PatchError::Unsupported(what))
        }
    };
    let kept = |held: bool, is: Option<bool>| is.unwrap_or(!held);
    let same_account = |was: &m::CostAccount, is: &m::CostAccount| {
        (
            &was.short_name,
            &was.name,
            was.parent,
            was.sequence,
            &was.description,
        ) == (
            &is.short_name,
            &is.name,
            is.parent,
            is.sequence,
            &is.description,
        )
    };
    same(
        base.cost_accounts.iter().all(|(id, account)| {
            let held = first.cost_accounts.contains_key(id);
            kept(
                held,
                edited
                    .cost_accounts
                    .get(id)
                    .map(|is| same_account(account, is)),
            )
        }) && edited
            .cost_accounts
            .keys()
            .all(|id| base.cost_accounts.contains_key(id)),
        "cost accounts",
    )?;
    let same_category = |was: &m::ExpenseCategory, is: &m::ExpenseCategory| {
        (&was.name, was.sequence) == (&is.name, is.sequence)
    };
    same(
        base.expense_categories.iter().all(|(id, category)| {
            let held = first.expense_categories.contains_key(id);
            kept(
                held,
                edited
                    .expense_categories
                    .get(id)
                    .map(|is| same_category(category, is)),
            )
        }) && edited
            .expense_categories
            .keys()
            .all(|id| base.expense_categories.contains_key(id)),
        "expense categories",
    )?;
    Ok(())
}

/// The same notebook topics. One written through another project reads back
/// with a source row, so `source` doesn't count.
fn same_topics(base: &Schedule, edited: &Schedule) -> bool {
    let topic = |t: &m::NotebookTopic| {
        let mut t = t.clone();
        t.source = None;
        t
    };
    base.notebook_topics
        .values()
        .map(topic)
        .eq(edited.notebook_topics.values().map(topic))
}

fn check_unsupported(
    (base, first): (&Schedule, &Schedule),
    edited: &Schedule,
    deleted: &BTreeSet<m::ActivityId>,
) -> Result<(), PatchError> {
    let same = |a: bool, what| {
        if a {
            Ok(())
        } else {
            Err(PatchError::Unsupported(what))
        }
    };
    // A node's code, name and place can change, and new nodes can come.
    let wbs_kept = |was: &m::Wbs, is: &m::Wbs| {
        (was.id, was.project, was.is_project_node) == (is.id, is.project, is.is_project_node)
    };
    same(
        base.wbs
            .iter()
            .all(|(id, was)| edited.wbs.get(id).is_some_and(|is| wbs_kept(was, is))),
        "WBS",
    )?;
    same(
        edited
            .wbs
            .iter()
            .filter(|(id, _)| !base.wbs.contains_key(id))
            .all(|(_, node)| !node.is_project_node),
        "a new WBS node as the project's node",
    )?;
    let working_time_only = |was: &m::Calendar, is: &m::Calendar| {
        let mut same_time = is.clone();
        same_time.work_week = was.work_week.clone();
        same_time.exceptions = was.exceptions.clone();
        // Hours per day, week, month and year are written as they change.
        same_time.hours_per_period = was.hours_per_period;
        same_time == *was
    };
    same(
        base.calendars.iter().all(|(id, was)| {
            edited
                .calendars
                .get(id)
                .is_some_and(|is| is == was || working_time_only(was, is))
        }),
        "calendars other than their working time and hours per day, week, month and year",
    )?;
    check_dictionaries((base, first), edited)?;
    // Fields can come; one that's there stays as it is. One written through
    // another project reads back with a source row, so `source` doesn't count.
    let field = |t: &m::UdfType| (t.subject.clone(), t.label.clone(), t.data_type.clone());
    same(
        base.udf_types.iter().all(|(id, was)| {
            edited
                .udf_types
                .get(id)
                .is_some_and(|is| field(was) == field(is))
        }),
        "existing UDF definitions",
    )?;
    same(same_topics(base, edited), "notebook topics")?;
    // Assignments of deleted activities are removed with them.
    // Rows move when another project's edit removes some, so `source` doesn't
    // count. Assignments can come and go, and one that stays can change only
    // its cost account, units, costs, curve and typed spreads, which
    // `ASSIGNMENT_FIELDS` writes.
    let live = |a: &&m::Assignment| !deleted.contains(&a.activity);
    let content = |a: &'_ m::Assignment| (a.id, a.activity, a.resource, a.project);
    same(
        base.assignments.values().filter(live).all(|a| {
            edited
                .assignments
                .get(&a.id)
                .is_none_or(|is| content(a) == content(is))
        }),
        "resource assignments",
    )?;
    // An expense can change anything but where it is, even one whose
    // activity is deleted, which would take it along.
    let place = |x: &m::Expense| (x.activity, x.project);
    same(
        base.expenses.values().all(|x| {
            edited
                .expenses
                .get(&x.id)
                .is_none_or(|is| place(x) == place(is))
        }),
        "moving an expense to another activity",
    )?;
    // Curves stay as the file has them, and an assignment can take only one
    // of those. Rows move when another project's edit removes some, so
    // `source` doesn't count.
    same(
        base.resource_curves
            .values()
            .map(curve_content)
            .eq(edited.resource_curves.values().map(curve_content)),
        "resource curves",
    )?;
    same(
        edited
            .assignments
            .values()
            .filter_map(|a| a.curve)
            .all(|c| base.resource_curves.contains_key(&c)),
        "an assignment curve that isn't in the file",
    )?;
    let mut project = base.project.clone();
    project.data_date = edited.project.data_date;
    project.planned_start = edited.project.planned_start;
    project.must_finish_by = edited.project.must_finish_by;
    project.stored.scheduled_finish = edited.project.stored.scheduled_finish;
    project.codes.clone_from(&edited.project.codes);
    same(
        project == edited.project,
        "project fields other than its dates",
    )
}

/// A newly typed spread is marked with curve 9, which must not be a curve
/// the file lists, read or not, or it would read back as that curve. One the
/// file already marks keeps its own marker (TF-040).
fn check_marker(raw: &RawXer, base: &Schedule, edited: &Schedule) -> Result<(), PatchError> {
    let newly_typed = edited.assignments.values().any(|a| {
        a.typed_spread.is_some()
            && base
                .assignments
                .get(&a.id)
                .is_none_or(|was| was.typed_spread.is_none())
    });
    let listed = raw
        .tables()
        .iter()
        .filter(|t| t.name == "RSRCCURVDATA")
        .filter_map(|t| t.field_index("curv_id").map(|c| (t, c)))
        .any(|(t, c)| {
            t.rows
                .iter()
                .any(|r| r.get(c).and_then(|v| v.parse::<u64>().ok()) == Some(MARKER.0))
        });
    if newly_typed && listed {
        return Err(PatchError::Unsupported(
            "a hand-typed spread in a file holding the curve that marks one",
        ));
    }
    Ok(())
}

/// Whether the assignment's row names a curve the file lacks and mapping
/// cleared it with a warning, because no hand-typed spread could be read
/// beside it (TF-040). Its spread is unknown, so it can't be changed.
fn names_a_cleared_curve(raw: &RawXer, at: RowRef, was: &m::Assignment) -> bool {
    let table = &raw.tables()[at.table];
    let cell = table
        .field_index("curv_id")
        .and_then(|column| table.rows.get(at.row)?.get(column));
    cell.is_some_and(|cell| !cell.is_empty() && was.curve.is_none() && was.typed_spread.is_none())
}

/// A curve but its source row.
fn curve_content(c: &m::ResourceCurve) -> (m::ResourceCurveId, &str, bool, [f64; m::CURVE_POINTS]) {
    (c.id, &c.name, c.default, c.points)
}

fn row_of(source: Option<m::SourceRef>, entity: &str) -> Result<RowRef, PatchError> {
    source
        .and_then(RowRef::from_source_ref)
        .ok_or_else(|| PatchError::NoSourceRow(entity.to_owned()))
}

/// How to write one model field as one XER cell. `None` means the value
/// can't be written exactly.
pub(crate) type Field<T> = (&'static str, fn(&T) -> Option<String>);

fn unrepresentable(table: &'static str, field: &'static str, entity: &str) -> PatchError {
    PatchError::Unrepresentable {
        table,
        field,
        entity: entity.to_owned(),
        reason: if field == "phys_complete_pct" {
            "percent complete must be finite with at most 4 decimal places"
        } else {
            "hours cannot be represented in the supported export precision"
        },
    }
}

/// Minutes as decimal hours (`480` → `8`, `30` → `0.5`). Nonterminating
/// values use six decimal places and round-trip to the same minute (TF-012).
/// The optional return is shared with other fallible field formatters.
#[must_use]
pub fn hours(minutes: m::Minutes) -> Option<String> {
    let m = minutes.get();
    if m % 3 != 0 {
        let magnitude = i128::from(m).abs();
        let scaled = (magnitude * 1_000_000 + 30) / 60;
        let sign = if m < 0 { "-" } else { "" };
        return Some(format!(
            "{sign}{}.{:06}",
            scaled / 1_000_000,
            scaled % 1_000_000
        ));
    }
    // m / 60 = (m / 3) * 5 / 100.
    let mut coefficient = i128::from(m / 3) * 5;
    let mut scale = 2u32;
    while scale > 0 && coefficient % 10 == 0 {
        coefficient /= 10;
        scale -= 1;
    }
    let digits = coefficient.unsigned_abs().to_string();
    let sign = if coefficient < 0 { "-" } else { "" };
    let scale = scale as usize;
    Some(if scale == 0 {
        format!("{sign}{digits}")
    } else {
        let digits = format!("{digits:0>width$}", width = scale + 1);
        let (int, frac) = digits.split_at(digits.len() - scale);
        format!("{sign}{int}.{frac}")
    })
}

/// Percent complete as written: at most 4 decimal places, so binary-float
/// noise like `0.30000000000000004` is refused rather than written.
fn percent(value: Option<f64>) -> Option<String> {
    let Some(v) = value else {
        return Some(String::new());
    };
    let text = v.to_string();
    let decimals = text.split_once('.').map_or(0, |(_, frac)| frac.len());
    (v.is_finite() && decimals <= 4).then_some(text)
}

fn date(value: Option<m::DateTime>) -> String {
    value.map(|d| d.to_string()).unwrap_or_default()
}

fn opt_hours(value: Option<m::Minutes>) -> Option<String> {
    value.map_or(Some(String::new()), hours)
}

/// A stored number, as written: empty for none, and refused if it isn't finite.
pub(crate) fn decimal(value: Option<f64>) -> Option<String> {
    match value {
        None => Some(String::new()),
        Some(v) if v.is_finite() => Some(v.to_string()),
        Some(_) => None,
    }
}

fn flag(value: Option<bool>) -> String {
    match value {
        Some(true) => "Y".to_owned(),
        Some(false) => "N".to_owned(),
        None => String::new(),
    }
}

fn opt<T: ToString>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn kind(c: Option<m::Constraint>) -> String {
    c.map(|c| enums::constraint_code(c.kind).to_owned())
        .unwrap_or_default()
}

fn cdate(c: Option<m::Constraint>) -> String {
    date(c.and_then(|c| c.date))
}

/// What a new WBS node holds of its own.
const NEW_WBS_CELLS: [(&str, &str); 5] = [
    ("proj_node_flag", "N"),
    ("sum_data_flag", "N"),
    ("est_wt", "1"),
    ("orig_cost", "0"),
    ("indep_remain_total_cost", "0"),
];

/// Settings a new node takes from the project's root: how it's owned, and none
/// of the money. How it earns value comes from the model (`WBS_EARNED_VALUE`).
const WBS_SETTINGS: [&str; 2] = ["obs_id", "status_code"];

/// How a node earns value, as written (TF-041).
pub(crate) const WBS_EARNED_VALUE: [Field<m::Wbs>; 4] = [
    ("ev_compute_type", |w| {
        Some(match &w.earned_value.technique {
            None => String::new(),
            Some(m::EarnedValueTechnique::ActivityPercentComplete) => "EC_Cmp_pct".to_owned(),
            Some(m::EarnedValueTechnique::Other(code)) => code.clone(),
        })
    }),
    ("ev_user_pct", |w| decimal(w.earned_value.user_percent)),
    ("ev_etc_compute_type", |w| {
        Some(match &w.earned_value.estimate {
            None => String::new(),
            Some(m::EstimateMethod::RemainingCost) => "EE_Rem_hr".to_owned(),
            Some(m::EstimateMethod::PerformanceFactorOne) => "EE_PF_1".to_owned(),
            Some(m::EstimateMethod::PerformanceFactorCpi) => "EE_PF_cpi".to_owned(),
            Some(m::EstimateMethod::PerformanceFactorUser) => "EE_PF_user".to_owned(),
            Some(m::EstimateMethod::Other(code)) => code.clone(),
        })
    }),
    ("ev_etc_user_value", |w| decimal(w.earned_value.user_factor)),
];

const WBS_FIELDS: [Field<m::Wbs>; 9] = [
    ("wbs_short_name", |w| Some(w.code.clone())),
    ("wbs_name", |w| Some(w.name.clone())),
    ("parent_wbs_id", |w| Some(opt(w.parent))),
    ("seq_num", |w| Some(w.sequence.to_string())),
    ("obs_id", |w| Some(opt(w.obs))),
    WBS_EARNED_VALUE[0],
    WBS_EARNED_VALUE[1],
    WBS_EARNED_VALUE[2],
    WBS_EARNED_VALUE[3],
];

const NEW_WBS_FIELDS: [Field<m::Wbs>; 7] = [
    ("wbs_id", |w| Some(w.id.to_string())),
    ("proj_id", |w| Some(w.project.to_string())),
    ("wbs_short_name", |w| Some(w.code.clone())),
    ("wbs_name", |w| Some(w.name.clone())),
    ("parent_wbs_id", |w| Some(opt(w.parent))),
    ("seq_num", |w| Some(w.sequence.to_string())),
    ("obs_id", |w| Some(opt(w.obs))),
];

/// A UDF value's `UDFVALUE` column and text, or `None` for a number that
/// can't be written exactly.
fn udf_cell(value: &m::UdfValue) -> Option<(&'static str, String)> {
    Some(match value {
        m::UdfValue::Text(text) => ("udf_text", text.clone()),
        m::UdfValue::Number(number) => ("udf_number", decimal(Some(*number))?),
        m::UdfValue::Date(date) => ("udf_date", date.to_string()),
        m::UdfValue::Code(code) => ("udf_code_id", code.to_string()),
    })
}

/// The `UDFVALUE` rows giving `owner` a value of `kind`, as mapping reads
/// them: a row naming no project counts for any.
fn udf_value_rows(
    raw: &RawXer,
    kind: m::UdfTypeId,
    owner: u64,
    project: Option<m::ProjectId>,
) -> Vec<RowRef> {
    let mut found = Vec::new();
    for (table, t) in raw.tables().iter().enumerate() {
        if t.name != "UDFVALUE" {
            continue;
        }
        let (Some(of), Some(fk)) = (t.field_index("udf_type_id"), t.field_index("fk_id")) else {
            continue;
        };
        let proj = t.field_index("proj_id");
        for (row, cells) in t.rows.iter().enumerate() {
            let number = |column: usize| cells.get(column)?.parse::<u64>().ok();
            let in_project = match (project, proj.and_then(number)) {
                (Some(want), Some(held)) => want.0 == held,
                _ => true,
            };
            if number(of) == Some(kind.0) && number(fk) == Some(owner) && in_project {
                found.push(RowRef { table, row });
            }
        }
    }
    found
}

const NEW_UDF_TYPE_FIELDS: [Field<m::UdfType>; 6] = [
    ("udf_type_id", |t| Some(t.id.to_string())),
    ("table_name", |t| {
        crate::fresh::udf_table(&t.subject).map(str::to_owned)
    }),
    ("udf_type_name", |t| Some(format!("user_field_{}", t.id))),
    ("udf_type_label", |t| Some(t.label.clone())),
    ("logical_data_type", |t| {
        crate::fresh::udf_type_code(&t.data_type).map(str::to_owned)
    }),
    ("super_flag", |_| Some("N".to_owned())),
];

/// What a step's row holds that an edit can change.
const STEP_FIELDS: [Field<m::Step>; 5] = [
    ("proc_name", |s| Some(s.name.clone())),
    ("seq_num", |s| Some(s.sequence.to_string())),
    ("proc_wt", |s| decimal(s.weight)),
    ("complete_pct", |s| decimal(s.percent)),
    ("complete_flag", |s| Some(flag(Some(s.completed)))),
];

/// A step as a new row of `TASKPROC`.
struct StepRow {
    step: m::Step,
    activity: m::ActivityId,
    project: m::ProjectId,
}

const NEW_STEP_FIELDS: [Field<StepRow>; 9] = [
    ("proc_id", |r| Some(r.step.id.to_string())),
    ("task_id", |r| Some(r.activity.to_string())),
    ("proj_id", |r| Some(r.project.to_string())),
    ("seq_num", |r| Some(r.step.sequence.to_string())),
    ("proc_name", |r| Some(r.step.name.clone())),
    ("complete_flag", |r| Some(flag(Some(r.step.completed)))),
    ("proc_wt", |r| decimal(r.step.weight)),
    ("complete_pct", |r| decimal(r.step.percent)),
    ("proc_descr", |r| {
        Some(crate::mapping::notes::encode(&r.step.description))
    }),
];

/// A note as a row of `TASKMEMO` or `WBSMEMO`.
struct NoteRow {
    id: u64,
    owner: u64,
    topic: m::NotebookTopicId,
    project: m::ProjectId,
    html: String,
}

/// Where one kind of note is written.
struct NoteTable {
    table: &'static str,
    id: &'static str,
    text: &'static str,
    fields: [Field<NoteRow>; 5],
}

const ACTIVITY_NOTES: NoteTable = NoteTable {
    table: "TASKMEMO",
    id: "memo_id",
    text: "task_memo",
    fields: [
        ("memo_id", |n| Some(n.id.to_string())),
        ("task_id", |n| Some(n.owner.to_string())),
        ("memo_type_id", |n| Some(n.topic.to_string())),
        ("proj_id", |n| Some(n.project.to_string())),
        ("task_memo", |n| {
            Some(crate::mapping::notes::encode(&n.html))
        }),
    ],
};

const WBS_NOTES: NoteTable = NoteTable {
    table: "WBSMEMO",
    id: "wbs_memo_id",
    text: "wbs_memo",
    fields: [
        ("wbs_memo_id", |n| Some(n.id.to_string())),
        ("proj_id", |n| Some(n.project.to_string())),
        ("memo_type_id", |n| Some(n.topic.to_string())),
        ("wbs_id", |n| Some(n.owner.to_string())),
        ("wbs_memo", |n| Some(crate::mapping::notes::encode(&n.html))),
    ],
};

const NEW_OBS_FIELDS: [Field<m::Obs>; 5] = [
    ("obs_id", |o| Some(o.id.to_string())),
    ("parent_obs_id", |o| Some(opt(o.parent))),
    ("obs_name", |o| Some(o.name.clone())),
    ("obs_descr", |o| Some(o.description.clone())),
    ("seq_num", |o| Some(o.sequence.to_string())),
];

/// Columns a new row can do without, for exports older than them.
/// A new row's default that a missing column can stand for: the column reads
/// as not stated, as an empty cell does in P6's own exports, and the value is
/// the edit's default rather than one asked for. A resource's units per hour
/// default to 1; about a fifth of the corpus's resource rows leave the cell
/// empty, and two files have no column.
pub const UNSTATED_DEFAULTS: [(&str, &str, &str); 1] = [("RSRC", "def_qty_per_hr", "1")];

const OPTIONAL_COLUMNS: [(&str, &str); 4] = [
    ("TASKRSRC", "rsrc_type"),
    ("TASKRSRC", "target_start_date"),
    ("TASKRSRC", "target_end_date"),
    // An internal name P6 makes itself; some exports leave the column out.
    ("UDFTYPE", "udf_type_name"),
];

/// True if this `TASKRSRC` row records work or money already spent.
fn holds_actuals(raw: &RawXer, at: RowRef) -> bool {
    let table = &raw.tables()[at.table];
    let row = &table.rows[at.row];
    ["act_reg_qty", "act_ot_qty", "act_reg_cost", "act_ot_cost"]
        .iter()
        .filter_map(|field| row.get(table.field_index(field)?))
        .any(|cell| cell.parse::<f64>().is_ok_and(|value| value != 0.0))
}

/// Rows in other tables that belong to these assignments.
fn rows_naming_assignments(raw: &RawXer, ids: &BTreeSet<u64>) -> BTreeSet<RowRef> {
    let mut rows = BTreeSet::new();
    if ids.is_empty() {
        return rows;
    }
    for (t, table) in raw.tables().iter().enumerate() {
        let Some(column) = table.field_index("taskrsrc_id") else {
            continue;
        };
        if table.name == "TASKRSRC" {
            continue;
        }
        for (r, row) in table.rows.iter().enumerate() {
            let id = row.get(column).and_then(|cell| cell.parse::<u64>().ok());
            if id.is_some_and(|id| ids.contains(&id)) {
                rows.insert(RowRef { table: t, row: r });
            }
        }
    }
    rows
}

/// A new `TASKRSRC` row. Its dates are the activity's before recalculating, and
/// assignment dates aren't kept current after that, which `edit` warns about.
struct NewAssignment {
    assignment: m::Assignment,
    /// The resource's type; none for a role alone.
    kind: Option<m::ResourceType>,
    start: Option<m::DateTime>,
    finish: Option<m::DateTime>,
}

const NEW_ASSIGNMENT_FIELDS: [Field<NewAssignment>; 8] = [
    ("taskrsrc_id", |r| Some(r.assignment.id.to_string())),
    ("task_id", |r| Some(r.assignment.activity.to_string())),
    ("rsrc_id", |r| Some(opt(r.assignment.resource))),
    ("role_id", |r| Some(opt(r.assignment.role))),
    ("proj_id", |r| Some(r.assignment.project.to_string())),
    ("rsrc_type", |r| {
        Some(r.kind.map_or("", enums::resource_code).to_owned())
    }),
    ("target_start_date", |r| Some(date(r.start))),
    ("target_end_date", |r| Some(date(r.finish))),
];

const NEW_CODE_TYPE_FIELDS: [Field<m::ActivityCodeType>; 6] = [
    ("actv_code_type_id", |t| Some(t.id.to_string())),
    ("actv_code_type", |t| Some(t.name.clone())),
    ("actv_code_type_scope", |t| {
        Some(enums::scope_code(t.scope).to_owned())
    }),
    ("proj_id", |t| Some(opt(t.project))),
    ("actv_short_len", |t| Some(opt(t.max_length))),
    ("seq_num", |t| Some(t.sequence.to_string())),
];

const NEW_PROJECT_CODE_TYPE_FIELDS: [Field<m::ProjectCodeType>; 4] = [
    ("proj_catg_type_id", |t| Some(t.id.to_string())),
    ("proj_catg_type", |t| Some(t.name.clone())),
    ("proj_catg_short_len", |t| Some(opt(t.max_length))),
    ("seq_num", |t| Some(t.sequence.to_string())),
];

const NEW_PROJECT_CODE_FIELDS: [Field<m::ProjectCode>; 6] = [
    ("proj_catg_id", |c| Some(c.id.to_string())),
    ("proj_catg_type_id", |c| Some(c.code_type.to_string())),
    ("parent_proj_catg_id", |c| Some(opt(c.parent))),
    ("proj_catg_short_name", |c| Some(c.value.clone())),
    ("proj_catg_name", |c| Some(c.description.clone())),
    ("seq_num", |c| Some(c.sequence.to_string())),
];

const NEW_ROLE_FIELDS: [Field<m::Role>; 5] = [
    ("role_id", |r| Some(r.id.to_string())),
    ("role_short_name", |r| Some(r.short_name.clone())),
    ("role_name", |r| Some(r.name.clone())),
    ("parent_role_id", |r| Some(opt(r.parent))),
    ("def_cost_qty_link_flag", |r| {
        Some(
            if r.default_cost_follows_units {
                "Y"
            } else {
                "N"
            }
            .to_owned(),
        )
    }),
];

const NEW_RESOURCE_FIELDS: [Field<m::Resource>; 8] = [
    ("rsrc_id", |r| Some(r.id.to_string())),
    ("rsrc_short_name", |r| Some(r.short_name.clone())),
    ("rsrc_name", |r| Some(r.name.clone())),
    ("rsrc_type", |r| {
        Some(enums::resource_code(r.kind).to_owned())
    }),
    ("clndr_id", |r| Some(opt(r.calendar))),
    ("parent_rsrc_id", |r| Some(opt(r.parent))),
    ("def_qty_per_hr", |r| decimal(r.default_units_per_hour)),
    ("def_cost_qty_link_flag", |r| {
        Some(flag(Some(r.default_cost_follows_units)))
    }),
];

/// The one field of a rate an edit changes.
const RATE_LIMIT_FIELDS: [Field<m::ResourceRate>; 1] =
    [("max_qty_per_hr", |r| decimal(r.max_units_per_hour))];

/// A new rate, with its resource.
const NEW_RATE_FIELDS: [Field<(m::ResourceId, m::ResourceRate)>; 9] = [
    ("rsrc_rate_id", |(_, r)| Some(r.id.to_string())),
    ("rsrc_id", |(id, _)| Some(id.to_string())),
    ("start_date", |(_, r)| Some(date(r.effective))),
    ("cost_per_qty", |(_, r)| decimal(r.prices[0])),
    ("cost_per_qty2", |(_, r)| decimal(r.prices[1])),
    ("cost_per_qty3", |(_, r)| decimal(r.prices[2])),
    ("cost_per_qty4", |(_, r)| decimal(r.prices[3])),
    ("cost_per_qty5", |(_, r)| decimal(r.prices[4])),
    ("max_qty_per_hr", |(_, r)| decimal(r.max_units_per_hour)),
];

const ROLE_RATE_LIMIT_FIELDS: [Field<m::RoleRate>; 1] =
    [("max_qty_per_hr", |r| decimal(r.max_units_per_hour))];

/// A new role rate, with its role.
const NEW_ROLE_RATE_FIELDS: [Field<(m::RoleId, m::RoleRate)>; 9] = [
    ("role_rate_id", |(_, r)| Some(r.id.to_string())),
    ("role_id", |(id, _)| Some(id.to_string())),
    ("start_date", |(_, r)| Some(date(r.effective))),
    ("cost_per_qty", |(_, r)| decimal(r.prices[0])),
    ("cost_per_qty2", |(_, r)| decimal(r.prices[1])),
    ("cost_per_qty3", |(_, r)| decimal(r.prices[2])),
    ("cost_per_qty4", |(_, r)| decimal(r.prices[3])),
    ("cost_per_qty5", |(_, r)| decimal(r.prices[4])),
    ("max_qty_per_hr", |(_, r)| decimal(r.max_units_per_hour)),
];

/// What a new resource holds that the model doesn't: the settings nearly
/// every corpus row holds.
const NEW_RESOURCE_CELLS: [(&str, &str); 4] = [
    ("cost_qty_type", "QT_Hour"),
    ("active_flag", "Y"),
    ("auto_compute_act_flag", "Y"),
    ("ot_flag", "N"),
];

const NEW_CODE_FIELDS: [Field<m::ActivityCode>; 6] = [
    ("actv_code_id", |c| Some(c.id.to_string())),
    ("actv_code_type_id", |c| Some(c.code_type.to_string())),
    ("parent_actv_code_id", |c| Some(opt(c.parent))),
    ("short_name", |c| Some(c.value.clone())),
    ("actv_code_name", |c| Some(c.description.clone())),
    ("seq_num", |c| Some(c.sequence.to_string())),
];

const CALENDAR_FIELDS: [Field<m::Calendar>; 5] = [
    ("clndr_data", |c| crate::fresh::calendar_data(c).ok()),
    ("day_hr_cnt", |c| opt_hours(c.hours_per_period.day)),
    ("week_hr_cnt", |c| opt_hours(c.hours_per_period.week)),
    ("month_hr_cnt", |c| opt_hours(c.hours_per_period.month)),
    ("year_hr_cnt", |c| opt_hours(c.hours_per_period.year)),
];

const NEW_CALENDAR_FIELDS: [Field<m::Calendar>; 10] = [
    ("clndr_id", |c| Some(c.id.to_string())),
    ("clndr_name", |c| Some(c.name.clone())),
    ("clndr_type", |c| {
        Some(
            match c.kind {
                m::CalendarType::Global => "CA_Base",
                m::CalendarType::Project => "CA_Project",
                m::CalendarType::Resource => "CA_Rsrc",
            }
            .to_owned(),
        )
    }),
    ("proj_id", |c| Some(opt(c.project))),
    ("default_flag", |c| Some(flag(Some(c.is_default)))),
    ("clndr_data", |c| crate::fresh::calendar_data(c).ok()),
    ("day_hr_cnt", |c| opt_hours(c.hours_per_period.day)),
    ("week_hr_cnt", |c| opt_hours(c.hours_per_period.week)),
    ("month_hr_cnt", |c| opt_hours(c.hours_per_period.month)),
    ("year_hr_cnt", |c| opt_hours(c.hours_per_period.year)),
];

pub(crate) const PROJECT_FIELDS: [Field<m::Project>; 5] = [
    ("proj_id", |p| Some(p.id.to_string())),
    ("last_recalc_date", |p| Some(date(p.data_date))),
    ("plan_start_date", |p| Some(date(p.planned_start))),
    ("plan_end_date", |p| Some(date(p.must_finish_by))),
    ("scd_end_date", |p| Some(date(p.stored.scheduled_finish))),
];

pub(crate) const TASK_FIELDS: [Field<m::Activity>; 39] = [
    ("task_id", |a| Some(a.id.to_string())),
    ("proj_id", |a| Some(a.project.to_string())),
    ("wbs_id", |a| Some(a.wbs.to_string())),
    ("clndr_id", |a| Some(a.calendar.to_string())),
    ("task_code", |a| Some(a.code.clone())),
    ("task_name", |a| Some(a.name.clone())),
    ("task_type", |a| {
        Some(enums::activity_code(a.activity_type).to_owned())
    }),
    ("duration_type", |a| {
        Some(enums::duration_code(a.duration_type).to_owned())
    }),
    ("complete_pct_type", |a| {
        Some(enums::percent_code(a.percent_complete_type).to_owned())
    }),
    ("status_code", |a| {
        Some(enums::status_code(a.status).to_owned())
    }),
    ("target_drtn_hr_cnt", |a| hours(a.original_duration)),
    ("remain_drtn_hr_cnt", |a| hours(a.remaining_duration)),
    ("phys_complete_pct", |a| {
        percent(a.physical_percent_complete)
    }),
    ("target_start_date", |a| Some(date(a.planned_start))),
    ("target_end_date", |a| Some(date(a.planned_finish))),
    ("act_start_date", |a| Some(date(a.actual_start))),
    ("act_end_date", |a| Some(date(a.actual_finish))),
    ("suspend_date", |a| Some(date(a.suspend))),
    ("resume_date", |a| Some(date(a.resume))),
    ("expect_end_date", |a| Some(date(a.expected_finish))),
    ("cstr_type", |a| Some(kind(a.primary_constraint))),
    ("cstr_date", |a| Some(cdate(a.primary_constraint))),
    ("cstr_type2", |a| Some(kind(a.secondary_constraint))),
    ("cstr_date2", |a| Some(cdate(a.secondary_constraint))),
    ("external_early_start_date", |a| {
        Some(date(a.external_early_start))
    }),
    ("external_late_end_date", |a| {
        Some(date(a.external_late_finish))
    }),
    ("early_start_date", |a| Some(date(a.stored.early_start))),
    ("early_end_date", |a| Some(date(a.stored.early_finish))),
    ("late_start_date", |a| Some(date(a.stored.late_start))),
    ("late_end_date", |a| Some(date(a.stored.late_finish))),
    ("restart_date", |a| {
        Some(date(a.stored.remaining_early_start))
    }),
    ("reend_date", |a| {
        Some(date(a.stored.remaining_early_finish))
    }),
    ("rem_late_start_date", |a| {
        Some(date(a.stored.remaining_late_start))
    }),
    ("rem_late_end_date", |a| {
        Some(date(a.stored.remaining_late_finish))
    }),
    ("total_float_hr_cnt", |a| opt_hours(a.stored.total_float)),
    ("free_float_hr_cnt", |a| opt_hours(a.stored.free_float)),
    ("driving_path_flag", |a| {
        Some(flag(a.stored.on_longest_path))
    }),
    ("float_path", |a| Some(opt(a.stored.float_path))),
    ("float_path_order", |a| Some(opt(a.stored.float_path_order))),
];

/// Every expense cell the model holds.
pub(crate) const EXPENSE_FIELDS: [Field<m::Expense>; 17] = [
    ("cost_item_id", |x| Some(x.id.to_string())),
    ("task_id", |x| Some(x.activity.to_string())),
    ("proj_id", |x| Some(x.project.to_string())),
    ("acct_id", |x| Some(opt(x.cost_account))),
    ("cost_type_id", |x| Some(opt(x.category))),
    ("cost_name", |x| Some(x.name.clone())),
    ("cost_load_type", |x| {
        Some(opt(x.accrual.map(enums::accrual_code)))
    }),
    ("target_cost", |x| decimal(x.budgeted_cost)),
    ("act_cost", |x| decimal(x.actual_cost)),
    ("remain_cost", |x| decimal(x.remaining_cost)),
    ("cost_per_qty", |x| decimal(x.price_per_unit)),
    ("target_qty", |x| decimal(x.budgeted_units)),
    ("qty_name", |x| Some(x.unit_of_measure.clone())),
    ("auto_compute_act_flag", |x| {
        Some(flag(Some(x.auto_compute_actuals)))
    }),
    ("po_number", |x| Some(x.po_number.clone())),
    ("vendor_name", |x| Some(x.vendor.clone())),
    ("cost_descr", |x| Some(x.description.clone())),
];

/// The curve that marks a hand-typed spread (TF-040).
const MARKER: m::ResourceCurveId = m::ResourceCurveId(spread_data::MARKER_CURVE);

/// An assignment's `curv_id`: its curve, the marker for a hand-typed spread,
/// or empty for an even spread. `None` for both a curve and a typed spread,
/// or a typed spread with nothing in it, which would read back as neither.
fn assignment_curve(a: &m::Assignment) -> Option<String> {
    match (a.curve, &a.typed_spread) {
        (Some(_), Some(_)) => None,
        (curve, None) => Some(opt(curve)),
        (None, Some(t)) if t.is_empty() => None,
        (None, Some(_)) => Some(MARKER.to_string()),
    }
}

/// One of an assignment's hand-typed spreads, empty without one.
fn typed_cell(
    a: &m::Assignment,
    part: fn(&m::TypedSpread) -> &[m::SpreadSegment],
) -> Option<String> {
    a.typed_spread
        .as_ref()
        .map_or(Some(String::new()), |t| spread_data::encode(part(t)))
}

/// An assignment's cost account, units, costs, curve and hand-typed spreads.
pub(crate) const ASSIGNMENT_FIELDS: [Field<m::Assignment>; 19] = [
    ("acct_id", |a| Some(opt(a.cost_account))),
    ("target_qty", |a| decimal(a.amounts.budgeted_units)),
    ("act_reg_qty", |a| decimal(a.amounts.actual_units)),
    ("act_ot_qty", |a| decimal(a.amounts.actual_overtime_units)),
    ("remain_qty", |a| decimal(a.amounts.remaining_units)),
    ("target_qty_per_hr", |a| {
        decimal(a.amounts.budgeted_units_per_hour)
    }),
    ("remain_qty_per_hr", |a| {
        decimal(a.amounts.remaining_units_per_hour)
    }),
    ("target_cost", |a| decimal(a.amounts.budgeted_cost)),
    ("act_reg_cost", |a| decimal(a.amounts.actual_cost)),
    ("act_ot_cost", |a| decimal(a.amounts.actual_overtime_cost)),
    ("remain_cost", |a| decimal(a.amounts.remaining_cost)),
    ("cost_per_qty", |a| decimal(a.amounts.price_per_unit)),
    ("cost_per_qty_source_type", |a| {
        Some(opt(a.amounts.price_source.map(enums::price_source_code)))
    }),
    ("rate_type", |a| {
        Some(opt(a.amounts.rate_type.map(enums::rate_type_code)))
    }),
    ("cost_qty_link_flag", |a| {
        Some(flag(Some(a.amounts.cost_follows_units)))
    }),
    ("curv_id", assignment_curve),
    ("target_crv", |a| typed_cell(a, |t| &t.planned)),
    ("remain_crv", |a| typed_cell(a, |t| &t.remaining)),
    ("actual_crv", |a| typed_cell(a, |t| &t.actual)),
];

pub(crate) const PRED_FIELDS: [Field<m::Relationship>; 7] = [
    ("task_pred_id", |r| Some(r.id.to_string())),
    ("task_id", |r| Some(r.successor.to_string())),
    ("pred_task_id", |r| Some(r.predecessor.to_string())),
    ("proj_id", |r| Some(r.successor_project.to_string())),
    ("pred_proj_id", |r| Some(r.predecessor_project.to_string())),
    ("pred_type", |r| {
        Some(enums::relationship_code(r.kind).to_owned())
    }),
    ("lag_hr_cnt", |r| hours(r.lag)),
];

#[cfg(test)]
mod tests;
