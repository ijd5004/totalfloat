//! Deterministic CPM scheduling engine for TotalFloat (ARCHITECTURE.md, Deterministic engine).
//!
//! Forward and backward passes, float, constraints, progress and the data
//! date, critical and longest path, and provenance for every computed date.
//! Results are reported in the fields XER files store.
//!
//! Pure functions; no I/O and no dependency on codecs.
//!
//! # Recording provenance
//!
//! Provenance is part of the result, not a debug feature: `explain` renders it,
//! and a date nobody can account for isn't trustworthy. Engine code follows
//! these rules:
//!
//! - Every date set in [`ActivityResult::values`] gets an entry in
//!   [`ActivityResult::provenance`] under the same [`DateField`].
//! - Record what won, not what was considered. Where several relationships or
//!   constraints compete for a date, the one that set it is the provenance.
//!   Ties go to the lowest relationship ID, so output is deterministic.
//! - When a relationship drives a date, [`DateProvenance::Relationship`] names
//!   the date on the other activity that drove it. Set `from_date` from the
//!   date actually used (a finish-to-start link follows the predecessor's
//!   finish), because [`ScheduleResult::driving_chain`] walks those links.
//! - When a calendar moves a date to working time, wrap the original cause in
//!   [`DateProvenance::CalendarAdjustment`] rather than replacing it.
//! - A date that follows from another of the activity's own dates by its
//!   duration is [`DateProvenance::Duration`], so a chain carries on through
//!   the activity to whatever drove the other date.
//! - Results are stored in `BTreeMap`s, and `driving_relationships` is sorted
//!   by ID, to keep output deterministic.

#![forbid(unsafe_code)]

mod cpm;
mod explain;
mod graph;
mod provenance;
mod result;
mod spread;

pub use explain::{ExplainError, ExplainStep, Explanation, RENDERED_ENDS, explain};
pub use provenance::{DateField, DateProvenance};
pub use result::{ActivityResult, ChainLink, ProjectResult, ScheduleResult};
pub use spread::{
    Distribution, Period, PeriodUnits, SpreadError, assignment_calendar, remaining_distribution,
    spread,
};

use totalfloat_model::{RelationshipId, Schedule, ScheduleOptions};

