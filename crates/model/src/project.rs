//! Projects and schedule options.

use std::collections::BTreeMap;

use crate::{
    ActivityId, CalendarId, CriticalDefinition, DateTime, FloatPathMode, FloatType, LagCalendar,
    OutOfSequenceMode, ProjectCodeId, ProjectCodeTypeId, ProjectId, SourceRef, UdfTypeId, UdfValue,
};

/// One project (schedule). XER: a `PROJECT` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// Internal ID. XER: `PROJECT.proj_id`.
    pub id: ProjectId,
    /// The user-facing Project ID. XER: `PROJECT.proj_short_name`.
    pub short_name: String,
    /// Project name. XER: `wbs_name` of the project's root WBS node
    /// (`PROJWBS.proj_node_flag = 'Y'`); `PROJECT` has no name column.
    pub name: Option<String>,
    /// Data date: remaining work is scheduled after it. XER:
    /// `PROJECT.last_recalc_date` (to be confirmed against the corpus).
    pub data_date: Option<DateTime>,
    /// Planned start. XER: `PROJECT.plan_start_date`.
    pub planned_start: Option<DateTime>,
    /// Must finish by: when set, late dates are computed from it. XER:
    /// `PROJECT.plan_end_date` (to be confirmed against the corpus).
    pub must_finish_by: Option<DateTime>,
    /// Default calendar for new activities. XER: `PROJECT.clndr_id`.
    pub default_calendar: Option<CalendarId>,
    /// Scheduling options.
    pub options: ScheduleOptions,
    /// Results stored in the file at its last calculation.
    pub stored: ProjectStoredResults,
    /// User-defined field values. XER: `UDFVALUE` rows whose type applies to `PROJECT`.
    pub udfs: BTreeMap<UdfTypeId, UdfValue>,
    /// Physical percent complete follows activities' steps. XER:
    /// `PROJECT.step_complete_flag`.
    pub percent_from_steps: bool,
    /// Project code assignments, at most one value per code type. XER: `PROJPCAT`.
    pub codes: BTreeMap<ProjectCodeTypeId, ProjectCodeId>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

impl Project {
    /// A project with the given IDs and every other field empty or default.
    #[must_use]
    pub fn new(id: ProjectId, short_name: impl Into<String>) -> Self {
        Self {
            id,
            short_name: short_name.into(),
            name: None,
            data_date: None,
            planned_start: None,
            must_finish_by: None,
            default_calendar: None,
            options: ScheduleOptions::default(),
            stored: ProjectStoredResults::default(),
            udfs: BTreeMap::new(),
            codes: BTreeMap::new(),
            percent_from_steps: false,
            source: None,
        }
    }
}

/// Project-level results stored in the file, kept so the file writes back as it
/// was read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectStoredResults {
    /// Scheduled finish. XER: `PROJECT.scd_end_date`.
    pub scheduled_finish: Option<DateTime>,
}

