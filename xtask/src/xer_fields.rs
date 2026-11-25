//! Aggregate field names and lexical value shapes without retaining values.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use totalfloat_xer::{RawXer, raw, registry};

#[derive(Debug, Default)]
struct FieldStats {
    occurrences: u64,
    missing_cells: u64,
    unknown_enum_codes: u64,
    shapes: BTreeMap<&'static str, u64>,
}

#[derive(Debug, Default)]
struct TableStats {
    occurrences: u64,
    rows: u64,
    extra_cells: u64,
    fields: BTreeMap<String, FieldStats>,
}

#[derive(Debug, Default)]
struct Inventory {
    files: u64,
    parsed: u64,
    rejected: BTreeMap<String, u64>,
    tables: BTreeMap<String, TableStats>,
}

impl Inventory {
    fn observe(&mut self, xer: &RawXer) {
        self.parsed += 1;
        for table in xer.tables() {
            let stats = self.tables.entry(table.name.clone()).or_default();
            stats.occurrences += 1;
            stats.rows += table.rows.len() as u64;
            for (index, field) in table.fields.iter().enumerate() {
                let field_stats = stats.fields.entry(field.clone()).or_default();
                field_stats.occurrences += 1;
                let kind = registry::field(&table.name, field).map(|f| f.kind);
                for row in &table.rows {
                    if let Some(value) = row.get(index) {
                        *field_stats.shapes.entry(shape(value)).or_default() += 1;
                        if let Some(registry::FieldKind::Enum(codes)) = kind
                            && !value.is_empty()
                            && !codes.contains(&value)
                        {
                            field_stats.unknown_enum_codes += 1;
                        }
                    } else {
                        field_stats.missing_cells += 1;
                    }
                }
            }
            stats.extra_cells += table
                .rows
                .iter()
                .map(|row| row.cells.len().saturating_sub(table.fields.len()) as u64)
                .sum::<u64>();
        }
    }

    fn report(&self, out: &mut impl fmt::Write) -> fmt::Result {
        let rejected: u64 = self.rejected.values().sum();
        writeln!(
            out,
            "files={} parsed={} rejected={rejected}",
            self.files, self.parsed
        )?;
        for (reason, count) in &self.rejected {
            writeln!(out, "rejection {reason:?} count={count}")?;
        }
        for (table_name, table) in &self.tables {
            let known = registry::table(table_name).is_some();
            writeln!(
                out,
                "table {table_name:?} known={known} occurrences={} rows={} extra_cells={}",
                table.occurrences, table.rows, table.extra_cells
            )?;
            for (name, field) in &table.fields {
                let kind =
                    registry::field(table_name, name).map_or("unknown", |f| kind_name(f.kind));
                writeln!(
                    out,
                    "  field {name:?} kind={kind} occurrences={} missing_cells={} unknown_enum_codes={} shapes={:?}",
                    field.occurrences, field.missing_cells, field.unknown_enum_codes, field.shapes
                )?;
            }
        }
        Ok(())
    }
}

/// Bounded shape vocabulary; values and value-derived examples never leave here.
fn shape(value: &str) -> &'static str {
    if value.is_empty() {
        "empty"
    } else if matches!(value, "Y" | "N") {
        "flag"
    } else if value.contains(['\n', '\r']) {
        "multiline"
    } else if datetime_shape(value) {
        "datetime"
    } else if integer_shape(value) {
        "integer"
    } else if value.parse::<f64>().is_ok_and(f64::is_finite) {
        "decimal"
    } else if guid_shape(value) {
        "guid-like"
    } else {
        "text"
    }
}

fn integer_shape(value: &str) -> bool {
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn datetime_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 16
        && bytes.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            10 => *b == b' ',
            13 => *b == b':',
            _ => b.is_ascii_digit(),
        })
}

fn guid_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    match bytes.len() {
        22 => bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"+/_-$".contains(b)),
        32 => bytes.iter().all(u8::is_ascii_hexdigit),
        36 => bytes.iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_hexdigit(),
        }),
        _ => false,
    }
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_files(&entry.path(), files)?;
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("xer"))
        {
            files.push(entry.path());
        }
    }
    Ok(())
}

