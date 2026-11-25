//! Why each computed date is what it is.

use totalfloat_model::{
    ActivityId, CalendarId, ConstraintType, DateTime, LagCalendar, Minutes, RelationshipId,
    RelationshipType,
};

/// A date the engine computes for an activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DateField {
    /// XER: `TASK.early_start_date`.
    EarlyStart,
    /// XER: `TASK.early_end_date`.
    EarlyFinish,
    /// XER: `TASK.late_start_date`.
    LateStart,
    /// XER: `TASK.late_end_date`.
    LateFinish,
    /// XER: `TASK.restart_date`.
    RemainingEarlyStart,
    /// XER: `TASK.reend_date`.
    RemainingEarlyFinish,
    /// XER: `TASK.rem_late_start_date`.
    RemainingLateStart,
    /// XER: `TASK.rem_late_end_date`.
    RemainingLateFinish,
    /// XER: `TASK.act_start_date`. An input, kept so a chain can end on it.
    ActualStart,
    /// XER: `TASK.act_end_date`. An input, kept so a chain can end on it.
    ActualFinish,
}

/// What controlled a computed date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateProvenance {
    /// A relationship drove this date. In the forward pass `from` is the
    /// predecessor; in the backward pass it is the successor.
    Relationship {
        /// The driving relationship.
        relationship: RelationshipId,
        /// The activity at the other end.
        from: ActivityId,
        /// The date on `from` that drove this one. Walking a chain follows it.
        from_date: DateField,
        /// Relationship type.
        kind: RelationshipType,
        /// Lag applied, in working minutes: the relationship's lag, or what was
        /// left of it if that side of the link had already happened.
        lag: Minutes,
        /// The calendar option the lag was measured on.
        lag_calendar: LagCalendar,
    },
    /// A constraint on the activity.
    Constraint {
        /// Constraint type.
        kind: ConstraintType,
        /// Constraint date.
        date: DateTime,
    },
    /// A date stored for a link to a project outside this schedule.
    ExternalDate {
        /// The external date.
        date: DateTime,
    },
    /// A selected expected finish date controls the activity's finish and
    /// effective remaining duration.
    ExpectedFinish,
    /// Free float was consumed by an as-late-as-possible constraint.
    AsLateAsPossible {
        /// Successor or project boundary that limited the move.
        cause: Box<DateProvenance>,
    },
    /// The project data date.
    DataDate,
    /// The activity's resume date, later than the data date.
    Resume,
    /// An actual date recorded on the activity.
    Actual,
    /// The project planned start.
    ProjectStart,
    /// The project must-finish-by date.
    MustFinishBy,
    /// The project's scheduled finish.
    ProjectFinish,
    /// The activity has no predecessor (forward) or successor (backward).
    OpenEnd,
    /// This date is another of the activity's own dates plus or minus its
    /// duration, such as a finish that follows from its start.
    Duration {
        /// The date it was measured from.
        from_date: DateField,
    },
    /// The span of a level-of-effort activity's neighbors.
    LoeSpan,
    /// The span of a WBS summary activity's members.
    WbsSummarySpan,
    /// `cause` gave a time the calendar doesn't work, so the date moved.
    CalendarAdjustment {
        /// The time before moving to working time.
        unadjusted: DateTime,
        /// The calendar that moved it.
        calendar: CalendarId,
        /// What gave the unadjusted time.
        cause: Box<DateProvenance>,
    },
}

impl DateProvenance {
    /// A stable name for this cause, for reports and JSON output.
    #[must_use]
    pub const fn origin_code(&self) -> &'static str {
        match self {
            Self::Relationship { .. } => "relationship",
            Self::Constraint { .. } => "constraint",
            Self::ExternalDate { .. } => "external-date",
            Self::ExpectedFinish => "expected-finish",
            Self::AsLateAsPossible { .. } => "as-late-as-possible",
            Self::DataDate => "data-date",
            Self::Resume => "resume",
            Self::Actual => "actual",
            Self::ProjectStart => "project-start",
            Self::MustFinishBy => "must-finish-by",
            Self::ProjectFinish => "project-finish",
            Self::OpenEnd => "open-end",
            Self::Duration { .. } => "duration",
            Self::LoeSpan => "level-of-effort-span",
            Self::WbsSummarySpan => "wbs-summary-span",
            Self::CalendarAdjustment { .. } => "calendar-adjustment",
        }
    }

    /// The underlying cause, looking through calendar and ALAP wrappers.
    #[must_use]
    pub fn driver(&self) -> &Self {
        match self {
            Self::CalendarAdjustment { cause, .. } | Self::AsLateAsPossible { cause } => {
                cause.driver()
            }
            other => other,
        }
    }
}
