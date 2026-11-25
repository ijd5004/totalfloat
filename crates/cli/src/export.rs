//! `query --csv` and `query --xlsx`: a query's activities and columns for a
//! spreadsheet. Every match is exported; the cells are `query`'s, as
//! its JSON gives them.

use std::path::Path;

use rust_xlsxwriter::{Format, Workbook};
use serde_json::{Map, Value};

use crate::error::CliError;
use crate::output::{create_new_bytes, refuse_existing};
use crate::report::{csv_cell, csv_field};

/// The rows as CSV: a header row of column names, then a row per activity.
/// Text a spreadsheet would run as a formula gets a leading `'`.
#[must_use]
pub fn csv(columns: &[String], rows: &[Map<String, Value>]) -> String {
    let mut out = columns
        .iter()
        .map(|c| csv_field(c))
        .collect::<Vec<_>>()
        .join(",");
    out.push('\n');
    for row in rows {
        let cells: Vec<String> = columns
            .iter()
            .map(|c| csv_field(&csv_cell(row.get(c).unwrap_or(&Value::Null))))
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

fn sheet_error(error: &rust_xlsxwriter::XlsxError) -> CliError {
    CliError::new(
        "export-failed",
        format!("the workbook couldn't be written: {error}"),
        "this is a bug; --csv gives the same rows",
    )
}

/// The rows as an Excel workbook in a new file at `output`: one sheet, a
/// bold header row, numbers as numbers and true/false as booleans.
///
/// # Errors
///
/// An existing output, or one that can't be written.
pub fn xlsx(
    columns: &[String],
    rows: &[Map<String, Value>],
    output: &Path,
) -> Result<(), CliError> {
    xlsx_sheets(columns, &[("Activities".to_owned(), rows)], &[], output)
}

/// Several sheets of the same columns in one new workbook, each named. The
/// `text` columns are formatted as text, so what's typed in them stays as
/// typed.
///
/// # Errors
///
/// An existing output, a name a sheet can't take, or one that can't be
/// written.
pub fn xlsx_sheets(
    columns: &[String],
    sheets: &[(String, &[Map<String, Value>])],
    text: &[&str],
    output: &Path,
) -> Result<(), CliError> {
    refuse_existing(output)?;
    let mut book = Workbook::new();
    let bold = Format::new().set_bold();
    // A blank cell for an empty value, so a row of nothing but blanks is
    // still there.
    let plain = Format::new();
    let as_text = Format::new().set_num_format("@");
    for (name, rows) in sheets {
        let sheet = book.add_worksheet();
        sheet.set_name(name).map_err(|e| sheet_error(&e))?;
        for (c, name) in columns.iter().enumerate() {
            let c = u16::try_from(c).map_err(|_| too_wide())?;
            sheet
                .write_string_with_format(0, c, name, &bold)
                .map_err(|e| sheet_error(&e))?;
            if text.contains(&name.as_str()) {
                sheet
                    .set_column_format(c, &as_text)
                    .map_err(|e| sheet_error(&e))?;
            }
        }
        for (r, row) in rows.iter().enumerate() {
            let r = u32::try_from(r + 1).map_err(|_| too_wide())?;
            for (c, name) in columns.iter().enumerate() {
                let c = u16::try_from(c).map_err(|_| too_wide())?;
                let written = match row.get(name) {
                    None | Some(Value::Null) => sheet.write_blank(r, c, &plain).map(|_| ()),
                    Some(Value::Number(n)) => match n.as_f64() {
                        Some(n) => sheet.write_number(r, c, n).map(|_| ()),
                        None => sheet.write_string(r, c, n.to_string()).map(|_| ()),
                    },
                    Some(Value::Bool(b)) => sheet.write_boolean(r, c, *b).map(|_| ()),
                    Some(Value::String(text)) if text.is_empty() => {
                        sheet.write_blank(r, c, &plain).map(|_| ())
                    }
                    // Written as text, never as a formula, whatever it starts with.
                    Some(Value::String(text)) => sheet.write_string(r, c, text).map(|_| ()),
                    Some(other) => sheet.write_string(r, c, other.to_string()).map(|_| ()),
                };
                written.map_err(|e| sheet_error(&e))?;
            }
        }
    }
    let bytes = book.save_to_buffer().map_err(|e| sheet_error(&e))?;
    create_new_bytes(&bytes, output)
}

fn too_wide() -> CliError {
    CliError::new(
        "export-failed",
        "more columns or rows than a workbook holds",
        "narrow the query, or use --csv",
    )
}