pub(crate) fn run(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::NotFound, "no XER files in corpus directory").into(),
        );
    }
    let mut inventory = Inventory::default();
    for path in files {
        inventory.files += 1;
        match raw::read(std::fs::read(path)?) {
            Ok(xer) => inventory.observe(&xer),
            Err(error) => *inventory.rejected.entry(error.to_string()).or_default() += 1,
        }
    }
    let mut report = String::new();
    inventory.report(&mut report)?;
    print!("{report}");
    Ok(())
}

fn kind_name(kind: registry::FieldKind) -> &'static str {
    use registry::FieldKind;
    match kind {
        FieldKind::Id => "id",
        FieldKind::ForeignId(_) => "foreign-id",
        FieldKind::Text => "text",
        FieldKind::Int => "int",
        FieldKind::Decimal => "decimal",
        FieldKind::DateTime => "datetime",
        FieldKind::Flag => "flag",
        FieldKind::Enum(_) => "enum",
        FieldKind::Guid => "guid",
        FieldKind::Blob => "blob",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_vocabulary_never_contains_values() {
        for (value, expected) in [
            ("", "empty"),
            ("Y", "flag"),
            ("N", "flag"),
            ("+123", "integer"),
            ("-2.50", "decimal"),
            ("2026-03-04 08:00", "datetime"),
            ("line\nbreak", "multiline"),
            ("confidential name", "text"),
            ("0123456789abcdef0123456789abcdef", "guid-like"),
            ("NaN", "text"),
        ] {
            assert_eq!(shape(value), expected);
        }
    }

    #[test]
    fn inventory_reports_unknowns_missing_and_extra_cells_without_values() {
        let source = b"ERMHDR\tsecret_header\n%T\tTASK\n%F\ttask_id\ttask_type\tfuture_field\n%R\t1\tTT_Task\tsecret_text\n%R\t2\tFUTURE_CODE\n%R\t3\tTT_Mile\t\tsecret_extra\n%T\tFUTURE_TABLE\n%F\tf\n%R\tsecret_other\n%E\n";
        let xer = raw::read(source.as_slice()).unwrap();
        let mut inventory = Inventory {
            files: 1,
            ..Inventory::default()
        };
        inventory.observe(&xer);
        let mut report = String::new();
        inventory.report(&mut report).unwrap();
        for secret in [
            "secret_header",
            "secret_text",
            "secret_extra",
            "secret_other",
            "FUTURE_CODE",
            "TT_Task",
        ] {
            assert!(!report.contains(secret));
        }
        assert!(report.contains("table \"FUTURE_TABLE\" known=false"));
        assert!(report.contains("field \"future_field\" kind=unknown"));
        assert!(report.contains("missing_cells=1"));
        assert!(report.contains("unknown_enum_codes=1"));
        assert!(report.contains("extra_cells=1"));
        assert_eq!(inventory.tables["TASK"].rows, 3);
    }
    #[test]
    fn report_is_independent_of_input_order_and_escapes_names() {
        let a = raw::read(b"ERMHDR\n%T\tZ\n%F\tz\n%R\t1\n%E\n".as_slice()).unwrap();
        let b = raw::read(b"ERMHDR\n%T\tA\n%F\ta\rname\n%R\tprivate\n%E\n".as_slice()).unwrap();
        let mut first = Inventory::default();
        first.observe(&a);
        first.observe(&b);
        let mut second = Inventory::default();
        second.observe(&b);
        second.observe(&a);
        let mut first_report = String::new();
        let mut second_report = String::new();
        first.report(&mut first_report).unwrap();
        second.report(&mut second_report).unwrap();
        assert_eq!(first_report, second_report);
        assert!(first_report.contains("a\\rname"));
        assert!(!first_report.contains("a\rname"));
        assert!(!first_report.contains("private"));
    }
}
