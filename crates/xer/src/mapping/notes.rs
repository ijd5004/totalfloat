//! Notebook topics, and notes on activities and WBS nodes.
use super::{DiagnosticKind, Diagnostics, entities::insert, optional_id, text};
use crate::typed::{TypedTable, TypedXer};
use std::collections::BTreeMap;
use totalfloat_model as m;

/// A note's HTML as an XER cell holds it: P6 doubles each quote and writes
/// each line break as two DEL characters.
#[must_use]
pub(crate) fn decode(cell: &str) -> String {
    cell.replace("\u{7f}\u{7f}", "\n").replace("\"\"", "\"")
}

/// A note's HTML as an XER cell: [`decode`] undone. Tabs, which would end
/// the cell, become spaces, and carriage returns go.
#[must_use]
pub(crate) fn encode(html: &str) -> String {
    html.replace('"', "\"\"")
        .replace('\r', "")
        .replace('\n', "\u{7f}\u{7f}")
        .replace('\t', " ")
}

pub(super) fn apply(
    typed: &TypedXer<'_>,
    schedules: &mut BTreeMap<m::ProjectId, m::Schedule>,
    report: &mut Diagnostics,
) {
    let topics = topics(typed, report);
    for schedule in schedules.values_mut() {
        schedule.notebook_topics.clone_from(&topics);
    }
    for table in &typed.tables {
        match table {
            TypedTable::ActivityNote { rows, .. } => {
                for row in rows {
                    let source = row.meta.source;
                    let (Some(id), Some(project), Some(task), Some(topic)) = (
                        optional_id(&row.memo_id),
                        optional_id(&row.proj_id).map(m::ProjectId),
                        optional_id(&row.task_id).map(m::ActivityId),
                        optional_id(&row.memo_type_id).map(m::NotebookTopicId),
                    ) else {
                        unidentified(report, source, "TASKMEMO");
                        continue;
                    };
                    let note = m::Note {
                        id,
                        html: decode(&text(&row.task_memo)),
                        source: source.to_source_ref(),
                    };
                    let Some(schedule) = schedules.get_mut(&project) else {
                        orphan(report, source, "proj_id");
                        continue;
                    };
                    if !schedule.notebook_topics.contains_key(&topic) {
                        orphan(report, source, "memo_type_id");
                    } else if let Some(activity) = schedule.activities.get_mut(&task) {
                        insert(&mut activity.notes, topic, note, source, report);
                    } else {
                        orphan(report, source, "task_id");
                    }
                }
            }
            TypedTable::WbsNote { rows, .. } => {
                for row in rows {
                    let source = row.meta.source;
                    let (Some(id), Some(project), Some(wbs), Some(topic)) = (
                        optional_id(&row.wbs_memo_id),
                        optional_id(&row.proj_id).map(m::ProjectId),
                        optional_id(&row.wbs_id).map(m::WbsId),
                        optional_id(&row.memo_type_id).map(m::NotebookTopicId),
                    ) else {
                        unidentified(report, source, "WBSMEMO");
                        continue;
                    };
                    let note = m::Note {
                        id,
                        html: decode(&text(&row.wbs_memo)),
                        source: source.to_source_ref(),
                    };
                    let Some(schedule) = schedules.get_mut(&project) else {
                        orphan(report, source, "proj_id");
                        continue;
                    };
                    if !schedule.notebook_topics.contains_key(&topic) {
                        orphan(report, source, "memo_type_id");
                    } else if let Some(node) = schedule.wbs.get_mut(&wbs) {
                        insert(&mut node.notes, topic, note, source, report);
                    } else {
                        orphan(report, source, "wbs_id");
                    }
                }
            }
            _ => {}
        }
    }
}

fn unidentified(report: &mut Diagnostics, source: crate::typed::RowRef, table: &str) {
    report.warn(
        DiagnosticKind::UnrepresentedValue,
        Some(source),
        table,
        "note identity is missing",
    );
}

fn orphan(report: &mut Diagnostics, source: crate::typed::RowRef, field: &str) {
    report.warn(
        DiagnosticKind::OrphanRow,
        Some(source),
        field,
        "note's project, activity, WBS node or topic is not in the model; note omitted",
    );
}

fn topics(
    typed: &TypedXer<'_>,
    report: &mut Diagnostics,
) -> BTreeMap<m::NotebookTopicId, m::NotebookTopic> {
    let mut topics = BTreeMap::new();
    for table in &typed.tables {
        let TypedTable::NotebookTopic { rows, .. } = table else {
            continue;
        };
        for row in rows {
            let source = row.meta.source;
            let Some(id) = report
                .id(&row.memo_type_id, source, "memo_type_id")
                .map(m::NotebookTopicId)
            else {
                continue;
            };
            let flag = |cell: &crate::typed::Cell<'_, bool>| cell.value().copied().unwrap_or(false);
            let topic = m::NotebookTopic {
                id,
                name: text(&row.memo_type),
                sequence: row.seq_num.value().copied().unwrap_or(0),
                for_eps: flag(&row.eps_flag),
                for_projects: flag(&row.proj_flag),
                for_wbs: flag(&row.wbs_flag),
                for_activities: flag(&row.task_flag),
                source: source.to_source_ref(),
            };
            insert(&mut topics, id, topic, source, report);
        }
    }
    topics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_decode_and_encode_back() {
        let cell = "<HTML><META content=\"\"text/html\"\">\u{7f}\u{7f}<P>a</P></HTML>";
        let html = decode(cell);
        assert_eq!(html, "<HTML><META content=\"text/html\">\n<P>a</P></HTML>");
        assert_eq!(encode(&html), cell);
    }
}
