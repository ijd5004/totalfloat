//! Enumerations, as XER files hold them (ARCHITECTURE.md, Data model).
//!
//! Each variant's documentation names the XER code it corresponds to. The
//! conversions themselves live in the `xer` crate.

/// What kind of activity this is. XER: `TASK.task_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ActivityType {
    /// Task dependent: scheduled on the activity's own calendar. `TT_Task`.
    #[default]
    TaskDependent,
    /// Resource dependent: scheduled on its resources' calendars. `TT_Rsrc`.
    ResourceDependent,
    /// Level of effort: dates span its predecessors and successors. `TT_LOE`.
    LevelOfEffort,
    /// Start milestone (zero duration). `TT_Mile`.
    StartMilestone,
    /// Finish milestone (zero duration). `TT_FinMile`.
    FinishMilestone,
    /// WBS summary: dates span the activities in its WBS node. `TT_WBS`.
    WbsSummary,
}

impl ActivityType {
    /// True for level of effort and WBS summary activities, whose dates and
    /// progress come from the activities they span.
    #[must_use]
    pub const fn is_span(self) -> bool {
        matches!(self, Self::LevelOfEffort | Self::WbsSummary)
    }

    /// True for start and finish milestones.
    #[must_use]
    pub const fn is_milestone(self) -> bool {
        matches!(self, Self::StartMilestone | Self::FinishMilestone)
    }
}

/// Which of duration, units and units per time stay fixed when one changes.
/// XER: `TASK.duration_type`, `PROJECT.def_duration_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DurationType {
    /// Fixed units per time. `DT_FixedRate`.
    FixedUnitsPerTime,
    /// Fixed duration and units per time. `DT_FixedDrtn`.
    FixedDurationAndUnitsPerTime,
    /// Fixed units. `DT_FixedQty`.
    FixedUnits,
    /// Fixed duration and units. `DT_FixedDUR2`. The default here because it
    /// is the project default in 53 of 60 public sample projects.
    #[default]
    FixedDurationAndUnits,
}

/// How an activity's percent complete is measured.
/// XER: `TASK.complete_pct_type`, `PROJECT.def_complete_pct_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum PercentCompleteType {
    /// Entered by the user. `CP_Phys`.
    Physical,
    /// From original and remaining duration. `CP_Drtn`.
    #[default]
    Duration,
    /// From actual and remaining units. `CP_Units`.
    Units,
}

/// Progress status. XER: `TASK.status_code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ActivityStatus {
    /// No actual start. `TK_NotStart`.
    #[default]
    NotStarted,
    /// Actual start, no actual finish. `TK_Active`.
    InProgress,
    /// Actual finish. `TK_Complete`.
    Completed,
}

/// A date constraint. XER: `TASK.cstr_type` (primary) and `TASK.cstr_type2`
/// (secondary), with dates in `cstr_date` / `cstr_date2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConstraintType {
    /// Start on. `CS_MSO`.
    StartOn,
    /// Start on or after. `CS_MSOA`.
    StartOnOrAfter,
    /// Start on or before. `CS_MSOB`.
    StartOnOrBefore,
    /// Finish on. `CS_MEO`.
    FinishOn,
    /// Finish on or after. `CS_MEOA`.
    FinishOnOrAfter,
    /// Finish on or before. `CS_MEOB`.
    FinishOnOrBefore,
    /// As late as possible (no date). `CS_ALAP`.
    AsLateAsPossible,
    /// Mandatory start: overrides logic. `CS_MANDSTART`.
    MandatoryStart,
    /// Mandatory finish: overrides logic. `CS_MANDFIN`.
    MandatoryFinish,
}

impl ConstraintType {
    /// True for every constraint type except as late as possible.
    #[must_use]
    pub const fn requires_date(self) -> bool {
        !matches!(self, Self::AsLateAsPossible)
    }

    /// True if an activity with this primary constraint can have a secondary.
    #[must_use]
    pub const fn allows_secondary(self) -> bool {
        !matches!(
            self,
            Self::StartOn | Self::FinishOn | Self::MandatoryStart | Self::MandatoryFinish
        )
    }

    /// True if this constraint can be a secondary one.
    #[must_use]
    pub const fn can_be_secondary(self) -> bool {
        !matches!(self, Self::MandatoryStart | Self::MandatoryFinish)
    }
}

/// Relationship type. XER: `TASKPRED.pred_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum RelationshipType {
    /// Finish to start. `PR_FS`.
    #[default]
    FinishToStart,
    /// Start to start. `PR_SS`.
    StartToStart,
    /// Finish to finish. `PR_FF`.
    FinishToFinish,
    /// Start to finish. `PR_SF`.
    StartToFinish,
}

/// Calendar scope. XER: `CALENDAR.clndr_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CalendarType {
    /// Global calendar shared across projects. `CA_Base`.
    #[default]
    Global,
    /// Belongs to one project. `CA_Project`.
    Project,
    /// Belongs to a resource. `CA_Rsrc`.
    Resource,
}

