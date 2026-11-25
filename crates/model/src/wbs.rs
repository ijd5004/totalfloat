//! Work breakdown structure.

use std::collections::BTreeMap;

use crate::{Note, NotebookTopicId, ObsId, ProjectId, SourceRef, UdfTypeId, UdfValue, WbsId};

/// How a node's activities earn value (`docs/rules/TF-041-earned-value.md`).
/// XER: `PROJWBS.ev_*`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EarnedValue {
    /// How earned value is measured. XER: `PROJWBS.ev_compute_type`.
    pub technique: Option<EarnedValueTechnique>,
    /// The percent a custom technique uses. XER: `PROJWBS.ev_user_pct`.
    pub user_percent: Option<f64>,
    /// How the estimate to complete is worked out. XER: `PROJWBS.ev_etc_compute_type`.
    pub estimate: Option<EstimateMethod>,
    /// The performance factor a user gives, applied to the budget left as the
    /// `user_factor` option of TF-041 says (divided by default). XER:
    /// `PROJWBS.ev_etc_user_value`.
    pub user_factor: Option<f64>,
}

/// How earned value is measured. Only the corpus's technique is named; others
/// are kept as their code until a P6 export shows what they hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EarnedValueTechnique {
    /// From each activity's percent complete. `EC_Cmp_pct`.
    ActivityPercentComplete,
    /// A code no corpus file holds.
    Other(String),
}

/// How the estimate to complete is worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstimateMethod {
    /// The remaining cost. `EE_Rem_hr`.
    RemainingCost,
    /// Budget left at a performance factor of 1. `EE_PF_1`.
    PerformanceFactorOne,
    /// Budget left at the cost performance index. `EE_PF_cpi`.
    PerformanceFactorCpi,
    /// Budget left at the performance factor the user gives. `EE_PF_user`.
    PerformanceFactorUser,
    /// A code no corpus file holds.
    Other(String),
}

/// A WBS node. XER: a `PROJWBS` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Wbs {
    /// XER: `PROJWBS.wbs_id`.
    pub id: WbsId,
    /// XER: `PROJWBS.proj_id`.
    pub project: ProjectId,
    /// Parent node; `None` at the top. XER: `PROJWBS.parent_wbs_id`. The
    /// project's root node points at an EPS node outside the file, which the
    /// codec maps to `None`.
    pub parent: Option<WbsId>,
    /// WBS code segment. XER: `PROJWBS.wbs_short_name`.
    pub code: String,
    /// XER: `PROJWBS.wbs_name`.
    pub name: String,
    /// Sort order among siblings. XER: `PROJWBS.seq_num`.
    pub sequence: i64,
    /// True for the node that represents the project itself. XER: `PROJWBS.proj_node_flag`.
    pub is_project_node: bool,
    /// How its activities earn value.
    pub earned_value: EarnedValue,
    /// The responsible manager: a node of the OBS. XER: `PROJWBS.obs_id`.
    pub obs: Option<ObsId>,
    /// Notes, at most one per notebook topic. XER: `WBSMEMO`.
    pub notes: BTreeMap<NotebookTopicId, Note>,
    /// User-defined field values. XER: `UDFVALUE` rows whose type applies to `PROJWBS`.
    pub udfs: BTreeMap<UdfTypeId, UdfValue>,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A node of the organizational breakdown structure: who is responsible for
/// work. The OBS belongs to the whole database, so every project in a file
/// holds every node. XER: an `OBS` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Obs {
    /// XER: `OBS.obs_id`.
    pub id: ObsId,
    /// Parent node; `None` at the top. XER: `OBS.parent_obs_id`.
    pub parent: Option<ObsId>,
    /// XER: `OBS.obs_name`.
    pub name: String,
    /// XER: `OBS.obs_descr` (stored as HTML by P6).
    pub description: String,
    /// Sort order among siblings. XER: `OBS.seq_num`.
    pub sequence: i64,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}
