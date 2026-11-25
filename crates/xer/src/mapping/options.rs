//! Option projection. Unknown/defaulted behavior stays explicit (TF-008).
use super::{DiagnosticKind, Diagnostics, enums, optional_id, parent_id, unsigned};
use crate::typed::{Cell, ProjectRow, RowRef, ScheduleOptionsRow, TypedTable, TypedXer};
use std::collections::{BTreeMap, BTreeSet};
use totalfloat_model as m;

pub(super) fn project(project: &mut m::Project, row: &ProjectRow<'_>, report: &mut Diagnostics) {
    match row.critical_path_type.value().copied() {
        Some("CT_DrivPath") => project.options.critical = m::CriticalDefinition::LongestPath,
        Some("CT_TotFloat") => {
            if let Some(threshold) = report.minutes(
                &row.critical_drtn_hr_cnt,
                row.meta.source,
                "critical_drtn_hr_cnt",
            ) {
                project.options.critical = m::CriticalDefinition::TotalFloatAtMost(threshold);
            } else {
                report.warn(
                    DiagnosticKind::DefaultedOption,
                    Some(row.meta.source),
                    "critical_drtn_hr_cnt",
                    "critical threshold uses documented model default",
                );
            }
        }
        _ => report.warn(
            DiagnosticKind::DefaultedOption,
            Some(row.meta.source),
            "critical_path_type",
            "critical definition uses documented model default",
        ),
    }
}

pub(super) fn apply(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    let mut seen = BTreeSet::new();
    for table in &typed.tables {
        if let TypedTable::ScheduleOptions { rows, .. } = table {
            for row in rows {
                let Some(id) = optional_id(&row.proj_id).map(m::ProjectId) else {
                    report.warn(
                        DiagnosticKind::OrphanRow,
                        Some(row.meta.source),
                        "proj_id",
                        "schedule options have no project identity",
                    );
                    continue;
                };
                let Some(schedule) = schedules.get_mut(&id) else {
                    report.warn(
                        DiagnosticKind::OrphanRow,
                        Some(row.meta.source),
                        "proj_id",
                        "schedule options refer to an absent project",
                    );
                    continue;
                };
                if !seen.insert(id) {
                    report.warn(
                        DiagnosticKind::DuplicateId,
                        Some(row.meta.source),
                        "proj_id",
                        "duplicate options; first row used",
                    );
                    continue;
                }
                options(&mut schedule.project.options, row, report);
            }
        }
    }
    for (id, schedule) in schedules {
        if !seen.contains(id) {
            report.warn(
                DiagnosticKind::DefaultedOption,
                schedule.project.source.and_then(RowRef::from_source_ref),
                "SCHEDOPTIONS",
                "no options row; documented model defaults used (TF-008)",
            );
        }
    }
}

fn flag(
    target: &mut bool,
    cell: &Cell<'_, bool>,
    source: RowRef,
    field: &str,
    report: &mut Diagnostics,
) {
    if let Some(value) = cell.value() {
        *target = *value;
    } else {
        report.warn(
            DiagnosticKind::DefaultedOption,
            Some(source),
            field,
            "missing or invalid option flag; documented model default retained",
        );
    }
}

fn options(
    options: &mut m::ScheduleOptions,
    row: &ScheduleOptionsRow<'_>,
    report: &mut Diagnostics,
) {
    let source = row.meta.source;
    match (
        row.sched_retained_logic.value(),
        row.sched_progress_override.value(),
    ) {
        (Some(true), Some(false)) => options.out_of_sequence = m::OutOfSequenceMode::RetainedLogic,
        (Some(false), Some(true)) => {
            options.out_of_sequence = m::OutOfSequenceMode::ProgressOverride;
        }
        (Some(false), Some(false)) => options.out_of_sequence = m::OutOfSequenceMode::ActualDates,
        _ => report.warn(
            DiagnosticKind::DefaultedOption,
            Some(source),
            "sched_retained_logic",
            "missing or contradictory progress flags; default retained",
        ),
    }
    if let Some(value) = row
        .sched_calendar_on_relationship_lag
        .value()
        .and_then(|code| enums::lag_calendar(code))
    {
        options.lag_calendar = value;
    } else {
        report.warn(
            DiagnosticKind::DefaultedOption,
            Some(source),
            "sched_calendar_on_relationship_lag",
            "unmapped lag-calendar code; default retained",
        );
    }
    if let Some(value) = row
        .sched_float_type
        .value()
        .and_then(|code| enums::float_type(code))
    {
        options.float_type = value;
    } else {
        report.warn(
            DiagnosticKind::DefaultedOption,
            Some(source),
            "sched_float_type",
            "unmapped float definition; default retained",
        );
    }
    flag(
        &mut options.make_open_ended_critical,
        &row.sched_open_critical_flag,
        source,
        "sched_open_critical_flag",
        report,
    );
    flag(
        &mut options.ss_lag_from_early_start,
        &row.sched_lag_early_start_flag,
        source,
        "sched_lag_early_start_flag",
        report,
    );
    flag(
        &mut options.use_expected_finish,
        &row.sched_use_expect_end_flag,
        source,
        "sched_use_expect_end_flag",
        report,
    );
    flag(
        &mut options.float_from_project_finish,
        &row.sched_use_project_end_date_for_float,
        source,
        "sched_use_project_end_date_for_float",
        report,
    );
    flag(
        &mut options.set_planned_to_forecast,
        &row.sched_setplantoforecast,
        source,
        "sched_setplantoforecast",
        report,
    );
    paths(options, row, report);
    report.warn(
        DiagnosticKind::UnconfirmedOption,
        Some(source),
        "sched_outer_depend_type",
        "external relationship option is not confirmed; model default retained (TF-008)",
    );
}

fn paths(options: &mut m::ScheduleOptions, row: &ScheduleOptionsRow<'_>, report: &mut Diagnostics) {
    let source = row.meta.source;
    flag(
        &mut options.float_paths.enabled,
        &row.enable_multiple_longest_path_calc,
        source,
        "enable_multiple_longest_path_calc",
        report,
    );
    if let Some(total) = row.use_total_float_multiple_longest_paths.value() {
        options.float_paths.mode = if *total {
            m::FloatPathMode::TotalFloat
        } else {
            m::FloatPathMode::FreeFloat
        };
    } else {
        report.warn(
            DiagnosticKind::DefaultedOption,
            Some(source),
            "use_total_float_multiple_longest_paths",
            "missing path ranking flag; default retained",
        );
    }
    match row.limit_multiple_longest_path_calc.value() {
        Some(false) => options.float_paths.max_paths = None,
        Some(true) => {
            if let Some(limit) = unsigned(&row.max_multiple_longest_path) {
                options.float_paths.max_paths = Some(limit);
            } else {
                report.warn(
                    DiagnosticKind::DefaultedOption,
                    Some(source),
                    "max_multiple_longest_path",
                    "invalid path limit; default retained",
                );
            }
        }
        None => report.warn(
            DiagnosticKind::DefaultedOption,
            Some(source),
            "limit_multiple_longest_path_calc",
            "missing path-limit flag; default retained",
        ),
    }
    options.float_paths.end_activity =
        parent_id(&row.key_activity_for_multiple_longest_paths).map(m::ActivityId);
    if row
        .key_activity_for_multiple_longest_paths
        .value()
        .is_some_and(|id| *id < 0)
    {
        report.warn(
            DiagnosticKind::UnrepresentedValue,
            Some(source),
            "key_activity_for_multiple_longest_paths",
            "negative path-end sentinel retained raw; no activity inferred",
        );
    }
}
