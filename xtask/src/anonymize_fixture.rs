//! Authored supplemental metadata case for independent anonymizer validation.
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, path::Path};
use totalfloat_xer::{RawXer, anonymize::anonymize, raw};
fn row(xer: &mut RawXer, name: &str, values: &[(&str, &str)]) -> Result<(), Box<dyn Error>> {
    let table = xer.table_index(name).ok_or("missing authored table")?;
    let cells: Vec<String> = xer.tables()[table]
        .fields
        .iter()
        .map(|field| {
            values
                .iter()
                .find(|(key, _)| *key == field)
                .map_or_else(String::new, |(_, value)| (*value).to_owned())
        })
        .collect();
    xer.push_row(table, &cells)?;
    Ok(())
}
fn bytes(xer: &RawXer) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    xer.write(&mut bytes)?;
    Ok(bytes)
}
fn generate() -> Result<BTreeMap<String, Vec<u8>>, Box<dyn Error>> {
    let mut source = raw::read(include_bytes!("../../fixtures/xer/fresh-writer.xer").as_slice())?;
    row(
        &mut source,
        "MEMOTYPE",
        &[
            ("memo_type_id", "1"),
            ("seq_num", "0"),
            ("eps_flag", "N"),
            ("proj_flag", "Y"),
            ("wbs_flag", "Y"),
            ("task_flag", "Y"),
            ("memo_type", "Synthetic notebook"),
        ],
    )?;
    row(
        &mut source,
        "ROLES",
        &[
            ("role_id", "1"),
            ("seq_num", "0"),
            ("role_name", "Synthetic role"),
            ("role_short_name", "SROLE"),
            ("role_descr", "Synthetic role description"),
            ("def_cost_qty_link_flag", "Y"),
        ],
    )?;
    source.push_table(
        "RCATTYPE",
        [
            "rsrc_catg_type_id",
            "seq_num",
            "rsrc_catg_short_len",
            "rsrc_catg_type",
        ]
        .map(str::to_owned)
        .to_vec(),
    )?;
    row(
        &mut source,
        "RCATTYPE",
        &[
            ("rsrc_catg_type_id", "1"),
            ("seq_num", "0"),
            ("rsrc_catg_short_len", "10"),
            ("rsrc_catg_type", "Synthetic category"),
        ],
    )?;
    source.push_table(
        "UMEASURE",
        ["unit_id", "seq_num", "unit_abbrev", "unit_name"]
            .map(str::to_owned)
            .to_vec(),
    )?;
    row(
        &mut source,
        "UMEASURE",
        &[
            ("unit_id", "1"),
            ("seq_num", "0"),
            ("unit_abbrev", "hr"),
            ("unit_name", "Synthetic hours"),
        ],
    )?;
    let table = source
        .table_index("SCHEDOPTIONS")
        .ok_or("missing options")?;
    let field = source.tables()[table]
        .field_index("LevelPriorityList")
        .ok_or("missing priority list")?;
    source.set_cell(table, 0, field, "priority_type,/ASC\x7f\x7f")?;
    let output = anonymize(&source, 42)?.xer;
    let source = bytes(&source)?;
    let output = bytes(&output)?;
    let manifest = format!(
        "# Supplemental anonymization fixtures\n\nAuthored synthetic inputs, Apache-2.0, with no stored results. Derived only from the authored fresh-writer fixture and synthetic supplemental metadata.\n\n| File | Operation | SHA-256 | Results |\n|---|---|---|---|\n| `source.xer` | Authored MEMOTYPE, ROLES, RCATTYPE, UMEASURE and priority-list content | `{:x}` | no stored results |\n| `anonymized.xer` | Field-aware anonymization, seed 42 | `{:x}` | no stored results |\n",
        Sha256::digest(&source),
        Sha256::digest(&output)
    );
    Ok(BTreeMap::from([
        ("source.xer".to_owned(), source),
        ("anonymized.xer".to_owned(), output),
        ("MANIFEST.md".to_owned(), manifest.into_bytes()),
    ]))
}
pub(crate) fn run(out: &Path) -> Result<(), Box<dyn Error>> {
    let files = generate()?;
    crate::generated::write(out, files)?;
    println!(
        "generated authored source and anonymized output; XER Tools validation not run by this command"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplemental_application_fixtures_regenerate_exactly() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/xer/validation/anonymize-metadata");
        for (name, bytes) in generate().unwrap() {
            assert_eq!(std::fs::read(root.join(&name)).unwrap(), bytes, "{name}");
        }
    }
}
