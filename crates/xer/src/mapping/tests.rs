use super::*;
use crate::raw;

fn fixture() -> RawXer {
    raw::read(include_bytes!("../../../../fixtures/xer/model-mapping.xer").as_slice()).unwrap()
}

#[test]
fn export_precision_recognition_is_exact_at_the_tokens_scale() {
    for (text, minute) in [
        ("7.816667", 469),
        ("6.166666", 370),
        ("-6.166667", -370),
        ("-6.166666", -370),
        ("0.0166667", 1),
        ("0.0166666", 1),
        ("0.0166666666666666666666666666666666666667", 1),
    ] {
        assert_eq!(
            text.parse::<Decimal>().unwrap().export_minutes(),
            Some(m::Minutes::new(minute)),
            "{text}"
        );
    }
    for text in [
        "0.01667",
        "0.016668",
        "0.0166660",
        "0.01",
        "0.000001",
        "1e-64",
        "99999999999999999999999999999999999999",
    ] {
        assert_eq!(
            text.parse::<Decimal>().unwrap().export_minutes(),
            None,
            "{text}"
        );
    }
}

#[test]
fn empty_relationship_lag_is_zero_but_malformed_lag_is_omitted() {
    for (token, accepted) in [
        ("", true),
        ("0.016667", true),
        ("bad", false),
        (" ", false),
        ("0.01", false),
    ] {
        let mut raw = fixture();
        let t = raw.table_index("TASKPRED").unwrap();
        let c = raw.tables()[t].field_index("lag_hr_cnt").unwrap();
        raw.set_cell(t, 0, c, token).unwrap();
        let (schedules, report) = to_model(&raw);
        assert_eq!(
            schedules[0]
                .relationships
                .contains_key(&m::RelationshipId(1001)),
            accepted,
            "{token}"
        );
        if token.is_empty() {
            assert_eq!(
                schedules[0].relationships[&m::RelationshipId(1001)].lag,
                m::Minutes::ZERO
            );
            assert!(!report.omitted.contains(&RowRef { table: t, row: 0 }));
        }
    }
}

#[test]
fn synthetic_multi_project_fixture_maps_all_entities_and_provenance() {
    let raw = fixture();
    let (schedules, report) = to_model(&raw);
    assert_eq!(schedules.len(), 2);
    assert_eq!(report.skipped_rows, 0);
    assert_eq!(report.non_whole_minute_hour_fields.len(), 0);
    let first = &schedules[0];
    let second = &schedules[1];
    assert_eq!(first.project.id, m::ProjectId(1));
    assert_eq!(first.project.name.as_deref(), Some("Synthetic Project One"));
    assert_eq!(first.activities.len(), 2);
    assert_eq!(second.activities.len(), 1);
    assert_eq!(first.relationships.len(), 3);
    assert_eq!(second.relationships.len(), 1);
    assert!(first.is_external(&first.relationships[&m::RelationshipId(1002)]));
    assert_eq!(
        first.relationships[&m::RelationshipId(1003)].predecessor_project,
        m::ProjectId(99)
    );
    assert_eq!(first.wbs[&m::WbsId(11)].parent, None);
    let activity = &first.activities[&m::ActivityId(101)];
    assert_eq!(activity.activity_type, m::ActivityType::TaskDependent);
    assert_eq!(activity.status, m::ActivityStatus::InProgress);
    assert_eq!(
        activity.original_duration,
        "8.0000"
            .parse::<Decimal>()
            .unwrap()
            .checked_minutes()
            .unwrap()
    );
    assert_eq!(
        activity.stored.total_float,
        "-1.5".parse::<Decimal>().unwrap().checked_minutes()
    );
    assert_eq!(
        activity.actual_start,
        Some("2026-01-01 08:00".parse().unwrap())
    );
    assert_ne!(activity.planned_start, activity.stored.early_start);
    assert_eq!(
        activity.codes[&m::ActivityCodeTypeId(701)],
        m::ActivityCodeId(711)
    );
    assert_eq!(activity.udfs[&m::UdfTypeId(902)], m::UdfValue::Number(1.25));
    assert_eq!(
        first.project.udfs[&m::UdfTypeId(901)],
        m::UdfValue::Text("Synthetic note".into())
    );
    assert!(matches!(
        first.wbs[&m::WbsId(12)].udfs[&m::UdfTypeId(903)],
        m::UdfValue::Date(_)
    ));
    assert_eq!(first.resources.len(), 2);
    assert_eq!(second.resources.len(), 1);
    assert_eq!(
        first.resources[&m::ResourceId(501)].parent,
        Some(m::ResourceId(500))
    );
    assert_eq!(
        first.resources[&m::ResourceId(501)].udfs[&m::UdfTypeId(904)],
        m::UdfValue::Number(2.5)
    );
    assert_eq!(
        first.assignments[&m::AssignmentId(801)].udfs[&m::UdfTypeId(905)],
        m::UdfValue::Text("Synthetic assignment".into())
    );
    assert_eq!(first.activity_codes.len(), 2);
    assert!(
        !first
            .activity_code_types
            .contains_key(&m::ActivityCodeTypeId(702))
    );
    assert_eq!(
        second.project.options.out_of_sequence,
        m::OutOfSequenceMode::ProgressOverride
    );
    assert_eq!(
        second.project.options.lag_calendar,
        m::LagCalendar::Successor
    );
    assert_eq!(
        second.project.options.critical,
        m::CriticalDefinition::LongestPath
    );
    assert_eq!(
        second.project.options.float_paths.mode,
        m::FloatPathMode::FreeFloat
    );
    check_provenance(&raw, &schedules);
}

