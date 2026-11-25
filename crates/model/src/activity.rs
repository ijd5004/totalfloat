//! Activities, their constraints and the results stored for them.

use std::collections::BTreeMap;

use crate::{
    ActivityCodeId, ActivityCodeTypeId, ActivityId, ActivityStatus, ActivityType, CalendarId,
    ConstraintType, DateTime, DurationType, Minutes, Note, NotebookTopicId, PercentCompleteType,
    ProjectId, SourceRef, StepId, UdfTypeId, UdfValue, WbsId,
};

/// A unit of work. XER: a `TASK` row.
///
/// Fields are scheduling inputs. The dates and float stored in the file are kept
/// separately in [`StoredResults`] so they are never mistaken for inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    /// Internal ID. XER: `TASK.task_id`.
    pub id: ActivityId,
    /// Owning project. XER: `TASK.proj_id`.
    pub project: ProjectId,
    /// WBS node. XER: `TASK.wbs_id`.
    pub wbs: WbsId,
    /// Calendar. XER: `TASK.clndr_id`.
    pub calendar: CalendarId,
    /// The user-facing Activity ID. XER: `TASK.task_code`.
    pub code: String,
    /// Activity name. XER: `TASK.task_name`.
    pub name: String,
    /// XER: `TASK.task_type`.
    pub activity_type: ActivityType,
    /// XER: `TASK.duration_type`.
    pub duration_type: DurationType,
    /// XER: `TASK.complete_pct_type`.
    pub percent_complete_type: PercentCompleteType,
    /// XER: `TASK.status_code`.
    pub status: ActivityStatus,
    /// Original (planned) duration. XER: `TASK.target_drtn_hr_cnt` (decimal hours).
    pub original_duration: Minutes,
    /// Remaining duration as of the data date. XER: `TASK.remain_drtn_hr_cnt` (decimal hours).
    pub remaining_duration: Minutes,
    /// Physical percent complete, 0 to 100. XER: `TASK.phys_complete_pct`.
    pub physical_percent_complete: Option<f64>,
    /// Planned start (not a baseline). XER: `TASK.target_start_date`.
    pub planned_start: Option<DateTime>,
    /// Planned finish. XER: `TASK.target_end_date`.
    pub planned_finish: Option<DateTime>,
    /// Actual start. XER: `TASK.act_start_date`.
    pub actual_start: Option<DateTime>,
    /// Actual finish. XER: `TASK.act_end_date`.
    pub actual_finish: Option<DateTime>,
    /// Suspend date. XER: `TASK.suspend_date`.
    pub suspend: Option<DateTime>,
    /// Resume date. XER: `TASK.resume_date`.
    pub resume: Option<DateTime>,
    /// Expected finish. XER: `TASK.expect_end_date`.
    pub expected_finish: Option<DateTime>,
    /// Primary constraint. XER: `TASK.cstr_type`, `TASK.cstr_date`.
    pub primary_constraint: Option<Constraint>,
    /// Secondary constraint. XER: `TASK.cstr_type2`, `TASK.cstr_date2`.
    pub secondary_constraint: Option<Constraint>,
    /// External early start, from a relationship to another project. XER:
    /// `TASK.external_early_start_date`.
    pub external_early_start: Option<DateTime>,
    /// External late finish, from a relationship to another project. XER:
    /// `TASK.external_late_end_date`.
    pub external_late_finish: Option<DateTime>,
    /// Activity code assignments, at most one value per code type. XER: `TASKACTV`.
    pub codes: BTreeMap<ActivityCodeTypeId, ActivityCodeId>,
    /// User-defined field values. XER: `UDFVALUE` rows whose type applies to `TASK`.
    pub udfs: BTreeMap<UdfTypeId, UdfValue>,
    /// Notes, at most one per notebook topic. XER: `TASKMEMO`.
    pub notes: BTreeMap<NotebookTopicId, Note>,
    /// Steps the work is broken into. XER: `TASKPROC`.
    pub steps: BTreeMap<StepId, Step>,
    /// Results stored in the file at its last calculation.
    pub stored: StoredResults,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

