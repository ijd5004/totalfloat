//! Activity codes and user-defined fields.

use crate::{
    ActivityCodeId, ActivityCodeTypeId, CodeScope, DateTime, ProjectCodeId, ProjectCodeTypeId,
    ProjectId, SourceRef, UdfCodeId, UdfDataType, UdfSubject, UdfTypeId,
};

/// An activity code dictionary. XER: an `ACTVTYPE` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCodeType {
    /// XER: `ACTVTYPE.actv_code_type_id`.
    pub id: ActivityCodeTypeId,
    /// XER: `ACTVTYPE.actv_code_type`.
    pub name: String,
    /// XER: `ACTVTYPE.actv_code_type_scope`.
    pub scope: CodeScope,
    /// Owning project for project-scoped codes. XER: `ACTVTYPE.proj_id`.
    pub project: Option<ProjectId>,
    /// Maximum code value length. XER: `ACTVTYPE.actv_short_len`.
    pub max_length: Option<u32>,
    /// Sort order. XER: `ACTVTYPE.seq_num`.
    pub sequence: i64,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// One value in an activity code dictionary. XER: an `ACTVCODE` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCode {
    /// XER: `ACTVCODE.actv_code_id`.
    pub id: ActivityCodeId,
    /// XER: `ACTVCODE.actv_code_type_id`.
    pub code_type: ActivityCodeTypeId,
    /// Parent value for hierarchical codes. XER: `ACTVCODE.parent_actv_code_id`.
    pub parent: Option<ActivityCodeId>,
    /// The code value. XER: `ACTVCODE.short_name`.
    pub value: String,
    /// XER: `ACTVCODE.actv_code_name`.
    pub description: String,
    /// Sort order. XER: `ACTVCODE.seq_num`.
    pub sequence: i64,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A project code dictionary. Project codes belong to the whole database, so
/// every project in a file holds every type. XER: a `PCATTYPE` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCodeType {
    /// XER: `PCATTYPE.proj_catg_type_id`.
    pub id: ProjectCodeTypeId,
    /// XER: `PCATTYPE.proj_catg_type`.
    pub name: String,
    /// Maximum code value length. XER: `PCATTYPE.proj_catg_short_len`.
    pub max_length: Option<u32>,
    /// Sort order. XER: `PCATTYPE.seq_num`.
    pub sequence: i64,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// One value in a project code dictionary. XER: a `PCATVAL` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCode {
    /// XER: `PCATVAL.proj_catg_id`.
    pub id: ProjectCodeId,
    /// XER: `PCATVAL.proj_catg_type_id`.
    pub code_type: ProjectCodeTypeId,
    /// Parent value for hierarchical codes. XER: `PCATVAL.parent_proj_catg_id`.
    pub parent: Option<ProjectCodeId>,
    /// The code value. XER: `PCATVAL.proj_catg_short_name`.
    pub value: String,
    /// XER: `PCATVAL.proj_catg_name`.
    pub description: String,
    /// Sort order. XER: `PCATVAL.seq_num`.
    pub sequence: i64,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A user-defined field definition. XER: a `UDFTYPE` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdfType {
    /// XER: `UDFTYPE.udf_type_id`.
    pub id: UdfTypeId,
    /// What kind of entity the field applies to. XER: `UDFTYPE.table_name`.
    pub subject: UdfSubject,
    /// Label shown to users. XER: `UDFTYPE.udf_type_label`.
    pub label: String,
    /// XER: `UDFTYPE.logical_data_type`.
    pub data_type: UdfDataType,
    /// Source row link (see [`SourceRef`]).
    pub source: Option<SourceRef>,
}

/// A user-defined field value. XER: one of `UDFVALUE.udf_text`, `udf_number`,
/// `udf_date` or `udf_code_id`, depending on the field's data type.
#[derive(Debug, Clone, PartialEq)]
pub enum UdfValue {
    /// Text. XER: `UDFVALUE.udf_text`.
    Text(String),
    /// A number or cost. XER: `UDFVALUE.udf_number`.
    Number(f64),
    /// A date. XER: `UDFVALUE.udf_date`.
    Date(DateTime),
    /// A code-list value. XER: `UDFVALUE.udf_code_id`.
    Code(UdfCodeId),
}