/// Why a schedule couldn't be calculated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// Reserved for an unimplemented calculation service; retained for callers
    /// and test doubles. The CPM entry point reports unsupported inputs explicitly.
    #[error("the requested scheduling calculation is not implemented")]
    NotImplemented,
    /// An input needs a scheduling feature outside the implemented subset.
    #[error("unsupported scheduling input: {0}")]
    Unsupported(&'static str),
    /// Required input is absent or internally inconsistent.
    #[error("invalid scheduling input: {0}")]
    InvalidInput(&'static str),
    /// A directed cycle, including the first activity again at the end.
    #[error("dependency cycle: {0:?}")]
    Cycle(Vec<totalfloat_model::ActivityId>),
    /// Invalid work pattern or arithmetic beyond the supported civil range.
    #[error("calendar arithmetic: {0}")]
    Calendar(#[from] totalfloat_calendar::CalendarError),
}

/// The relationships in a project that its recorded progress has broken, in ID
/// order: work that started or finished before its logic allowed. Relationships
/// to other projects aren't included.
#[must_use]
pub fn out_of_sequence(schedule: &Schedule) -> Vec<RelationshipId> {
    cpm::broken_relationships(schedule, &[])
}

/// [`out_of_sequence`], with relationships to the other projects in
/// `projects` included: each end is found in whichever loaded project holds
/// it. A relationship whose other end isn't loaded is left out.
#[must_use]
pub fn out_of_sequence_with_projects(
    schedule: &Schedule,
    projects: &[Schedule],
) -> Vec<RelationshipId> {
    cpm::broken_relationships(schedule, projects)
}

/// Calculates one project.
///
/// Pure and deterministic: the same inputs always give the same result.
///
/// # Errors
///
/// Rejects Actual Dates mode, a relationship to a project that isn't loaded with no
/// external date to stand in for it, unsupported activity inputs, contradictory progress,
/// malformed constraints, missing anchors/calendars, invalid
/// durations/endpoints, dependency cycles and calendar range errors. See TF-014
/// and TF-016 to TF-027 for the implemented subset.
pub fn schedule(
    schedule: &Schedule,
    options: &ScheduleOptions,
) -> Result<ScheduleResult, EngineError> {
    cpm::calculate(schedule, options, &[], None)
}

/// Calculates one project with other projects loaded alongside it, all of
/// them open together: [`schedule_open_together`] with every project in
/// `projects` open.
///
/// # Errors
///
/// As [`schedule`], and also rejects a relationship whose other end isn't loaded.
pub fn schedule_with_projects(
    schedule: &Schedule,
    options: &ScheduleOptions,
    projects: &[Schedule],
) -> Result<ScheduleResult, EngineError> {
    let opened = cpm::stored_finish(schedule, projects);
    cpm::calculate(schedule, options, projects, opened)
}

/// Calculates one of several projects open together.
///
/// `projects` may include `schedule` itself. Their activities aren't recalculated:
/// a relationship to one reads the dates stored on it (TF-030). Scheduling several projects together repeats this
/// for each until their stored dates stop changing.
///
/// `opened_finish` is the latest finish of the other projects open with this
/// one: their [`ProjectResult::network_finish`]. When float is measured from
/// the opened projects' finish ([`ScheduleOptions::float_from_project_finish`]
/// off), late dates count back from the later of it and this project's own
/// finish, and that is the project's scheduled finish.
///
/// [`schedule_with_projects`] takes it as the latest early finish stored on the
/// other projects' activities.
///
/// # Errors
///
/// As [`schedule_with_projects`].
pub fn schedule_open_together(
    schedule: &Schedule,
    options: &ScheduleOptions,
    projects: &[Schedule],
    opened_finish: Option<totalfloat_model::DateTime>,
) -> Result<ScheduleResult, EngineError> {
    cpm::calculate(schedule, options, projects, opened_finish)
}

/// A project calculated, with what the calculation read and worked out, so
/// the next calculation of the same project can take from it whatever an edit
/// doesn't reach.
#[derive(Clone)]
pub struct Calculated {
    result: ScheduleResult,
    kept: Option<cpm::Kept>,
}

impl Calculated {
    /// The result, as [`schedule_open_together`] gives it.
    #[must_use]
    pub const fn result(&self) -> &ScheduleResult {
        &self.result
    }

    /// The result, without what was kept.
    #[must_use]
    pub fn into_result(self) -> ScheduleResult {
        self.result
    }

    /// Whether a later calculation can take anything from this one: nothing
    /// outside the project fed it.
    #[must_use]
    pub const fn reusable(&self) -> bool {
        self.kept.is_some()
    }

    /// How many activities' results were worked out rather than taken from
    /// the previous calculation: all of them when nothing was taken. `None`
    /// when nothing was kept of this calculation.
    #[must_use]
    pub fn worked_out(&self) -> Option<usize> {
        self.kept.as_ref().map(|kept| kept.worked_out)
    }
}

impl std::fmt::Debug for Calculated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Calculated")
            .field("result", &self.result)
            .field("reusable", &self.reusable())
            .finish_non_exhaustive()
    }
}

/// [`schedule_open_together`], taking from `previous`, a calculation of the
/// same project, whatever the changes since don't reach.
///
/// The result is the one [`schedule_open_together`] gives, whatever
/// `previous` is. An activity is worked out again in the forward pass when
/// its own inputs or links changed, or a predecessor's early dates did, and
/// in the backward pass when its own inputs, links or early dates changed,
/// or a successor's late dates did; everything else is taken from
/// `previous`, which worked it out from the same inputs. Nothing is taken
/// when the options, the project's dates or calendars, or `opened_finish`
/// changed, or when a relationship to another project feeds either
/// calculation; late dates and the results that read the project finish are
/// worked out again when the finish moves.
///
/// # Errors
///
/// As [`schedule_open_together`].
pub fn schedule_reusing(
    schedule: &Schedule,
    options: &ScheduleOptions,
    (projects, opened_finish): (&[Schedule], Option<totalfloat_model::DateTime>),
    previous: Option<&Calculated>,
) -> Result<Calculated, EngineError> {
    let previous = previous.and_then(|p| Some((p.kept.as_ref()?, &p.result)));
    let (result, kept) =
        cpm::calculate_reusing(schedule, options, (projects, opened_finish), previous, true)?;
    Ok(Calculated { result, kept })
}

/// [`schedule_with_projects`], taking from `previous`, a calculation of the
/// same project, whatever the changes since don't reach, as
/// [`schedule_reusing`] does.
///
/// # Errors
///
/// As [`schedule_with_projects`].
pub fn schedule_with_projects_reusing(
    schedule: &Schedule,
    options: &ScheduleOptions,
    projects: &[Schedule],
    previous: Option<&Calculated>,
) -> Result<Calculated, EngineError> {
    let opened = cpm::stored_finish(schedule, projects);
    schedule_reusing(schedule, options, (projects, opened), previous)
}

#[cfg(test)]
mod tests;
