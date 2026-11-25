//! Canonical model projection from raw XER data (ARCHITECTURE.md, Lossless round trips and Data model).
//!
//! This projection recognizes exact minutes and TF-012 export-precision
//! spellings. Other unrepresentable required rows remain raw and are reported;
//! dates/durations are never invented. Inspect diagnostics before using any imported schedule.

use crate::{
    RawXer, calendar_data,
    typed::{Cell, CellState, Decimal, RowRef, TypedTable, TypedXer},
};
use std::collections::{BTreeMap, BTreeSet};
use totalfloat_model as m;

mod entities;
pub(crate) mod enums;
pub(crate) mod notes;
mod options;
mod shared;
mod steps;

/// Stable mapping warning categories, without private cell contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticKind {
    /// Raw reader diagnostics remain relevant to interpretation.
    Raw,
    /// Typed scalar warnings remain relevant to interpretation.
    Typed,
    /// Calendar definitions could not all be resolved.
    Calendar,
    /// A required identity, enum or integer-minute input cannot be represented.
    SkippedRow,
    /// A declared row's owning project does not occur in PROJECT.
    OrphanRow,
    /// Missing predecessor project recovered from unambiguous raw TASK ownership.
    InferredProject,
    /// An identity is repeated; the first representable row wins.
    DuplicateId,
    /// An optional value cannot be represented and remains raw.
    UnrepresentedValue,
    /// A schedule option uses the model's documented fallback.
    DefaultedOption,
    /// An option's semantics have not been verified (TF-008).
    UnconfirmedOption,
    /// A UDF target/type cannot be attached safely.
    UnattachedUdf,
    /// A referenced calendar has no effective model.
    UnavailableCalendar,
    /// A required reference in the mapped model points at an entity that
    /// isn't there (see `Schedule::validate`); the entity is kept.
    Integrity,
    /// Stored values disagree with each other, such as a hand-typed spread
    /// whose units aren't its assignment's; both are kept as the file has
    /// them.
    InconsistentValues,
}

/// One recoverable mapping problem. Source coordinates always refer to `RawXer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Warning category.
    pub kind: DiagnosticKind,
    /// Raw table/row coordinates, if the problem belongs to a row.
    pub source: Option<RowRef>,
    /// Exact field name, if applicable.
    pub field: String,
    /// Description without original values or project names.
    pub message: String,
}

/// Import report. The raw source remains authoritative for unrepresented values.
#[derive(Debug, Default)]
pub struct Diagnostics {
    /// Bounded warning details in deterministic traversal order.
    pub details: Vec<Diagnostic>,
    /// Complete warning counts, including suppressed details.
    pub counts: BTreeMap<DiagnosticKind, usize>,
    /// Warning details omitted after the first 1,000.
    pub suppressed: usize,
    /// Nonintegral-minute hour fields, counted once per raw cell independently of mapping.
    pub non_whole_minute_hour_fields: BTreeMap<String, usize>,
    /// Whole-minute hour tokens that overflow the model minute range.
    pub out_of_range_hour_fields: BTreeMap<String, usize>,
    /// Distinct rows omitted because a required value was unrepresentable;
    /// excludes orphan/duplicate rows.
    pub skipped_rows: usize,
    /// Complete counts of fields that prevented a required row from mapping.
    pub skipped_fields: BTreeMap<String, usize>,
    /// Every raw row the projection left out (skipped, orphaned or
    /// duplicate), uncapped. Strict conversion's contract: each raw row is
    /// either represented in a schedule or listed here, never silently dropped
    /// and never rounded into place.
    pub omitted: BTreeSet<RowRef>,
}

/// A scheduling input couldn't be read, so its row is left out.
struct Unreadable;

impl Diagnostics {
    fn warn(
        &mut self,
        kind: DiagnosticKind,
        source: Option<RowRef>,
        field: &str,
        message: impl Into<String>,
    ) {
        *self.counts.entry(kind).or_default() += 1;
        let omits = matches!(
            kind,
            DiagnosticKind::SkippedRow | DiagnosticKind::OrphanRow | DiagnosticKind::DuplicateId
        );
        let newly_omitted = omits && source.is_some_and(|row| self.omitted.insert(row));
        if kind == DiagnosticKind::SkippedRow {
            if newly_omitted {
                self.skipped_rows += 1;
            }
            *self.skipped_fields.entry(field.to_owned()).or_default() += 1;
        }
        if self.details.len() < 1000 {
            self.details.push(Diagnostic {
                kind,
                source,
                field: field.to_owned(),
                message: message.into(),
            });
        } else {
            self.suppressed += 1;
        }
    }