fn check_provenance(raw: &RawXer, schedules: &[m::Schedule]) {
    for schedule in schedules {
        assert_eq!(schedule.validate(), []);
        let source = RowRef::from_source_ref(schedule.project.source.unwrap()).unwrap();
        assert_eq!(raw.tables()[source.table].name, "PROJECT");
        for activity in schedule.activities.values() {
            let source = RowRef::from_source_ref(activity.source.unwrap()).unwrap();
            assert_eq!(raw.tables()[source.table].name, "TASK");
            assert_eq!(
                raw.tables()[source.table].rows[source.row]
                    .get(0)
                    .unwrap()
                    .parse::<u64>()
                    .unwrap(),
                activity.id.0
            );
        }
        for calendar in schedule.calendars.values() {
            assert!(calendar.source.is_some());
        }
        for resource in schedule.resources.values() {
            assert!(resource.source.is_some());
        }
        for assignment in schedule.assignments.values() {
            assert!(assignment.source.is_some());
        }
        for code in schedule.activity_codes.values() {
            assert!(code.source.is_some());
        }
        for definition in schedule.udf_types.values() {
            assert!(definition.source.is_some());
        }
    }
    let mut bytes = Vec::new();
    raw.write(&mut bytes).unwrap();
    assert_eq!(bytes, raw.source());
}

#[test]
fn strict_minute_conversion_never_rounds_or_invents_required_duration() {
    let mut raw = fixture();
    let task = raw.table_index("TASK").unwrap();
    let column = raw.tables()[task]
        .field_index("target_drtn_hr_cnt")
        .unwrap();
    raw.set_cell(task, 0, column, "0.01").unwrap();
    let (schedules, report) = to_model(&raw);
    assert!(!schedules[0].activities.contains_key(&m::ActivityId(101)));
    assert_eq!(report.skipped_rows, 1);
    assert_eq!(
        report.non_whole_minute_hour_fields["TASK.target_drtn_hr_cnt"],
        1
    );
    assert!(
        report
            .details
            .iter()
            .any(|d| d.kind == DiagnosticKind::SkippedRow
                && d.source
                    == Some(RowRef {
                        table: task,
                        row: 0
                    }))
    );
    assert_eq!(raw.tables()[task].rows[0].get(column), Some("0.01"));
}

