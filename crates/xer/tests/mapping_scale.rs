//! `to_model` on a generated enterprise-sized file: 100 projects of 1,000
//! activities each. The digest pins the complete output (schedules and
//! diagnostics), so indexing work inside the mapper can't change behavior.
//! Run with `--release --nocapture` for the timing.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::fmt::{self, Write as _};
use std::time::Instant;
use totalfloat_xer::{raw, to_model};

const PROJECTS: u64 = 100;
const ACTIVITIES: u64 = 1_000;
const GROUPS: u64 = 50;
const RESOURCES: u64 = 200;
// Schedules hold resource curves, and assignments a
// curve, none here. Diagnostic counts were checked unchanged.
// Assignments hold a hand-typed spread, none here.
// Diagnostic counts were checked unchanged.
// Schedules hold project code types and values, and the
// project its codes, none here. Diagnostic counts were checked unchanged.
// Schedules hold the OBS, and WBS nodes a manager, none
// here. Diagnostic counts were checked unchanged.
// Schedules hold notebook topics, and activities and WBS
// nodes notes, none here. Diagnostic counts were checked unchanged.
// Activities hold steps and projects whether they measure
// by them, none here. Diagnostic counts were checked unchanged.
// Assignments hold a cost account, none here. Diagnostic
// counts were checked unchanged.
// Diagnostic messages cite rules (TF-008); counts were checked unchanged.
// Schedules no longer hold financial periods, a risk register or issues;
// counts were checked unchanged.
const OUTPUT_DIGEST: u64 = 0x8db9_77af_bedf_8bd1;

/// FNV-1a over everything written, so the output needn't be held in memory.
struct Digest(u64);

impl fmt::Write for Digest {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        Ok(())
    }
}

fn table(out: &mut String, name: &str, header: &[&str], rows: impl Iterator<Item = Vec<String>>) {
    writeln!(out, "%T\t{name}\n%F\t{}", header.join("\t")).unwrap();
    for row in rows {
        writeln!(out, "%R\t{}", row.join("\t")).unwrap();
    }
}

macro_rules! row {
    ($($cell:expr),* $(,)?) => { vec![$($cell.to_string()),*] };
}

fn task_id(project: u64, index: u64) -> u64 {
    project * 10_000 + index
}

fn resource_id(index: u64) -> u64 {
    5_000 + index % RESOURCES
}

fn tasks() -> impl Iterator<Item = (u64, u64)> {
    (1..=PROJECTS).flat_map(|p| (0..ACTIVITIES).map(move |i| (p, i)))
}

fn calendars(out: &mut String) {
    let week = include_str!("../../../fixtures/xer/model-mapping.xer")
        .lines()
        .find(|line| line.starts_with("%R\t10\t"))
        .and_then(|line| line.split('\t').nth(11))
        .unwrap();
    let global = [(10, ""), (11, "10")].into_iter().map(|(id, base)| {
        row![
            id, "Global", "CA_Base", "", base, "N", 8, 40, 160, 1920, week
        ]
    });
    let local = (1..=PROJECTS).map(|p| {
        row![
            1_000 + p,
            "Local",
            "CA_Project",
            p,
            10,
            "N",
            8,
            40,
            160,
            1920,
            week
        ]
    });
    table(
        out,
        "CALENDAR",
        &[
            "clndr_id",
            "clndr_name",
            "clndr_type",
            "proj_id",
            "base_clndr_id",
            "default_flag",
            "day_hr_cnt",
            "week_hr_cnt",
            "month_hr_cnt",
            "year_hr_cnt",
            "clndr_data",
        ],
        global.chain(local),
    );
}

fn structure(out: &mut String) {
    table(
        out,
        "PROJECT",
        &["proj_id", "proj_short_name", "last_recalc_date", "clndr_id"],
        (1..=PROJECTS).map(|p| row![p, format!("P{p}"), "2026-01-05 08:00", 10]),
    );
    table(
        out,
        "PROJWBS",
        &["wbs_id", "proj_id", "parent_wbs_id", "wbs_short_name"],
        (1..=PROJECTS).flat_map(|p| {
            [
                row![p * 100, p, 9_999, "ROOT"],
                row![p * 100 + 1, p, p * 100, "WORK"],
            ]
        }),
    );
    table(
        out,
        "TASK",
        &[
            "task_id",
            "proj_id",
            "wbs_id",
            "clndr_id",
            "rsrc_id",
            "task_code",
            "task_type",
            "duration_type",
            "complete_pct_type",
            "status_code",
            "target_drtn_hr_cnt",
            "remain_drtn_hr_cnt",
        ],
        tasks().map(|(p, i)| {
            // Every seventh activity names a responsible resource with no assignment.
            let responsible = if i % 7 == 0 {
                resource_id(i).to_string()
            } else {
                String::new()
            };
            // One project references a calendar with no effective model.
            let calendar = match (p, i % 3) {
                (7, 0) => 9,
                (_, 0) => 1_000 + p,
                (_, 1) => 11,
                _ => 10,
            };
            row![
                task_id(p, i),
                p,
                p * 100 + 1,
                calendar,
                responsible,
                format!("A{i}"),
                "TT_Task",
                "DT_FixedDrtn",
                "CP_Drtn",
                "TK_NotStart",
                8,
                8,
            ]
        }),
    );
    table(
        out,
        "TASKPRED",
        &[
            "task_pred_id",
            "proj_id",
            "task_id",
            "pred_proj_id",
            "pred_task_id",
            "pred_type",
            "lag_hr_cnt",
        ],
        tasks().filter(|(_, i)| *i > 0).map(|(p, i)| {
            // Every hundredth relationship crosses into the previous project.
            let pred = if i % 100 == 0 && p > 1 { p - 1 } else { p };
            row![
                task_id(p, i),
                p,
                task_id(p, i),
                pred,
                task_id(pred, i - 1),
                "PR_FS",
                0
            ]
        }),
    );
}

