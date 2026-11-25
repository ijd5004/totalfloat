//! Deterministic P6 23.12 XER export from a canonical model, without source bytes.
//!
//! Source provenance is regenerated. Unrepresentable values and model states
//! that cannot survive our mapper are refused, never silently dropped. XER Tools
//! evidence is in docs/validation/xertools-2.2b.md.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use totalfloat_model as m;

use crate::{
    RawXer,
    mapping::enums,
    patch,
    raw::{self, Encoding, LineEnding},
};

mod entities;
pub(crate) use entities::calendar_data;
mod schema;

/// The columns P6 23.12 writes for `table`, in order, if it's a table the
/// fresh writer knows.
/// A UDF data type's XER code, if it has an observed one.
pub(crate) fn udf_type_code(kind: &m::UdfDataType) -> Option<&'static str> {
    entities::udf_type(kind).ok()
}

/// The XER table a UDF subject names, if it's one P6 exports.
pub(crate) fn udf_table(subject: &m::UdfSubject) -> Option<&'static str> {
    entities::subject(subject).ok()
}

pub(crate) fn columns(table: &str) -> Option<&'static [&'static str]> {
    schema::TABLES
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, columns)| *columns)
}

/// Target header version, selected by exact header spelling across P6 exports.
pub const VERSION: &str = "23.12";

