//! Project-owned entities, plus relationships with either end in the project.
use super::{
    DiagnosticKind, Diagnostics, Unreadable, date, enums, number, optional_id, parent_id, text,
    unsigned,
};
use crate::typed::{Cell, CellState, RowRef, TaskPredecessorRow, TaskRow, TypedTable, TypedXer};
use std::collections::BTreeMap;
use totalfloat_model as m;

pub(super) fn apply(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    for table in &typed.tables {
        match table {
            TypedTable::Wbs { rows, .. } => {
                for row in rows {
                    let source = row.meta.source;
                    let Some(project) =
                        report.id(&row.proj_id, source, "proj_id").map(m::ProjectId)
                    else {
                        continue;
                    };
                    let Some(schedule) = schedules.get_mut(&project) else {
                        report.warn(
                            DiagnosticKind::OrphanRow,
                            Some(source),
                            "proj_id",
                            "WBS owner is absent",
                        );
                        continue;
                    };
                    let Some(id) = report.id(&row.wbs_id, source, "wbs_id").map(m::WbsId) else {
                        continue;
                    };
                    let is_project_node = row.proj_node_flag.value().copied().unwrap_or(false);
                    let node = m::Wbs {
                        id,
                        project,
                        parent: if is_project_node {
                            None
                        } else {
                            parent_id(&row.parent_wbs_id).map(m::WbsId)
                        },
                        code: text(&row.wbs_short_name),
                        name: text(&row.wbs_name),
                        sequence: row.seq_num.value().copied().unwrap_or(0),
                        is_project_node,
                        earned_value: earned_value(row),
                        obs: parent_id(&row.obs_id).map(m::ObsId),
                        notes: std::collections::BTreeMap::new(),
                        udfs: BTreeMap::new(),
                        source: source.to_source_ref(),
                    };
                    if is_project_node && schedule.project.name.is_none() {
                        schedule.project.name = Some(node.name.clone());
                    }
                    insert(&mut schedule.wbs, id, node, source, report);
                }
            }
            TypedTable::Task { rows, .. } => {
                for row in rows {
                    let Some(activity) = activity(row, report) else {
                        continue;
                    };
                    if let Some(schedule) = schedules.get_mut(&activity.project) {
                        insert(
                            &mut schedule.activities,
                            activity.id,
                            activity,
                            row.meta.source,
                            report,
                        );
                    } else {
                        report.warn(
                            DiagnosticKind::OrphanRow,
                            Some(row.meta.source),
                            "proj_id",
                            "activity owner is absent",
                        );
                    }
                }
            }
            _ => {}
        }
    }
    relationships(typed, schedules, report);
}

fn relationships(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    // TASK ownership is evidence when an older relationship omits pred_proj_id.
    let owners = raw_owners(typed);
    for table in &typed.tables {
        if let TypedTable::TaskPredecessor { rows, .. } = table {
            for row in rows {
                let Some(relationship) = relationship(row, &owners, report) else {
                    continue;
                };
                let mut attached = false;
                for id in [
                    relationship.successor_project,
                    relationship.predecessor_project,
                ] {
                    if id == relationship.predecessor_project
                        && id == relationship.successor_project
                        && attached
                    {
                        continue;
                    }
                    if let Some(schedule) = schedules.get_mut(&id) {
                        insert(
                            &mut schedule.relationships,
                            relationship.id,
                            relationship.clone(),
                            row.meta.source,
                            report,
                        );
                        attached = true;
                    }
                }
                if !attached {
                    report.warn(
                        DiagnosticKind::OrphanRow,
                        Some(row.meta.source),
                        "proj_id",
                        "neither relationship end belongs to an imported project",
                    );
                }
            }
        }
    }
}

fn raw_owners(typed: &TypedXer<'_>) -> BTreeMap<m::ActivityId, m::ProjectId> {
    let mut result = BTreeMap::new();
    let mut ambiguous = std::collections::BTreeSet::new();
    for table in &typed.tables {
        if let TypedTable::Task { rows, .. } = table {
            for row in rows {
                if let Some((id, project)) = optional_id(&row.task_id)
                    .map(m::ActivityId)
                    .zip(optional_id(&row.proj_id).map(m::ProjectId))
                {
                    if result.get(&id).is_some_and(|old| *old != project) {
                        ambiguous.insert(id);
                    } else {
                        result.insert(id, project);
                    }
                }
            }
        }
    }
    for id in ambiguous {
        result.remove(&id);
    }
    result
}

