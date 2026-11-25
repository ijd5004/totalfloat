//! Canonical schedule model for TotalFloat.
//!
//! Projects, activities, relationships, calendars, IDs, enums and schedule
//! options, modeled on the concepts XER files hold (ARCHITECTURE.md, Data model). Times are civil
//! datetimes at minute resolution; durations, lags and float are integer
//! minutes.
//!
//! This crate does no I/O and depends on no other TotalFloat crate.
//!
//! Every field's documentation names its XER equivalent (see
//! docs/GLOSSARY.md). Results stored in a file are kept apart from scheduling
//! inputs, in [`StoredResults`] and [`ProjectStoredResults`].

#![forbid(unsafe_code)]

mod activity;
mod calendar;
mod codes;
mod enums;
mod expense;
mod ids;
mod notes;
mod project;
mod relationship;
mod resource;
mod schedule;
mod time;
mod wbs;

pub use activity::{Activity, Constraint, Step, StoredResults};
pub use calendar::{Calendar, HoursPerPeriod, WorkInterval, WorkWeek};
pub use codes::{ActivityCode, ActivityCodeType, ProjectCode, ProjectCodeType, UdfType, UdfValue};
pub use enums::{
    ActivityStatus, ActivityType, CalendarType, CodeScope, ConstraintType, CriticalDefinition,
    DurationType, FloatPathMode, FloatType, LagCalendar, OutOfSequenceMode, PercentCompleteType,
    PriceSource, RateType, RelationshipType, ResourceType, UdfDataType, UdfSubject,
};
pub use expense::{Accrual, CostAccount, Expense, ExpenseCategory};
pub use ids::{
    ActivityCodeId, ActivityCodeTypeId, ActivityId, AssignmentId, CalendarId, CostAccountId,
    ExpenseCategoryId, ExpenseId, NotebookTopicId, ObsId, ProjectCodeId, ProjectCodeTypeId,
    ProjectId, RelationshipId, ResourceCurveId, ResourceId, ResourceRateId, ResourceRoleId, RoleId,
    RoleRateId, SourceRef, StepId, UdfCodeId, UdfTypeId, WbsId,
};
pub use notes::{Note, NotebookTopic};
pub use project::{FloatPathOptions, Project, ProjectStoredResults, ScheduleOptions};
pub use relationship::Relationship;
pub use resource::{
    Amounts, Assignment, CURVE_POINTS, Resource, ResourceCurve, ResourceRate, ResourceRole, Role,
    RoleRate, SpreadSegment, TypedSpread, rate_in_force,
};
pub use schedule::{Diagnostic, EntityRef, Schedule};
pub use time::{
    Date, DateTime, MINUTES_PER_DAY, MINUTES_PER_HOUR, Minutes, ParseTimeError, TimeOfDay, Weekday,
};
pub use wbs::{EarnedValue, EarnedValueTechnique, EstimateMethod, Obs, Wbs};