/// A seed that follows these bytes, for GUIDs that differ between files.
#[must_use]
pub fn seed_of(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Explicit export metadata; there is no ambient clock or random source.
#[derive(Debug, Clone, Copy)]
pub struct FreshOptions {
    /// Export date in the header; not a scheduling input or computed date.
    pub export_date: m::Date,
    /// Seed for deterministic generated GUIDs.
    pub seed: u64,
    /// Output encoding. Edited text must be representable exactly.
    pub encoding: Encoding,
}

impl FreshOptions {
    /// Default to Windows-1252 and seed 0, with an explicit date.
    #[must_use]
    pub const fn new(export_date: m::Date) -> Self {
        Self {
            export_date,
            seed: 0,
            encoding: Encoding::Windows1252,
        }
    }
}

/// Why a complete exact model export is unavailable.
#[derive(Debug, thiserror::Error)]
pub enum FreshError {
    /// The canonical graph has validation findings.
    #[error("the input model has {0} integrity findings")]
    InvalidModel(usize),
    /// An input is outside this exporter's exact supported domain.
    #[error("fresh export does not support {0}")]
    Unsupported(&'static str),
    /// A scalar or field cannot be represented in the target schema.
    #[error("cannot represent {table}.{field} exactly in P6 23.12")]
    Unrepresentable {
        /// Table name.
        table: &'static str,
        /// Column name.
        field: &'static str,
    },
    /// The model did not survive a read-back, so no output is returned.
    #[error(
        "the model cannot round-trip exactly through the target schema (source links excepted)"
    )]
    ModelMismatch,
    /// Raw writer rejected an invalid or unencodable value.
    #[error(transparent)]
    Edit(#[from] raw::EditError),
    /// Raw reader refused generated output.
    #[error(transparent)]
    Read(#[from] raw::ReadError),
    /// In-memory serialization failed.
    #[error(transparent)]
    Write(#[from] std::io::Error),
}

fn scalar(table: &'static str, field: &'static str) -> FreshError {
    FreshError::Unrepresentable { table, field }
}

type Cells = BTreeMap<&'static str, String>;

fn text<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

fn flag(value: bool) -> String {
    if value { "Y" } else { "N" }.to_owned()
}

fn hours(
    value: Option<m::Minutes>,
    table: &'static str,
    field: &'static str,
) -> Result<String, FreshError> {
    value.map_or_else(
        || Ok(String::new()),
        |value| patch::hours(value).ok_or_else(|| scalar(table, field)),
    )
}

fn fields<T>(
    table: &'static str,
    writers: &[patch::Field<T>],
    value: &T,
) -> Result<Cells, FreshError> {
    writers
        .iter()
        .map(|(name, write)| Ok((*name, write(value).ok_or_else(|| scalar(table, name))?)))
        .collect()
}

struct Writer {
    xer: RawXer,
    seed: u64,
}

impl Writer {
    fn row(&mut self, table: &'static str, id: u64, mut cells: Cells) -> Result<(), FreshError> {
        let index = self.xer.table_index(table).ok_or(FreshError::Unsupported(
            "a table absent from the selected schema",
        ))?;
        let columns = &self.xer.tables()[index].fields;
        if columns.iter().any(|name| name == "guid") {
            cells.insert("guid", guid(self.seed, table, id));
        }
        // P6's usual values where the model gave none, as the patch writer
        // gives a row it adds, so a resource isn't inactive and an activity
        // has a priority.
        for &(field, value) in crate::patch::new_row_defaults(table) {
            if columns.iter().any(|name| name == field) {
                let cell = cells.entry(field).or_default();
                if cell.is_empty() {
                    value.clone_into(cell);
                }
            }
        }
        for (name, value) in &cells {
            if !value.is_empty() && !columns.iter().any(|column| column == name) {
                return Err(scalar(table, name));
            }
        }
        let values: Vec<String> = columns
            .iter()
            .map(|name| {
                cells.get(name.as_str()).cloned().unwrap_or_else(|| {
                    match crate::registry::field(table, name).map(|f| f.kind) {
                        Some(crate::registry::FieldKind::Flag) => "N".to_owned(),
                        _ => String::new(),
                    }
                })
            })
            .collect();
        self.xer.push_row(index, &values)?;
        Ok(())
    }
}

pub(crate) fn guid(seed: u64, table: &str, id: u64) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut digest = Sha256::new();
    digest.update(b"totalfloat.fresh-guid/1\0");
    digest.update(seed.to_le_bytes());
    digest.update(table.as_bytes());
    digest.update([0]);
    digest.update(id.to_le_bytes());
    let digest = digest.finalize();
    // P6's observed representation is 16 bytes in unpadded standard base64.
    let mut result = String::with_capacity(22);
    for chunk in digest[..16].chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..=chunk.len() {
            let shift = 18 - index * 6;
            result.push(char::from(ALPHABET[((word >> shift) & 63) as usize]));
        }
    }
    result
}

/// The model as its fresh rows read back: a value it doesn't state, in a
/// column new rows default (`new_row_defaults`), reads back as the default,
/// as P6's own exports never leave those cells empty.
fn as_written(schedule: &mut m::Schedule) {
    *schedule = owned(schedule);
    let default = |table: &str, field: &str| {
        patch::new_row_defaults(table)
            .iter()
            .find(|(name, _)| *name == field)
            .map(|(_, value)| *value)
    };
    let number = |table: &str, field: &str| -> Option<f64> {
        default(table, field).and_then(|value| value.parse().ok())
    };
    for activity in schedule.activities.values_mut() {
        let percent = &mut activity.physical_percent_complete;
        *percent = percent.or_else(|| number("TASK", "phys_complete_pct"));
    }
    for assignment in schedule.assignments.values_mut() {
        let a = &mut assignment.amounts;
        for (value, field) in [
            (&mut a.budgeted_units, "target_qty"),
            (&mut a.actual_units, "act_reg_qty"),
            (&mut a.actual_overtime_units, "act_ot_qty"),
            (&mut a.remaining_units, "remain_qty"),
            (&mut a.budgeted_units_per_hour, "target_qty_per_hr"),
            (&mut a.remaining_units_per_hour, "remain_qty_per_hr"),
            (&mut a.budgeted_cost, "target_cost"),
            (&mut a.actual_cost, "act_reg_cost"),
            (&mut a.actual_overtime_cost, "act_ot_cost"),
            (&mut a.remaining_cost, "remain_cost"),
            (&mut a.price_per_unit, "cost_per_qty"),
        ] {
            *value = value.or_else(|| number("TASKRSRC", field));
        }
        a.price_source = a.price_source.or_else(|| {
            default("TASKRSRC", "cost_per_qty_source_type").and_then(enums::price_source)
        });
        a.rate_type = a
            .rate_type
            .or_else(|| default("TASKRSRC", "rate_type").and_then(enums::rate_type));
    }
}

/// The schedule as a fresh export holds it: every WBS node has a
/// responsible OBS node, so a model with no OBS gets one node owning every
/// WBS node, and a node with no manager takes the first top OBS node.
fn owned(schedule: &m::Schedule) -> m::Schedule {
    let mut owned = schedule.clone();
    if owned.obs.is_empty() {
        let node = m::Obs {
            id: m::ObsId(1),
            parent: None,
            name: "TotalFloat".to_owned(),
            description: "Synthetic export".to_owned(),
            sequence: 0,
            source: None,
        };
        owned.obs.insert(node.id, node);
    }
    let top = owned
        .obs
        .values()
        .find(|node| node.parent.is_none())
        .or_else(|| owned.obs.values().next())
        .map(|node| node.id);
    for wbs in owned.wbs.values_mut() {
        wbs.obs = wbs.obs.or(top);
    }
    owned
}

fn normalize(schedule: &mut m::Schedule) {
    schedule.project.source = None;
    macro_rules! clear { ($($field:ident),*) => { $( for value in schedule.$field.values_mut() { value.source = None; } )* }; }
    clear!(
        wbs,
        activities,
        relationships,
        calendars,
        resources,
        roles,
        assignments,
        resource_curves,
        cost_accounts,
        expense_categories,
        expenses,
        activity_code_types,
        activity_codes,
        udf_types,
        project_code_types,
        project_codes,
        obs,
        notebook_topics
    );
    for activity in schedule.activities.values_mut() {
        for note in activity.notes.values_mut() {
            note.source = None;
        }
        for step in activity.steps.values_mut() {
            step.source = None;
        }
    }
    for node in schedule.wbs.values_mut() {
        for note in node.notes.values_mut() {
            note.source = None;
        }
    }
    for resource in schedule.resources.values_mut() {
        for rate in &mut resource.rates {
            rate.source = None;
        }
        for filled in &mut resource.roles {
            filled.source = None;
        }
    }
    for role in schedule.roles.values_mut() {
        for rate in &mut role.rates {
            rate.source = None;
        }
    }
}

/// What a standalone fresh export can't hold, if the schedule has it.
fn unsupported(schedule: &m::Schedule) -> Option<&'static str> {
    if schedule
        .relationships
        .values()
        .any(|r| schedule.is_external(r))
    {
        return Some("external relationships in a standalone fresh export");
    }
    // A typed spread is marked by a curve ID, which would read back as the
    // curve if the schedule held one with that ID (TF-039).
    if schedule
        .assignments
        .values()
        .any(|a| a.typed_spread.is_some())
        && schedule
            .resource_curves
            .contains_key(&m::ResourceCurveId(crate::spread_data::MARKER_CURVE))
    {
        return Some("a hand-typed spread beside a curve with the ID that marks one");
    }
    if schedule.project.options.ignore_external_relationships {
        return Some("the unconfirmed external-dependency option");
    }
    if schedule
        .wbs
        .values()
        .filter(|node| node.is_project_node)
        .count()
        != 1
    {
        return Some("a project without exactly one project WBS root");
    }
    None
}

