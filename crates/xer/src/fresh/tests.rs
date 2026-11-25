use super::*;

fn full_model() -> m::Schedule {
    let input =
        raw::read(include_bytes!("../../../../fixtures/xer/model-mapping.xer").as_slice()).unwrap();
    let mut schedule = crate::to_model(&input).0.swap_remove(0);
    let project = schedule.project.id;
    schedule.relationships.retain(|_, relationship| {
        relationship.predecessor_project == project && relationship.successor_project == project
    });
    normalize(&mut schedule);
    schedule
}

/// The model as its fresh export reads back: values it leaves unstated in
/// columns new rows default read back as the defaults.
fn written(model: &m::Schedule) -> m::Schedule {
    let mut written = model.clone();
    super::as_written(&mut written);
    written
}

fn options() -> FreshOptions {
    FreshOptions::new("2026-01-01".parse().unwrap())
}

fn bytes(xer: &RawXer) -> Vec<u8> {
    let mut result = Vec::new();
    xer.write(&mut result).unwrap();
    result
}

#[test]
fn full_model_round_trips_without_source_bytes_or_lost_entities() {
    let model = full_model();
    assert_eq!(model.validate(), []);
    let output = write(&model, options()).unwrap();
    let (mut mapped, diagnostics) = crate::to_model(&output);
    assert_eq!(diagnostics.omitted, std::collections::BTreeSet::new());
    normalize(&mut mapped[0]);
    assert_eq!(mapped[0], written(&model));
    assert_eq!(output.header.version(), Some(VERSION));
    // Every table but those written only when used, which none here is:
    // RSRCROLE for a resource filling a role, the expense tables, curves and
    // project codes, WBS notes and steps.
    let unused = [
        "RSRCROLE",
        "ACCOUNT",
        "COSTTYPE",
        "PROJCOST",
        "RSRCCURVDATA",
        "PCATTYPE",
        "PCATVAL",
        "PROJPCAT",
        "WBSMEMO",
        "TASKPROC",
    ];
    for table in unused {
        assert!(output.table(table).is_none(), "{table}");
    }
    assert_eq!(output.tables().len(), schema::TABLES.len() - unused.len());
    let written = bytes(&output);
    assert_eq!(bytes(&raw::read(written.clone()).unwrap()), written);
}

#[test]
fn explicit_metadata_and_seed_are_deterministic_and_guids_have_p6_shape() {
    let model = full_model();
    let first = write(&model, options()).unwrap();
    assert_eq!(bytes(&first), bytes(&write(&model, options()).unwrap()));
    let mut changed = options();
    changed.seed = 99;
    let second = write(&model, changed).unwrap();
    assert_ne!(bytes(&first), bytes(&second));
    let mut guids = std::collections::BTreeSet::new();
    for table in first.tables() {
        if let Some(index) = table.field_index("guid") {
            for row in &table.rows {
                let guid = &row.cells[index];
                assert_eq!(guid.len(), 22);
                assert!(
                    guid.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"+/".contains(&b))
                );
                assert!(guids.insert(guid));
            }
        }
    }
    assert!(guids.len() > 5);
}

#[test]
fn export_precision_minutes_roundtrip_but_invalid_graphs_and_options_are_refused() {
    let mut model = full_model();
    model
        .activities
        .values_mut()
        .next()
        .unwrap()
        .remaining_duration = m::Minutes(1);
    let written = write(&model, options()).unwrap();
    assert_eq!(
        crate::to_model(&written).0[0]
            .activities
            .values()
            .next()
            .unwrap()
            .remaining_duration,
        m::Minutes(1)
    );
    let mut model = full_model();
    model.project.options.ignore_external_relationships = true;
    assert!(matches!(
        write(&model, options()),
        Err(FreshError::Unsupported(_))
    ));
    let mut model = full_model();
    model.wbs.clear();
    assert!(write(&model, options()).is_err());
    let mut model = full_model();
    model.project.name = None;
    assert!(matches!(
        write(&model, options()),
        Err(FreshError::ModelMismatch)
    ));
}

