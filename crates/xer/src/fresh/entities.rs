//! Model entities into fresh rows. Unmodeled optional columns remain empty.

use std::fmt::Write as _;

use super::{Cells, FreshError, Writer, enums, fields, flag, hours, m, patch, scalar, text};

fn strings(values: &[(&'static str, &str)]) -> Cells {
    values
        .iter()
        .map(|(name, value)| (*name, (*value).to_owned()))
        .collect()
}

pub(super) fn defaults(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    writer.row(
        "CURRTYPE",
        1,
        strings(&[
            ("curr_id", "1"),
            ("decimal_digit_cnt", "2"),
            ("curr_symbol", "$"),
            ("decimal_symbol", "."),
            ("digit_group_symbol", ","),
            ("pos_curr_fmt_type", "$1.1"),
            ("neg_curr_fmt_type", "($1.1)"),
            ("curr_type", "US Dollar"),
            ("curr_short_name", "USD"),
            ("group_digit_cnt", "3"),
            ("base_exch_rate", "1"),
        ]),
    )?;
    for node in schedule.obs.values() {
        writer.row("OBS", node.id.0, obs_cells(node))?;
    }
    for topic in schedule.notebook_topics.values() {
        writer.row(
            "MEMOTYPE",
            topic.id.0,
            Cells::from([
                ("memo_type_id", topic.id.to_string()),
                ("seq_num", topic.sequence.to_string()),
                ("eps_flag", flag(topic.for_eps)),
                ("proj_flag", flag(topic.for_projects)),
                ("wbs_flag", flag(topic.for_wbs)),
                ("task_flag", flag(topic.for_activities)),
                ("memo_type", topic.name.clone()),
            ]),
        )?;
    }
    Ok(())
}

fn obs_cells(node: &m::Obs) -> Cells {
    Cells::from([
        ("obs_id", node.id.to_string()),
        ("parent_obs_id", text(node.parent)),
        ("seq_num", node.sequence.to_string()),
        ("obs_name", node.name.clone()),
        ("obs_descr", node.description.clone()),
    ])
}

pub(super) fn project(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    let project = &schedule.project;
    let options = &project.options;
    let mut row = fields("PROJECT", &patch::PROJECT_FIELDS, project)?;
    row.insert("proj_short_name", project.short_name.clone());
    row.insert("clndr_id", text(project.default_calendar));
    row.insert("project_flag", flag(true));
    row.insert("export_flag", flag(true));
    row.insert("step_complete_flag", flag(project.percent_from_steps));
    row.insert("name_sep_char", ".".to_owned());
    row.insert("fy_start_month_num", "1".to_owned());
    row.insert("task_code_base", "1000".to_owned());
    row.insert("task_code_step", "10".to_owned());
    row.insert(
        "def_duration_type",
        enums::duration_code(m::DurationType::default()).to_owned(),
    );
    row.insert(
        "def_complete_pct_type",
        enums::percent_code(m::PercentCompleteType::default()).to_owned(),
    );
    row.insert(
        "def_task_type",
        enums::activity_code(m::ActivityType::default()).to_owned(),
    );
    let (critical, limit) = match options.critical {
        m::CriticalDefinition::LongestPath => ("CT_DrivPath", None),
        m::CriticalDefinition::TotalFloatAtMost(limit) => ("CT_TotFloat", Some(limit)),
    };
    row.insert("critical_path_type", critical.to_owned());
    row.insert(
        "critical_drtn_hr_cnt",
        hours(limit, "PROJECT", "critical_drtn_hr_cnt")?,
    );
    writer.row("PROJECT", project.id.0, row)?;
    let retained = options.out_of_sequence == m::OutOfSequenceMode::RetainedLogic;
    let override_progress = options.out_of_sequence == m::OutOfSequenceMode::ProgressOverride;
    let rows = Cells::from([
        ("schedoptions_id", "1".to_owned()),
        ("proj_id", project.id.to_string()),
        ("sched_retained_logic", flag(retained)),
        ("sched_progress_override", flag(override_progress)),
        (
            "sched_float_type",
            enums::float_type_code(options.float_type).to_owned(),
        ),
        (
            "sched_calendar_on_relationship_lag",
            enums::lag_calendar_code(options.lag_calendar).to_owned(),
        ),
        (
            "sched_open_critical_flag",
            flag(options.make_open_ended_critical),
        ),
        (
            "sched_lag_early_start_flag",
            flag(options.ss_lag_from_early_start),
        ),
        (
            "sched_use_expect_end_flag",
            flag(options.use_expected_finish),
        ),
        ("sched_outer_depend_type", "SD_Both".to_owned()),
        (
            "sched_use_project_end_date_for_float",
            flag(options.float_from_project_finish),
        ),
        (
            "sched_setplantoforecast",
            flag(options.set_planned_to_forecast),
        ),
        (
            "enable_multiple_longest_path_calc",
            flag(options.float_paths.enabled),
        ),
        (
            "use_total_float_multiple_longest_paths",
            flag(options.float_paths.mode == m::FloatPathMode::TotalFloat),
        ),
        (
            "limit_multiple_longest_path_calc",
            flag(options.float_paths.max_paths.is_some()),
        ),
        (
            "max_multiple_longest_path",
            text(options.float_paths.max_paths),
        ),
        (
            "key_activity_for_multiple_longest_paths",
            text(options.float_paths.end_activity),
        ),
    ]);
    writer.row("SCHEDOPTIONS", 1, rows)
}

fn shifts(intervals: &[m::WorkInterval]) -> String {
    let mut result = String::new();
    for (index, interval) in intervals.iter().enumerate() {
        // TF-011: midnight in the finish position means the end of the day.
        // The model retains minute 1440; the observed XER spelling is 00:00.
        let finish = if interval.finish.minutes() == 1440 {
            "00:00".to_owned()
        } else {
            interval.finish.to_string()
        };
        let _ = write!(result, "(0||{index}(s|{}|f|{finish})())", interval.start);
    }
    result
}

pub(crate) fn calendar_data(calendar: &m::Calendar) -> Result<String, FreshError> {
    let mut week = String::new();
    for (index, intervals) in calendar.work_week.0.iter().enumerate() {
        let _ = write!(week, "(0||{}()({}))", index + 1, shifts(intervals));
    }
    let epoch = m::Date::from_ymd(1899, 12, 30).ok_or(FreshError::Unsupported("calendar epoch"))?;
    let mut exceptions = String::new();
    for (index, (date, intervals)) in calendar.exceptions.iter().enumerate() {
        let _ = write!(
            exceptions,
            "(0||{index}(d|{})({}))",
            date.days_since(epoch),
            shifts(intervals)
        );
    }
    Ok(format!(
        "(0||CalendarData()((0||DaysOfWeek()({week}))(0||Exceptions()({exceptions}))))"
    ))
}

/// Each resource's prices over time.
fn rates(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    for resource in schedule.resources.values() {
        for rate in &resource.rates {
            const PRICES: [&str; 5] = [
                "cost_per_qty",
                "cost_per_qty2",
                "cost_per_qty3",
                "cost_per_qty4",
                "cost_per_qty5",
            ];
            let price = |n: usize| {
                patch::decimal(rate.prices[n]).ok_or_else(|| scalar("RSRCRATE", PRICES[n]))
            };
            let cells = Cells::from([
                ("rsrc_rate_id", rate.id.to_string()),
                ("rsrc_id", resource.id.to_string()),
                ("start_date", text(rate.effective)),
                ("cost_per_qty", price(0)?),
                ("cost_per_qty2", price(1)?),
                ("cost_per_qty3", price(2)?),
                ("cost_per_qty4", price(3)?),
                ("cost_per_qty5", price(4)?),
                (
                    "max_qty_per_hr",
                    patch::decimal(rate.max_units_per_hour)
                        .ok_or_else(|| scalar("RSRCRATE", "max_qty_per_hr"))?,
                ),
            ]);
            writer.row("RSRCRATE", rate.id.0, cells)?;
        }
    }
    Ok(())
}

/// The roles with their prices over time, and the roles each resource fills.
fn roles(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    for role in schedule.roles.values() {
        writer.row(
            "ROLES",
            role.id.0,
            Cells::from([
                ("role_id", role.id.to_string()),
                ("parent_role_id", text(role.parent)),
                ("role_name", role.name.clone()),
                ("role_short_name", role.short_name.clone()),
                (
                    "def_cost_qty_link_flag",
                    flag(role.default_cost_follows_units),
                ),
            ]),
        )?;
        for rate in &role.rates {
            const PRICES: [&str; 5] = [
                "cost_per_qty",
                "cost_per_qty2",
                "cost_per_qty3",
                "cost_per_qty4",
                "cost_per_qty5",
            ];
            let mut cells = Cells::from([
                ("role_rate_id", rate.id.to_string()),
                ("role_id", role.id.to_string()),
                ("start_date", text(rate.effective)),
                (
                    "max_qty_per_hr",
                    patch::decimal(rate.max_units_per_hour)
                        .ok_or_else(|| scalar("ROLERATE", "max_qty_per_hr"))?,
                ),
            ]);
            for (n, field) in PRICES.into_iter().enumerate() {
                let price =
                    patch::decimal(rate.prices[n]).ok_or_else(|| scalar("ROLERATE", field))?;
                cells.insert(field, price);
            }
            writer.row("ROLERATE", rate.id.0, cells)?;
        }
    }
    for resource in schedule.resources.values() {
        for filled in &resource.roles {
            let role = schedule.roles.get(&filled.role);
            writer.row(
                "RSRCROLE",
                filled.id.0,
                Cells::from([
                    ("rsrc_id", resource.id.to_string()),
                    ("role_id", filled.role.to_string()),
                    ("skill_level", text(filled.skill_level)),
                    (
                        "role_short_name",
                        role.map(|r| r.short_name.clone()).unwrap_or_default(),
                    ),
                    (
                        "role_name",
                        role.map(|r| r.name.clone()).unwrap_or_default(),
                    ),
                    ("rsrc_short_name", resource.short_name.clone()),
                    ("rsrc_name", resource.name.clone()),
                    ("rsrc_type", enums::resource_code(resource.kind).to_owned()),
                    ("rsrc_role_id", filled.id.to_string()),
                ]),
            )?;
        }
    }
    Ok(())
}

/// Expense categories and cost accounts.
fn cost_dictionaries(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    for category in schedule.expense_categories.values() {
        writer.row(
            "COSTTYPE",
            category.id.0,
            Cells::from([
                ("cost_type_id", category.id.to_string()),
                ("seq_num", text(category.sequence)),
                ("cost_type", category.name.clone()),
            ]),
        )?;
    }
    for account in schedule.cost_accounts.values() {
        writer.row(
            "ACCOUNT",
            account.id.0,
            Cells::from([
                ("acct_id", account.id.to_string()),
                ("parent_acct_id", text(account.parent)),
                ("acct_seq_num", text(account.sequence)),
                ("acct_name", account.name.clone()),
                ("acct_short_name", account.short_name.clone()),
                ("acct_descr", account.description.clone()),
            ]),
        )?;
    }
    Ok(())
}

/// `RSRCCURVDATA.pct_usage_0` to `pct_usage_20`.
const CURVE_POINTS: [&str; m::CURVE_POINTS] = [
    "pct_usage_0",
    "pct_usage_1",
    "pct_usage_2",
    "pct_usage_3",
    "pct_usage_4",
    "pct_usage_5",
    "pct_usage_6",
    "pct_usage_7",
    "pct_usage_8",
    "pct_usage_9",
    "pct_usage_10",
    "pct_usage_11",
    "pct_usage_12",
    "pct_usage_13",
    "pct_usage_14",
    "pct_usage_15",
    "pct_usage_16",
    "pct_usage_17",
    "pct_usage_18",
    "pct_usage_19",
    "pct_usage_20",
];

/// Resource curves.
fn curves(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    for curve in schedule.resource_curves.values() {
        let mut cells = Cells::from([
            ("curv_id", curve.id.to_string()),
            ("curv_name", curve.name.clone()),
            ("default_flag", flag(curve.default)),
        ]);
        for (field, point) in CURVE_POINTS.into_iter().zip(curve.points) {
            let point = patch::decimal(Some(point)).ok_or_else(|| scalar("RSRCCURVDATA", field))?;
            cells.insert(field, point);
        }
        writer.row("RSRCCURVDATA", curve.id.0, cells)?;
    }
    Ok(())
}

/// Project code types and values, and the project's codes.
fn project_codes(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    for kind in schedule.project_code_types.values() {
        writer.row(
            "PCATTYPE",
            kind.id.0,
            Cells::from([
                ("proj_catg_type_id", kind.id.to_string()),
                ("proj_catg_type", kind.name.clone()),
                ("proj_catg_short_len", text(kind.max_length)),
                ("seq_num", kind.sequence.to_string()),
            ]),
        )?;
    }
    for code in schedule.project_codes.values() {
        writer.row(
            "PCATVAL",
            code.id.0,
            Cells::from([
                ("proj_catg_id", code.id.to_string()),
                ("proj_catg_type_id", code.code_type.to_string()),
                ("parent_proj_catg_id", text(code.parent)),
                ("proj_catg_short_name", code.value.clone()),
                ("proj_catg_name", code.description.clone()),
                ("seq_num", code.sequence.to_string()),
            ]),
        )?;
    }
    for (kind, code) in &schedule.project.codes {
        writer.row(
            "PROJPCAT",
            schedule.project.id.0,
            Cells::from([
                ("proj_id", schedule.project.id.to_string()),
                ("proj_catg_type_id", kind.to_string()),
                ("proj_catg_id", code.to_string()),
            ]),
        )?;
    }
    Ok(())
}

pub(super) fn dictionaries(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    for calendar in schedule.calendars.values() {
        let kind = match calendar.kind {
            m::CalendarType::Global => "CA_Base",
            m::CalendarType::Project => "CA_Project",
            m::CalendarType::Resource => "CA_Rsrc",
        };
        let mut row = Cells::from([
            ("clndr_id", calendar.id.to_string()),
            ("clndr_name", calendar.name.clone()),
            ("clndr_type", kind.to_owned()),
            ("proj_id", text(calendar.project)),
            ("base_clndr_id", text(calendar.base)),
            ("default_flag", flag(calendar.is_default)),
            ("clndr_data", calendar_data(calendar)?),
        ]);
        for (field, value) in [
            ("day_hr_cnt", calendar.hours_per_period.day),
            ("week_hr_cnt", calendar.hours_per_period.week),
            ("month_hr_cnt", calendar.hours_per_period.month),
            ("year_hr_cnt", calendar.hours_per_period.year),
        ] {
            row.insert(field, hours(value, "CALENDAR", field)?);
        }
        writer.row("CALENDAR", calendar.id.0, row)?;
    }
    for resource in schedule.resources.values() {
        let cells = Cells::from([
            ("rsrc_id", resource.id.to_string()),
            ("rsrc_short_name", resource.short_name.clone()),
            ("rsrc_name", resource.name.clone()),
            ("rsrc_type", enums::resource_code(resource.kind).to_owned()),
            ("clndr_id", text(resource.calendar)),
            ("parent_rsrc_id", text(resource.parent)),
            (
                "def_qty_per_hr",
                patch::decimal(resource.default_units_per_hour)
                    .ok_or_else(|| scalar("RSRC", "def_qty_per_hr"))?,
            ),
            (
                "def_cost_qty_link_flag",
                flag(resource.default_cost_follows_units),
            ),
            ("curr_id", "1".to_owned()),
        ]);
        writer.row("RSRC", resource.id.0, cells)?;
    }
    rates(writer, schedule)?;
    roles(writer, schedule)?;
    cost_dictionaries(writer, schedule)?;
    curves(writer, schedule)?;
    for kind in schedule.activity_code_types.values() {
        writer.row(
            "ACTVTYPE",
            kind.id.0,
            Cells::from([
                ("actv_code_type_id", kind.id.to_string()),
                ("actv_code_type", kind.name.clone()),
                (
                    "actv_code_type_scope",
                    enums::scope_code(kind.scope).to_owned(),
                ),
                ("proj_id", text(kind.project)),
                ("actv_short_len", text(kind.max_length)),
                ("seq_num", kind.sequence.to_string()),
            ]),
        )?;
    }
    project_codes(writer, schedule)?;
    for code in schedule.activity_codes.values() {
        writer.row(
            "ACTVCODE",
            code.id.0,
            Cells::from([
                ("actv_code_id", code.id.to_string()),
                ("actv_code_type_id", code.code_type.to_string()),
                ("parent_actv_code_id", text(code.parent)),
                ("short_name", code.value.clone()),
                ("actv_code_name", code.description.clone()),
                ("seq_num", code.sequence.to_string()),
            ]),
        )?;
    }
    Ok(())
}

/// An activity's steps.
fn steps(
    writer: &mut Writer,
    schedule: &m::Schedule,
    activity: &m::Activity,
) -> Result<(), FreshError> {
    for step in activity.steps.values() {
        let number = |value, field| patch::decimal(value).ok_or_else(|| scalar("TASKPROC", field));
        writer.row(
            "TASKPROC",
            step.id.0,
            Cells::from([
                ("proc_id", step.id.to_string()),
                ("task_id", activity.id.to_string()),
                ("proj_id", schedule.project.id.to_string()),
                ("seq_num", step.sequence.to_string()),
                ("proc_name", step.name.clone()),
                ("complete_flag", flag(step.completed)),
                ("proc_wt", number(step.weight, "proc_wt")?),
                ("complete_pct", number(step.percent, "complete_pct")?),
                (
                    "proc_descr",
                    crate::mapping::notes::encode(&step.description),
                ),
            ]),
        )?;
    }
    Ok(())
}

pub(super) fn network(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    for node in schedule.wbs.values() {
        let mut cells = fields("PROJWBS", &patch::WBS_EARNED_VALUE, node)?;
        cells.retain(|_, value| !value.is_empty());
        cells.extend(Cells::from([
            ("wbs_id", node.id.to_string()),
            ("proj_id", node.project.to_string()),
            ("parent_wbs_id", text(node.parent)),
            ("wbs_short_name", node.code.clone()),
            ("wbs_name", node.name.clone()),
            ("seq_num", node.sequence.to_string()),
            ("proj_node_flag", flag(node.is_project_node)),
            ("obs_id", text(node.obs)),
            ("status_code", "WS_Open".to_owned()),
        ]));
        writer.row("PROJWBS", node.id.0, cells)?;
        for (topic, note) in &node.notes {
            writer.row(
                "WBSMEMO",
                note.id,
                Cells::from([
                    ("wbs_memo_id", note.id.to_string()),
                    ("proj_id", node.project.to_string()),
                    ("memo_type_id", topic.to_string()),
                    ("wbs_id", node.id.to_string()),
                    ("wbs_memo", crate::mapping::notes::encode(&note.html)),
                ]),
            )?;
        }
    }
    for activity in schedule.activities.values() {
        writer.row(
            "TASK",
            activity.id.0,
            fields("TASK", &patch::TASK_FIELDS, activity)?,
        )?;
        steps(writer, schedule, activity)?;
        for (topic, note) in &activity.notes {
            writer.row(
                "TASKMEMO",
                note.id,
                Cells::from([
                    ("memo_id", note.id.to_string()),
                    ("task_id", activity.id.to_string()),
                    ("memo_type_id", topic.to_string()),
                    ("proj_id", schedule.project.id.to_string()),
                    ("task_memo", crate::mapping::notes::encode(&note.html)),
                ]),
            )?;
        }
        for (kind, code) in &activity.codes {
            writer.row(
                "TASKACTV",
                activity.id.0,
                Cells::from([
                    ("task_id", activity.id.to_string()),
                    ("proj_id", schedule.project.id.to_string()),
                    ("actv_code_type_id", kind.to_string()),
                    ("actv_code_id", code.to_string()),
                ]),
            )?;
        }
    }
    for relationship in schedule.relationships.values() {
        writer.row(
            "TASKPRED",
            relationship.id.0,
            fields("TASKPRED", &patch::PRED_FIELDS, relationship)?,
        )?;
    }
    for expense in schedule.expenses.values() {
        writer.row(
            "PROJCOST",
            expense.id.0,
            fields("PROJCOST", &patch::EXPENSE_FIELDS, expense)?,
        )?;
    }
    for assignment in schedule.assignments.values() {
        let mut cells = fields("TASKRSRC", &patch::ASSIGNMENT_FIELDS, assignment)?;
        cells.extend([
            ("taskrsrc_id", assignment.id.to_string()),
            ("task_id", assignment.activity.to_string()),
            ("rsrc_id", text(assignment.resource)),
            ("role_id", text(assignment.role)),
            ("proj_id", assignment.project.to_string()),
        ]);
        writer.row("TASKRSRC", assignment.id.0, cells)?;
    }
    Ok(())
}

pub(crate) fn udf_type(kind: &m::UdfDataType) -> Result<&'static str, FreshError> {
    Ok(match kind {
        m::UdfDataType::Text => "FT_TEXT",
        m::UdfDataType::StartDate => "FT_START_DATE",
        m::UdfDataType::FinishDate => "FT_END_DATE",
        m::UdfDataType::Cost => "FT_MONEY",
        m::UdfDataType::Number => "FT_FLOAT_2_DECIMALS",
        m::UdfDataType::Integer => "FT_INT",
        m::UdfDataType::Indicator => "FT_STATICTYPE",
        m::UdfDataType::Other(_) => {
            return Err(FreshError::Unsupported("an unobserved UDF data type"));
        }
        m::UdfDataType::Code => {
            return Err(FreshError::Unsupported(
                "an unobserved UDF code-list encoding",
            ));
        }
    })
}

pub(crate) fn subject(subject: &m::UdfSubject) -> Result<&'static str, FreshError> {
    Ok(match subject {
        m::UdfSubject::Project => "PROJECT",
        m::UdfSubject::Wbs => "PROJWBS",
        m::UdfSubject::Activity => "TASK",
        m::UdfSubject::Resource => "RSRC",
        m::UdfSubject::Assignment => "TASKRSRC",
        m::UdfSubject::Other(_) => {
            return Err(FreshError::Unsupported("an unobserved UDF subject"));
        }
    })
}

