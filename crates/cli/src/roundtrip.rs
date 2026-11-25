//! `totalfloat roundtrip`: read an XER, write it back to memory, and report
//! whether the bytes are identical (the lossless guarantee in ARCHITECTURE.md, Lossless round trips).

use std::path::Path;

use serde::Serialize;

use crate::error::{CliError, exit};
use crate::output::print_line;

/// `totalfloat.roundtrip/1`.
#[derive(Debug, Serialize)]
pub struct Report {
    schema: &'static str,
    file: String,
    bytes: usize,
    identical: bool,
    first_difference: Option<Difference>,
}

/// Where the written bytes first differ from the original.
#[derive(Debug, Serialize)]
pub struct Difference {
    /// 0-based byte offset.
    offset: usize,
    /// 1-based line number in the original.
    line: usize,
}

/// Runs the command and returns the exit code.
pub fn run(path: &Path, json: bool) -> Result<u8, CliError> {
    let raw = crate::location::read(path)?;
    let original = raw.source();
    let mut written = Vec::with_capacity(original.len());
    raw.write(&mut written).map_err(|e| {
        CliError::new(
            "write-failed",
            format!("writing to memory failed: {e}"),
            "report this as a bug",
        )
    })?;
    let first_difference = (written != original).then(|| {
        let offset = written
            .iter()
            .zip(original)
            .take_while(|(a, b)| a == b)
            .count();
        // Pieces between line breaks = line breaks before the offset + 1.
        let line = original[..offset].split(|&b| b == b'\n').count();
        Difference { offset, line }
    });
    let report = Report {
        schema: "totalfloat.roundtrip/1",
        file: path.display().to_string(),
        bytes: original.len(),
        identical: first_difference.is_none(),
        first_difference,
    };
    if json {
        print_line(&serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        match &report.first_difference {
            None => print_line(&format!(
                "{}: byte-identical ({} bytes)",
                report.file, report.bytes
            )),
            Some(d) => print_line(&format!(
                "{}: NOT byte-identical; first difference at byte {} (line {})",
                report.file, d.offset, d.line
            )),
        }
    }
    Ok(if report.identical {
        exit::OK
    } else {
        exit::FINDINGS
    })
}