pub(super) fn insert<K: Ord, V>(
    map: &mut BTreeMap<K, V>,
    id: K,
    value: V,
    source: crate::typed::RowRef,
    report: &mut Diagnostics,
) {
    if let std::collections::btree_map::Entry::Vacant(entry) = map.entry(id) {
        entry.insert(value);
    } else {
        report.warn(
            DiagnosticKind::DuplicateId,
            Some(source),
            "",
            "duplicate entity identity; first representable row used",
        );
    }
}

/// A constraint, or none if its type is empty.
fn constraint(
    report: &mut Diagnostics,
    source: RowRef,
    code: (&Cell<'_, &str>, &str),
    date_cell: (&Cell<'_, m::DateTime>, &str),
) -> Result<Option<m::Constraint>, Unreadable> {
    let date = report.input_date(date_cell.0, source, date_cell.1)?;
    if matches!(code.0.state(), CellState::Empty | CellState::Missing) {
        return Ok(None);
    }
    if let Some(kind) = code.0.value().and_then(|code| enums::constraint(code)) {
        return Ok(Some(m::Constraint { kind, date }));
    }
    report.warn(
        DiagnosticKind::SkippedRow,
        Some(source),
        code.1,
        "constraint type is not recognized",
    );
    Err(Unreadable)
}

fn activity(row: &TaskRow<'_>, report: &mut Diagnostics) -> Option<m::Activity> {
    let source = row.meta.source;
    let id = m::ActivityId(report.id(&row.task_id, source, "task_id")?);
    let project = m::ProjectId(report.id(&row.proj_id, source, "proj_id")?);
    let wbs = m::WbsId(report.id(&row.wbs_id, source, "wbs_id")?);
    let calendar = m::CalendarId(report.id(&row.clndr_id, source, "clndr_id")?);
    let mut activity = m::Activity::new(
        id,
        project,
        wbs,
        calendar,
        text(&row.task_code),
        text(&row.task_name),
    );
    activity.activity_type = report.code(&row.task_type, source, "task_type", enums::activity)?;
    activity.duration_type =
        report.code(&row.duration_type, source, "duration_type", enums::duration)?;
    activity.percent_complete_type = report.code(
        &row.complete_pct_type,
        source,
        "complete_pct_type",
        enums::percent,
    )?;
    activity.status = report.code(&row.status_code, source, "status_code", enums::status)?;
    activity.original_duration =
        report.required_minutes(&row.target_drtn_hr_cnt, source, "target_drtn_hr_cnt")?;
    activity.remaining_duration =
        report.required_minutes(&row.remain_drtn_hr_cnt, source, "remain_drtn_hr_cnt")?;
    activity.physical_percent_complete = number(&row.phys_complete_pct);
    activity.planned_start = report
        .input_date(&row.target_start_date, source, "target_start_date")
        .ok()?;
    activity.planned_finish = report
        .input_date(&row.target_end_date, source, "target_end_date")
        .ok()?;
    activity.actual_start = report
        .input_date(&row.act_start_date, source, "act_start_date")
        .ok()?;
    activity.actual_finish = report
        .input_date(&row.act_end_date, source, "act_end_date")
        .ok()?;
    activity.suspend = report
        .input_date(&row.suspend_date, source, "suspend_date")
        .ok()?;
    activity.resume = report
        .input_date(&row.resume_date, source, "resume_date")
        .ok()?;
    activity.expected_finish = report
        .input_date(&row.expect_end_date, source, "expect_end_date")
        .ok()?;
    activity.primary_constraint = constraint(
        report,
        source,
        (&row.cstr_type, "cstr_type"),
        (&row.cstr_date, "cstr_date"),
    )
    .ok()?;
    activity.secondary_constraint = constraint(
        report,
        source,
        (&row.cstr_type2, "cstr_type2"),
        (&row.cstr_date2, "cstr_date2"),
    )
    .ok()?;
    activity.external_early_start = report
        .input_date(
            &row.external_early_start_date,
            source,
            "external_early_start_date",
        )
        .ok()?;
    activity.external_late_finish = report
        .input_date(
            &row.external_late_end_date,
            source,
            "external_late_end_date",
        )
        .ok()?;
    activity.stored = m::StoredResults {
        early_start: date(&row.early_start_date),
        early_finish: date(&row.early_end_date),
        late_start: date(&row.late_start_date),
        late_finish: date(&row.late_end_date),
        remaining_early_start: date(&row.restart_date),
        remaining_early_finish: date(&row.reend_date),
        remaining_late_start: date(&row.rem_late_start_date),
        remaining_late_finish: date(&row.rem_late_end_date),
        total_float: report.minutes(&row.total_float_hr_cnt, source, "total_float_hr_cnt"),
        free_float: report.minutes(&row.free_float_hr_cnt, source, "free_float_hr_cnt"),
        on_longest_path: row.driving_path_flag.value().copied(),
        float_path: unsigned(&row.float_path),
        float_path_order: unsigned(&row.float_path_order),
    };
    activity.source = source.to_source_ref();
    Some(activity)
}

fn relationship(
    row: &TaskPredecessorRow<'_>,
    owners: &BTreeMap<m::ActivityId, m::ProjectId>,
    report: &mut Diagnostics,
) -> Option<m::Relationship> {
    let source = row.meta.source;
    let predecessor = m::ActivityId(report.id(&row.pred_task_id, source, "pred_task_id")?);
    let predecessor_project = if let Some(id) = optional_id(&row.pred_proj_id) {
        Some(m::ProjectId(id))
    } else if row.pred_proj_id.original().is_none_or(str::is_empty) {
        let owner = owners.get(&predecessor).copied();
        if owner.is_some() {
            report.warn(
                DiagnosticKind::InferredProject,
                Some(source),
                "pred_proj_id",
                "missing predecessor project derived from unambiguous raw TASK ownership",
            );
        }
        owner
    } else {
        None
    };
    let Some(predecessor_project) = predecessor_project else {
        report.warn(
            DiagnosticKind::SkippedRow,
            Some(source),
            "pred_proj_id",
            "predecessor project cannot be resolved from source identities",
        );
        return None;
    };
    Some(m::Relationship {
        id: m::RelationshipId(report.id(&row.task_pred_id, source, "task_pred_id")?),
        predecessor,
        successor: m::ActivityId(report.id(&row.task_id, source, "task_id")?),
        predecessor_project,
        successor_project: m::ProjectId(report.id(&row.proj_id, source, "proj_id")?),
        kind: report.code(&row.pred_type, source, "pred_type", enums::relationship)?,
        lag: if row.lag_hr_cnt.original() == Some("") {
            m::Minutes::ZERO
        } else {
            report.required_minutes(&row.lag_hr_cnt, source, "lag_hr_cnt")?
        },
        source: source.to_source_ref(),
    })
}

/// A cell's code, if it holds one.
fn present<'a>(cell: &Cell<'a, &'a str>) -> Option<&'a str> {
    cell.value().copied().filter(|c| !c.is_empty())
}

/// How a node earns value. A code the model doesn't name is kept as written.
fn earned_value(row: &crate::typed::WbsRow<'_>) -> m::EarnedValue {
    let code = present;
    m::EarnedValue {
        technique: code(&row.ev_compute_type).map(|c| match c {
            "EC_Cmp_pct" => m::EarnedValueTechnique::ActivityPercentComplete,
            other => m::EarnedValueTechnique::Other(other.to_owned()),
        }),
        user_percent: number(&row.ev_user_pct),
        estimate: code(&row.ev_etc_compute_type).map(|c| match c {
            "EE_Rem_hr" => m::EstimateMethod::RemainingCost,
            "EE_PF_1" => m::EstimateMethod::PerformanceFactorOne,
            "EE_PF_cpi" => m::EstimateMethod::PerformanceFactorCpi,
            "EE_PF_user" => m::EstimateMethod::PerformanceFactorUser,
            other => m::EstimateMethod::Other(other.to_owned()),
        }),
        user_factor: number(&row.ev_etc_user_value),
    }
}