/// Create a fresh XER with P6 23.12 table/column order and exact scalar values.
///
/// Requires a valid standalone project with one project WBS root, consistent
/// project/root names and all referenced calendars/entities. IDs in the model
/// are retained. Currency/OBS/options records use their own table-local ID 1.
/// GUIDs are generated deterministically by seed, table and identity.
///
/// The generated file is read back and compared with the entire input model,
/// ignoring only source links. Unused dictionaries lost by the mapper, unknown
/// UDF encodings and other unrepresentable states therefore produce errors.
/// This function never computes schedule dates or floats.
///
/// # Errors
///
/// Returns [`FreshError`] on invalid graphs, external relationships, unconfirmed
/// external-dependency options, unsupported schemas, unencodable text, or a model read-back mismatch.
pub fn write(schedule: &m::Schedule, options: FreshOptions) -> Result<RawXer, FreshError> {
    let findings = schedule.validate();
    if !findings.is_empty() {
        return Err(FreshError::InvalidModel(findings.len()));
    }
    if let Some(what) = unsupported(schedule) {
        return Err(FreshError::Unsupported(what));
    }
    let schedule = &owned(schedule);
    let date = options.export_date.to_string();
    let mut xer = RawXer::new(
        &[
            VERSION,
            &date,
            "Project",
            "TotalFloat",
            "TotalFloat",
            "TotalFloat",
            "Project Management",
            "USD",
        ],
        options.encoding,
        LineEnding::CrLf,
    )?;
    // The reference export the layout follows has none of these tables; each is
    // written only when used, so other files stay as they were.
    let fills_roles = schedule.resources.values().any(|r| !r.roles.is_empty());
    let unused = |name: &str| match name {
        "RSRCROLE" => !fills_roles,
        "ACCOUNT" => schedule.cost_accounts.is_empty(),
        "COSTTYPE" => schedule.expense_categories.is_empty(),
        "PROJCOST" => schedule.expenses.is_empty(),
        "RSRCCURVDATA" => schedule.resource_curves.is_empty(),
        "PCATTYPE" => schedule.project_code_types.is_empty(),
        "PCATVAL" => schedule.project_codes.is_empty(),
        "PROJPCAT" => schedule.project.codes.is_empty(),
        "WBSMEMO" => schedule.wbs.values().all(|node| node.notes.is_empty()),
        "TASKPROC" => schedule.activities.values().all(|a| a.steps.is_empty()),
        _ => false,
    };
    for (name, columns) in schema::TABLES {
        if unused(name) {
            continue;
        }
        xer.push_table(
            name,
            columns.iter().map(|column| (*column).to_owned()).collect(),
        )?;
    }
    let mut writer = Writer {
        xer,
        seed: options.seed,
    };
    entities::defaults(&mut writer, schedule)?;
    entities::project(&mut writer, schedule)?;
    entities::dictionaries(&mut writer, schedule)?;
    entities::network(&mut writer, schedule)?;
    entities::udfs(&mut writer, schedule)?;
    let mut bytes = Vec::new();
    writer.xer.write(&mut bytes)?;
    let output = raw::read(bytes)?;
    // Read-back equality alone does not prove that every generated scalar
    // conforms to the registry. Refuse invalid tokens in known columns.
    let typed = crate::typed::TypedXer::decode(&output);
    if typed.diagnostic_counts.keys().any(|kind| {
        matches!(
            kind,
            crate::typed::DiagnosticKind::Value(_)
                | crate::typed::DiagnosticKind::DuplicateField
                | crate::typed::DiagnosticKind::ExtraCells
        )
    }) {
        return Err(FreshError::Unsupported(
            "an unknown enum or invalid scalar encoding",
        ));
    }
    let (mut mapped, diagnostics) = crate::to_model(&output);
    if !diagnostics.omitted.is_empty() || mapped.len() != 1 {
        return Err(FreshError::ModelMismatch);
    }
    let mut before = schedule.clone();
    normalize(&mut before);
    as_written(&mut before);
    normalize(&mut mapped[0]);
    if before != mapped[0] {
        return Err(FreshError::ModelMismatch);
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
