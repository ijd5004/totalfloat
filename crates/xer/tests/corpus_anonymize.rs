//! Field-aware anonymization audit; prints aggregates and schema names only.
use std::{collections::BTreeMap, path::PathBuf};

use totalfloat_xer::{
    anonymize::{AnonymizeError, anonymize},
    raw, to_model,
};

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one corpus audit with aggregate-only diagnostics"
)]
#[allow(
    clippy::manual_assert_eq,
    reason = "assert_eq would disclose private schedule values on failure"
)]
fn corpus_anonymization_preserves_cells_structure_and_stored_results() {
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS unset; skipping anonymization corpus audit");
        return;
    };
    let root = PathBuf::from(root);
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/manifest.toml"
    ))
    .unwrap();
    let manifest: toml::Value = toml::from_str(&text)
        .unwrap_or_else(|_| panic!("invalid corpus manifest; content withheld"));
    let mut refusals = BTreeMap::<String, usize>::new();
    let (mut seen, mut accepted, mut projects, mut activities) = (0, 0, 0, 0);
    for entry in manifest["file"].as_array().unwrap() {
        if !entry["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag.as_str() == Some("p6-export"))
        {
            continue;
        }
        seen += 1;
        let path = root
            .join("files")
            .join(entry["id"].as_str().unwrap())
            .join(entry["filename"].as_str().unwrap());
        let original = raw::read(std::fs::read(path).unwrap()).unwrap();
        let output = match anonymize(&original, 42) {
            Ok(output) => output,
            Err(AnonymizeError::Unsupported { table, field }) => {
                *refusals.entry(format!("{table}.{field}")).or_default() += 1;
                continue;
            }
            Err(AnonymizeError::Layout) => {
                *refusals.entry("unsupported layout".to_owned()).or_default() += 1;
                continue;
            }
            Err(AnonymizeError::LabelSpace) => {
                *refusals.entry("code label space".to_owned()).or_default() += 1;
                continue;
            }
            Err(
                AnonymizeError::Verification
                | AnonymizeError::Edit(_)
                | AnonymizeError::Write(_)
                | AnonymizeError::Read(_),
            ) => panic!("anonymization failed unexpectedly; private values withheld"),
        };
        accepted += 1;
        let fields: BTreeMap<_, _> = output
            .report
            .iter()
            .map(|field| ((field.table.as_str(), field.field.as_str()), field.count))
            .collect();
        for (before, after) in original.tables().iter().zip(output.xer.tables()) {
            assert!(before.name == after.name && before.fields == after.fields);
            assert_eq!(before.rows.len(), after.rows.len());
            for (row, actual) in before.rows.iter().zip(&after.rows) {
                assert_eq!(row.cells.len(), actual.cells.len());
                for ((field, value), changed) in
                    before.fields.iter().zip(&row.cells).zip(&actual.cells)
                {
                    let scrubbed = !value.is_empty()
                        && fields.contains_key(&(before.name.as_str(), field.as_str()));
                    assert!(
                        if scrubbed {
                            value != changed
                        } else {
                            value == changed
                        },
                        "field-aware cell preservation failed; private values withheld"
                    );
                }
            }
        }
        let mut bytes = Vec::new();
        output.xer.write(&mut bytes).unwrap();
        let reread = raw::read(bytes.clone()).unwrap();
        let mut roundtrip = Vec::new();
        reread.write(&mut roundtrip).unwrap();
        assert!(bytes == roundtrip, "output must round-trip exactly");
        let before = to_model(&original).0;
        let after = to_model(&reread).0;
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(&after) {
            projects += 1;
            assert!(before.project.stored == after.project.stored);
            assert!(before.project.options == after.project.options);
            assert!(before.project.data_date == after.project.data_date);
            assert!(before.relationships == after.relationships);
            assert_eq!(before.activities.len(), after.activities.len());
            for (id, activity) in &before.activities {
                activities += 1;
                let other = &after.activities[id];
                assert!(activity.stored == other.stored);
                assert!(activity.original_duration == other.original_duration);
                assert!(activity.remaining_duration == other.remaining_duration);
                assert!(activity.calendar == other.calendar);
                assert!(activity.codes == other.codes);
            }
        }
    }
    println!(
        "anonymization: exports={seen} accepted={accepted} projects={projects} activities={activities} refusals={refusals:?}"
    );
    assert!(seen > 0, "corpus has no P6 exports");
    assert!(
        accepted > 0,
        "no corpus exports supported; report coverage before release"
    );
}