/// Which calendar measures relationship lag. XER:
/// `SCHEDOPTIONS.sched_calendar_on_relationship_lag`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum LagCalendar {
    /// The predecessor's calendar. `rcal_Predecessor`.
    #[default]
    Predecessor,
    /// The successor's calendar. `rcal_Successor`.
    Successor,
    /// A 24-hour, 7-day calendar. `rcal_24Hour`.
    TwentyFourHour,
    /// The project's default calendar. `rcal_ProjDefault`.
    ProjectDefault,
}

/// How total float is computed. XER: `SCHEDOPTIONS.sched_float_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum FloatType {
    /// Late start minus early start. `FT_SS`.
    StartFloat,
    /// Late finish minus early finish. `FT_FF`.
    #[default]
    FinishFloat,
    /// The smaller of start float and finish float. `FT_Min`.
    SmallestFloat,
}

/// Which activities are critical. XER: `PROJECT.critical_path_type` with the
/// threshold in `PROJECT.critical_drtn_hr_cnt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CriticalDefinition {
    /// Total float less than or equal to the threshold. `CT_TotFloat`.
    TotalFloatAtMost(crate::Minutes),
    /// On the longest path. `CT_DrivPath`.
    LongestPath,
}

impl Default for CriticalDefinition {
    fn default() -> Self {
        Self::TotalFloatAtMost(crate::Minutes::ZERO)
    }
}

/// How remaining work of out-of-sequence activities is scheduled. XER:
/// `SCHEDOPTIONS.sched_retained_logic` and `SCHEDOPTIONS.sched_progress_override`
/// (both `N` means actual dates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum OutOfSequenceMode {
    /// Remaining work waits for its predecessors.
    #[default]
    RetainedLogic,
    /// Remaining work ignores the logic that progress broke.
    ProgressOverride,
    /// Schedule from actual dates.
    ActualDates,
}

/// Which chains multiple float path analysis ranks by. XER:
/// `SCHEDOPTIONS.use_total_float_multiple_longest_paths` (`Y` = total float).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum FloatPathMode {
    /// Rank by total float.
    #[default]
    TotalFloat,
    /// Rank by free float.
    FreeFloat,
}

/// Where an assignment's price per unit comes from.
/// XER: `TASKRSRC.cost_per_qty_source_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PriceSource {
    /// The resource's rate. `ST_Rsrc`.
    Resource,
    /// The role's rate. `ST_Role`.
    Role,
    /// Entered on the assignment. `ST_Custom`.
    Custom,
}

/// Which of a resource's five prices an assignment uses. XER: `TASKRSRC.rate_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RateType {
    /// The first price. `COST_PER_QTY`.
    Price1,
    /// `COST_PER_QTY2`.
    Price2,
    /// `COST_PER_QTY3`.
    Price3,
    /// `COST_PER_QTY4`.
    Price4,
    /// `COST_PER_QTY5`.
    Price5,
}

/// Resource kind. XER: `RSRC.rsrc_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ResourceType {
    /// Labor. `RT_Labor`.
    #[default]
    Labor,
    /// Nonlabor (equipment). `RT_Equip`.
    Nonlabor,
    /// Material. `RT_Mat`.
    Material,
}

/// Where an activity code dictionary applies. XER: `ACTVTYPE.actv_code_type_scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CodeScope {
    /// Available to every project. `AS_Global`.
    #[default]
    Global,
    /// Available to projects under one EPS node. `AS_EPS` (not yet observed).
    Eps,
    /// Belongs to one project. `AS_Project`.
    Project,
}

/// What kind of entity a user-defined field applies to. XER: `UDFTYPE.table_name`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UdfSubject {
    /// Projects. `PROJECT`.
    Project,
    /// WBS nodes. `PROJWBS`.
    Wbs,
    /// Activities. `TASK`.
    Activity,
    /// Resources. `RSRC`.
    Resource,
    /// Resource assignments. `TASKRSRC`.
    Assignment,
    /// Any other table, by XER table name. Values ride along untouched.
    Other(String),
}

/// A user-defined field's data type. XER: `UDFTYPE.logical_data_type`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UdfDataType {
    /// Free text. `FT_TEXT`.
    Text,
    /// A start date. `FT_START_DATE`.
    StartDate,
    /// A finish date. `FT_END_DATE`.
    FinishDate,
    /// A cost. `FT_MONEY`.
    Cost,
    /// A decimal number. `FT_FLOAT_2_DECIMALS`.
    Number,
    /// An integer. `FT_INT`.
    Integer,
    /// An indicator. `FT_STATICTYPE` (meaning not yet confirmed).
    Indicator,
    /// A value from a code list. XER code not yet observed.
    Code,
    /// Any other XER code, kept verbatim.
    Other(String),
}
