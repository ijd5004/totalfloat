//! Every MS Project XML file in the corpus reads, keeps all its
//! resources, and comes back the same through an export. Prints
//! counts only, never content.
use std::path::PathBuf;

use totalfloat_interop::{msproject, msproject_write};

/// The corpus's MS Project XML files.
fn corpus_files() -> Option<Vec<PathBuf>> {
    let root = PathBuf::from(std::env::var_os("TOTALFLOAT_CORPUS")?);
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("files"))
        .ok()?
        .flatten()
        .flat_map(|entry| {
            std::fs::read_dir(entry.path())
                .into_iter()
                .flatten()
                .flatten()
        })
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
        })
        .collect();
    files.sort();
    Some(files)
}

/// The file's resources with a UID above 0, the one Project keeps for
/// itself, counted from the text.
fn resources_listed(text: &str) -> usize {
    let Some(list) = text.split("<Resources>").nth(1) else {
        return 0;
    };
    let list = list.split("</Resources>").next().unwrap_or("");
    list.split("<Resource>")
        .skip(1)
        .filter(|resource| {
            resource
                .split("<UID>")
                .nth(1)
                .and_then(|rest| rest.split("</UID>").next())
                .and_then(|uid| uid.trim().parse::<i64>().ok())
                .is_some_and(|uid| uid > 0)
        })
        .count()
}

/// Same to a hundredth, as the file holds costs.
fn near(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() < 0.01,
        (None, None) => true,
        _ => false,
    }
}

#[test]
fn every_corpus_ms_project_file_reads_and_comes_back_through_an_export() {
    let Some(files) = corpus_files() else {
        eprintln!("TOTALFLOAT_CORPUS unset; skipping MS Project XML corpus test");
        return;
    };
    let (mut read, mut assignments, mut with_work) = (0, 0, 0);
    for (n, path) in files.iter().enumerate() {
        let text = std::fs::read_to_string(path).unwrap();
        let imported = msproject::read(&text)
            .unwrap_or_else(|e| panic!("corpus MS Project file {n} doesn't read: {e}"));
        read += 1;
        let schedule = imported.schedule;
        assert_eq!(
            schedule.resources.len(),
            resources_listed(&text),
            "corpus MS Project file {n}: resources"
        );
        let again = msproject::read(&msproject_write::write(&schedule).text)
            .unwrap_or_else(|e| panic!("corpus MS Project file {n} doesn't read back: {e}"))
            .schedule;
        assert_eq!(
            (
                again.activities.len(),
                again.relationships.len(),
                again.resources.len(),
                again.assignments.len()
            ),
            (
                schedule.activities.len(),
                schedule.relationships.len(),
                schedule.resources.len(),
                schedule.assignments.len()
            ),
            "corpus MS Project file {n}: counts after an export"
        );
        for (before, after) in schedule
            .assignments
            .values()
            .zip(again.assignments.values())
        {
            let (a, b) = (&before.amounts, &after.amounts);
            assignments += 1;
            with_work += usize::from(a.budgeted_units.is_some());
            assert!(
                near(a.budgeted_units, b.budgeted_units)
                    && near(a.actual_units, b.actual_units)
                    && near(a.remaining_units, b.remaining_units)
                    && near(a.budgeted_units_per_hour, b.budgeted_units_per_hour)
                    && near(a.budgeted_cost, b.budgeted_cost)
                    && near(a.actual_cost, b.actual_cost)
                    && near(a.remaining_cost, b.remaining_cost),
                "corpus MS Project file {n}: an assignment's amounts after an export"
            );
        }
    }
    println!("ms project corpus: files={read} assignments={assignments} with_work={with_work}");
}
