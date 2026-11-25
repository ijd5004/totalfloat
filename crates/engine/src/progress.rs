//! Progress rules: remaining work, data-date anchors and validation (TF-020).
use super::{Activity, ActivityStatus, DateProvenance, DateTime, Dates, EngineError};
use crate::graph::Graph;
use totalfloat_model::{OutOfSequenceMode, RelationshipType, Schedule};

pub(super) fn validate(a: &Activity) -> Result<(), EngineError> {
    let (actual_start, actual_finish) = (a.actual_start.is_some(), a.actual_finish.is_some());
    let (suspend, resume) = (a.suspend.is_some(), a.resume.is_some());
    match a.status {
        ActivityStatus::NotStarted if actual_start || actual_finish || suspend || resume => Err(
            EngineError::InvalidInput("not-started activity with actual, suspend or resume dates"),
        ),
        ActivityStatus::InProgress if !actual_start || actual_finish => {
            Err(EngineError::InvalidInput(
                "in-progress activity needs an actual start and no actual finish",
            ))
        }
        ActivityStatus::InProgress if resume && !suspend => Err(EngineError::InvalidInput(
            "resume date without a suspend date",
        )),
        ActivityStatus::InProgress if suspend && !resume => Err(EngineError::Unsupported(
            "suspended activity without a resume date",
        )),
        ActivityStatus::Completed if !actual_finish => Err(EngineError::InvalidInput(
            "completed activity without an actual finish",
        )),
        _ => Ok(()),
    }
}

/// Whether a relationship's logic was broken by the recorded progress.
pub(super) fn out_of_sequence(kind: RelationshipType, pred: &Activity, succ: &Activity) -> bool {
    let started = |a: &Activity| a.status != ActivityStatus::NotStarted;
    let finished = |a: &Activity| a.status == ActivityStatus::Completed;
    match kind {
        RelationshipType::FinishToStart => started(succ) && !finished(pred),
        RelationshipType::StartToStart => started(succ) && !started(pred),
        RelationshipType::FinishToFinish => finished(succ) && !finished(pred),
        RelationshipType::StartToFinish => finished(succ) && !started(pred),
    }
}

/// Progress needs a data date. Actual Dates mode has no evidence behind it yet,
/// so it is refused rather than approximated (TF-020).
pub(super) fn check_network(
    input: &Schedule,
    graph: &Graph<'_>,
    mode: OutOfSequenceMode,
) -> Result<(), EngineError> {
    if graph
        .activities
        .iter()
        .all(|a| a.status == ActivityStatus::NotStarted)
    {
        return Ok(());
    }
    if input.project.data_date.is_none() {
        return Err(EngineError::InvalidInput("progress without a data date"));
    }
    if mode == OutOfSequenceMode::ActualDates {
        return Err(EngineError::Unsupported("actual dates scheduling"));
    }
    Ok(())
}

/// A completed activity has no remaining work, so it collapses to one point at
/// the data date, kept as it is, working time or not.
pub(super) fn completed_dates(data_date: DateTime) -> Dates {
    Dates {
        start: data_date,
        finish: data_date,
        start_cause: DateProvenance::DataDate,
        finish_cause: DateProvenance::DataDate,
    }
}

/// Where remaining work may restart: the data date, or a later resume date.
pub(super) fn remaining_anchor(a: &Activity, data_date: DateTime) -> (DateTime, DateProvenance) {
    match a.resume.filter(|resume| *resume > data_date) {
        Some(resume) => (resume, DateProvenance::Resume),
        None => (data_date, DateProvenance::DataDate),
    }
}