#[test]
fn omitted_rows_are_listed_once_even_with_several_bad_fields() {
    let mut raw = fixture();
    let task = raw.table_index("TASK").unwrap();
    let t = &raw.tables()[task];
    let original = t.field_index("target_drtn_hr_cnt").unwrap();
    let remaining = t.field_index("remain_drtn_hr_cnt").unwrap();
    raw.set_cell(task, 0, original, "0.01").unwrap();
    raw.set_cell(task, 0, remaining, "0.01").unwrap();
    let (schedules, report) = to_model(&raw);
    let at = RowRef {
        table: task,
        row: 0,
    };
    assert!(report.omitted.contains(&at));
    assert_eq!(report.skipped_rows, 1, "one row, however many bad fields");
    // Every TASK row is either an activity or listed as omitted.
    let mapped: usize = schedules.iter().map(|s| s.activities.len()).sum();
    let omitted = report.omitted.iter().filter(|r| r.table == task).count();
    assert_eq!(mapped + omitted, raw.tables()[task].rows.len());
}

#[test]
fn optional_nonminute_float_remains_raw_instead_of_fabricated_minutes() {
    let mut raw = fixture();
    let task = raw.table_index("TASK").unwrap();
    let column = raw.tables()[task]
        .field_index("total_float_hr_cnt")
        .unwrap();
    raw.set_cell(task, 0, column, "0.01").unwrap();
    let (schedules, report) = to_model(&raw);
    assert_eq!(schedules[0].activities.len(), 2);
    assert_eq!(
        schedules[0].activities[&m::ActivityId(101)]
            .stored
            .total_float,
        None
    );
    assert_eq!(
        report.non_whole_minute_hour_fields["TASK.total_float_hr_cnt"],
        1
    );
}

#[test]
fn missing_options_use_documented_defaults_and_unlimited_paths_are_explicit() {
    let mut raw = fixture();
    let table = raw.table_index("SCHEDOPTIONS").unwrap();
    let column = raw.tables()[table]
        .field_index("limit_multiple_longest_path_calc")
        .unwrap();
    raw.set_cell(table, 0, column, "N").unwrap();
    let (schedules, _) = to_model(&raw);
    assert_eq!(schedules[0].project.options.float_paths.max_paths, None);
    let raw = raw::read(
        b"ERMHDR\n%T\tPROJECT\n%F\tproj_id\tproj_short_name\n%R\t1\tSYN\n%E\n".as_slice(),
    )
    .unwrap();
    let (schedules, report) = to_model(&raw);
    assert_eq!(schedules[0].project.options, m::ScheduleOptions::default());
    assert!(report.counts[&DiagnosticKind::DefaultedOption] > 0);
}

#[test]
fn float_type_codes_map_and_unknown_codes_are_reported() {
    for (code, expected) in [
        ("FT_FF", m::FloatType::FinishFloat),
        ("FT_SS", m::FloatType::StartFloat),
        ("FT_Min", m::FloatType::SmallestFloat),
    ] {
        let mut raw = fixture();
        let table = raw.table_index("SCHEDOPTIONS").unwrap();
        let column = raw.tables()[table].field_index("sched_float_type").unwrap();
        raw.set_cell(table, 0, column, code).unwrap();
        let (schedules, report) = to_model(&raw);
        assert_eq!(schedules[0].project.options.float_type, expected);
        assert!(!report.counts.contains_key(&DiagnosticKind::DefaultedOption));
    }

    let mut raw = fixture();
    let table = raw.table_index("SCHEDOPTIONS").unwrap();
    let column = raw.tables()[table].field_index("sched_float_type").unwrap();
    raw.set_cell(table, 0, column, "FT_Unknown").unwrap();
    let (schedules, report) = to_model(&raw);
    assert_eq!(
        schedules[0].project.options.float_type,
        m::FloatType::FinishFloat
    );
    assert_eq!(report.counts[&DiagnosticKind::DefaultedOption], 1);
}

#[test]
fn upper_case_lag_calendar_code_is_reported_not_guessed() {
    let mut raw = fixture();
    let table = raw.table_index("SCHEDOPTIONS").unwrap();
    let column = raw.tables()[table]
        .field_index("sched_calendar_on_relationship_lag")
        .unwrap();
    raw.set_cell(table, 0, column, "RCAL_SUCCESSOR").unwrap();
    let (schedules, report) = to_model(&raw);
    assert_eq!(
        schedules[0].project.options.lag_calendar,
        m::ScheduleOptions::default().lag_calendar
    );
    assert_eq!(report.counts[&DiagnosticKind::DefaultedOption], 1);
}

