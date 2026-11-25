//! Layer 2: typed table rows with source references and recoverable diagnostics.
//!
//! Every registered table has a distinct row struct. Cells preserve their
//! lexical input, including malformed tokens. Unknown columns live in row
//! extras; all original bytes remain in the borrowed [`RawXer`]. No defaults,
//! scheduling, or lossy decimal conversions happen here.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::{
    RawXer,
    raw::{RawRow, RawTable},
    registry::{self, FieldKind},
};

mod decimal;
mod value;

pub use decimal::Decimal;
pub use value::{Cell, CellState, Value, ValueError};

/// Maximum retained diagnostic details; aggregate counts include suppressed ones.
pub const MAX_DIAGNOSTICS: usize = 1000;

/// Diagnostic importance. Recoverable input problems are warnings in this layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Original content remains usable and lossless.
    Warning,
    /// Reserved for consumers that identify an unusable required value.
    Error,
}

/// Stable diagnostic categories for aggregate reporting without cell values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticKind {
    /// Table has no compiled schema and remains raw.
    UnknownTable,
    /// Column has no compiled schema and remains in extras.
    UnknownField,
    /// A repeated field name cannot be bound uniquely; the first column wins.
    DuplicateField,
    /// Cell count exceeds the `%F` declaration; overflow remains available.
    ExtraCells,
    /// A scalar value could not be parsed as declared by the registry.
    Value(ValueError),
}

/// A nonfatal issue, with raw source coordinates and no cell contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Importance of the issue.
    pub severity: Severity,
    /// Aggregate category.
    pub kind: DiagnosticKind,
    /// XER table name.
    pub table: String,
    /// Zero-based index into [`RawXer::tables`].
    pub table_index: usize,
    /// Zero-based row index; `None` for a schema-level issue.
    pub row: Option<usize>,
    /// Field name; empty for a table/row-level issue.
    pub field: String,
    /// Description, without the original cell value.
    pub message: String,
}

/// Exact coordinates of a raw row, independent of its entity ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RowRef {
    /// Zero-based table index.
    pub table: usize,
    /// Zero-based row index.
    pub row: usize,
}

impl RowRef {
    /// Packs checked 32-bit table/row indices into the model's opaque source link.
    #[must_use]
    pub fn to_source_ref(self) -> Option<totalfloat_model::SourceRef> {
        let table = u32::try_from(self.table).ok()?;
        let row = u32::try_from(self.row).ok()?;
        Some(totalfloat_model::SourceRef(
            (u64::from(table) << 32) | u64::from(row),
        ))
    }

    /// Unpacks a source link produced by this codec.
    #[must_use]
    pub fn from_source_ref(source: totalfloat_model::SourceRef) -> Option<Self> {
        Some(Self {
            table: usize::try_from(source.0 >> 32).ok()?,
            row: usize::try_from(source.0 & u64::from(u32::MAX)).ok()?,
        })
    }
}

/// Provenance and unmodeled cells shared by each typed row struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowMeta<'a> {
    /// Link back to the original raw row (table and row indices).
    pub source: RowRef,
    /// Unknown columns and their cells, keyed by exact name (first occurrence).
    pub extras: BTreeMap<&'a str, &'a str>,
    /// Duplicate column occurrences after the first: (column index, original cell).
    pub duplicates: Vec<(usize, &'a str)>,
    /// Cells beyond the field declaration, kept in their original order.
    pub overflow: Vec<&'a str>,
}

/// Common inspection without erasing the concrete row's typed fields.
pub trait RowView {
    /// Source indices and unmodeled cells.
    fn meta(&self) -> &RowMeta<'_>;
    /// Visits registered fields, including empty/missing cells, in registry order.
    fn visit_fields(&self, visitor: &mut dyn FnMut(&'static str, Cow<'_, str>));
}

/// A borrowed typed view. The raw input is never modified by decoding.
#[derive(Debug)]
pub struct TypedXer<'a> {
    /// Entire raw file, including unrecognized tables and every original byte.
    pub raw: &'a RawXer,
    /// Recognized tables in file order, including repeated table instances.
    pub tables: Vec<TypedTable<'a>>,
    /// Indices of unrecognized tables, still available through `raw`.
    pub unknown_tables: Vec<usize>,
    /// First [`MAX_DIAGNOSTICS`] warning details: table/row order, then registry field order.
    pub diagnostics: Vec<Diagnostic>,
    /// Counts across all warnings, including suppressed details.
    pub diagnostic_counts: BTreeMap<DiagnosticKind, usize>,
    /// Count of warning details omitted due to the limit.
    pub suppressed_diagnostics: usize,
}