#[test]
fn calendar_intervals_exceptions_and_options_survive_exactly() {
    let mut model = full_model();
    let calendar = model.calendars.values_mut().next().unwrap();
    calendar
        .exceptions
        .insert("2026-01-06".parse().unwrap(), vec![]);
    model.project.options.out_of_sequence = m::OutOfSequenceMode::ActualDates;
    model.project.options.float_type = m::FloatType::StartFloat;
    model.project.options.lag_calendar = m::LagCalendar::TwentyFourHour;
    model.project.options.float_paths.max_paths = None;
    let output = write(&model, options()).unwrap();
    let mut imported = crate::to_model(&output).0.remove(0);
    normalize(&mut imported);
    assert_eq!(imported, written(&model));
}

#[test]
fn model_ids_are_retained_and_default_ids_are_table_local() {
    let model = full_model();
    let output = write(&model, options()).unwrap();
    for (table, field) in [
        ("CURRTYPE", "curr_id"),
        ("OBS", "obs_id"),
        ("SCHEDOPTIONS", "schedoptions_id"),
    ] {
        let table = output.table(table).unwrap();
        assert_eq!(
            table.rows[0].get(table.field_index(field).unwrap()),
            Some("1")
        );
    }
    let table = output.table("PROJECT").unwrap();
    assert_eq!(
        table.rows[0].get(table.field_index("proj_id").unwrap()),
        Some(model.project.id.to_string().as_str())
    );
    assert!(output.table("RSRCLEVELLIST").is_none());
}

proptest::proptest! {
    #[test]
    fn exact_duration_domain_round_trips_with_arbitrary_guid_seeds(units in 0_i64..100_000, seed in proptest::prelude::any::<u64>()) {
        let mut model = full_model();
        let activity = model.activities.values_mut().next().unwrap();
        activity.original_duration = m::Minutes(units * 3);
        activity.remaining_duration = activity.original_duration;
        let mut options = options(); options.seed = seed;
        let output = write(&model, options).unwrap();
        let mut imported = crate::to_model(&output).0.remove(0);
        normalize(&mut imported);
        proptest::prop_assert_eq!(imported, written(&model));
    }
}

proptest::proptest! {
    #[test]
    fn assignment_units_and_costs_round_trip(
        cents in proptest::collection::vec(0_i32..1_000_000_000, 11),
        linked in proptest::prelude::any::<bool>(),
        rate in 0_usize..5,
    ) {
        let mut model = full_model();
        assert!(!model.assignments.is_empty(), "the fixture needs an assignment");
        let value = |i: usize| Some(f64::from(cents[i]) / 100.0);
        let rates = [
            m::RateType::Price1,
            m::RateType::Price2,
            m::RateType::Price3,
            m::RateType::Price4,
            m::RateType::Price5,
        ];
        for assignment in model.assignments.values_mut() {
            assignment.amounts = m::Amounts {
                budgeted_units: value(0),
                actual_units: value(1),
                actual_overtime_units: value(2),
                remaining_units: value(3),
                budgeted_units_per_hour: value(4),
                remaining_units_per_hour: value(5),
                budgeted_cost: value(6),
                actual_cost: value(7),
                actual_overtime_cost: value(8),
                remaining_cost: value(9),
                price_per_unit: value(10),
                price_source: Some(m::PriceSource::Role),
                rate_type: Some(rates[rate]),
                cost_follows_units: linked,
            };
        }
        for resource in model.resources.values_mut() {
            resource.default_units_per_hour = value(4);
            resource.default_cost_follows_units = linked;
        }
        let output = write(&model, options()).unwrap();
        let mut imported = crate::to_model(&output).0.remove(0);
        normalize(&mut imported);
        proptest::prop_assert_eq!(imported, written(&model));
    }
}