#[test]
fn missing_predecessor_project_uses_raw_ownership_even_if_activity_cannot_map() {
    let mut raw = fixture();
    let pred = raw.table_index("TASKPRED").unwrap();
    let project_column = raw.tables()[pred].field_index("pred_proj_id").unwrap();
    raw.set_cell(pred, 1, project_column, "").unwrap();
    let task = raw.table_index("TASK").unwrap();
    let duration_column = raw.tables()[task]
        .field_index("target_drtn_hr_cnt")
        .unwrap();
    raw.set_cell(task, 0, duration_column, "0.01").unwrap();
    let (schedules, report) = to_model(&raw);
    assert!(!schedules[0].activities.contains_key(&m::ActivityId(101)));
    // Project 1's copy would point at the omitted activity 101, so strict
    // conversion omits it there (no dangling references) and says so.
    assert!(
        !schedules[0]
            .relationships
            .contains_key(&m::RelationshipId(1002))
    );
    assert!(report.omitted.contains(&RowRef {
        table: pred,
        row: 1
    }));
    // Project 2 keeps it as an external relationship whose predecessor
    // project was inferred from raw TASK ownership.
    assert_eq!(
        schedules[1].relationships[&m::RelationshipId(1002)].predecessor_project,
        m::ProjectId(1)
    );
    assert_eq!(report.counts[&DiagnosticKind::InferredProject], 1);
    for schedule in &schedules {
        assert_eq!(schedule.validate(), Vec::new());
    }
}

#[test]
fn unknown_codes_or_missing_identity_never_get_synthetic_ids() {
    let mut raw = fixture();
    let task = raw.table_index("TASK").unwrap();
    let code = raw.tables()[task].field_index("task_type").unwrap();
    raw.set_cell(task, 0, code, "TT_Future").unwrap();
    let (schedules, report) = to_model(&raw);
    assert!(!schedules[0].activities.contains_key(&m::ActivityId(101)));
    assert!(report.skipped_rows > 0);
    let project = raw.table_index("PROJECT").unwrap();
    raw.set_cell(project, 0, 0, "").unwrap();
    let (schedules, report) = to_model(&raw);
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].project.id, m::ProjectId(2));
    assert!(report.skipped_fields["proj_id"] > 0);
}

#[test]
fn whole_minutes_outside_model_range_are_separate_from_fractional_minutes() {
    let value = "1e38".parse::<Decimal>().unwrap();
    assert!(value.is_whole_minutes());
    assert_eq!(value.checked_minutes(), None);
    let mut raw = fixture();
    let task = raw.table_index("TASK").unwrap();
    let column = raw.tables()[task]
        .field_index("target_drtn_hr_cnt")
        .unwrap();
    raw.set_cell(task, 0, column, "1e38").unwrap();
    let (_, report) = to_model(&raw);
    assert_eq!(report.non_whole_minute_hour_fields.len(), 0);
    assert_eq!(
        report.out_of_range_hour_fields["TASK.target_drtn_hr_cnt"],
        1
    );
}

fn with_cell(table: &str, field: &str, value: &str) -> RawXer {
    let bytes = include_bytes!("../../../../fixtures/xer/model-mapping.xer");
    let mut raw = raw::read(bytes.as_slice()).unwrap();
    let index = raw.table_index(table).unwrap();
    let column = raw.tables()[index].field_index(field).unwrap();
    raw.set_cell(index, 0, column, value).unwrap();
    raw
}

#[test]
fn an_unreadable_scheduling_date_leaves_its_row_out() {
    for (field, value) in [
        ("act_start_date", "not a date"),
        ("expect_end_date", "not a date"),
        ("cstr_date", "not a date"),
        ("cstr_type", "CS_Unknown"),
    ] {
        let raw = with_cell("TASK", field, value);
        let (_, diagnostics) = to_model(&raw);
        assert_eq!(diagnostics.skipped_fields.get(field), Some(&1), "{field}");
    }
}

#[test]
fn an_empty_date_and_an_unreadable_stored_date_keep_their_row() {
    for (field, value) in [("act_start_date", ""), ("early_start_date", "not a date")] {
        let raw = with_cell("TASK", field, value);
        assert!(to_model(&raw).1.omitted.is_empty(), "{field}");
    }
}