    fn id(&mut self, cell: &Cell<'_, i64>, source: RowRef, field: &str) -> Option<u64> {
        if let Some(id) = optional_id(cell) {
            return Some(id);
        }
        self.warn(
            DiagnosticKind::SkippedRow,
            Some(source),
            field,
            "required identity is missing, negative or out of range; row retained raw",
        );
        None
    }

    fn required_minutes(
        &mut self,
        cell: &Cell<'_, Decimal>,
        source: RowRef,
        field: &str,
    ) -> Option<m::Minutes> {
        if let Some(value) = cell.value().and_then(|value| value.export_minutes()) {
            return Some(value);
        }
        self.warn(
            DiagnosticKind::SkippedRow,
            Some(source),
            field,
            "required duration/lag is missing or not recognized whole minutes; row retained raw",
        );
        None
    }

    /// An optional date the schedule is calculated from. Empty means no date, but
    /// a date that can't be read isn't the same, so it leaves the row out, and
    /// the error means that. Stored results stay optional.
    fn input_date(
        &mut self,
        cell: &Cell<'_, m::DateTime>,
        source: RowRef,
        field: &str,
    ) -> Result<Option<m::DateTime>, Unreadable> {
        if !matches!(cell.state(), CellState::Invalid(_)) {
            return Ok(date(cell));
        }
        self.warn(
            DiagnosticKind::SkippedRow,
            Some(source),
            field,
            "scheduling date is not readable",
        );
        Err(Unreadable)
    }

    fn minutes(
        &mut self,
        cell: &Cell<'_, Decimal>,
        source: RowRef,
        field: &str,
    ) -> Option<m::Minutes> {
        let value = cell.value().and_then(|value| value.export_minutes());
        if value.is_none() && cell.original().is_some_and(|s| !s.is_empty()) {
            self.warn(
                DiagnosticKind::UnrepresentedValue,
                Some(source),
                field,
                "hour value is not representable as exact minutes; retained raw",
            );
        }
        value
    }

    /// An optional code: empty is `None`, as is one the model doesn't know,
    /// which is reported. A patched file keeps the cell as it was.
    fn optional_code<T>(
        &mut self,
        cell: &Cell<'_, &str>,
        source: RowRef,
        field: &str,
        parse: fn(&str) -> Option<T>,
    ) -> Option<T> {
        let code = cell.value().copied().filter(|code| !code.is_empty())?;
        let parsed = parse(code);
        if parsed.is_none() {
            self.warn(
                DiagnosticKind::UnrepresentedValue,
                Some(source),
                field,
                "code isn't one the model knows; a patched file keeps it, a new one leaves it out",
            );
        }
        parsed
    }

    fn code<T>(
        &mut self,
        cell: &Cell<'_, &str>,
        source: RowRef,
        field: &str,
        parse: fn(&str) -> Option<T>,
    ) -> Option<T> {
        if let Some(value) = cell.value().and_then(|code| parse(code)) {
            return Some(value);
        }
        self.warn(
            DiagnosticKind::SkippedRow,
            Some(source),
            field,
            "required code is missing or unsupported; row retained raw",
        );
        None
    }
}

fn optional_id(cell: &Cell<'_, i64>) -> Option<u64> {
    u64::try_from(*cell.value()?).ok()
}
fn parent_id(cell: &Cell<'_, i64>) -> Option<u64> {
    optional_id(cell).filter(|id| *id > 0)
}
fn text(cell: &Cell<'_, &str>) -> String {
    cell.value().copied().unwrap_or("").to_owned()
}
fn date(cell: &Cell<'_, m::DateTime>) -> Option<m::DateTime> {
    cell.value().copied()
}
fn number(cell: &Cell<'_, Decimal>) -> Option<f64> {
    cell.value()?
        .to_string()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}
fn unsigned(cell: &Cell<'_, i64>) -> Option<u32> {
    u32::try_from(*cell.value()?).ok()
}