fn resources(out: &mut String) {
    let groups = (0..GROUPS).map(|g| row![4_000 + g, format!("G{g}"), "RT_Equip", "", ""]);
    let people = (0..RESOURCES).map(|r| {
        row![
            5_000 + r,
            format!("R{r}"),
            "RT_Labor",
            11,
            4_000 + r % GROUPS
        ]
    });
    table(
        out,
        "RSRC",
        &[
            "rsrc_id",
            "rsrc_short_name",
            "rsrc_type",
            "clndr_id",
            "parent_rsrc_id",
        ],
        groups.chain(people),
    );
    table(
        out,
        "TASKRSRC",
        &["taskrsrc_id", "proj_id", "task_id", "rsrc_id"],
        tasks()
            .filter(|(_, i)| i % 10 == 0)
            .map(|(p, i)| row![task_id(p, i), p, task_id(p, i), resource_id(i + p)]),
    );
}

fn codes(out: &mut String) {
    let global = (1..=5).map(|t| row![t, "Global", "AS_Global", "", 10, t]);
    // Odd projects own a project code type; even ones get theirs through use.
    let local = (1..=PROJECTS).map(|p| row![100 + p, "Local", "AS_Project", p, 10, 1]);
    table(
        out,
        "ACTVTYPE",
        &[
            "actv_code_type_id",
            "actv_code_type",
            "actv_code_type_scope",
            "proj_id",
            "actv_short_len",
            "seq_num",
        ],
        global.chain(local),
    );
    table(
        out,
        "ACTVCODE",
        &[
            "actv_code_id",
            "actv_code_type_id",
            "short_name",
            "actv_code_name",
            "seq_num",
        ],
        (1..=5)
            .chain((1..=PROJECTS).map(|p| 100 + p))
            .flat_map(|t| (0..3).map(move |c| row![t * 10 + c, t, format!("C{c}"), "Value", c])),
    );
    table(
        out,
        "TASKACTV",
        &["proj_id", "task_id", "actv_code_type_id", "actv_code_id"],
        tasks().filter(|(_, i)| i % 3 == 0).map(|(p, i)| {
            // Project 3 also uses project 4's type, so use selects it as well.
            let kind = match (p, i % 2) {
                (3, 0) => 104,
                (_, 0) => 100 + p,
                _ => 1 + i % 5,
            };
            row![p, task_id(p, i), kind, kind * 10 + i % 3]
        }),
    );
}

fn udfs(out: &mut String) {
    table(
        out,
        "UDFTYPE",
        &[
            "udf_type_id",
            "table_name",
            "udf_type_label",
            "logical_data_type",
        ],
        [
            row![901, "PROJECT", "Project", "FT_TEXT"],
            row![902, "TASK", "Task", "FT_FLOAT_2_DECIMALS"],
            row![903, "PROJWBS", "Wbs", "FT_START_DATE"],
            row![904, "RSRC", "Resource", "FT_MONEY"],
            row![905, "TASKRSRC", "Assignment", "FT_TEXT"],
        ]
        .into_iter(),
    );
    let projects = (1..=PROJECTS).flat_map(|p| {
        [
            row![901, p, p, "note", "", ""],
            row![903, p * 100 + 1, p, "", "", "2026-01-05 08:00"],
        ]
    });
    let activities = tasks().filter(|(_, i)| i % 5 == 0).flat_map(|(p, i)| {
        [
            row![902, task_id(p, i), p, "", 1.25, ""],
            row![905, task_id(p, i), p, "assignment", "", ""],
        ]
    });
    // No project on the row: every schedule holding the target gets the value,
    // and unknown targets are reported.
    let shared = (0..20).map(|r| row![904, 5_000 + r, "", "", 2.5, ""]);
    let missing = [row![902, 1, "", "", 1, ""], row![902, 2, 1, "", 1, ""]];
    table(
        out,
        "UDFVALUE",
        &[
            "udf_type_id",
            "fk_id",
            "proj_id",
            "udf_text",
            "udf_number",
            "udf_date",
        ],
        projects.chain(activities).chain(shared).chain(missing),
    );
}

fn generated() -> String {
    let mut out = String::from("ERMHDR\t23.12\t2026-01-01\tProject\tsynthetic\n");
    calendars(&mut out);
    structure(&mut out);
    resources(&mut out);
    codes(&mut out);
    udfs(&mut out);
    out.push_str("%E\n");
    out
}

#[test]
fn enterprise_sized_file_maps_to_the_pinned_output() {
    let raw = raw::read(generated().as_bytes()).unwrap();
    let start = Instant::now();
    let (schedules, report) = to_model(&raw);
    println!("to_model: {:?}", start.elapsed());

    assert_eq!(schedules.len(), 100);
    assert!(schedules.iter().all(|s| s.activities.len() == 1_000));
    assert!(!schedules[0].resources.is_empty());
    assert!(!schedules[0].activity_codes.is_empty());
    assert_eq!(report.skipped_rows, 0);

    let mut digest = Digest(0xcbf2_9ce4_8422_2325);
    write!(digest, "{schedules:?}{report:?}").unwrap();
    println!("digest: {:#x}, report: {:?}", digest.0, report.counts);
    assert_eq!(digest.0, OUTPUT_DIGEST);
}
