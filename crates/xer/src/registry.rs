//! Compiled XER field vocabulary, derived from observed exports (ARCHITECTURE.md, Lossless round trips).
//!
//! This describes storage kinds, not required columns or schema versions.
//! Unknown names return `None`; missing columns and unknown enum codes must
//! remain recoverable by the typed layer. See `docs/xer-format.md` for evidence
//! and limits. No corpus values are embedded here except format enum codes.

/// The storage representation of a known XER field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// A table's own integer identity.
    Id,
    /// An integer identity in the named XER table (possibly outside this file).
    ForeignId(&'static str),
    /// Decoded text, including names and unclassified legacy fields.
    Text,
    /// An integer, without floating-point conversion.
    Int,
    /// A decimal quantity; durations and floats are in hours at this layer.
    Decimal,
    /// A civil datetime in `yyyy-mm-dd hh:mm` form.
    DateTime,
    /// A `Y` / `N` flag; an empty cell is distinct from either value.
    Flag,
    /// Known format codes. Future codes are diagnostics, not fatal errors.
    Enum(&'static [&'static str]),
    /// A P6 GUID (often compressed, rather than a UUID with hyphens).
    Guid,
    /// Structured opaque content; preserved by the raw layer.
    Blob,
}

/// One known column. Its position is always determined from `%F`, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Exact, case-sensitive XER field name.
    pub name: &'static str,
    /// Storage kind; see notes for provisional interpretations.
    pub kind: FieldKind,
    /// Evidence or interpretation caveats; empty when no extra caveat applies.
    pub notes: &'static str,
}

/// A known table with columns observed across multiple export versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Table {
    /// Exact, case-sensitive XER table name.
    pub name: &'static str,
    /// Known fields in name order, independent of their order in a file.
    pub fields: &'static [Field],
    /// Version and scope notes.
    pub notes: &'static str,
}

impl Table {
    /// Looks up an exact column name; unknown columns are allowed.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&'static Field> {
        self.fields.iter().find(|field| field.name == name)
    }
}

/// All known tables, in name order. This is not a list of required tables.
#[must_use]
pub fn tables() -> &'static [Table] {
    data::TABLES
}

/// Looks up a case-sensitive table name, returning `None` for extensions.
#[must_use]
pub fn table(name: &str) -> Option<&'static Table> {
    tables().iter().find(|table| table.name == name)
}

/// Looks up a known table/column pair, returning `None` for extensions.
#[must_use]
pub fn field(table_name: &str, field_name: &str) -> Option<&'static Field> {
    table(table_name)?.field(field_name)
}

mod data;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn vocabulary_is_sorted_unique_and_extensible() {
        assert!(tables().windows(2).all(|pair| pair[0].name < pair[1].name));
        for table in tables() {
            assert!(
                table
                    .fields
                    .windows(2)
                    .all(|pair| pair[0].name < pair[1].name)
            );
            for field in table.fields {
                if let FieldKind::Enum(codes) = field.kind {
                    assert_ne!(codes.len(), 0);
                    assert_eq!(codes.len(), codes.iter().collect::<BTreeSet<_>>().len());
                    assert!(codes.iter().all(|code| !code.is_empty()));
                }
            }
        }
        assert!(table("FUTURE_TABLE").is_none());
        assert!(field("TASK", "future_field").is_none());
        assert!(field("task", "task_id").is_none());
        assert_eq!(
            field("TASKPRED", "pred_task_id").unwrap().kind,
            FieldKind::ForeignId("TASK")
        );
        assert_eq!(
            field("TASK", "cstr_date2").unwrap().kind,
            FieldKind::DateTime
        );
        assert_eq!(
            field("CALENDAR", "clndr_data").unwrap().kind,
            FieldKind::Blob
        );
    }
}
