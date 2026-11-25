//! Compare fresh output with corpus schema metadata; never copy schedule rows.
use std::{collections::BTreeMap, path::PathBuf};

use totalfloat_xer::{
    fresh::{self, FreshOptions},
    raw, to_model,
};

#[test]
fn fresh_schema_follows_a_corpus_export_and_the_most_common_exact_version() {
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS unset; skipping fresh schema comparison");
        return;
    };
    let root = PathBuf::from(root);
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/manifest.toml"
    ))
    .unwrap();
    let manifest: toml::Value =
        toml::from_str(&text).unwrap_or_else(|_| panic!("invalid corpus manifest"));
    let mut versions = BTreeMap::<String, usize>::new();
    let mut reference = None;
    for entry in manifest["file"].as_array().unwrap() {
        let tags = entry["tags"].as_array().unwrap();
        if !tags.iter().any(|tag| tag.as_str() == Some("p6-export")) {
            continue;
        }
        let path = root
            .join("files")
            .join(entry["id"].as_str().unwrap())
            .join(entry["filename"].as_str().unwrap());
        let xer = raw::read(std::fs::read(path).unwrap()).unwrap();
        *versions
            .entry(xer.header.version().unwrap().to_owned())
            .or_default() += 1;
        // The manifest tags the export the fresh layout follows.
        if tags
            .iter()
            .any(|tag| tag.as_str() == Some("fresh-reference"))
        {
            reference = Some(xer);
        }
    }
    let (version, count) = versions.iter().max_by_key(|(_, count)| *count).unwrap();
    assert_eq!(
        version,
        fresh::VERSION,
        "corpus changed; deliberately review the target version"
    );
    let reference = reference.expect("no corpus file is tagged fresh-reference");
    assert_eq!(reference.header.version(), Some(fresh::VERSION));
    let fixture =
        raw::read(include_bytes!("../../../fixtures/xer/fresh-writer.xer").as_slice()).unwrap();
    let model = to_model(&fixture).0.remove(0);
    let mut options = FreshOptions::new("2026-01-01".parse().unwrap());
    options.seed = 42;
    let generated = fresh::write(&model, options).unwrap();
    // No leveling configuration is modeled or emitted. Its unused optional
    // table triggers a blank-field error in XER Tools 2.2b; all others match.
    let expected: Vec<_> = reference
        .tables()
        .iter()
        .filter(|table| table.name != "RSRCLEVELLIST")
        .map(|table| (&table.name, &table.fields))
        .collect();
    let actual: Vec<_> = generated
        .tables()
        .iter()
        .map(|table| (&table.name, &table.fields))
        .collect();
    assert_eq!(
        actual, expected,
        "schema metadata differs from the selected reference"
    );
    assert!(generated.tables().iter().all(|table| {
        table
            .rows
            .iter()
            .all(|row| row.cells.len() == table.fields.len())
    }));
    println!(
        "fresh schema: version={version} exports_at_exact_version={count} tables={}",
        actual.len()
    );
}
