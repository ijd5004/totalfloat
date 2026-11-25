//! On every corpus file: where a project links to no other and measures float
//! from its own finish, the calculation an edit makes of it (each project on
//! its own, as `schedule::settle` calculates it) gives the same quality checks
//! as `assess` calculating it among the file's projects. Counts only,
//! never content. Skips when `TOTALFLOAT_CORPUS` is unset.

#![allow(clippy::unwrap_used, reason = "test")]

use std::path::Path;

use totalfloat_check::{Thresholds, assess, assess_calculated};
use totalfloat_engine::schedule_reusing;

#[test]
fn an_edit_s_calculation_gives_the_checks_assess_finds_on_every_corpus_file() {
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS unset; skipping the assess corpus test");
        return;
    };
    let mut files = Vec::new();
    for entry in std::fs::read_dir(Path::new(&root).join("files")).unwrap() {
        for file in std::fs::read_dir(entry.unwrap().path()).unwrap() {
            files.push(file.unwrap().path());
        }
    }
    files.sort();
    let thresholds = Thresholds::default();
    let (mut same, mut linked, mut refused) = (0, 0, 0);
    for (index, file) in files.iter().enumerate() {
        let Ok(raw) = totalfloat_xer::raw::read(std::fs::read(file).unwrap()) else {
            continue;
        };
        let schedules = totalfloat_xer::to_model(&raw).0;
        for (at, schedule) in schedules.iter().enumerate() {
            let options = &schedule.project.options;
            let links_out = schedule
                .relationships
                .values()
                .any(|link| schedule.is_external(link));
            if links_out || !options.float_from_project_finish {
                linked += 1;
                continue;
            }
            let Ok(calculated) = schedule_reusing(schedule, options, (&schedules, None), None)
            else {
                refused += 1;
                continue;
            };
            let given = assess_calculated(schedule, &schedules, &thresholds, &calculated);
            assert!(
                given == assess(schedule, &schedules, &thresholds),
                "file {index}, project {at}"
            );
            same += 1;
        }
    }
    eprintln!(
        "assess: {same} projects the same, {linked} left to assess (linked or float from the opened projects' finish), {refused} refused"
    );
    assert!(same > 0);
}
