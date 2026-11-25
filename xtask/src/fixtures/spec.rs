use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Spec {
    pub schema: u32,
    pub description: String,
    pub data_date: String,
    pub export_date: String,
    pub seed: u64,
    pub calendars: Vec<Calendar>,
    pub activities: Vec<Activity>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    pub options: Options,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Calendar {
    pub id: u64,
    pub name: String,
    /// Sunday=0, Monday=1, ... Saturday=6.
    pub weekdays: Vec<usize>,
    pub intervals: Vec<String>,
    #[serde(default)]
    pub exceptions: BTreeMap<String, Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Activity {
    pub id: u64,
    pub name: String,
    pub calendar: u64,
    pub kind: String,
    pub duration: i64,
    pub remaining: Option<i64>,
    pub actual_start: Option<String>,
    pub actual_finish: Option<String>,
    pub suspend: Option<String>,
    pub resume: Option<String>,
    pub expected_finish: Option<String>,
    pub constraint: Option<Constraint>,
    pub secondary_constraint: Option<Constraint>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Constraint {
    pub kind: String,
    pub date: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Relationship {
    pub predecessor: u64,
    pub successor: u64,
    pub kind: String,
    pub lag: i64,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct Options {
    pub float_type: Option<String>,
    pub lag_calendar: Option<String>,
    pub out_of_sequence: Option<String>,
    pub make_open_ended_critical: Option<bool>,
    pub float_from_project_finish: Option<bool>,
    pub use_expected_finish: Option<bool>,
    pub ss_lag_from_early_start: Option<bool>,
    pub set_planned_to_forecast: Option<bool>,
}