/// Maps every representable PROJECT row and its entities without scheduling.
///
/// Durations/lag not recognized by TF-012, invalid IDs and unknown required
/// enums cause a diagnosed row omission. No synthetic IDs are used.
/// The original raw data is not mutated and remains available for all omissions.
#[must_use]
pub fn to_model(raw: &RawXer) -> (Vec<m::Schedule>, Diagnostics) {
    let typed = TypedXer::decode(raw);
    let calendars = calendar_data::decode_calendars(&typed);
    let mut report = Diagnostics::default();

    let mut schedules = BTreeMap::new();
    for table in &typed.tables {
        if let TypedTable::Project { rows, .. } = table {
            for row in rows {
                let Some(id) = report.id(&row.proj_id, row.meta.source, "proj_id") else {
                    continue;
                };
                let source = row.meta.source;
                let dates = (
                    report.input_date(&row.last_recalc_date, source, "last_recalc_date"),
                    report.input_date(&row.plan_start_date, source, "plan_start_date"),
                    report.input_date(&row.plan_end_date, source, "plan_end_date"),
                );
                let (Ok(data_date), Ok(planned_start), Ok(must_finish_by)) = dates else {
                    continue;
                };
                let mut project = m::Project::new(m::ProjectId(id), text(&row.proj_short_name));
                project.data_date = data_date;
                project.planned_start = planned_start;
                project.must_finish_by = must_finish_by;
                project.default_calendar = parent_id(&row.clndr_id).map(m::CalendarId);
                project.stored.scheduled_finish = date(&row.scd_end_date);
                project.percent_from_steps =
                    row.step_complete_flag.value().copied().unwrap_or(false);
                project.source = row.meta.source.to_source_ref();
                if let std::collections::btree_map::Entry::Vacant(entry) =
                    schedules.entry(project.id)
                {
                    options::project(&mut project, row, &mut report);
                    entry.insert(m::Schedule::new(project));
                } else {
                    report.warn(
                        DiagnosticKind::DuplicateId,
                        Some(row.meta.source),
                        "proj_id",
                        "duplicate project; first representable row used",
                    );
                }
            }
        }
    }
    options::apply(&typed, &mut schedules, &mut report);
    entities::apply(&typed, &mut schedules, &mut report);
    shared::apply(&typed, &calendars, &mut schedules, &mut report);
    notes::apply(&typed, &mut schedules, &mut report);
    steps::apply(&typed, &mut schedules, &mut report);
    for schedule in schedules.values_mut() {
        integrity(schedule, &mut report);
    }
    observations(raw, &typed, &calendars, &mut report);
    (schedules.into_values().collect(), report)
}

/// An expense whose activity isn't in the project can't be used, so it's
/// omitted.
fn omit_orphan_expenses(schedule: &mut m::Schedule, report: &mut Diagnostics) {
    let activities = &schedule.activities;
    let mut dropped = Vec::new();
    schedule.expenses.retain(|_, x| {
        let kept = activities.contains_key(&x.activity);
        if !kept {
            dropped.push(x.source);
        }
        kept
    });
    for source in dropped {
        report.warn(
            DiagnosticKind::OrphanRow,
            source.and_then(RowRef::from_source_ref),
            "task_id",
            "expense activity is not in the model; expense omitted",
        );
    }
}

/// An expense keeps its cost when its cost account or category isn't in the
/// file, and a cost account its place when its parent isn't.
fn clear_missing_cost_links(
    schedule: &mut m::Schedule,
    cleared: &mut Vec<(Option<m::SourceRef>, &'static str, &'static str)>,
) {
    let ids: std::collections::BTreeSet<m::CostAccountId> =
        schedule.cost_accounts.keys().copied().collect();
    for account in schedule.cost_accounts.values_mut() {
        if account.parent.is_some_and(|p| !ids.contains(&p)) {
            account.parent = None;
            cleared.push((
                account.source,
                "parent_acct_id",
                "parent cost account is not in the model; cleared",
            ));
        }
    }
    for a in schedule.assignments.values_mut() {
        if a.cost_account.is_some_and(|c| !ids.contains(&c)) {
            a.cost_account = None;
            cleared.push((
                a.source,
                "acct_id",
                "assignment cost account is not in the model; cleared",
            ));
        }
    }
    let categories = &schedule.expense_categories;
    for x in schedule.expenses.values_mut() {
        if x.cost_account.is_some_and(|a| !ids.contains(&a)) {
            x.cost_account = None;
            cleared.push((
                x.source,
                "acct_id",
                "expense cost account is not in the model; cleared",
            ));
        }
        if x.category.is_some_and(|c| !categories.contains_key(&c)) {
            x.category = None;
            cleared.push((
                x.source,
                "cost_type_id",
                "expense category is not in the model; cleared",
            ));
        }
    }
}

