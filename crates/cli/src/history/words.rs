//! An edit in plain words, for the title of a change recorded without a
//! reason: "A6 duration", "Added link A3 to A5". The log, compare lists,
//! revert prompts and merge dialog all show these, in the app and on the
//! command line, so they never name an edit by its command (`set_duration`).

use serde_json::Value;

/// One edit, as stored in a changeset's commands, in plain words.
#[allow(clippy::too_many_lines, reason = "one short arm per kind of edit")]
pub fn edit_words(edit: &Value) -> String {
    let text = |key: &str| edit[key].as_str().unwrap_or("?");
    let has = |key: &str| edit[key].as_str().is_some();
    let activity = || text("activity");
    let link = || format!("{} to {}", text("predecessor"), text("successor"));
    // What a field or note belongs to: an activity, an activity's resource, a WBS node or a resource.
    let owner = || {
        if has("activity") && has("resource") {
            format!("{}'s {}", activity(), text("resource"))
        } else if has("activity") {
            activity().to_owned()
        } else if has("wbs") {
            format!("WBS {}", text("wbs"))
        } else if has("resource") {
            format!("resource {}", text("resource"))
        } else {
            "Project".to_owned()
        }
    };
    let op = edit["op"].as_str().unwrap_or("edit");
    match op {
        "set_duration" => format!("{} duration", activity()),
        "set_constraint" => format!("{} constraint", activity()),
        "clear_constraint" => format!("{} constraint cleared", activity()),
        "set_calendar" => format!("{} calendar", activity()),
        "set_name" => format!("{} name", activity()),
        "set_type" => format!("{} type", activity()),
        "set_expected_finish" => format!("{} expected finish", activity()),
        "set_progress" => format!("{} progress", activity()),
        "clear_progress" => format!("{} progress cleared", activity()),
        "set_suspend" => format!("{} suspended", activity()),
        "clear_suspend" => format!("{} suspension cleared", activity()),
        "add_activity" => format!("Added {}", text("code")),
        "delete_activity" => format!("Deleted {}", activity()),
        "set_wbs" => format!("Moved {} to WBS {}", activity(), text("wbs")),
        "add_link" => format!("Added link {}", link()),
        "remove_link" => format!("Removed link {}", link()),
        "change_link" => format!("Changed link {}", link()),
        "add_wbs" => format!("Added WBS {}", text("code")),
        "rename_wbs" => format!("Renamed WBS {}", text("wbs")),
        "move_wbs" => format!("Moved WBS {} under {}", text("wbs"), text("parent")),
        "set_wbs_obs" => format!("WBS {} responsible OBS", text("wbs")),
        "add_obs" => format!("Added OBS {}", text("obs")),
        "add_resource" => format!("Added resource {}", text("resource")),
        "set_resource_limit" => format!("Resource {} limit", text("resource")),
        "assign_resource" => format!("Assigned {} to {}", text("resource"), activity()),
        "unassign_resource" => format!("Unassigned {} from {}", text("resource"), activity()),
        "set_assignment_units" => format!("{} units of {}", activity(), text("resource")),
        "set_curve" => format!("{} curve of {}", activity(), text("resource")),
        "set_spread" => format!("{} spread of {}", activity(), text("resource")),
        "add_role" => format!("Added role {}", text("role")),
        "set_role_limit" => format!("Role {} limit", text("role")),
        "assign_role" => format!("Assigned role {} to {}", text("role"), activity()),
        "unassign_role" => format!("Unassigned role {} from {}", text("role"), activity()),
        "set_cost_account" => format!("{} cost account", activity()),
        "add_expense" => format!("Added expense {} to {}", text("expense"), activity()),
        "set_expense" => format!("{} expense {}", activity(), text("expense")),
        "delete_expense" => format!("Deleted expense {} from {}", text("expense"), activity()),
        "set_code" => format!("{} code {}", activity(), text("code_type")),
        "clear_code" => format!("{} code {} cleared", activity(), text("code_type")),
        "add_code_type" => format!("Added code type {}", text("name")),
        "add_code_value" => format!("Added code {} {}", text("code_type"), text("value")),
        "set_project_code" => format!("Project code {}", text("code_type")),
        "clear_project_code" => format!("Project code {} cleared", text("code_type")),
        "add_project_code_type" => format!("Added project code type {}", text("name")),
        "add_project_code_value" => {
            format!("Added project code {} {}", text("code_type"), text("value"))
        }
        "set_udf" => format!("{} {}", owner(), text("field")),
        "clear_udf" => format!("{} {} cleared", owner(), text("field")),
        "add_udf_type" => format!("Added field {}", text("field")),
        "add_step" => format!("Added step {} to {}", text("step"), activity()),
        "set_step" => format!("{} step {}", activity(), text("step")),
        "complete_step" => format!("Completed step {} of {}", text("step"), activity()),
        "delete_step" => format!("Deleted step {} from {}", text("step"), activity()),
        "add_note" => format!("Added note {} to {}", text("topic"), owner()),
        "set_note" => format!("{} note {}", owner(), text("topic")),
        "delete_note" => format!("Deleted note {} from {}", text("topic"), owner()),
        "add_calendar" => format!("Added calendar {}", text("name")),
        "add_calendar_exception" => {
            format!(
                "Calendar {} exception on {}",
                text("calendar"),
                text("date")
            )
        }
        "remove_calendar_exception" => {
            format!(
                "Calendar {} exception on {} removed",
                text("calendar"),
                text("date")
            )
        }
        "set_work_week" => format!("Calendar {} {}", text("calendar"), text("day")),
        "set_hours_per_day" => format!("Calendar {} hours per day", text("calendar")),
        "set_hours_per_period" => {
            format!("Calendar {} hours per {}", text("calendar"), text("period"))
        }
        // An edit this list doesn't know yet: its command as words.
        other => {
            let words = other.replace('_', " ");
            let mut chars = words.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::edit_words;

    #[test]
    fn edits_read_as_words() {
        let cases = [
            (
                json!({"op": "set_duration", "activity": "A6", "original": "5d"}),
                "A6 duration",
            ),
            (
                json!({"op": "add_link", "predecessor": "A3", "successor": "A5"}),
                "Added link A3 to A5",
            ),
            (
                json!({"op": "change_link", "predecessor": "A3", "successor": "A5", "lag": "2d"}),
                "Changed link A3 to A5",
            ),
            (
                json!({"op": "add_wbs", "code": "QA", "name": "Quality", "parent": "P"}),
                "Added WBS QA",
            ),
            (
                json!({"op": "set_wbs", "activity": "A4", "wbs": "P.QA"}),
                "Moved A4 to WBS P.QA",
            ),
            (
                json!({"op": "delete_activity", "activity": "A4"}),
                "Deleted A4",
            ),
            (
                json!({"op": "add_activity", "code": "A9", "name": "Cure", "wbs": "P"}),
                "Added A9",
            ),
            (
                json!({"op": "set_resource_limit", "resource": "CC", "max_units_per_hour": 2.0}),
                "Resource CC limit",
            ),
            (
                json!({"op": "set_udf", "field": "Weight", "value": "3", "wbs": "P.1"}),
                "WBS P.1 Weight",
            ),
            (
                json!({"op": "set_udf", "field": "Weight", "value": "3", "activity": "A2", "resource": "CC"}),
                "A2's CC Weight",
            ),
            (
                json!({"op": "clear_udf", "field": "Weight", "resource": "CC"}),
                "resource CC Weight cleared",
            ),
            (
                json!({"op": "add_note", "topic": "Access", "text": "Gate", "activity": "A2"}),
                "Added note Access to A2",
            ),
        ];
        for (edit, words) in cases {
            assert_eq!(edit_words(&edit), words, "{edit}");
        }
    }

    #[test]
    fn an_edit_it_doesnt_know_reads_as_words_too() {
        assert_eq!(
            edit_words(&json!({"op": "set_something_new"})),
            "Set something new"
        );
    }
}
