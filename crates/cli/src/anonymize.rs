//! `totalfloat anonymize`: validate in memory before creating a new output.

use std::path::Path;

use serde::Serialize;
use totalfloat_xer::anonymize::anonymize;

use crate::error::{CliError, exit};
use crate::output::{permissions_of, print_line, refuse_existing, write_new};

#[derive(Serialize)]
struct FieldCount {
    table: String,
    field: String,
    count: usize,
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    seed: u64,
    scrubbed_cells: usize,
    retained_metadata: [&'static str; 6],
    external_validation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<Vec<FieldCount>>,
}

/// Runs the command; unsupported content or failed verification never reaches an output file.
pub fn run(
    input: &Path,
    output: &Path,
    seed: u64,
    report: bool,
    json: bool,
) -> Result<u8, CliError> {
    refuse_existing(output)?;
    let raw = crate::location::read(input)?;
    let result = anonymize(&raw, seed).map_err(|error| {
        CliError::new(
            "anonymization-refused",
            error.to_string(),
            "review unsupported fields or code-length limits; no output was created",
        )
    })?;
    write_new(&result.xer, permissions_of(input)?, output)?;
    let fields: Vec<_> = result
        .report
        .into_iter()
        .map(|field| FieldCount {
            table: field.table,
            field: field.field,
            count: field.count,
        })
        .collect();
    let result = Report {
        schema: "totalfloat.anonymize/1",
        seed,
        scrubbed_cells: fields.iter().map(|field| field.count).sum(),
        retained_metadata: [
            "IDs",
            "GUIDs",
            "dates",
            "costs",
            "currency settings",
            "operational codes",
        ],
        external_validation: "not-run",
        fields: report.then_some(fields),
    };
    if json {
        print_line(&serde_json::to_string_pretty(&result).unwrap_or_default());
    } else {
        print_line(&format!(
            "Wrote anonymized XER ({} scrubbed cells).",
            result.scrubbed_cells
        ));
        print_line(
            "IDs, GUIDs, dates and costs retained; review before sharing. XER Tools validation not run.",
        );
        if let Some(fields) = result.fields {
            for field in fields {
                print_line(&format!(
                    "  {}.{}: {}",
                    field.table, field.field, field.count
                ));
            }
        }
    }
    Ok(exit::OK)
}