impl<'a> TypedXer<'a> {
    /// Decodes all recognized rows without failing on malformed values.
    #[must_use]
    pub fn decode(raw: &'a RawXer) -> Self {
        let mut result = Self {
            raw,
            tables: Vec::new(),
            unknown_tables: Vec::new(),
            diagnostics: Vec::new(),
            diagnostic_counts: BTreeMap::new(),
            suppressed_diagnostics: 0,
        };
        let mut sink = Sink::default();
        for (index, table) in raw.tables().iter().enumerate() {
            let Some(spec) = registry::table(&table.name) else {
                sink.push(
                    table,
                    index,
                    None,
                    "",
                    DiagnosticKind::UnknownTable,
                    "table remains in raw storage",
                );
                result.unknown_tables.push(index);
                continue;
            };
            let mut columns = BTreeMap::new();
            for (column, name) in table.fields.iter().enumerate() {
                if columns.contains_key(name.as_str()) {
                    sink.push(
                        table,
                        index,
                        None,
                        name,
                        DiagnosticKind::DuplicateField,
                        "duplicate column; first occurrence used for typed binding",
                    );
                } else {
                    columns.insert(
                        name.as_str(),
                        ColumnBinding {
                            index: column,
                            spec: spec.field(name),
                        },
                    );
                }
                if spec.field(name).is_none() {
                    sink.push(
                        table,
                        index,
                        None,
                        name,
                        DiagnosticKind::UnknownField,
                        "unknown column remains in extras",
                    );
                }
            }
            if let Some(decoded) = decode_table(table, index, &columns, &mut sink) {
                result.tables.push(decoded);
            } else {
                sink.push(
                    table,
                    index,
                    None,
                    "",
                    DiagnosticKind::UnknownTable,
                    "table has no typed row binding; retained raw",
                );
                result.unknown_tables.push(index);
            }
        }
        result.diagnostics = sink.details;
        result.diagnostic_counts = sink.counts;
        result.suppressed_diagnostics = sink.suppressed;
        result
    }

    /// First recognized table with this exact name. Repeats remain in `tables`.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&TypedTable<'a>> {
        self.tables.iter().find(|table| table.name() == name)
    }
}

#[derive(Default)]
struct Sink {
    details: Vec<Diagnostic>,
    counts: BTreeMap<DiagnosticKind, usize>,
    suppressed: usize,
}

impl Sink {
    fn push(
        &mut self,
        table: &RawTable,
        table_index: usize,
        row: Option<usize>,
        field: &str,
        kind: DiagnosticKind,
        message: &str,
    ) {
        *self.counts.entry(kind).or_default() += 1;
        if self.details.len() < MAX_DIAGNOSTICS {
            self.details.push(Diagnostic {
                severity: Severity::Warning,
                kind,
                table: table.name.clone(),
                table_index,
                row,
                field: field.to_owned(),
                message: message.to_owned(),
            });
        } else {
            self.suppressed += 1;
        }
    }
}

struct ColumnBinding {
    index: usize,
    spec: Option<&'static registry::Field>,
}

/// What a table's header says about all its rows, resolved once.
struct RowPlan {
    /// For each field of the row type, in order: its name and, if the table
    /// has it, its column and kind.
    fields: Vec<(&'static str, Option<(usize, FieldKind)>)>,
    /// Columns repeating an earlier name.
    duplicates: Vec<usize>,
    /// Columns the registry doesn't know.
    extras: Vec<usize>,
}

impl RowPlan {
    fn new(
        table: &RawTable,
        columns: &BTreeMap<&str, ColumnBinding>,
        fields: &[&'static str],
    ) -> Self {
        let (mut duplicates, mut extras) = (Vec::new(), Vec::new());
        for (index, name) in table.fields.iter().enumerate() {
            match columns.get(name.as_str()) {
                Some(binding) if binding.index == index => {
                    if binding.spec.is_none() {
                        extras.push(index);
                    }
                }
                _ => duplicates.push(index),
            }
        }
        let column = |name: &&'static str| {
            columns.get(name).map(|binding| {
                let kind = binding.spec.map_or(FieldKind::Text, |field| field.kind);
                (binding.index, kind)
            })
        };
        Self {
            fields: fields.iter().map(|name| (*name, column(name))).collect(),
            duplicates,
            extras,
        }
    }
}

struct RowDecoder<'a, 'b> {
    table: &'a RawTable,
    row: &'a RawRow,
    source: RowRef,
    plan: &'b RowPlan,
    /// The next field of [`RowPlan::fields`]; rows decode in field order.
    next: usize,
    sink: &'b mut Sink,
}

impl<'a> RowDecoder<'a, '_> {
    fn cell<T: Value<'a>>(&mut self, name: &'static str) -> Cell<'a, T> {
        let planned = self.plan.fields.get(self.next).copied();
        self.next += 1;
        debug_assert_eq!(planned.map(|(field, _)| field), Some(name));
        let Some((_, Some((index, kind)))) = planned else {
            return Cell::missing();
        };
        // TF-010: omitted trailing cells are empty, not invalid values.
        let cell = Cell::decode(kind, self.row.get(index).unwrap_or(""));
        if let CellState::Invalid(error) = cell.state() {
            self.sink.push(
                self.table,
                self.source.table,
                Some(self.source.row),
                name,
                DiagnosticKind::Value(*error),
                &error.to_string(),
            );
        }
        cell
    }

    fn meta(&mut self) -> RowMeta<'a> {
        let mut extras = BTreeMap::new();
        let mut duplicates = Vec::new();
        for &index in &self.plan.duplicates {
            duplicates.push((index, self.row.get(index).unwrap_or("")));
        }
        for &index in &self.plan.extras {
            let value = self.row.get(index).unwrap_or("");
            extras.insert(self.table.fields[index].as_str(), value);
        }
        let overflow: Vec<_> = self
            .row
            .cells
            .iter()
            .skip(self.table.fields.len())
            .collect();
        if !overflow.is_empty() {
            self.sink.push(
                self.table,
                self.source.table,
                Some(self.source.row),
                "",
                DiagnosticKind::ExtraCells,
                "extra cells remain in row overflow",
            );
        }
        RowMeta {
            source: self.source,
            extras,
            duplicates,
            overflow,
        }
    }
}

mod rows;
use rows::decode_table;
pub use rows::*;

#[cfg(test)]
mod tests;