/// A resource's assignment keeps its resource when the role it names isn't
/// in the file; one by role alone is left out before this. A role whose
/// parent isn't in the file loses its parent, as a resource does.
fn clear_missing_roles(
    schedule: &mut m::Schedule,
    cleared: &mut Vec<(Option<m::SourceRef>, &'static str, &'static str)>,
) {
    let ids: std::collections::BTreeSet<m::RoleId> = schedule.roles.keys().copied().collect();
    for role in schedule.roles.values_mut() {
        if role.parent.is_some_and(|p| !ids.contains(&p)) {
            role.parent = None;
            cleared.push((
                role.source,
                "parent_role_id",
                "parent role is not in the model; cleared",
            ));
        }
    }
    let roles = &schedule.roles;
    for a in schedule.assignments.values_mut() {
        if a.role.is_some_and(|r| !roles.contains_key(&r)) {
            a.role = None;
            cleared.push((
                a.source,
                "role_id",
                "assignment role is not in the model; cleared",
            ));
        }
    }
}

/// Clears optional references to roles, curves, cost accounts and expense
/// categories that aren't in the model.
fn clear_missing_links(
    schedule: &mut m::Schedule,
    cleared: &mut Vec<(Option<m::SourceRef>, &'static str, &'static str)>,
) {
    clear_missing_roles(schedule, cleared);
    clear_missing_curves(schedule, cleared);
    clear_missing_cost_links(schedule, cleared);
    clear_missing_project_codes(schedule, cleared);
    clear_missing_obs(schedule, cleared);
}

/// An OBS node whose parent the file lacks loses it, and a WBS node naming an
/// OBS node the file lacks has no responsible manager, each with a warning.
fn clear_missing_obs(
    schedule: &mut m::Schedule,
    cleared: &mut Vec<(Option<m::SourceRef>, &'static str, &'static str)>,
) {
    let nodes: BTreeSet<m::ObsId> = schedule.obs.keys().copied().collect();
    for node in schedule.obs.values_mut() {
        if node.parent.is_some_and(|p| !nodes.contains(&p)) {
            node.parent = None;
            cleared.push((
                node.source,
                "parent_obs_id",
                "parent OBS node is not in the model; cleared",
            ));
        }
    }
    for node in schedule.wbs.values_mut() {
        if node.obs.is_some_and(|o| !nodes.contains(&o)) {
            node.obs = None;
            cleared.push((
                node.source,
                "obs_id",
                "WBS responsible OBS node is not in the model; cleared",
            ));
        }
    }
}

/// A project code value whose parent the file lacks is cleared, as a role
/// whose parent is missing.
fn clear_missing_project_codes(
    schedule: &mut m::Schedule,
    cleared: &mut Vec<(Option<m::SourceRef>, &'static str, &'static str)>,
) {
    let codes: BTreeSet<m::ProjectCodeId> = schedule.project_codes.keys().copied().collect();
    for code in schedule.project_codes.values_mut() {
        if code.parent.is_some_and(|p| !codes.contains(&p)) {
            code.parent = None;
            cleared.push((
                code.source,
                "parent_proj_catg_id",
                "parent project code is not in the model; cleared",
            ));
        }
    }
}

/// An assignment naming a curve the file doesn't hold spreads by a curve
/// that can't be known. One with a hand-typed spread was given it, and its
/// curve, the file's marker, cleared without a warning (TF-039).
fn clear_missing_curves(
    schedule: &mut m::Schedule,
    cleared: &mut Vec<(Option<m::SourceRef>, &'static str, &'static str)>,
) {
    let curves = &schedule.resource_curves;
    for a in schedule.assignments.values_mut() {
        if a.curve.is_some_and(|c| !curves.contains_key(&c)) {
            a.curve = None;
            cleared.push((
                a.source,
                "curv_id",
                "assignment curve is not in the model; cleared",
            ));
        }
    }
}

