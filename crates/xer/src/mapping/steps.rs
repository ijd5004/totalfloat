//! Activity steps.
use super::{DiagnosticKind, Diagnostics, entities::insert, number, optional_id, text};
use crate::typed::{TypedTable, TypedXer};
use std::collections::BTreeMap;
use totalfloat_model as m;

pub(super) fn apply(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    for table in &typed.tables {
        let TypedTable::Step { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let (Some(id), Some(project), Some(task)) = (
                optional_id(&row.proc_id).map(m::StepId),
                optional_id(&row.proj_id).map(m::ProjectId),
                optional_id(&row.task_id).map(m::ActivityId),
            ) else {
                report.warn(
                    DiagnosticKind::UnrepresentedValue,
                    Some(source),
                    "TASKPROC",
                    "step identity is missing",
                );
                continue;
            };
            let step = m::Step {
                id,
                name: text(&row.proc_name),
                sequence: row.seq_num.value().copied().unwrap_or(0),
                weight: number(&row.proc_wt),
                percent: number(&row.complete_pct),
                completed: row.complete_flag.value().copied().unwrap_or(false),
                description: super::notes::decode(&text(&row.proc_descr)),
                source: source.to_source_ref(),
            };
            match schedules
                .get_mut(&project)
                .and_then(|s| s.activities.get_mut(&task))
            {
                Some(activity) => insert(&mut activity.steps, id, step, source, report),
                None => report.warn(
                    DiagnosticKind::OrphanRow,
                    Some(source),
                    "task_id",
                    "step's activity is not in the model; step omitted",
                ),
            }
        }
    }
}