proptest::proptest! {
    #[test]
    fn resource_rates_round_trip(
        cents in proptest::collection::vec(0_i32..1_000_000_000, 6),
        day in 1_u8..28,
    ) {
        let mut model = full_model();
        let value = |i: usize| Some(f64::from(cents[i]) / 100.0);
        let effective = format!("2026-02-{day:02} 08:00").parse().ok();
        let resource = model.resources.values_mut().next().unwrap();
        resource.rates = vec![
            m::ResourceRate {
                id: m::ResourceRateId(1),
                effective: "2026-01-01 00:00".parse().ok(),
                prices: [value(0), value(1), None, value(2), None],
                max_units_per_hour: value(3),
                source: None,
            },
            m::ResourceRate {
                id: m::ResourceRateId(2),
                effective,
                prices: [value(4), None, None, None, value(5)],
                max_units_per_hour: None,
                source: None,
            },
        ];
        let output = write(&model, options()).unwrap();
        let mut imported = crate::to_model(&output).0.remove(0);
        normalize(&mut imported);
        proptest::prop_assert_eq!(imported, written(&model));
    }
}

#[test]
fn wbs_earned_value_settings_round_trip() {
    let mut model = full_model();
    for (n, node) in model.wbs.values_mut().enumerate() {
        node.earned_value = m::EarnedValue {
            technique: Some(if n == 0 {
                m::EarnedValueTechnique::ActivityPercentComplete
            } else {
                m::EarnedValueTechnique::Other("EC_UNSEEN".to_owned())
            }),
            user_percent: Some(25.5),
            estimate: Some(m::EstimateMethod::PerformanceFactorUser),
            user_factor: Some(1.25),
        };
    }
    let output = write(&model, options()).unwrap();
    let mut imported = crate::to_model(&output).0.remove(0);
    normalize(&mut imported);
    assert_eq!(imported, written(&model));
}

#[test]
fn unverified_udf_extensions_are_refused() {
    let mut model = full_model();
    model.udf_types.values_mut().next().unwrap().data_type =
        m::UdfDataType::Other("FT_PRIVATE_UNKNOWN".to_owned());
    assert!(matches!(
        write(&model, options()),
        Err(FreshError::Unsupported(_))
    ));
    let mut model = full_model();
    let mut kind = model.udf_types.values().next().unwrap().clone();
    kind.id = m::UdfTypeId(999);
    kind.subject = m::UdfSubject::Other("FUTURE_TABLE".to_owned());
    model.udf_types.insert(kind.id, kind);
    assert_eq!(model.validate(), []);
    assert!(matches!(
        write(&model, options()),
        Err(FreshError::Unsupported(_))
    ));
}

#[test]
fn encoding_choice_is_exact_and_external_ties_are_refused() {
    let mut model = full_model();
    model.project.short_name = "Synthetic snowflake ❄".to_owned();
    assert!(matches!(write(&model, options()), Err(FreshError::Edit(_))));
    let mut utf8 = options();
    utf8.encoding = Encoding::Utf8 { bom: true };
    let output = write(&model, utf8).unwrap();
    assert!(bytes(&output).starts_with(&[0xef, 0xbb, 0xbf]));
    assert_eq!(
        crate::to_model(&output).0[0].project.short_name,
        model.project.short_name
    );
    let input =
        raw::read(include_bytes!("../../../../fixtures/xer/model-mapping.xer").as_slice()).unwrap();
    let with_external = crate::to_model(&input).0.remove(0);
    assert!(matches!(
        write(&with_external, options()),
        Err(FreshError::Unsupported(_))
    ));
}

