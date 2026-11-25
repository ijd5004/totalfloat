# TF-028: Resource-dependent activities

Which calendar a resource-dependent activity's dates run on.

## Statement

1. A resource-dependent activity is scheduled like any other activity, on the calendar of the resources assigned to it in place of its own calendar. Its duration, float and dates are counted on that calendar, and so is lag measured on its calendar (TF-006).
2. A resource with no calendar of its own counts as being on the activity's calendar. An assignment to a role with no resource names no calendar and doesn't count.
3. A resource-dependent activity with no counted assignment runs on its own calendar.
4. When its resources are on different calendars, the project is refused: that would need each assignment scheduled on its own.
5. An assignment to a resource the file doesn't hold is invalid input.
6. TotalFloat doesn't level resources: an activity's dates never move because a resource is busy elsewhere.

## Options

None. An XER file marks the type as `TASK.task_type` `TT_Rsrc`, and gives each resource's calendar in `RSRC.clndr_id`. MS Project XML has no resource-dependent activity type.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 4, "Calendars", pp. 66–68: activity and resource calendars both set when work can happen. Items 1 and 2.
- `standard`: [AACE 10S-90](sources.md#aace-10s-90), RESOURCE CALENDAR: a calendar that sets a resource's working and nonworking periods. Item 1.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §6.2, pp. 81–85: tools time-phase resources against a task or a resource calendar, and their rules differ. Items 1 and 4.
- `observed-file`: files in the corpus with resource-dependent activities. Their stored dates and durations follow the assigned resources' calendar. No corpus activity has resources on several calendars. Items 1 and 2.
- `own-choice`: items 3 to 6; the reasoning is under Rationale.

## Rationale

A resource-dependent activity is work that happens when its resources work, so its dates run on their calendar (item 1). A resource with no calendar gives nothing to replace the activity's own, so the activity's calendar stands (items 2 and 3).

Resources on different calendars could each work different hours on the same activity. Scheduling that faithfully needs dates per assignment, which TotalFloat doesn't calculate yet, and picking one resource's calendar would be a guess, so the project is refused (item 4). Leveling changes dates for reasons that aren't in the network, so it is a separate calculation TotalFloat doesn't do (item 6; the GAO guide treats it as a separate step, pp. 57–61).

## Tests

`crates/engine/src/tests.rs`: `resource_dependent_work_runs_on_its_resources_calendar` (items 1 to 5), `lag_from_another_project_uses_its_resource_calendar` (item 1, lag).

## Differences

None known.