fn values(
    writer: &mut Writer,
    project: m::ProjectId,
    id: u64,
    values: &std::collections::BTreeMap<m::UdfTypeId, m::UdfValue>,
) -> Result<(), FreshError> {
    for (kind, value) in values {
        let mut row = Cells::from([
            ("udf_type_id", kind.to_string()),
            ("fk_id", id.to_string()),
            ("proj_id", project.to_string()),
        ]);
        match value {
            m::UdfValue::Text(text) => {
                row.insert("udf_text", text.clone());
            }
            m::UdfValue::Number(number) => {
                row.insert("udf_number", number.to_string());
            }
            m::UdfValue::Date(date) => {
                row.insert("udf_date", date.to_string());
            }
            m::UdfValue::Code(code) => {
                row.insert("udf_code_id", code.to_string());
            }
        }
        writer.row("UDFVALUE", id, row)?;
    }
    Ok(())
}

pub(super) fn udfs(writer: &mut Writer, schedule: &m::Schedule) -> Result<(), FreshError> {
    for kind in schedule.udf_types.values() {
        writer.row(
            "UDFTYPE",
            kind.id.0,
            Cells::from([
                ("udf_type_id", kind.id.to_string()),
                ("table_name", subject(&kind.subject)?.to_owned()),
                ("udf_type_label", kind.label.clone()),
                ("logical_data_type", udf_type(&kind.data_type)?.to_owned()),
                ("udf_type_name", format!("UDF_{}", kind.id)),
                ("export_flag", flag(true)),
            ]),
        )?;
    }
    let project = schedule.project.id;
    values(writer, project, project.0, &schedule.project.udfs)?;
    for node in schedule.wbs.values() {
        values(writer, project, node.id.0, &node.udfs)?;
    }
    for activity in schedule.activities.values() {
        values(writer, project, activity.id.0, &activity.udfs)?;
    }
    for resource in schedule.resources.values() {
        values(writer, project, resource.id.0, &resource.udfs)?;
    }
    for assignment in schedule.assignments.values() {
        values(writer, project, assignment.id.0, &assignment.udfs)?;
    }
    Ok(())
}
