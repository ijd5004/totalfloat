# Glossary

Scheduling terms used across the codebase, with the XER tables and fields they correspond to. XER names and codes reflect commonly observed exports. The compiled [field registry and observed enum subsets](xer-format.md) are checked against the corpus. Missing columns and future codes remain supported; calculation rules are in [rules/](rules/).

## Files and structure

| Term | Meaning | XER |
|---|---|---|
| XER | P6's tab-delimited export format: a header line, then tables | `ERMHDR` header; `%T` table, `%F` fields, `%R` row, `%E` end |
| PMXML | P6's XML export format | — |
| MS Project XML (MSPDI) | MS Project's documented XML format | — |
| MPP | MS Project's binary format (undocumented) | — |
| EPS / OBS | Enterprise project structure / organizational breakdown structure (hierarchies above projects) | `PROJECT`, `OBS` |
| Project | One schedule | `PROJECT` (`proj_id`, `proj_short_name`) |
| WBS | Work breakdown structure: the hierarchy activities sit under | `PROJWBS` (`wbs_id`, `parent_wbs_id`, `wbs_short_name`, `proj_node_flag`) |
| Activity | A unit of work in the schedule | `TASK` (`task_id` internal ID, `task_code` Activity ID, `task_name`) |
| Activity codes | User-defined classification values on activities | `ACTVTYPE`, `ACTVCODE`, `TASKACTV` |
| UDF | User-defined field | `UDFTYPE`, `UDFVALUE` |
| Resource / assignment | Labor, equipment or material, and its assignment to an activity | `RSRC`, `TASKRSRC` |

## Activity properties

| Term | Meaning | XER |
|---|---|---|
| Activity type | Task dependent, resource dependent, level of effort, start milestone, finish milestone, WBS summary | `task_type`: `TT_Task`, `TT_Rsrc`, `TT_LOE`, `TT_Mile`, `TT_FinMile`, `TT_WBS` |
| Duration type | Which of duration, units and units/time stay fixed when one changes (`DT_FixedRate` is recognized but unobserved in the current corpus) | `duration_type`: `DT_FixedDrtn`, `DT_FixedDUR2`, `DT_FixedQty`, `DT_FixedRate` |
| Percent complete type | Physical, duration, or units | `complete_pct_type`: `CP_Phys`, `CP_Drtn`, `CP_Units` |
| Status | Not started, in progress, completed | `status_code`: `TK_NotStart`, `TK_Active`, `TK_Complete` |
| Original / remaining duration | Planned duration; work left as of the data date | `target_drtn_hr_cnt`, `remain_drtn_hr_cnt` (decimal hours) |
| Planned dates | Dates the activity was planned for (not a baseline) | `target_start_date`, `target_end_date` |
| Actual dates | When work actually started or finished | `act_start_date`, `act_end_date` |
| Suspend / resume | Pause in progress of a started activity | `suspend_date`, `resume_date` |
| Expected finish | A user-entered finish date the scheduler can honor | `expect_end_date` |
| Constraint | A date restriction on an activity; an activity can have a primary and a secondary | `cstr_type`/`cstr_date`, `cstr_type2`/`cstr_date2` |
| Constraint types | Start on / on or after / on or before; finish on / on or after / on or before; as late as possible; mandatory start / finish (`CS_MANDSTART` is recognized but unobserved in the current corpus) | `CS_MSO`, `CS_MSOA`, `CS_MSOB`, `CS_MEO`, `CS_MEOA`, `CS_MEOB`, `CS_ALAP`, `CS_MANDSTART`, `CS_MANDFIN` |

## Logic

| Term | Meaning | XER |
|---|---|---|
| Relationship (logic tie) | A dependency between two activities | `TASKPRED` (`task_id` successor, `pred_task_id`, `pred_type`, `lag_hr_cnt`) |
| FS / SS / FF / SF | Finish-to-start, start-to-start, finish-to-finish, start-to-finish | `PR_FS`, `PR_SS`, `PR_FF`, `PR_SF` |
| Lag / lead | Delay (positive) or overlap (negative) on a relationship | `lag_hr_cnt` |
| Lag calendar | Which calendar measures lag: predecessor, successor, 24-hour, or project default | `SCHEDOPTIONS.sched_calendar_on_relationship_lag` |
| Open end | An activity with no predecessor or no successor | — |
| External relationship | A relationship to an activity in another project | External early start / late finish dates |
| Fragnet | A fragment of network (activities plus logic) inserted to model a change or delay | — |

## Calculation

| Term | Meaning | XER |
|---|---|---|
| Data date | The "as of" date; remaining work is scheduled after it | `PROJECT.last_recalc_date` (verify) |
| Forward / backward pass | Computing early dates from the start, then late dates from the finish | — |
| Early / late dates | Earliest and latest an activity can start/finish without delaying the project | `early_start_date`, `early_end_date`, `late_start_date`, `late_end_date` |
| Remaining early / late dates | Early/late dates for remaining work | `restart_date`, `reend_date`, `rem_late_start_date`, `rem_late_end_date` |
| Total float | How long an activity can slip without delaying the project finish (or its constraint) | `total_float_hr_cnt` |
| Free float | How long an activity can slip without delaying any successor | `free_float_hr_cnt` |
| Critical | Total float at or below a threshold, or on the longest path, per schedule option | — |
| Longest path | The chain of driving relationships that determines the project finish | `driving_path_flag` |
| Driving relationship | A relationship that controls its successor's early date | — |
| Multiple float paths | Ranked chains of activities by float, ending at a chosen activity | `float_path`, `float_path_order` |
| Retained logic / progress override / actual dates | Options for scheduling the remaining work of out-of-sequence activities | Schedule options |
| Out-of-sequence progress | An activity started before its predecessors allow | — |
| Level of effort (LOE) | An activity whose dates span its predecessors and successors; similar to a "hammock" | `TT_LOE` |
| Hours per time period | Conversion factors used to display hours as days/weeks/months | `CALENDAR.day_hr_cnt` and similar, plus admin preferences |
| Schedule options | Project-level settings that change how a schedule is calculated | `PROJECT` and `SCHEDOPTIONS` (when present); see [TF-008](rules/TF-008-xer-schedule-options.md) |
| Calendar | Work week and exceptions | `CALENDAR` (`clndr_id`, `clndr_type`, `base_clndr_id`, `clndr_data`) |

## Analysis and controls

| Term | Meaning |
|---|---|
| Baseline | A saved copy of the schedule used to measure variance |
| Update | A periodic (usually monthly) progressed version of the schedule |
| TIA | Time impact analysis: inserting a fragnet to measure a delay's effect |
| Windows analysis | Forensic method comparing successive updates over time windows |
| DCMA 14-point | Defense Contract Management Agency schedule quality checks (logic, leads, lags, relationship types, hard constraints, high float, negative float, high duration, invalid dates, resources, missed tasks, critical path test, CPLI, BEI) |
| CPLI / BEI | Critical path length index / baseline execution index |
| Earned value | PV (BCWS), EV (BCWP), AC (ACWP), SPI, CPI, EAC, ETC |