/// Project settings that change how a schedule is calculated (TF-008).
///
/// XER stores most of these in `SCHEDOPTIONS` (one row per project) and the
/// critical-activity definition in `PROJECT`.
///
/// # Defaults
///
/// Each default in [`ScheduleOptions::default()`] is the most common value in
/// the 51 `SCHEDOPTIONS` rows (50 for fields older versions lack) and 60
/// `PROJECT` rows of the public sample corpus, which show what real
/// projects use. See docs/rules/TF-008-xer-schedule-options.md.
///
/// | Field | Default | Corpus evidence | Confirmed by a public source? |
/// |---|---|---|---|
/// | `out_of_sequence` | retained logic | 48 of 51 retained logic, 3 progress override | no |
/// | `lag_calendar` | predecessor | 45 of 51 (successor 4, 24-hour 1, project default 1) | no |
/// | `float_type` | finish float | 51 of 51 `FT_FF` | no |
/// | `critical` | total float ≤ 0 | 59 of 60 total float (49 with 0h threshold); 1 longest path | no |
/// | `make_open_ended_critical` | false | 50 of 50 `N` | no |
/// | `ss_lag_from_early_start` | true | 49 of 50 `Y` | no |
/// | `use_expected_finish` | true | 50 of 50 `Y` | no |
/// | `ignore_external_relationships` | false | not yet mapped (see TF-008) | no |
/// | `float_from_project_finish` | true | 46 of 50 `Y` | no |
/// | `set_planned_to_forecast` | false | 50 of 50 `N` | no |
/// | `float_paths` | off, total float, 10 paths | 43 of 50 off; 43 of 50 total float; 44 of 50 ten paths | no |
#[allow(
    clippy::struct_excessive_bools,
    reason = "mirrors the file's independent schedule-option flags one to one"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleOptions {
    /// Scheduling of out-of-sequence progress. XER: `SCHEDOPTIONS.sched_retained_logic`
    /// and `SCHEDOPTIONS.sched_progress_override`.
    pub out_of_sequence: OutOfSequenceMode,
    /// Calendar for relationship lag. XER: `SCHEDOPTIONS.sched_calendar_on_relationship_lag`.
    pub lag_calendar: LagCalendar,
    /// How total float is computed. XER: `SCHEDOPTIONS.sched_float_type`.
    pub float_type: FloatType,
    /// Which activities are critical. XER: `PROJECT.critical_path_type` and
    /// `PROJECT.critical_drtn_hr_cnt`.
    pub critical: CriticalDefinition,
    /// Make open-ended activities critical. XER: `SCHEDOPTIONS.sched_open_critical_flag`.
    pub make_open_ended_critical: bool,
    /// Measure start-to-start lag from the predecessor's early start (`true`)
    /// rather than its actual start. XER: `SCHEDOPTIONS.sched_lag_early_start_flag`.
    pub ss_lag_from_early_start: bool,
    /// Honor activities' expected finish dates. XER: `SCHEDOPTIONS.sched_use_expect_end_flag`.
    pub use_expected_finish: bool,
    /// Ignore relationships to and from other projects. XER: not yet mapped;
    /// `SCHEDOPTIONS.sched_outer_depend_type` is the candidate (only `SD_Both`
    /// observed so far).
    pub ignore_external_relationships: bool,
    /// Compute float against each project's own finish rather than the latest
    /// finish of the projects open together (TF-031). XER:
    /// `SCHEDOPTIONS.sched_use_project_end_date_for_float`.
    pub float_from_project_finish: bool,
    /// XER: `SCHEDOPTIONS.sched_setplantoforecast` (meaning to be confirmed).
    pub set_planned_to_forecast: bool,
    /// Multiple float path analysis.
    pub float_paths: FloatPathOptions,
}

impl Default for ScheduleOptions {
    /// See the defaults table on [`ScheduleOptions`].
    fn default() -> Self {
        Self {
            out_of_sequence: OutOfSequenceMode::RetainedLogic,
            lag_calendar: LagCalendar::Predecessor,
            float_type: FloatType::FinishFloat,
            critical: CriticalDefinition::default(),
            make_open_ended_critical: false,
            ss_lag_from_early_start: true,
            use_expected_finish: true,
            ignore_external_relationships: false,
            float_from_project_finish: true,
            set_planned_to_forecast: false,
            float_paths: FloatPathOptions::default(),
        }
    }
}

/// Multiple float path settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatPathOptions {
    /// Calculate multiple float paths. XER: `SCHEDOPTIONS.enable_multiple_longest_path_calc`.
    pub enabled: bool,
    /// Rank by total or free float. XER: `SCHEDOPTIONS.use_total_float_multiple_longest_paths`.
    pub mode: FloatPathMode,
    /// Maximum number of paths. XER: `SCHEDOPTIONS.max_multiple_longest_path`
    /// (applies when `SCHEDOPTIONS.limit_multiple_longest_path_calc = 'Y'`).
    pub max_paths: Option<u32>,
    /// The activity the paths end at; `None` means the project's end.
    /// XER: `SCHEDOPTIONS.key_activity_for_multiple_longest_paths`.
    pub end_activity: Option<ActivityId>,
}

impl Default for FloatPathOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: FloatPathMode::TotalFloat,
            max_paths: Some(10),
            end_activity: None,
        }
    }
}