#[test]
fn continuous_calendar_uses_midnight_finish_spelling_and_keeps_end_of_day() {
    let mut model = full_model();
    let calendar = model.calendars.values_mut().next().unwrap();
    let interval = m::WorkInterval::new(
        m::TimeOfDay::from_minutes(0).unwrap(),
        m::TimeOfDay::from_minutes(1440).unwrap(),
    )
    .unwrap();
    calendar.work_week = m::WorkWeek(std::array::from_fn(|_| vec![interval]));
    calendar
        .exceptions
        .insert("2026-01-06".parse().unwrap(), vec![interval]);
    let output = write(&model, options()).unwrap();
    let content = String::from_utf8(bytes(&output)).unwrap();
    assert!(content.contains("s|00:00|f|00:00"));
    assert!(!content.contains("f|24:00"));
    let mut mapped = crate::to_model(&output).0.remove(0);
    normalize(&mut mapped);
    assert_eq!(mapped, written(&model));
}

#[test]
fn midnight_application_fixture_regenerates_exactly() {
    let expected = include_bytes!("../../../../fixtures/xer/midnight-finish.xer");
    let raw = raw::read(expected.as_slice()).unwrap();
    let model = crate::to_model(&raw).0.remove(0);
    let mut options = options();
    options.seed = 42;
    assert_eq!(bytes(&write(&model, options).unwrap()), expected.as_slice());
}

/// Fresh rows of the four tables the patch writer adds rows to carry the
/// same defaults, from one place: each default is in every row, and is the
/// default itself wherever the model gives that column nothing.
#[test]
fn fresh_rows_carry_the_patch_writers_defaults() {
    let model = full_model();
    assert!(!model.resources.is_empty() && !model.assignments.is_empty());
    let output = write(&model, options()).unwrap();
    // The columns the model fills for each table, where its value wins.
    let modelled = |table: &str| -> Vec<&str> {
        match table {
            "TASK" => crate::patch::TASK_FIELDS.iter().map(|f| f.0).collect(),
            "TASKRSRC" => crate::patch::ASSIGNMENT_FIELDS
                .iter()
                .map(|f| f.0)
                .collect(),
            "PROJWBS" => vec!["proj_node_flag"],
            _ => Vec::new(),
        }
    };
    for table in ["TASK", "TASKRSRC", "PROJWBS", "RSRC"] {
        let defaults = crate::patch::new_row_defaults(table);
        assert!(!defaults.is_empty(), "{table}");
        let written = output.table(table).unwrap();
        assert!(!written.rows.is_empty(), "{table}");
        for row in &written.rows {
            for (field, value) in defaults {
                let Some(column) = written.field_index(field) else {
                    continue;
                };
                let cell = row.get(column).unwrap_or_default();
                assert!(!cell.is_empty(), "{table}.{field}");
                if !modelled(table).contains(field) {
                    assert_eq!(cell, *value, "{table}.{field}");
                }
            }
        }
    }
}

/// A default fills only what the model leaves empty: a value the model holds
/// is written as it is.
#[test]
fn a_model_value_wins_over_a_fresh_default() {
    let mut model = full_model();
    let activity = model.activities.values_mut().next().unwrap();
    activity.physical_percent_complete = Some(40.0);
    let activity = activity.id;
    for assignment in model.assignments.values_mut() {
        assignment.amounts.cost_follows_units = false;
    }
    let output = write(&model, options()).unwrap();
    let tasks = output.table("TASK").unwrap();
    let row = tasks
        .rows
        .iter()
        .find(|row| row.get(tasks.field_index("task_id").unwrap()) == Some(&activity.to_string()))
        .unwrap();
    assert_eq!(
        row.get(tasks.field_index("phys_complete_pct").unwrap()),
        Some("40")
    );
    let assignments = output.table("TASKRSRC").unwrap();
    let link = assignments.field_index("cost_qty_link_flag").unwrap();
    assert!(
        assignments
            .rows
            .iter()
            .all(|row| row.get(link) == Some("N"))
    );
}

