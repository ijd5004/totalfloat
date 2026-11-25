use super::*;
use totalfloat_model::Schedule;
fn batch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/xer/batches/001")
}
/// Every committed artifact regenerates exactly and no schedule holds results.
fn assert_regenerates_without_answers(directory: &Path, files: usize) -> Vec<Schedule> {
    let rendered = render(directory).unwrap();
    assert_eq!(rendered.len(), files);
    let mut schedules = Vec::new();
    for (name, bytes) in rendered {
        assert_eq!(
            std::fs::read(directory.join(&name)).unwrap(),
            bytes,
            "{name}"
        );
        if Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xer"))
        {
            let raw = totalfloat_xer::raw::read(bytes).unwrap();
            let (mut models, diagnostics) = totalfloat_xer::to_model(&raw);
            assert_eq!(models.len(), 1);
            assert!(diagnostics.omitted.is_empty());
            for activity in models[0].activities.values() {
                assert_eq!(activity.stored, totalfloat_model::StoredResults::default());
            }
            schedules.append(&mut models);
        }
    }
    schedules
}
#[test]
fn committed_batch_regenerates_exactly_and_contains_no_stored_answers() {
    assert_regenerates_without_answers(&batch(), 25);
}
#[test]
fn batch_002_regenerates_exactly_and_shows_a_calendar_edit_moving_a_span() {
    let directory = batch().with_file_name("002");
    let schedule = assert_regenerates_without_answers(&directory, 5).remove(0);
    let result = totalfloat_engine::schedule(&schedule, &schedule.project.options).unwrap();
    let id = totalfloat_model::ActivityId(1);
    let (start, finish) = (
        result.activities[&id].values.early_start.unwrap(),
        result.activities[&id].values.early_finish.unwrap(),
    );
    let calendar = schedule.calendars.values().next().unwrap();
    let before = totalfloat_calendar::WorkCalendar::new(calendar).unwrap();
    assert_eq!(before.work_between(start, finish).get(), 4800);

    // The calendar changes after the schedule: the same dates no longer span
    // the same work, with nothing wrong in the arithmetic (TF-029).
    let mut edited = calendar.clone();
    let friday: totalfloat_model::Date = "2026-01-16".parse().unwrap();
    edited.exceptions.remove(&friday).unwrap();
    let after = totalfloat_calendar::WorkCalendar::new(&edited).unwrap();
    assert_eq!(after.work_between(start, finish).get(), 5280);
}
#[test]
fn build_is_repeatable_and_refuses_to_overwrite_edited_artifacts() {
    let out = tempfile::tempdir().unwrap();
    build(&batch(), out.path()).unwrap();
    build(&batch(), out.path()).unwrap();
    let path = out.path().join("README.md");
    std::fs::write(&path, "hand-edited notes").unwrap();
    assert!(build(&batch(), out.path()).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "hand-edited notes");
}
#[test]
fn invalid_specs_fail_before_creating_any_output() {
    let source = std::fs::read_to_string(batch().join("rel-fs-positive.toml")).unwrap();
    let bad = [
        source.replace("schema = 1", "schema = 2"),
        source.replace("description = ", "descriptoin = "),
        source.replace("duration = 480", "duration = 9223372036854775808"),
        source.replace("successor = 2", "successor = 999"),
        source.replace("id = 2\n", "id = 1\n"),
    ];
    for source in bad {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.toml"), source).unwrap();
        let out = dir.path().join("output");
        assert!(build(dir.path(), &out).is_err());
        assert!(!out.exists());
    }
}

#[test]
fn batch_covers_requested_behaviors_with_distinct_lag_calendars() {
    use std::collections::BTreeSet;
    use totalfloat_model as m;
    let (mut ties, mut constraints, mut lag_calendars, mut floats, mut kinds, mut progress) = (
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    for (name, bytes) in render(&batch()).unwrap() {
        if !Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xer"))
        {
            continue;
        }
        let raw = totalfloat_xer::raw::read(bytes).unwrap();
        let model = totalfloat_xer::to_model(&raw).0.remove(0);
        assert_eq!(model.project.stored, m::ProjectStoredResults::default());
        lag_calendars.insert(format!("{:?}", model.project.options.lag_calendar));
        floats.insert(format!("{:?}", model.project.options.float_type));
        for a in model.activities.values() {
            kinds.insert(a.activity_type);
            if let Some(c) = a.primary_constraint {
                constraints.insert(format!("{:?}", c.kind));
            }
        }
        for r in model.relationships.values() {
            if r.lag != m::Minutes::ZERO {
                ties.insert((r.kind, r.lag.get().signum()));
            }
        }
        let a = &model.activities[&m::ActivityId(1)];
        let b = &model.activities[&m::ActivityId(2)];
        let project = &model.calendars[&model.project.default_calendar.unwrap()];
        let pred = &model.calendars[&a.calendar];
        let succ = &model.calendars[&b.calendar];
        assert_ne!(project.work_week, pred.work_week);
        assert_ne!(project.work_week, succ.work_week);
        assert_ne!(pred.work_week, succ.work_week);
        for calendar in [project, pred, succ] {
            assert!(
                calendar.work_week.0.iter().any(|day| day
                    .iter()
                    .map(|shift| shift.duration().get())
                    .sum::<i64>()
                    < 1440),
                "every authored workweek must differ from the 24-hour lag calendar"
            );
        }
        if name.starts_with("oos-") {
            assert_eq!(a.status, m::ActivityStatus::NotStarted);
            assert_eq!(b.status, m::ActivityStatus::InProgress);
            assert!(b.actual_start < model.project.data_date);
            progress.insert(format!("{:?}", model.project.options.out_of_sequence));
        }
    }
    assert_eq!(ties.len(), 8);
    assert_eq!(constraints.len(), 9);
    assert_eq!(lag_calendars.len(), 4);
    assert_eq!(floats.len(), 3);
    assert_eq!(progress.len(), 2);
    for kind in [
        m::ActivityType::LevelOfEffort,
        m::ActivityType::WbsSummary,
        m::ActivityType::StartMilestone,
        m::ActivityType::FinishMilestone,
    ] {
        assert!(kinds.contains(&kind));
    }
}

#[test]
fn duplicate_project_guids_are_refused_before_output() {
    let dir = tempfile::tempdir().unwrap();
    let source = std::fs::read_to_string(batch().join("rel-fs-positive.toml")).unwrap();
    std::fs::write(dir.path().join("a.toml"), &source).unwrap();
    std::fs::write(dir.path().join("b.toml"), &source).unwrap();
    let out = dir.path().join("out");
    assert!(
        build(dir.path(), &out)
            .unwrap_err()
            .to_string()
            .contains("duplicate project GUID")
    );
    assert!(!out.exists());
}

#[test]
fn whole_minute_fixture_specs_use_export_precision() {
    let source = std::fs::read_to_string(batch().join("rel-fs-positive.toml"))
        .unwrap()
        .replace("duration = 480", "duration = 1");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("one-minute.toml"), source).unwrap();
    let output = render(dir.path()).unwrap();
    let bytes = output.values().find(|b| b.starts_with(b"ERMHDR")).unwrap();
    let raw = totalfloat_xer::raw::read(bytes.as_slice()).unwrap();
    let schedules = totalfloat_xer::to_model(&raw).0;
    assert!(
        schedules[0]
            .activities
            .values()
            .any(|a| a.original_duration == totalfloat_model::Minutes(1))
    );
}
