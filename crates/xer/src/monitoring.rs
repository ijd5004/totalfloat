//! The thresholds a project watches, as P6 stores them (`PROJTHRS`, each on a
//! `THRSPARM` parameter), read for reporting. Field names come from P6
//! exports. Codes such as a threshold's status are kept as written. These
//! tables are never written.

use crate::raw::{RawTable, RawXer};

/// A threshold a project watches.
#[derive(Debug, Clone, PartialEq)]
pub struct Threshold {
    /// `PROJTHRS.thresh_id`.
    pub id: u64,
    /// `PROJTHRS.proj_id`.
    pub project: Option<u64>,
    /// `THRSPARM.thresh_parm_name` of its parameter.
    pub name: String,
    /// `THRSPARM.thresh_field_name`: the field watched, such as
    /// `total_float_hr_cnt`.
    pub field: String,
    /// `THRSPARM.thresh_parm_type`, as written, such as `TH_Drtn`.
    pub kind: String,
    /// `PROJTHRS.status_code`, as written, such as `TS_Enable`.
    pub status: String,
    /// `PROJTHRS.priority_type`, as written.
    pub priority: String,
    /// `PROJTHRS.thresh_type`: `TM_Task` watches activities, `TM_WBS` WBS
    /// nodes.
    pub level: String,
    /// `PROJTHRS.wbs_id`: the node watched, with everything under it.
    pub wbs: Option<u64>,
    /// `PROJTHRS.lo_parm_value`: values below it are raised.
    pub low: Option<f64>,
    /// `PROJTHRS.hi_parm_value`: values above it are raised.
    pub high: Option<f64>,
    /// `PROJTHRS.window_start` and `window_end`, as written, when either is
    /// given: the dates the threshold watches.
    pub window: Option<(String, String)>,
}

/// A table's cell, trimmed; empty where the table has no such column.
fn field(table: &RawTable, row: usize, name: &str) -> String {
    table
        .field_index(name)
        .and_then(|column| table.rows.get(row)?.get(column))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// The thresholds a file's projects watch, by `thresh_id`.
#[must_use]
pub fn thresholds(raw: &RawXer) -> Vec<Threshold> {
    let tables = raw.tables();
    let mut parameters = std::collections::BTreeMap::new();
    for table in tables.iter().filter(|t| t.name == "THRSPARM") {
        for row in 0..table.rows.len() {
            if let Ok(id) = field(table, row, "thresh_parm_id").parse::<u64>() {
                parameters.insert(
                    id,
                    (
                        field(table, row, "thresh_parm_name"),
                        field(table, row, "thresh_field_name"),
                        field(table, row, "thresh_parm_type"),
                    ),
                );
            }
        }
    }
    let mut out = Vec::new();
    for table in tables.iter().filter(|t| t.name == "PROJTHRS") {
        for row in 0..table.rows.len() {
            let Ok(id) = field(table, row, "thresh_id").parse::<u64>() else {
                continue;
            };
            let parameter = field(table, row, "thresh_parm_id").parse::<u64>().ok();
            let (name, watched, kind) = parameter
                .and_then(|p| parameters.get(&p).cloned())
                .unwrap_or_default();
            out.push(Threshold {
                id,
                project: field(table, row, "proj_id").parse().ok(),
                name,
                field: watched,
                kind,
                status: field(table, row, "status_code"),
                priority: field(table, row, "priority_type"),
                level: field(table, row, "thresh_type"),
                wbs: field(table, row, "wbs_id").parse().ok(),
                low: field(table, row, "lo_parm_value").parse().ok(),
                high: field(table, row, "hi_parm_value").parse().ok(),
                window: {
                    let (from, to) = (
                        field(table, row, "window_start"),
                        field(table, row, "window_end"),
                    );
                    (!from.is_empty() || !to.is_empty()).then_some((from, to))
                },
            });
        }
    }
    out.sort_by_key(|t| t.id);
    out
}
