# TF-036: Codes, resources and roles made by edits

How edits set activity codes, and what a new code type, resource, role or role assignment holds.

## Statement

1. **Activity codes.** An activity holds one value of each code type, so setting a value replaces any value of that type it held. A new value goes last in its type, and its code fits the type's stored length limit.
2. **New code types.** A new code type belongs to the project unless it's made global, and goes last in order. Its values hold up to 12 characters unless the edit gives another limit. A global type can't take a name that a type in any project in the file holds.
3. **New resources.** A new resource is labor unless given a type. It is active, counts its units in hours, starts assignments at 1 unit an hour, and its cost follows its units. It uses the file's first currency and, unless given one, the file's default global calendar. A project calendar can't be a resource's calendar. Its short name can't repeat one that any project in the file holds.
4. **Shared by every project.** A resource, or a global code type, that no activity in the file uses belongs to every project, so an edit in any project can use it.
5. **Roles.** A role is a kind of work, with prices over time. A resource can fill several roles, and an assignment can say which role it's for.
6. **Assigning a role with no resource.** An assignment can name a role and no resource, so work can be staffed by kind before anyone is named. It holds no resource and no resource type. Its price is the role's first price in the role's rate in force when the work starts ([TF-001](TF-001-rate-in-force.md)), and it's marked as priced by its role, whether or not the role has a price yet. Its cost follows its units when the role's costs do.
7. **Counting a role assignment.** An assignment with a role and no resource has no resource type, so it isn't counted in the activity's labor or nonlabor unit totals or in its units percent complete ([TF-004](TF-004-units-percent-complete.md)).
8. **New roles.** A new role's cost follows its units, and it counts its units in hours.
9. **What edits don't change.** An edit can't change a role, or which resource or role an existing assignment is for.
10. **Where files hold it.** In XER: `TASKACTV`, `ACTVTYPE` (`actv_short_len`, `proj_id`) and `ACTVCODE` for codes; `RSRC`, `RSRCRATE` for resources; `ROLES`, `ROLERATE` and `RSRCROLE` for roles; `TASKRSRC.role_id`, `rsrc_id`, `rsrc_type` and `cost_per_qty_source_type` (`ST_Role`) for an assignment's role and where its price comes from.

## Options

None.

## Sources

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), Best Practice 3, pp. 49–61: resources are assigned to activities, and duration estimates follow from the effort and resources assigned. Items 3 and 5.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §6.2, pp. 81–85: resources are loaded on the schedule by quantity against a calendar. Items 3 and 6.
- `observed-file`: the XER files in the corpus. Their activity code values are nearly all 12 characters or shorter, though most of their code types set a shorter limit. Most resources have their cost follow their units, and nearly every resource is active, in hours and in the file's first currency. Every role's cost follows its units. Items 2, 3 and 8.
- `observed-file`: files in the corpus whose assignments for a role are marked `ST_Role` in `TASKRSRC.cost_per_qty_source_type`. None of the corpus's assignments with a role lacks a resource, so item 6's empty resource is written by our own reasoning. Items 6 and 10.
- `own-choice`: items 1, 2 (the 12-character default), 4, 6, 7 and 9; the reasoning is under Rationale.

## Rationale

- **One value per type** (item 1). A code type is one way of sorting activities, so two values of one type on one activity would put it in two places at once.
- **Defaults** (items 2, 3 and 8). New rows take the values real files hold, so they look like the rows around them to any software that reads the file. A new code type's limit is ours: 12 characters holds nearly every code value in the exports, and an edit can give another limit.
- **Unique names** (items 2 and 3). Edits and other software find resources and code types by name, so a repeated name would make them ambiguous.
- **Shared resources** (item 4). A file holds one resource list for all its projects and doesn't say which project an unused resource belongs to, so every project can use it.
- **Role assignments** (items 6 and 7). Staffing by kind before anyone is named is how early plans are loaded. A role is priced by its own rates, so the assignment is too. Without a resource there's no resource type, and counting its units as labor would be a guess.
- **Fixed links** (item 9). Changing which resource an assignment is for changes its price, calendar and units together. Removing the assignment and adding a new one says that plainly.

## Tests

`crates/edit/src/tests.rs`: `a_code_is_set_replaced_and_cleared`, `a_code_value_is_added_and_usable_at_once`, `code_refusals` (item 1), `a_new_code_type_takes_values_and_codes` (item 2), `a_new_resource_can_be_assigned`, `a_new_resource_is_refused_what_it_can_t_hold`, `a_new_assignment_takes_the_resource_s_cost_link`, `values_and_assignments_wait_for_what_they_name`, `resource_refusals` (items 3 and 4), `a_role_is_added_under_another_and_assigned_with_no_resource`, `an_assignment_by_role_takes_the_role_s_rate`, `an_assignment_by_role_is_the_role_s_even_with_no_price` (items 5, 6 and 8). The edit commands have no operation that changes a role or an assignment's resource or role (item 9).

## Differences

None known.
