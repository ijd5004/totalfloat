//! A file and its mapped model, so a command or MCP handle maps it once.

use totalfloat_model::{ActivityId, ActivityType, Schedule};
use totalfloat_xer::{RawXer, lost_network_rows, mapping::Diagnostics, to_model};

use crate::error::CliError;

/// An XER file, read and mapped.
#[derive(Debug)]
pub struct Loaded {
    /// The file as read, which writing starts from.
    pub raw: RawXer,
    /// One schedule per project.
    pub schedules: Vec<Schedule>,
    /// What mapping found.
    pub diagnostics: Diagnostics,
}

impl Loaded {
    /// Maps a file that's been read.
    #[must_use]
    pub fn new(raw: RawXer) -> Self {
        let (schedules, diagnostics) = to_model(&raw);
        Self {
            raw,
            schedules,
            diagnostics,
        }
    }

    /// Refuses a project that isn't the network its stored results came from: it lost tasks or
    /// relationships in mapping, or an assignment that sets a calendar, or the
    /// file has network rows that can't be traced to any project.
    ///
    /// # Errors
    ///
    /// `incomplete-network`, saying what was lost.
    pub fn refuse_incomplete(&self, schedule: &Schedule) -> Result<(), CliError> {
        refuse_incomplete((&self.raw, &self.diagnostics), schedule)
    }
}

/// [`Loaded::refuse_incomplete`] for a file and what mapping it found, held
/// apart from its schedules.
///
/// # Errors
///
/// `incomplete-network`, saying what was lost.
pub fn refuse_incomplete(
    (raw, diagnostics): (&RawXer, &Diagnostics),
    schedule: &Schedule,
) -> Result<(), CliError> {
    let hint = "`inspect` reports rows left out of the model; other projects can be used alone";
    let refuse = |message: String| Err(CliError::new("incomplete-network", message, hint));
    let id = schedule.project.id.0;
    let unowned = unowned_network_rows(raw, diagnostics);
    if unowned > 0 {
        return refuse(format!(
            "{unowned} tasks or relationships belong to no known project"
        ));
    }
    if let Some(rows) = lost_network_rows(raw, diagnostics).get(&id) {
        return refuse(format!(
            "project {id} lost {} tasks and {} relationships in mapping, so it isn't the network its stored results came from",
            rows.tasks, rows.relationships
        ));
    }
    if lost_assignment((raw, diagnostics), schedule) {
        return refuse(format!(
            "project {id} lost an assignment on resource-dependent work"
        ));
    }
    Ok(())
}

/// An omitted assignment on resource-dependent work: its calendar comes from
/// that row, so the project's dates would be calculated on the wrong one.
fn lost_assignment((raw, diagnostics): (&RawXer, &Diagnostics), schedule: &Schedule) -> bool {
    diagnostics.omitted.iter().any(|row| {
        let table = &raw.tables()[row.table];
        let task = table.field_index("task_id");
        table.name == "TASKRSRC"
            && task
                .and_then(|column| table.rows[row.row].get(column))
                .and_then(|cell| cell.parse().ok())
                .and_then(|id| schedule.activities.get(&ActivityId(id)))
                .is_some_and(|a| a.activity_type == ActivityType::ResourceDependent)
    })
}

/// Network rows that can't be traced to any project, so any may be short.
fn unowned_network_rows(raw: &RawXer, diagnostics: &Diagnostics) -> usize {
    diagnostics
        .omitted
        .iter()
        .filter(|row| {
            let table = &raw.tables()[row.table];
            matches!(table.name.as_str(), "TASK" | "TASKPRED")
                && table
                    .field_index("proj_id")
                    .and_then(|column| table.rows[row.row].get(column))
                    .and_then(|cell| cell.parse::<u64>().ok())
                    .is_none()
        })
        .count()
}