/// Strict conversion leaves no silent dangling references. Relationships and
/// assignments whose ends weren't mapped can't be used, so they are omitted;
/// optional references to missing entities are cleared with a warning; any
/// remaining finding (an activity whose WBS or calendar is missing) is
/// reported as [`DiagnosticKind::Integrity`].
fn integrity(schedule: &mut m::Schedule, report: &mut Diagnostics) {
    let row = |source: Option<m::SourceRef>| source.and_then(RowRef::from_source_ref);
    let pid = schedule.project.id;
    let activities = &schedule.activities;
    let mut dropped = Vec::new();
    schedule.relationships.retain(|_, r| {
        let kept = (r.predecessor_project != pid || activities.contains_key(&r.predecessor))
            && (r.successor_project != pid || activities.contains_key(&r.successor));
        if !kept {
            dropped.push(r.source);
        }
        kept
    });
    for source in dropped.drain(..) {
        report.warn(
            DiagnosticKind::OrphanRow,
            row(source),
            "task_id",
            "relationship end activity is not in the model; relationship omitted",
        );
    }
    let resources = &schedule.resources;
    let roles = &schedule.roles;
    schedule.assignments.retain(|_, a| {
        let kept = activities.contains_key(&a.activity)
            && a.resource.is_none_or(|r| resources.contains_key(&r))
            && (a.resource.is_some() || a.role.is_some_and(|r| roles.contains_key(&r)));
        if !kept {
            dropped.push(a.source);
        }
        kept
    });
    for source in dropped.drain(..) {
        report.warn(
            DiagnosticKind::OrphanRow,
            row(source),
            "task_id",
            "assignment activity, resource or role is not in the model; assignment omitted",
        );
    }

    let calendars = &schedule.calendars;
    let mut cleared = Vec::new();
    let project = &mut schedule.project;
    if project
        .default_calendar
        .is_some_and(|c| !calendars.contains_key(&c))
    {
        project.default_calendar = None;
        cleared.push((
            project.source,
            "clndr_id",
            "default calendar is not in the model; cleared",
        ));
    }
    if project
        .options
        .float_paths
        .end_activity
        .is_some_and(|a| !activities.contains_key(&a))
    {
        project.options.float_paths.end_activity = None;
        cleared.push((
            project.source,
            "key_activity_for_multiple_longest_paths",
            "float path end activity is not in the model; cleared",
        ));
    }
    let resource_ids: std::collections::BTreeSet<m::ResourceId> =
        schedule.resources.keys().copied().collect();
    for r in schedule.resources.values_mut() {
        if r.calendar.is_some_and(|c| !calendars.contains_key(&c)) {
            r.calendar = None;
            cleared.push((
                r.source,
                "clndr_id",
                "resource calendar is not in the model; cleared",
            ));
        }
        if r.parent.is_some_and(|p| !resource_ids.contains(&p)) {
            r.parent = None;
            cleared.push((
                r.source,
                "parent_rsrc_id",
                "parent resource is not in the model; cleared",
            ));
        }
    }
    omit_orphan_expenses(schedule, report);
    clear_missing_links(schedule, &mut cleared);
    for (source, field, message) in cleared {
        report.warn(
            DiagnosticKind::UnrepresentedValue,
            row(source),
            field,
            message,
        );
    }

    for finding in schedule.validate() {
        report.warn(DiagnosticKind::Integrity, None, "", finding.to_string());
    }
}

fn observations(
    raw: &RawXer,
    typed: &TypedXer<'_>,
    calendars: &calendar_data::CalendarSet<'_>,
    report: &mut Diagnostics,
) {
    for diagnostic in &raw.diagnostics {
        report.warn(
            DiagnosticKind::Raw,
            None,
            "",
            format!("raw reader diagnostic: {:?}", diagnostic.kind),
        );
    }
    for diagnostic in &typed.diagnostics {
        report.warn(
            DiagnosticKind::Typed,
            diagnostic.row.map(|row| RowRef {
                table: diagnostic.table_index,
                row,
            }),
            &diagnostic.field,
            &diagnostic.message,
        );
    }
    let omitted = typed.diagnostic_counts.values().sum::<usize>() - typed.diagnostics.len();
    *report.counts.entry(DiagnosticKind::Typed).or_default() += omitted;
    report.suppressed += omitted;
    for diagnostic in &calendars.diagnostics {
        report.warn(
            DiagnosticKind::Calendar,
            Some(diagnostic.source),
            "clndr_data",
            diagnostic.message,
        );
    }
    for table in raw.tables() {
        for (column, field) in table
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.ends_with("_hr_cnt"))
        {
            for token in table
                .rows
                .iter()
                .filter_map(|row| row.get(column))
                .filter(|s| !s.is_empty())
            {
                if let Ok(value) = token.parse::<Decimal>() {
                    if !value.is_whole_minutes() {
                        *report
                            .non_whole_minute_hour_fields
                            .entry(format!("{}.{}", table.name, field))
                            .or_default() += 1;
                    } else if value.checked_minutes().is_none() {
                        *report
                            .out_of_range_hour_fields
                            .entry(format!("{}.{}", table.name, field))
                            .or_default() += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