proptest::proptest! {
    /// Roles, their rates and parents, the roles resources fill, an
    /// assignment's role, and an assignment by role alone come back as written.
    #[test]
    fn roles_round_trip(
        count in 1_u64..5,
        parents in proptest::collection::vec(proptest::prelude::any::<bool>(), 4),
        cents in proptest::collection::vec(0_i32..1_000_000_000, 4),
        skill in proptest::option::of(1_i64..6),
        dated in proptest::prelude::any::<bool>(),
    ) {
        let mut model = full_model();
        let value = |i: usize| Some(f64::from(cents[i]) / 100.0);
        for n in 1..=count {
            let id = m::RoleId(n);
            let under = usize::try_from(n).unwrap() - 1;
            let parent = (n > 1 && parents[under]).then(|| m::RoleId(n - 1));
            model.roles.insert(id, m::Role {
                id,
                short_name: format!("R{n}"),
                name: format!("Role {n}"),
                parent,
                rates: vec![m::RoleRate {
                    id: m::RoleRateId(10 + n),
                    effective: dated.then(|| "2026-01-05 08:00".parse().unwrap()),
                    prices: [value(0), value(1), None, None, value(2)],
                    max_units_per_hour: value(3),
                    source: None,
                }],
                default_cost_follows_units: parents[0],
                source: None,
            });
        }
        let resource = model.resources.values_mut().next().unwrap();
        resource.roles = vec![m::ResourceRole {
            id: m::ResourceRoleId(1),
            role: m::RoleId(1),
            skill_level: skill,
            source: None,
        }];
        let first = model.assignments.values_mut().next().unwrap();
        first.role = Some(m::RoleId(count));
        let mut alone = first.clone();
        alone.id = m::AssignmentId(first.id.0 + 1000);
        alone.resource = None;
        alone.role = Some(m::RoleId(1));
        alone.amounts.price_source = Some(m::PriceSource::Role);
        model.assignments.insert(alone.id, alone);
        proptest::prop_assert_eq!(model.validate(), []);
        let output = write(&model, options()).unwrap();
        let (mut imported, diagnostics) = crate::to_model(&output);
        proptest::prop_assert_eq!(diagnostics.omitted, std::collections::BTreeSet::new());
        let mut imported = imported.remove(0);
        normalize(&mut imported);
        proptest::prop_assert_eq!(imported, written(&model));
    }

    /// Expenses, with their cost accounts and categories, come back as
    /// written, and their tables appear only when used.
    #[test]
    fn expenses_round_trip(
        count in 1_u64..5,
        cents in proptest::collection::vec(0_i32..1_000_000_000, 5),
        accrual in proptest::option::of(proptest::sample::select(vec![
            m::Accrual::Start,
            m::Accrual::End,
            m::Accrual::Uniform,
        ])),
        filed in proptest::prelude::any::<bool>(),
        auto in proptest::prelude::any::<bool>(),
    ) {
        let mut model = full_model();
        let value = |i: usize| Some(f64::from(cents[i]) / 100.0);
        if filed {
            for n in 1..=2 {
                let id = m::CostAccountId(n);
                model.cost_accounts.insert(id, m::CostAccount {
                    id,
                    short_name: format!("A{n}"),
                    name: format!("Account {n}"),
                    parent: (n > 1).then_some(m::CostAccountId(1)),
                    sequence: Some(i64::try_from(n).unwrap() * 10),
                    description: String::new(),
                    source: None,
                });
            }
            let id = m::ExpenseCategoryId(7);
            model.expense_categories.insert(id, m::ExpenseCategory {
                id,
                name: "Travel".to_owned(),
                sequence: Some(1),
                source: None,
            });
        }
        let activities: Vec<_> = model.activities.keys().copied().collect();
        for n in 1..=count {
            let id = m::ExpenseId(n);
            let at = usize::try_from(n).unwrap() % activities.len();
            model.expenses.insert(id, m::Expense {
                id,
                activity: activities[at],
                project: model.project.id,
                name: format!("Expense {n}"),
                cost_account: filed.then_some(m::CostAccountId(2)),
                category: filed.then_some(m::ExpenseCategoryId(7)),
                accrual,
                budgeted_cost: value(0),
                actual_cost: value(1),
                remaining_cost: value(2),
                price_per_unit: value(3),
                budgeted_units: value(4),
                unit_of_measure: if filed { "each".to_owned() } else { String::new() },
                auto_compute_actuals: auto,
                po_number: format!("PO-{n}"),
                vendor: "Vendor".to_owned(),
                description: String::new(),
                source: None,
            });
        }
        proptest::prop_assert_eq!(model.validate(), []);
        let output = write(&model, options()).unwrap();
        proptest::prop_assert!(output.table("PROJCOST").is_some());
        proptest::prop_assert_eq!(output.table("ACCOUNT").is_some(), filed);
        proptest::prop_assert_eq!(output.table("COSTTYPE").is_some(), filed);
        let (mut imported, diagnostics) = crate::to_model(&output);
        proptest::prop_assert_eq!(diagnostics.omitted, std::collections::BTreeSet::new());
        let mut imported = imported.remove(0);
        normalize(&mut imported);
        proptest::prop_assert_eq!(imported, written(&model));
    }

    /// Hand-typed spreads come back as written, each part on its own,
    /// beside a curve on another assignment.
    #[test]
    fn typed_spreads_round_trip(
        parts in proptest::collection::vec(
            proptest::collection::vec((0_u32..2_000_000, 1_i64..100_000), 0..6),
            3,
        ),
        curved in proptest::prelude::any::<bool>(),
    ) {
        let mut model = full_model();
        let [planned, remaining, actual] = <[Vec<m::SpreadSegment>; 3]>::try_from(
            parts
                .iter()
                .map(|part| {
                    part.iter()
                        .map(|&(thousandths, minutes)| m::SpreadSegment {
                            units: f64::from(thousandths) / 1000.0,
                            work: m::Minutes::new(minutes),
                        })
                        .collect()
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let spread = m::TypedSpread { planned, remaining, actual };
        let typed = !(spread.planned.is_empty()
            && spread.remaining.is_empty()
            && spread.actual.is_empty());
        let mut assignments = model.assignments.values_mut();
        assignments.next().unwrap().typed_spread = Some(spread);
        if curved {
            let id = m::ResourceCurveId(1);
            model.resource_curves.insert(id, m::ResourceCurve {
                id,
                name: "Linear".to_owned(),
                default: true,
                points: [5.0; m::CURVE_POINTS],
                source: None,
            });
            if let Some(other) = assignments.next() {
                other.curve = Some(id);
            }
        }
        proptest::prop_assert_eq!(model.validate(), []);
        let output = write(&model, options());
        if !typed {
            // A typed spread with nothing in it would read back as a missing
            // curve, so it's refused.
            proptest::prop_assert!(
                matches!(output, Err(FreshError::Unrepresentable { field: "curv_id", .. })),
                "{output:?}"
            );
            return Ok(());
        }
        let (mut imported, diagnostics) = crate::to_model(&output.unwrap());
        proptest::prop_assert_eq!(diagnostics.omitted, std::collections::BTreeSet::new());
        let mut imported = imported.remove(0);
        normalize(&mut imported);
        proptest::prop_assert_eq!(imported, written(&model));
    }

    /// Resource curves and an assignment's curve come back as written,
    /// and the curve table appears only when there are curves.
    #[test]
    fn curves_round_trip(
        count in 0_u64..4,
        halves in proptest::collection::vec(0_u32..400, m::CURVE_POINTS),
        default in proptest::prelude::any::<bool>(),
        curved in proptest::prelude::any::<bool>(),
    ) {
        let mut model = full_model();
        let mut points = [0.0; m::CURVE_POINTS];
        for (point, half) in points.iter_mut().zip(&halves) {
            *point = f64::from(*half) / 2.0;
        }
        for n in 1..=count {
            let id = m::ResourceCurveId(n);
            model.resource_curves.insert(id, m::ResourceCurve {
                id,
                name: format!("Curve {n}"),
                default,
                points,
                source: None,
            });
        }
        if curved && count > 0 {
            model.assignments.values_mut().next().unwrap().curve = Some(m::ResourceCurveId(count));
        }
        proptest::prop_assert_eq!(model.validate(), []);
        let output = write(&model, options()).unwrap();
        proptest::prop_assert_eq!(output.table("RSRCCURVDATA").is_some(), count > 0);
        let (mut imported, diagnostics) = crate::to_model(&output);
        proptest::prop_assert_eq!(diagnostics.omitted, std::collections::BTreeSet::new());
        let mut imported = imported.remove(0);
        normalize(&mut imported);
        proptest::prop_assert_eq!(imported, written(&model));
    }
}

/// The settings new rows take are what most rows in the corpus hold, and each
/// quantity or cost they set to zero is zero in some corpus row (TF-034,
/// TF-036). Reads the files `corpus/manifest.toml` lists, leaving
/// out other tools' test files (tagged `codec-only`), which are no evidence of
/// what real files hold. Runs only with `TOTALFLOAT_CORPUS`.
#[test]
fn new_row_defaults_are_what_corpus_rows_hold() {
    use std::collections::BTreeMap;
    let Some(root) = std::env::var_os("TOTALFLOAT_CORPUS") else {
        eprintln!("TOTALFLOAT_CORPUS is not set; skipping the new-row defaults corpus test");
        return;
    };
    let root = std::path::PathBuf::from(root);
    let manifest: toml::Table = toml::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/manifest.toml"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut files = Vec::new();
    for entry in manifest["file"].as_array().unwrap() {
        let tagged = |tag: &str| {
            entry["tags"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t.as_str() == Some(tag))
        };
        let path = root
            .join("files")
            .join(entry["id"].as_str().unwrap())
            .join(entry["filename"].as_str().unwrap());
        let xer = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xer"));
        if xer && !tagged("codec-only") {
            files.push(path);
        }
    }
    assert_ne!(
        files.len(),
        0,
        "no evidence XER files in the corpus manifest"
    );
    let tables = ["TASK", "TASKRSRC", "PROJWBS", "RSRC", "ROLES"];
    let mut held: BTreeMap<(&str, &str), BTreeMap<String, usize>> = BTreeMap::new();
    for file in files {
        let Ok(xer) = raw::read(std::fs::read(&file).unwrap()) else {
            continue;
        };
        for table in tables {
            let Some(rows) = xer.table(table) else {
                continue;
            };
            for &(field, _) in crate::patch::new_row_defaults(table) {
                let Some(column) = rows.field_index(field) else {
                    continue;
                };
                for row in &rows.rows {
                    let cell = row.get(column).unwrap_or_default();
                    if !cell.is_empty() {
                        *held
                            .entry((table, field))
                            .or_default()
                            .entry(cell.to_owned())
                            .or_default() += 1;
                    }
                }
            }
        }
    }
    let same = |a: &str, b: &str| {
        a == b
            || matches!((a.parse::<f64>(), b.parse::<f64>()),
                (Ok(x), Ok(y)) if x.total_cmp(&y).is_eq())
    };
    let mut unbacked = Vec::new();
    for table in tables {
        for &(field, value) in crate::patch::new_row_defaults(table) {
            let counts = held.get(&(table, field));
            let count = |wanted: &str| {
                counts.map_or(0, |c| {
                    c.iter()
                        .filter(|(v, _)| same(v, wanted))
                        .map(|(_, n)| n)
                        .sum::<usize>()
                })
            };
            let backed = if same(value, "0") {
                count(value) > 0
            } else {
                counts.is_some_and(|c| c.keys().all(|other| count(other) <= count(value)))
            };
            if !backed {
                unbacked.push(format!("{table}.{field} = {value}"));
            }
        }
    }
    assert!(
        unbacked.is_empty(),
        "new-row values the corpus doesn't back: {unbacked:?}"
    );
}
