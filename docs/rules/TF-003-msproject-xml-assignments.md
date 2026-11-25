# TF-003: MS Project XML assignments

How TotalFloat reads and writes an assignment's work, units and costs, and a resource's limit, in MS Project XML.

## Statement

1. **Work.** An assignment's `Work`, `ActualWork` and `RemainingWork` are durations, such as `PT24H0M0S`. For a work resource they are its units at completion, actual and remaining, in hours: TotalFloat's budgeted, actual and remaining units for labor and nonlabor resources.
2. **Material.** A material resource's work is written as a duration too, though its units are the material's own (tonnes, cubic metres), so TotalFloat neither reads nor writes it, and says so in the import or export notes.
3. **Units.** `Units` is how much of the resource works on the task, 1 being one full unit: the assignment's units per hour.
4. **Costs.** `Cost`, `ActualCost` and `RemainingCost` are in hundredths of the currency: an assignment of 24 hours at 50 an hour is written `120000`, a cost of 1,200. TotalFloat divides by 100 when it reads and multiplies by 100 when it writes.
5. **Resources.** Every resource the file lists comes across, whether or not anything is assigned to it.
6. **Summary tasks.** An assignment on a summary task is left out, because a WBS node holds no assignments in P6, and the import notes say so.
7. **Currency.** The schema requires `CurrencyCode`. TotalFloat doesn't record a currency yet, so it writes `USD`, and says so in the export notes when the schedule has costs.
8. **A resource's limit.** `MaxUnits` is a work resource's limit for its whole time, on the scale of an assignment's `Units` (item 3): 1 is one full unit, which TotalFloat holds as 1 unit per hour. A limit that changes over time is held as availability periods instead, each with `AvailableFrom`, `AvailableTo` (its last moment) and `AvailableUnits`. Outside every period the resource isn't available, which TotalFloat reads as a limit of 0 before the first period, and from midnight after a period's last day until the next period starts. A `From` on or before 1984-01-01 and a `To` on or after 2049-12-31 are open ends. With periods, `MaxUnits` isn't read. The limit read is a rate with no prices; one with no start date starts as TF-001 item 7 says, or the day before the first dated one if that's earlier. Material and cost resources' limits aren't read, since the corpus files hold a `MaxUnits` of 1 for them throughout.
9. **Writing a limit.** TotalFloat writes the limits in force over time (TF-001): one limit throughout as `MaxUnits`; several as one availability period per change, each ending one minute before the next starts, the first from 1984-01-01 00:00 and the last to 2049-12-31 23:59, its open ends. A material resource's limit, or a limit that is unset for part of the time, is left out and listed in the export notes. A resource with no limit is written with no `MaxUnits`.
10. **Where files hold it.** In MS Project XML: `Project/CurrencyCode`, then `Project/Assignments/Assignment` with `TaskUID`, `ResourceUID`, `ActualCost`, `ActualWork`, `Cost`, `RemainingCost`, `RemainingWork`, `Units` and `Work`, in that order, as the schema's sequence puts them; `Project/Resources/Resource` with `MaxUnits` after `Initials`, then `CalendarUID`, then `AvailabilityPeriods/AvailabilityPeriod`. In XER: `RSRCRATE.max_qty_per_hr` and `start_date`, and `TASKRSRC.target_qty`, `act_reg_qty`, `remain_qty`, `target_qty_per_hr`, `remain_qty_per_hr`, `target_cost`, `act_reg_cost` and `remain_cost`.

## Options

None.

## Sources

- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Assignment element: `Work`, `ActualWork` and `RemainingWork` are `xsd:duration`; `Units` is "the number of units for the assignment"; the cost elements are `xsd:decimal`, with no unit given. Supports items 1, 3 and 10.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Resource element: `MaxUnits` is a float, the maximum number of units the resource is available, with a default of 1.0; `AvailabilityPeriods` is the collection of periods during which the resource is available, each with `AvailableFrom`, `AvailableTo` ("the last date") and `AvailableUnits`, all optional. Supports items 8 to 10.
- `format-fact`: the MS Project XML files in the corpus. Their resources hold `MaxUnits` of 1, work and material resources alike, and none holds availability periods. Supports leaving material and cost limits unread (item 8).
- `format-fact`: [MPXJ](sources.md#mpxj) writes MS Project XML currency values as hundredths, and in the MS Project XML files in the corpus each assignment with a rate, work and cost holds a cost of 100 times the rate times the hours, such as 24 hours at 50 an hour written `120000`. Item 4; the schema doesn't say.
- `format-fact`: [MPXJ](sources.md#mpxj) documents 1984-01-01 00:00 as "NA" at the start of a period and 2049-12-31 23:59 as "NA" at its end, and writes those values. Items 8 and 9, the open-end dates.
- `own-choice`: items 1, 5 and 7; in items 8 and 9, `AvailableUnits` on the scale of `MaxUnits`, the start dates, the day-granular gaps and the notes. The reasoning is under Rationale.

## Rationale

- **Work at completion as the budget** (item 1). MS Project XML keeps a baseline's work apart from `Work`, which is the scheduled work at completion, so `Work` is the nearest the file has to a budget. TotalFloat's own design choice.
- **Unassigned resources kept** (item 5). A P6 project export lists only the resources in use, and the importer first followed it. The file lists every resource, and a planner opening it expects to find them all, so all come across. TotalFloat's own design choice.
- **USD when no currency is known** (item 7). The schema requires a code and the model holds none; writing one with a note keeps the file valid and tells the planner. TotalFloat's own design choice, until the schedule's own currency is carried across.
- **One scale for units** (item 8). The schema gives `MaxUnits` a default of 1.0, a number of units, and an assignment's `Units` is on the same scale (item 3). It calls `AvailableUnits` a percentage, but a period's units stand in for `MaxUnits` over that period, so they are read and written on its scale; reading 100 as 100 units would make a full-time resource a hundred people. TotalFloat's own design choice.
- **No availability outside the periods** (item 8). The schema calls them the periods during which the resource is available, so the time outside them is time it isn't. A gap is counted from midnight after a period's last day, because periods are given by day: a period ending at 17:00 and the next starting at 08:00 the next morning leave no gap.
- **Limits left out with a note** (item 9). A material resource has no limit in MS Project, and a period has no way to say there is no limit, so writing either would change the limit; the note tells the planner instead. TotalFloat's own design choice.

## Tests

`crates/interop/src/msproject.rs`: `an_assignment_s_work_units_and_costs_come_across`, `resources_with_no_work_come_across_and_shared_initials_are_numbered`, `an_assignment_on_a_summary_task_is_reported`. `a_work_resource_s_max_units_is_its_limit`, `availability_periods_are_a_limit_over_time`. `crates/interop/src/msproject_write.rs`: `elements_follow_the_schema_order`, `a_resource_s_limit_round_trips`, `a_limit_ms_project_can_t_hold_is_reported`. `crates/cli/tests/resource_limit.rs`: `a_limit_comes_back_from_ms_project_xml`. `crates/interop/tests/corpus_msproject.rs` reads every MS Project XML file in the corpus and round-trips it through an export.

## Differences

None known.
