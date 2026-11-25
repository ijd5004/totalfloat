//! Projects whose network lost rows in mapping.
//!
//! Strict conversion omits raw rows it can't represent (TF-012). A project that
//! lost a task or relationship runs on a different network from the file's, so
//! the results the file stores no longer describe what TotalFloat calculates.

use std::collections::BTreeMap;

use crate::{RawXer, mapping::Diagnostics};

/// Network rows one project lost in mapping. Counts only: rows hold content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LostRows {
    /// Omitted `TASK` rows.
    pub tasks: u64,
    /// Omitted `TASKPRED` rows.
    pub relationships: u64,
}

/// Lost network rows by internal project ID, in ID order.
///
/// A relationship belongs to both of its ends' projects, because each side's
/// network changes. Omitted rows in any other table leave the network whole.
#[must_use]
pub fn lost_network_rows(raw: &RawXer, diagnostics: &Diagnostics) -> BTreeMap<u64, LostRows> {
    let mut lost = BTreeMap::<u64, LostRows>::new();
    let task_projects = task_projects(raw);
    for row in &diagnostics.omitted {
        let table = &raw.tables()[row.table];
        // Older exports leave a relationship's project cells blank, and mapping
        // then goes by which project holds the task.
        let ends: &[(&str, &str)] = match table.name.as_str() {
            "TASK" => &[("proj_id", "task_id")],
            "TASKPRED" => &[("proj_id", "task_id"), ("pred_proj_id", "pred_task_id")],
            _ => continue,
        };
        let cell = |field: &str| table.rows[row.row].get(table.field_index(field)?);
        let mut projects: Vec<u64> = ends
            .iter()
            .filter_map(|(project, task)| {
                cell(project)
                    .and_then(|id| id.parse().ok())
                    .or_else(|| task_projects.get(cell(task)?).copied())
            })
            .collect();
        projects.dedup();
        for project in projects {
            let entry = lost.entry(project).or_default();
            match table.name.as_str() {
                "TASK" => entry.tasks += 1,
                _ => entry.relationships += 1,
            }
        }
    }
    lost
}

/// Which project each `TASK` row names, by its `task_id` cell.
fn task_projects(raw: &RawXer) -> BTreeMap<&str, u64> {
    let mut projects = BTreeMap::new();
    for table in raw.tables().iter().filter(|t| t.name == "TASK") {
        let (Some(task), Some(project)) =
            (table.field_index("task_id"), table.field_index("proj_id"))
        else {
            continue;
        };
        for row in &table.rows {
            if let (Some(task), Some(Ok(project))) =
                (row.get(task), row.get(project).map(str::parse))
            {
                projects.insert(task, project);
            }
        }
    }
    projects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{raw, to_model};

    fn fixture() -> RawXer {
        raw::read(include_bytes!("../../../fixtures/xer/model-mapping.xer").as_slice()).unwrap()
    }

    fn set(xer: &mut RawXer, table: &str, row: usize, field: &str, value: &str) {
        let table = xer.table_index(table).unwrap();
        let column = xer.tables()[table].field_index(field).unwrap();
        xer.set_cell(table, row, column, value).unwrap();
    }

    fn lost(xer: &RawXer) -> BTreeMap<u64, LostRows> {
        lost_network_rows(xer, &to_model(xer).1)
    }

    #[test]
    fn a_whole_file_lost_nothing() {
        assert_eq!(lost(&fixture()), BTreeMap::new());
    }

    #[test]
    fn a_relationship_inside_a_project_is_lost_by_that_project_only() {
        let mut xer = fixture();
        set(&mut xer, "TASKPRED", 0, "lag_hr_cnt", "bad");
        let relationship = LostRows {
            tasks: 0,
            relationships: 1,
        };
        assert_eq!(lost(&xer), BTreeMap::from([(1, relationship)]));
    }

    #[test]
    fn a_relationship_between_projects_is_lost_by_both() {
        let mut xer = fixture();
        set(&mut xer, "TASKPRED", 1, "lag_hr_cnt", "bad");
        let lost = lost(&xer);
        assert_eq!(lost.keys().copied().collect::<Vec<_>>(), [1, 2]);
        assert!(lost.values().all(|rows| rows.relationships == 1));
    }

    #[test]
    fn a_blank_project_cell_is_charged_through_task_ownership() {
        let mut xer = fixture();
        set(&mut xer, "TASKPRED", 1, "lag_hr_cnt", "bad");
        set(&mut xer, "TASKPRED", 1, "pred_proj_id", "");
        let lost = lost(&xer);
        assert_eq!(lost.keys().copied().collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn a_task_is_lost_with_the_relationships_that_touch_it() {
        let mut xer = fixture();
        set(&mut xer, "TASK", 0, "target_drtn_hr_cnt", "0.01");
        let lost = lost(&xer);
        assert_eq!(lost[&1].tasks, 1);
        assert_eq!(lost[&1].relationships, 2);
        assert_eq!(lost[&2].tasks, 0);
        assert_eq!(lost[&2].relationships, 1);
    }

    #[test]
    fn an_omitted_row_outside_the_network_changes_nothing() {
        let mut xer = fixture();
        set(&mut xer, "TASKRSRC", 0, "rsrc_id", "not a number");
        let (_, diagnostics) = to_model(&xer);
        assert!(!diagnostics.omitted.is_empty());
        assert_eq!(lost_network_rows(&xer, &diagnostics), BTreeMap::new());
    }
}
