//! Entity identifiers.
//!
//! Each wraps the integer ID from the source file (ARCHITECTURE.md, Data model: entities keep their
//! source IDs so round-trips and P6 re-imports match up). New entities get IDs
//! above the file's maximum.

use std::fmt;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u64);

        impl $name {
            /// Wraps a source ID.
            #[must_use]
            pub const fn new(id: u64) -> Self {
                Self(id)
            }

            /// The source ID.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

id_type!(
    /// A project. XER: `PROJECT.proj_id`.
    ProjectId
);
id_type!(
    /// A WBS node. XER: `PROJWBS.wbs_id`.
    WbsId
);
id_type!(
    /// An activity's internal ID. XER: `TASK.task_id`. The user-facing
    /// Activity ID is [`Activity::code`](crate::Activity::code).
    ActivityId
);
id_type!(
    /// A relationship. XER: `TASKPRED.task_pred_id`.
    RelationshipId
);
id_type!(
    /// A calendar. XER: `CALENDAR.clndr_id`.
    CalendarId
);
id_type!(
    /// A resource. XER: `RSRC.rsrc_id`.
    ResourceId
);
id_type!(
    /// A resource's rate. XER: `RSRCRATE.rsrc_rate_id`.
    ResourceRateId
);
id_type!(
    /// A role. XER: `ROLES.role_id`.
    RoleId
);
id_type!(
    /// A role's rate. XER: `ROLERATE.role_rate_id`.
    RoleRateId
);
id_type!(
    /// A role a resource can fill. XER: `RSRCROLE.rsrc_role_id`.
    ResourceRoleId
);
id_type!(
    /// A cost account. XER: `ACCOUNT.acct_id`.
    CostAccountId
);
id_type!(
    /// A resource curve. XER: `RSRCCURVDATA.curv_id`.
    ResourceCurveId
);
id_type!(
    /// An expense category. XER: `COSTTYPE.cost_type_id`.
    ExpenseCategoryId
);
id_type!(
    /// An expense. XER: `PROJCOST.cost_item_id`.
    ExpenseId
);
id_type!(
    /// A resource assignment. XER: `TASKRSRC.taskrsrc_id`.
    AssignmentId
);
id_type!(
    /// An activity code type (a code dictionary). XER: `ACTVTYPE.actv_code_type_id`.
    ActivityCodeTypeId
);
id_type!(
    /// An activity code value. XER: `ACTVCODE.actv_code_id`.
    ActivityCodeId
);
id_type!(
    /// An activity step. XER: `TASKPROC.proc_id`.
    StepId
);
id_type!(
    /// A notebook topic. XER: `MEMOTYPE.memo_type_id`.
    NotebookTopicId
);
id_type!(
    /// A node of the organizational breakdown structure. XER: `OBS.obs_id`.
    ObsId
);
id_type!(
    /// A project code type (a project code dictionary). XER:
    /// `PCATTYPE.proj_catg_type_id`.
    ProjectCodeTypeId
);
id_type!(
    /// A project code value. XER: `PCATVAL.proj_catg_id`.
    ProjectCodeId
);
id_type!(
    /// A user-defined field definition. XER: `UDFTYPE.udf_type_id`.
    UdfTypeId
);
id_type!(
    /// A value in a code-type UDF's code list. XER: `UDFVALUE.udf_code_id`.
    UdfCodeId
);

/// Opaque link from a model entity back to the source row it came from.
///
/// Assigned by the codec that produced the entity and meaningful only to that
/// codec; the XER patch writer uses it to rewrite exactly that row (ARCHITECTURE.md, Lossless round trips).
/// `None` on an entity means it has no source row (it was created in the model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRef(pub u64);
