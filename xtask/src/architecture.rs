//! Enforces the crate dependency rules in docs/ARCHITECTURE.md#crates.
//!
//! The "May depend on" column of the crate table is the source of truth. Every
//! workspace member must appear in the table, and every `totalfloat-*`
//! dependency (normal, dev or build) must be listed for that crate. To add an
//! edge, change the table first; it is an architectural decision.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Allowed internal dependencies for one crate.
#[derive(Debug)]
enum Allowed {
    Everything,
    Only(BTreeSet<String>),
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Every backtick-quoted word in `cell`.
fn backticked(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// Parse the crate table under `## Crates` into crate name → allowed deps.
fn allowed_edges(architecture_md: &str) -> BTreeMap<String, Allowed> {
    // Line-based so CRLF checkouts (Windows) parse the same way.
    let section: Vec<&str> = architecture_md
        .lines()
        .map(str::trim_end)
        .skip_while(|l| *l != "## Crates")
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .collect();
    let mut table = BTreeMap::new();
    for line in section.into_iter().filter(|l| l.starts_with("| `")) {
        let cells: Vec<&str> = line.trim_matches('|').split('|').collect();
        assert_eq!(cells.len(), 3, "crate table row has 3 columns: {line}");
        let name = backticked(cells[0])
            .into_iter()
            .next()
            .expect("crate name in backticks");
        let allowed = if cells[2].contains("everything") {
            Allowed::Everything
        } else {
            Allowed::Only(backticked(cells[2]).into_iter().collect())
        };
        table.insert(name, allowed);
    }
    assert!(!table.is_empty(), "crate table parsed as empty");
    table
}

/// Workspace member name (as in the table) → its manifest path.
fn members(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut members = BTreeMap::new();
    for entry in fs::read_dir(root.join("crates")).unwrap() {
        let dir = entry.unwrap().path();
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file() {
            let name = dir.file_name().unwrap().to_str().unwrap().to_owned();
            members.insert(name, manifest);
        }
    }
    members.insert("xtask".to_owned(), root.join("xtask/Cargo.toml"));
    members
}

/// Internal (`totalfloat-*`) dependencies of a manifest, prefix stripped.
fn internal_deps(manifest: &Path) -> BTreeSet<String> {
    let text = fs::read_to_string(manifest).unwrap();
    let doc: toml::Table = text.parse().unwrap();
    declared_internal(&doc)
}

/// Dependency sections at the top level and under `[target.*]`, reading a
/// renamed dependency by its real package name.
fn declared_internal(doc: &toml::Table) -> BTreeSet<String> {
    let sections = ["dependencies", "dev-dependencies", "build-dependencies"];
    let targets = doc
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(toml::Value::as_table);
    std::iter::once(doc)
        .chain(targets)
        .flat_map(|table| sections.iter().filter_map(|s| table.get(*s)))
        .filter_map(toml::Value::as_table)
        .flat_map(|deps| deps.iter())
        .map(|(name, spec)| {
            spec.get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(name)
        })
        .filter_map(|dep| dep.strip_prefix("totalfloat-"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn target_and_renamed_dependencies_count() {
    let doc: toml::Table = r#"
        [dependencies]
        totalfloat-model = "0"
        engine-alias = { package = "totalfloat-engine", version = "0" }
        [target.'cfg(unix)'.dev-dependencies]
        totalfloat-xer = "0"
    "#
    .parse()
    .unwrap();
    let found: Vec<String> = declared_internal(&doc).into_iter().collect();
    assert_eq!(found, ["engine", "model", "xer"]);
}

#[test]
fn crate_dependencies_match_architecture_table() {
    let root = workspace_root();
    let table = allowed_edges(&fs::read_to_string(root.join("docs/ARCHITECTURE.md")).unwrap());
    let mut violations = Vec::new();
    for (name, manifest) in members(&root) {
        let Some(allowed) = table.get(&name) else {
            violations.push(format!(
                "`{name}` is missing from the crate table in docs/ARCHITECTURE.md"
            ));
            continue;
        };
        if let Allowed::Only(allowed) = allowed {
            for dep in internal_deps(&manifest) {
                if !allowed.contains(&dep) {
                    violations.push(format!(
                        "`{name}` depends on `{dep}`, which docs/ARCHITECTURE.md does not allow"
                    ));
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn table_parser_reads_restricted_and_open_rows() {
    let md = "# A\n\n## Crates\n\n| Crate | Responsibility | May depend on |\n|---|---|---|\n\
              | `model` | Model. | — |\n| `engine` | CPM, `pure`. | `model`, `calendar` |\n\
              | `cli` | Binary. | everything |\n\n## Next\n| `x` | y | z |\n";
    let table = allowed_edges(md);
    assert_eq!(table.len(), 3);
    assert!(matches!(&table["model"], Allowed::Only(s) if s.is_empty()));
    assert!(
        matches!(&table["engine"], Allowed::Only(s) if s.iter().eq(["calendar", "model"].iter()))
    );
    assert!(matches!(table["cli"], Allowed::Everything));
    assert_eq!(allowed_edges(&md.replace('\n', "\r\n")).len(), 3);
}
