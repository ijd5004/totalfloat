//! Logic ties between activities.

use crate::{ActivityId, Minutes, ProjectId, RelationshipId, RelationshipType, SourceRef};

/// A dependency between two activities. XER: a `TASKPRED` row.
///
/// Either end may be in another project (an external relationship); compare
/// [`Relationship::predecessor_project`] and [`Relationship::successor_project`]
/// with the schedule's project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// XER: `TASKPRED.task_pred_id`.
    pub id: RelationshipId,
    /// XER: `TASKPRED.pred_task_id`.
    pub predecessor: ActivityId,
    /// XER: `TASKPRED.task_id`.
    pub successor: ActivityId,
    /// XER: `TASKPRED.pred_proj_id`.
    pub predecessor_project: ProjectId,
    /// XER: `TASKPRED.proj_id`.
    pub successor_project: ProjectId,
    /// XER: `TASKPRED.pred_type`.
    pub kind: RelationshipType,
    /// Lag (negative for a lead). XER: `TASKPRED.lag_hr_cnt` (decimal hours).
    pub lag: Minutes,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}