impl Activity {
    /// A not-started, task-dependent activity with zero duration and every
    /// optional field empty.
    #[must_use]
    pub fn new(
        id: ActivityId,
        project: ProjectId,
        wbs: WbsId,
        calendar: CalendarId,
        code: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            id,
            project,
            wbs,
            calendar,
            code: code.into(),
            name: name.into(),
            activity_type: ActivityType::default(),
            duration_type: DurationType::default(),
            percent_complete_type: PercentCompleteType::default(),
            status: ActivityStatus::default(),
            original_duration: Minutes::ZERO,
            remaining_duration: Minutes::ZERO,
            physical_percent_complete: None,
            planned_start: None,
            planned_finish: None,
            actual_start: None,
            actual_finish: None,
            suspend: None,
            resume: None,
            expected_finish: None,
            primary_constraint: None,
            secondary_constraint: None,
            external_early_start: None,
            external_late_finish: None,
            codes: BTreeMap::new(),
            udfs: BTreeMap::new(),
            notes: BTreeMap::new(),
            steps: BTreeMap::new(),
            stored: StoredResults::default(),
            source: None,
        }
    }
}

impl Activity {
    /// Percent complete from the steps: each step's percent, or 100 if it's
    /// complete, weighted by its weight (TF-042). `None` without steps or
    /// with no weight to measure by.
    #[must_use]
    pub fn steps_percent(&self) -> Option<f64> {
        let (mut done, mut total) = (0.0, 0.0);
        for step in self.steps.values() {
            let weight = step.weight.unwrap_or(0.0);
            done += weight * step.percent_complete();
            total += weight;
        }
        (total > 0.0).then(|| (done / total).clamp(0.0, 100.0))
    }
}

/// One step of an activity's work. XER: a `TASKPROC` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// XER: `TASKPROC.proc_id`.
    pub id: StepId,
    /// XER: `TASKPROC.proc_name`.
    pub name: String,
    /// Order among the activity's steps. XER: `TASKPROC.seq_num`.
    pub sequence: i64,
    /// How much of the activity it is, against the other steps. XER:
    /// `TASKPROC.proc_wt`.
    pub weight: Option<f64>,
    /// Percent complete, 0 to 100. XER: `TASKPROC.complete_pct`.
    pub percent: Option<f64>,
    /// Marked complete. XER: `TASKPROC.complete_flag`.
    pub completed: bool,
    /// What it is, as P6 keeps it (HTML). XER: `TASKPROC.proc_descr`, in
    /// the same cell format as notes.
    pub description: String,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

impl Step {
    /// 100 if complete, else its percent.
    #[must_use]
    pub fn percent_complete(&self) -> f64 {
        if self.completed {
            100.0
        } else {
            self.percent.unwrap_or(0.0)
        }
    }
}

/// A date constraint on an activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Constraint {
    /// Constraint type.
    pub kind: ConstraintType,
    /// Constraint date; `None` for as late as possible.
    pub date: Option<DateTime>,
}

/// Per-activity results stored in the file, kept so the file writes back as it
/// was read. Empty for activities that have never been scheduled.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StoredResults {
    /// XER: `TASK.early_start_date`.
    pub early_start: Option<DateTime>,
    /// XER: `TASK.early_end_date`.
    pub early_finish: Option<DateTime>,
    /// XER: `TASK.late_start_date`.
    pub late_start: Option<DateTime>,
    /// XER: `TASK.late_end_date`.
    pub late_finish: Option<DateTime>,
    /// Remaining early start. XER: `TASK.restart_date`.
    pub remaining_early_start: Option<DateTime>,
    /// Remaining early finish. XER: `TASK.reend_date`.
    pub remaining_early_finish: Option<DateTime>,
    /// Remaining late start. XER: `TASK.rem_late_start_date`.
    pub remaining_late_start: Option<DateTime>,
    /// Remaining late finish. XER: `TASK.rem_late_end_date`.
    pub remaining_late_finish: Option<DateTime>,
    /// Total float. XER: `TASK.total_float_hr_cnt` (decimal hours; empty for
    /// completed activities).
    pub total_float: Option<Minutes>,
    /// Free float. XER: `TASK.free_float_hr_cnt` (decimal hours).
    pub free_float: Option<Minutes>,
    /// On the longest path. XER: `TASK.driving_path_flag`.
    pub on_longest_path: Option<bool>,
    /// Float path number from multiple float path analysis. XER: `TASK.float_path`.
    pub float_path: Option<u32>,
    /// Order within the float path. XER: `TASK.float_path_order`.
    pub float_path_order: Option<u32>,
}
