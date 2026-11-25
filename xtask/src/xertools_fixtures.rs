//! Reproducible authored application-validation inputs, with no scheduling.
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, fmt::Write as _, path::Path};
use totalfloat_model as m;
use totalfloat_xer::{RawXer, patch, raw, to_model};

fn bytes(xer: &RawXer) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    xer.write(&mut bytes)?;
    Ok(bytes)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn cases() -> Result<BTreeMap<String, Vec<u8>>, Box<dyn Error>> {
    let source = include_bytes!("../../fixtures/xer/fresh-writer.xer");
    let raw = raw::read(source.as_slice())?;
    let base = to_model(&raw).0.remove(0);
    let mut manifest = format!(
        "# Patch validation fixtures\n\nAuthored synthetic inputs, Apache-2.0, with no stored results.\nSource: `fixtures/xer/fresh-writer.xer`, SHA-256 `{}`. Every patch starts independently from that authored source.\n\n| File | Operation | SHA-256 | Results |\n|---|---|---|---|\n",
        digest(source)
    );
    writeln!(
        manifest,
        "| `baseline.xer` | Byte-identical copy of the authored source | `{}` | no stored results |",
        digest(source)
    )?;
    let mut files = BTreeMap::new();
    files.insert("baseline.xer".to_owned(), source.to_vec());
    for name in [
        "duration",
        "lag",
        "precision-duration",
        "precision-lag",
        "project-date",
        "insert",
        "delete",
    ] {
        let mut edited = base.clone();
        let description = edit(&mut edited, name)?;
        let output = patch(&raw, &edited)?;
        let output = bytes(&output)?;
        // The read-back must retain referential integrity and contain no
        // invented scheduling answers, even after dependent-row cleanup.
        let mapped = to_model(&raw::read(output.clone())?).0.remove(0);
        if mapped.project.stored != m::ProjectStoredResults::default()
            || !mapped.validate().is_empty()
            || mapped
                .activities
                .values()
                .any(|a| a.stored != m::StoredResults::default())
        {
            return Err(
                "patch validation fixture lost integrity or gained computed answers".into(),
            );
        }
        writeln!(
            manifest,
            "| `{name}.xer` | {description} | `{}` | no stored results |",
            digest(&output)
        )?;
        files.insert(format!("{name}.xer"), output);
    }
    files.insert("MANIFEST.md".to_owned(), manifest.into_bytes());
    Ok(files)
}
fn edit(model: &mut m::Schedule, name: &str) -> Result<&'static str, Box<dyn Error>> {
    match name {
        "precision-duration" => {
            let a = model
                .activities
                .get_mut(&m::ActivityId(1))
                .ok_or("missing work task")?;
            a.original_duration = m::Minutes(469);
            a.remaining_duration = m::Minutes(469);
            Ok("Set task 1 durations to 469 minutes (six-decimal export precision)")
        }
        "precision-lag" => {
            model
                .relationships
                .get_mut(&m::RelationshipId(1))
                .ok_or("missing tie")?
                .lag = m::Minutes(-20);
            Ok("Set relationship 1 lag to negative 20 minutes (six-decimal export precision)")
        }
        "duration" => {
            let a = model
                .activities
                .get_mut(&m::ActivityId(1))
                .ok_or("missing work task")?;
            a.original_duration = m::Minutes(600);
            a.remaining_duration = m::Minutes(600);
            Ok("Set task 1 original/remaining duration to an explicit 600 minutes")
        }
        "lag" => {
            model
                .relationships
                .get_mut(&m::RelationshipId(1))
                .ok_or("missing tie")?
                .lag = m::Minutes(60);
            Ok("Set relationship 1 lag to an explicit 60 minutes")
        }
        "project-date" => {
            model.project.data_date = Some("2026-01-13 08:00".parse()?);
            Ok("Set the project data date to an explicit 2026-01-13 08:00 input")
        }
        "insert" => {
            let mut a = m::Activity::new(
                m::ActivityId(3),
                model.project.id,
                m::WbsId(10),
                m::CalendarId(100),
                "TF-003",
                "Synthetic appended work",
            );
            a.original_duration = m::Minutes(240);
            a.remaining_duration = m::Minutes(240);
            model.activities.insert(a.id, a);
            model.relationships.insert(
                m::RelationshipId(2),
                m::Relationship {
                    id: m::RelationshipId(2),
                    predecessor: m::ActivityId(2),
                    successor: m::ActivityId(3),
                    predecessor_project: model.project.id,
                    successor_project: model.project.id,
                    kind: m::RelationshipType::FinishToStart,
                    lag: m::Minutes::ZERO,
                    source: None,
                },
            );
            Ok("Append task 3 and FS relationship 2 using new table-local IDs")
        }
        "delete" => {
            model.activities.remove(&m::ActivityId(1));
            model.relationships.retain(|_, r| {
                r.predecessor != m::ActivityId(1) && r.successor != m::ActivityId(1)
            });
            model
                .assignments
                .retain(|_, a| a.activity != m::ActivityId(1));
            Ok(
                "Delete task 1; remove its relationship, assignment, activity-code link and UDF value",
            )
        }
        _ => Err("unknown validation case".into()),
    }
}
pub(crate) fn run(out: &Path) -> Result<(), Box<dyn Error>> {
    let files = cases()?;
    crate::generated::write(out, files)?;
    println!(
        "generated baseline and seven synthetic patch cases; application validation not run by this command"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn committed_cases_regenerate_exactly() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/xer/validation/patches");
        for (name, bytes) in cases().unwrap() {
            assert_eq!(std::fs::read(root.join(&name)).unwrap(), bytes, "{name}");
        }
    }
    #[test]
    fn deletion_removes_all_task_references() {
        let cases = cases().unwrap();
        let raw = raw::read(cases["delete.xer"].clone()).unwrap();
        for table in raw.tables() {
            for field in ["task_id", "pred_task_id"] {
                if let Some(index) = table.field_index(field) {
                    assert!(table.rows.iter().all(|row| row.get(index) != Some("1")));
                }
            }
        }
        assert_eq!(raw.table("TASKRSRC").unwrap().rows.len(), 0);
        assert_eq!(raw.table("TASKACTV").unwrap().rows.len(), 0);
        assert_eq!(raw.table("UDFVALUE").unwrap().rows.len(), 0);
    }
}
