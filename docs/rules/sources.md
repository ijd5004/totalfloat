# Rule sources

The bibliography that TotalFloat's rules cite ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules)). Each source has one anchor; a rule's Sources section links to it and says what the source supports. Entries record where a source is, what it covers and the terms it's published under. They don't reproduce it: a rule is written in our own words, and a short quoted phrase with its citation is the most any entry or rule copies.

## Capability tags

Entries tag the capabilities they cover:

| Tag | Capability |
|---|---|
| `relationships` | Relationship types and lags |
| `constraints` | Date constraints |
| `calendars` | Calendars, work weeks and exceptions |
| `float` | Total and free float, critical activities and paths |
| `out-of-sequence` | Progress recorded out of logical sequence; data date and status date |
| `loe-summary` | Level-of-effort, WBS-summary and summary activities |
| `resource-durations` | Durations set by resources: resource-dependent activities, duration and task types, effort-driven scheduling |
| `baselines` | Baselines |
| `earned-value` | Earned value |
| `file-format` | How a file format stores the data and the settings |

## Standards and literature

The first source class in [ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules): public CPM standards, guidance, papers and textbooks. Each entry gives the citation, how to get it, and the capabilities it covers, each with a section or page reference and a one-line summary in our own words. URLs were checked on 2026-11-16.

### Government and industry guidance

<a id="gao-sag"></a>
#### GAO Schedule Assessment Guide (GAO-16-89G)

U.S. Government Accountability Office. *GAO Schedule Assessment Guide: Best Practices for Project Schedules*. GAO-16-89G. Washington, DC, December 2015. 240 pp. <https://www.gao.gov/products/gao-16-89g>

- **Access:** free (a U.S. government work). Page numbers below are the guide's printed page numbers.
- **What it is:** ten best practices for building and assessing an integrated master schedule, used by GAO auditors across federal programs. Appendix VII compares it with DCMA, NDIA and other agency guidance.

| Capability | Where | Summary |
|---|---|---|
| `relationships` | Best Practice 2, pp. 28–30; "Lags and Leads", pp. 41–46 | Describes FS, SS and FF links; treats SF as a theoretical type that can usually be rewritten as FS logic (p. 29). A lag should stand for real waiting time, such as curing, and a lead (negative lag) lets a successor overlap its predecessor; using either to force dates is a misuse. |
| `constraints` | "Date Constraints", pp. 36–41; Appendix V, pp. 181–182 | Separates soft constraints (start or finish no earlier than) from hard ones (no later than, must start or finish on). Appendix V lists the standard constraint names from the DoD/UN/CEFACT XML schema, their common aliases, and whether each one acts on the forward pass, the backward pass or both. |
| `calendars` | Best Practice 4, "Calendars", pp. 66–68; Best Practice 6, pp. 81–82 | Activity and resource calendars set when work can happen, with holidays and shutdowns as exceptions; a software default calendar rarely has the right holidays. Several calendars in one network can give activities on one continuous driving path different total float (Figure 32). |
| `float` | Best Practice 7, "Definitions of Total Float and Free Float", pp. 92–93 | Total float is how far an activity can slip before the program finish slips; it goes negative when a finish constraint is earlier than logic allows, or after out-of-sequence progress. Free float is the part of total float usable before the next activity is delayed. Footnote 27 adds independent and interfering float; footnote 28 notes float is usually computed to the last activity in the file. Non-working time isn't float (p. 92). |
| `float` | Best Practice 6, pp. 75–89; Appendix IV, pp. 172–180 | The critical path is in theory the longest path from the status date to the forecast finish, but several calendars, out-of-sequence progress, date constraints and leveling can make total float misleading, so the activities with the least float may not be the ones driving the finish (p. 77). Also lists what breaks a valid critical path, such as constraints, lags and LOE activities. Appendix IV works a forward and backward pass by hand. |
| `out-of-sequence` | Best Practice 9, "Out-of-Sequence Logic", pp. 126–129 | Names two treatments: retained logic, where the rest of the started activity waits for its unfinished predecessor, and progress override, where it carries on as if that link weren't there. A worked example (Figures 45–47) shows the dates under each, and the guide asks for the logic to be repaired afterwards. |
| `loe-summary` | Best Practice 1, p. 15; "Summary Logic", pp. 34–35; Best Practice 6, pp. 79–86 | LOE work has no discrete output, should be marked as such, and should never drive the critical path; hammocks that take their dates from the work they span are preferred. Summary activities should take their dates from lower-level work and carry no links of their own. |
| `resource-durations` | Best Practice 3, pp. 49–61 | Duration estimates should be consistent with the effort and the resources assigned; resource leveling moves activities within their float, starting with those that have the most free float and fewest resources (pp. 57–61). |
| `earned-value` | Appendix III, pp. 166–171 | Explains how the schedule underpins earned value management; Table 8 (p. 170) lists ways of measuring progress (0/100, 50/50, percent complete, weighted milestones, units, equivalent units), and the text covers LOE and apportioned effort. |
| `baselines` | Best Practice 10, pp. 135–146 | The baseline is the approved plan that current dates are measured against; it changes only through formal change control, and baseline and trend analysis compare the two. |

<a id="dcma-14pt"></a>
#### DCMA 14-point schedule metrics (DCMA-EA PAM 200.1)

Defense Contract Management Agency. *Earned Value Management System (EVMS) Program Analysis Pamphlet (PAP)*. DCMA-EA PAM 200.1. July 2012. Section 4, "14 Point Schedule Metrics for IMS Analysis", pp. 25–29. Copy at <https://everyspec.com/DoD/DoD-PUBLICATIONS/DCMA-EA_PAM-200-1_10JUL2012_44680/>.

- **Access:** free (a U.S. government work).
- **What it is:** fourteen counts over the incomplete activities of a schedule, each with a threshold, meant to start questions rather than to pass or fail a schedule. `totalfloat check` implements them ([CHECKS.md](../CHECKS.md)).

| Capability | Where | Summary |
|---|---|---|
| `relationships` | §4.2 Leads, §4.3 Lags, §4.4 Relationship Types, pp. 25–26 | Leads should not be used; at most 5% of links should carry a lag; at least 90% of links should be FS; SF should be rare and justified. |
| `constraints` | §4.5 Hard Constraints, pp. 26–27 | Counts must start on, must finish on, start no later than and finish no later than as hard; as soon as possible, start no earlier than and finish no earlier than as soft. At most 5% of incomplete activities should carry a hard constraint. |
| `float` | §4.6 High Float, §4.7 Negative Float, p. 27 | Flags total float above 44 working days (more than 5% of activities) and any negative float. |
| `float` | §4.12 Critical Path Test, §4.13 CPLI, p. 29; §3.1.2.3, pp. 16–18 | An intentional slip on the critical path should move the finish (or make its float negative) by the same amount, or logic is broken. CPLI relates the critical path length in work days and its total float, and below 0.95 is a flag; total float is late finish minus early finish. |
| `resource-durations` | §4.10 Resources, p. 28 | Checks that activities with duration carry hours or cost; doesn't address how resources set durations. |
| `baselines` | §4.8 High Duration, §4.11 Missed Tasks, §4.14 BEI, pp. 27–29; §3.1.2.4, pp. 18–20 | Uses baseline durations and baseline finishes: activities that finished, or will finish, later than their baseline finish, and the ratio of baseline activities finished to those due. |
| `earned-value` | §3.1, pp. 13–21 | SPI, CPI, estimate at completion and their trends. |
| `earned-value` | §2.2 Independent Estimate at Completion, p. 3; §3.1.1.1, p. 14; §3.1.2.1, p. 16 | The cost performance index is earned value over actual cost, and the schedule performance index earned value over planned value. An estimate at completion from the cost index is actual cost plus the budget left divided by that index; a composite estimate divides by the product of the two indices. |

§4.9 (p. 28) also flags forecast dates before the status date and actual dates after it.

<a id="ndia-paseg"></a>
#### NDIA Planning & Scheduling Excellence Guide (PASEG), version 5.0

National Defense Industrial Association, Integrated Program Management Division. *Planning & Scheduling Excellence Guide (PASEG)*. Version 5.0. Arlington, VA: NDIA, September 12, 2022. Linked from <https://www.ndia.org/divisions/ipmd/committees/planning-and-scheduling>; copy at <https://www.humphreys-assoc.com/evms/evms-documents/ndia/PASEG_v5_Sep2022.pdf>.

- **Access:** free.
- **What it is:** an industry guide to building and running an integrated master schedule, including the Generally Accepted Scheduling Principles (chapter 2).

| Capability | Where | Summary |
|---|---|---|
| `relationships` | §5.3 Relationships/Logic, p. 53; §5.4 Lead/Lag Time, p. 56 | Guidance on the link types and on when a lag or lead is justified. |
| `constraints` | §5.5 Task Constraints, p. 59; §5.11, p. 74 | Soft constraints let logic move a task later; hard constraints stop logic moving it at all, or stop it moving later. |
| `calendars` | §5.10 Working Calendars, p. 71; §5.11, p. 74 | Project, task and resource calendars; calendar changes affect both passes; keep the number of calendars within what the team can check. |
| `float` | §5.11 Schedule Calculation Algorithm, p. 74 | Total float is late finish minus early finish, counted in workdays of the task's calendar. Free float is the gap from an activity's early finish to the earliest early start among its successors. |
| `float` | §5.11, pp. 74–76; §10.1 Critical and Driving Path Analysis, p. 138 | The forward pass gives earliest dates and the backward pass latest dates, using calendars, links, lags, durations, constraints and the status date. §10.1 covers critical and driving paths. |
| `float` | §8.1 Horizontal Traceability, p. 101 | An activity with no predecessor or no successor is open-ended ("dangling"): delays on it don't reach other work, so the critical path can't be found reliably. |
| `out-of-sequence` | §9.1.1 Statusing to Time Now, pp. 111–114 | Shows how out-of-sequence status moves forecast dates (Figure 9.1.1-4) and asks for logic to be corrected rather than status entered past it. |
| `loe-summary` | §5.7 Summaries and Hammocks, p. 65; §5.8 Level of Effort, p. 67; §5.9 Apportioned Effort, p. 69 | Summaries roll up dates, hours and cost and should carry no links; hammocks roll up dates and must not drive successors; LOE should be identifiable and never drive discrete work, and links on it may be removed; apportioned effort follows its base task and can't drive a path. |
| `resource-durations` | §6.2 Resources in the Schedule, pp. 81–85 | Tools compute resource loading from duration, total quantity and time phasing against a task or resource calendar, and their rules differ, so the settings must be understood. |
| `earned-value` | §6.1, p. 80; §10.4 Schedule Execution Metrics, pp. 166–195 | Cost and schedule integration; CPLI, SPI, BEI, current execution index, total float consumption index, earned schedule (SPIt) and an earned-schedule completion date. |
| `baselines` | §4.4 Schedule Baseline vs. Forecast, p. 42; §9.2 Baseline Maintenance, pp. 129–136 | Keeps baseline and forecast dates separate; covers baseline change control and rolling-wave planning. |

### AACE International

<a id="aace-10s-90"></a>
#### AACE 10S-90, Cost Engineering Terminology

AACE International. *Cost Engineering Terminology*. Recommended Practice 10S-90. Online edition, Rev. February 18, 2026. <https://library.aacei.org/terminology/welcome.shtml>

- **Access:** free (<https://web.aacei.org/resources/cost-engineering-terminology>). Each term carries its own revision date.
- **What it is:** AACE's glossary, which its other recommended practices use for their terms.

| Capability | Where (term entries) | Summary |
|---|---|---|
| `relationships` | FINISH-TO-START (FS), START-TO-START (SS), FINISH-TO-FINISH (FF), START-TO-FINISH (SF); LAG and LEAD and their per-type entries | Defines each link type and lag type. SF means the successor can finish only after the predecessor starts. |
| `constraints` | DATE CONSTRAINT | A date imposed on an activity's start or finish, which overrides logic. |
| `calendars` | CALENDAR, PROJECT CALENDAR, RESOURCE CALENDAR | Calendars define working and non-working periods for the project, an activity or a resource. |
| `float` | TOTAL FLOAT (TF), FREE FLOAT, INDEPENDENT FLOAT | Total float: the work periods an activity can slip without delaying project completion or a target milestone. Free float: how far an activity can move past its early dates without pushing any successor past its early dates. Independent float: flexibility that changes no other activity's float. |
| `float`, `out-of-sequence` | LONGEST PATH, LONGEST PATH VALUE, DATA DATE | The longest path is the chain that sets project duration; the data date separates actuals from forecast. |
| `out-of-sequence` | OUT-OF-SEQUENCE PROGRESS, RETAINED LOGIC, PROGRESS OVERRIDE | Out-of-sequence progress is progress recorded while a predecessor is incomplete. Under retained logic the remaining duration waits for predecessors; under progress override it proceeds as if the activity had no predecessor constraints. Software may offer this as a switch. |
| `loe-summary` | HAMMOCK ACTIVITY, LEVEL OF EFFORT (LOE) | A hammock takes its duration from the two points it spans and doesn't change other activities' dates. LOE is support work with no discrete output, and also the earned value technique for it. |
| `baselines` | BASELINE SCHEDULE | The frozen schedule performance is measured against, reset when scope changes significantly. |

<a id="aace-49r-06"></a>
#### AACE 49R-06, Identifying the Critical Path

AACE International. *Identifying the Critical Path*. Recommended Practice 49R-06. Rev. March 5, 2010. <https://web.aacei.org/docs/default-source/toc/toc_49r-06.pdf>

- **Access:** purchase (free to AACE members).

| Capability | Where | Summary |
|---|---|---|
| `float` | pp. 1–3 | No standard defines how CPM must be calculated, and different tools, or different options in one tool, give different results for the same logic. Gives four common ways to pick the critical path: lowest total float, any negative total float, longest path, and a longest-path value per activity. |
| `float` | p. 3 | Total float is late finish minus early finish, or late start minus early start, and the two can differ. |
| `calendars`, `constraints`, `loe-summary` | p. 3 | Activities on one logical chain can show different float because of date and float constraints, activity calendars, lag calendars, interruptible activities, and hammock, LOE or WBS summary activities. |
| `resource-durations` | p. 2 | Says resource-driven schedules are out of its scope. |

<a id="aace-92r-17"></a>
#### AACE 92R-17, Analyzing Near-Critical Paths

AACE International. *Analyzing Near-Critical Paths*. Recommended Practice 92R-17. Rev. October 18, 2017. <https://web.aacei.org/docs/default-source/toc/toc_92r-17.pdf>

- **Access:** purchase (free to AACE members).

| Capability | Where | Summary |
|---|---|---|
| `float` | Purpose and introduction, pp. 1–2 | Near-critical activities are at risk of becoming critical if delayed; the RP sets out how to identify them and track and trend them across updates. |

### Earned value

<a id="dod-evmsig"></a>
#### DoD Earned Value Management System Interpretation Guide (EVMSIG)

U.S. Department of Defense. *Earned Value Management System Interpretation Guide (EVMSIG)*. March 14, 2019. Listed at <https://www.acq.osd.mil/asda/dpc/api/ipm/policy-guidance.html>; copy at <https://www.humphreys-assoc.com/evms/evms-documents/dod/DoD_EVMSIG_14MAR2019.pdf>.

- **Access:** free (a U.S. government work).

| Capability | Where | Summary |
|---|---|---|
| `earned-value` | §3.5 Guideline 10, pp. 28–30; §3.7 Guideline 12, p. 32; §5.1 Guideline 22, p. 53; §7.2 Definitions, pp. 78–89 | Work is planned as discrete, LOE or apportioned effort. Discrete work earns value by an objective measure of progress; LOE earns it with the passage of time, so its earned value always equals its planned value. The glossary names the techniques (milestone, percent complete, 50/50, 0/100 and others). Schedule and cost variance are calculated at least monthly; schedule variance is expressed in money, not time. |
| `earned-value` | §7.2 Definitions: ACWP, p. 78; BAC, BCWP and BCWS, p. 79; cost variance, p. 80; EAC and ETC, p. 82; schedule variance, p. 87 | Actual cost, earned value (BCWP) and planned value (BCWS) may each be given for one period or cumulative to date. The estimate at completion is actual cost plus the estimate to complete. Cost variance is earned value less actual cost, and schedule variance earned value less planned value. The glossary also defines a milestone as a zero-duration event (p. 84). |
| `loe-summary` | §3.7 Guideline 12, pp. 32–33; §3.5, p. 29 | LOE is limited to work that has no measurable output or can't practicably be measured; apportioned effort earns value in proportion to its base work. |
| `baselines` | §3.3 Guideline 8, pp. 23–25; §6.2 Guideline 29, pp. 67–69 | Sets up the time-phased performance measurement baseline and keeps it reconciled under change control. |
| `relationships`, `float` | §3.1 Guideline 6, pp. 18–21 | The schedule should be a networked integrated master schedule that shows the sequence and interdependencies of the work and yields critical and driving paths. |

### Papers

<a id="kelley-walker-1959"></a>
#### Kelley and Walker (1959), Critical-Path Planning and Scheduling

Kelley, James E., Jr., and Morgan R. Walker. "Critical-Path Planning and Scheduling." In *Papers Presented at the December 1–3, 1959, Eastern Joint IRE-AIEE-ACM Computer Conference*, 160–173. New York: ACM, 1959. <https://doi.org/10.1145/1460299.1460318>

- **Access:** purchase from the ACM Digital Library; a free scan is hosted by Mosaic Projects (<https://mosaicprojects.com.au/PDF/PM-History_Critical_Path_Planning_&_Scheduling_Kelley_and_Walker_1959.pdf>). Pages below are the proceedings' page numbers.
- **What it is:** the founding paper of CPM, by Kelley (Remington Rand) and Walker (DuPont). It uses activity-on-arrow diagrams: jobs are arrows and events are nodes.

| Capability | Where | Summary |
|---|---|---|
| `relationships` | pp. 161–162 | One kind of dependency: arrows meet at events, and a job can start only when the event at its tail has occurred, that is, when every job ending there is done. |
| `float` | pp. 162–163 | Earliest event times from a forward pass, latest from a backward pass against a fixed completion time; the earliest and latest start and finish of each job follow from them. A job whose available time equals its duration is critical, and critical jobs form at least one continuous path from start to end; other jobs are "floaters" (p. 163). |
| `float` | p. 163 | Defines four floats from event times: total float, free float, independent float (never below zero), and interfering float (an event's latest minus earliest time). |
| `resource-durations` | pp. 164ff. | Each job has a direct-cost curve between a normal and a crash duration; the paper finds the cheapest set of durations for each project length. This is time–cost trade-off, not durations computed from resource units. |

<a id="fondahl-1961"></a>
#### Fondahl (1961), A Non-Computer Approach to the Critical Path Method

Fondahl, John W. *A Non-Computer Approach to the Critical Path Method for the Construction Industry*. Technical Report No. 9. Stanford, CA: Department of Civil Engineering, Stanford University, 1961; 2nd ed., 1962.

- **Access:** free. The 1962 second edition is a public-domain scan on HathiTrust (<http://hdl.handle.net/2027/mdp.39015000453970>). Pages below are the report's printed page numbers in that edition.
- **What it is:** the first description of activity-on-node ("circle and connecting line") networks, worked by hand. Later precedence diagramming builds on it.

| Capability | Where | Summary |
|---|---|---|
| `relationships` | pp. 19–20 | Every link is finish-to-start: an operation starts when its predecessors finish. Where a bar chart would show overlap, the operations are split so that only the dependent parts are linked. SS, FF and SF links with lags came later. |
| `calendars` | p. 26 | Times may be in working or calendar days; deliveries quoted in calendar days have to be converted to working days so the network stays in one unit. |
| `float` | pp. 26–28 | Forward pass in operation-number order; backward pass from the earliest project finish; operations with zero total float are critical, and at least one chain of them runs from start to finish. |
| `float` | pp. 27–29 | Total float is latest finish minus earliest finish (the same as latest start minus earliest start). Free float is the gap from an operation's earliest finish to the earliest early start of the operations that follow it. Interfering float is total float minus free float. |
| `relationships`, `float` | pp. 41–42 | Uses "lag" for the computed gap between an operation's earliest finish and a follower's earliest start, not an input delay: a zero-gap link is drawn doubled, and a chain of them marks the critical path. |
| `resource-durations` | pp. 41ff. | Shortens a project by crashing critical operations in order of cost slope (time–cost trade-off). |

### Textbooks

<a id="hendrickson-pmc"></a>
#### Hendrickson, Project Management for Construction

Hendrickson, Chris. *Project Management for Construction: Fundamental Concepts for Owners, Engineers, Architects and Builders*. Version 2.2. Pittsburgh: Carnegie Mellon University, 2008. First edition with Tung Au, Prentice Hall, 1989. <https://www.cmu.edu/cee/projects/PMbook/>

- **Access:** free online. The address now redirects to an updated adaptation, *Project Management for Construction (and Deconstruction)* (Hendrickson, Haas and Au, eCampusOntario). Chapter pages under the old address still served version 2.2 when this entry was written; section numbers below are from version 2.2.
- **What it is:** a standard construction management textbook. Chapters 10 to 12 cover CPM scheduling and control.

| Capability | Where | Summary |
|---|---|---|
| `relationships` | §10.6, §10.7 | Activity-on-node networks with SS, FF, SF and FS links, each with a lead or lag, and the calculations for them. |
| `constraints` | §10.6, §10.7 | Activity "windows" (an earliest start or latest finish) limit when an activity may be scheduled; they are treated as a special case of precedence constraints. |
| `calendars` | §10.5 | Converting project day numbers to calendar dates skips weekends and holidays unless the planner overrides it; shifts and hours per day can be set to match estimating units. |
| `float` | §10.2, §10.3 | The critical path and the forward and backward passes. |
| `float` | §10.4 | Free float, independent float and total float. Independent float is never more than free float, which is never more than total float; all three are zero on the critical path. |
| `resource-durations` | §10.8, §10.9; §11.4 | Resource-constrained scheduling by heuristics, for example delaying the competing activity with the later late start; durations change with the type and amount of resources applied (crashing). |

Sections 12.6 and 12.7 cover updating the schedule with actual progress; we found no treatment of out-of-sequence progress there. The book doesn't cover earned value by name (chapter 12 compares budgeted and actual cost).

## Vendor documentation and published schemas

The second source class in [ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules): what a vendor publishes openly about what a setting means and which file field holds it, cited and never copied.

Only openly licensed vendor material is cited. For Microsoft Project, that's the two documentation sets below, published in GitHub repositories under CC BY 4.0 and MIT. Primavera P6 has no openly licensed documentation, so P6 facts come from the other source classes: the standards and literature above, the files themselves (`docs/xer-format.md`), MPXJ's code lists (`format-fact`, below) and our own choices.

URLs were checked on 2026-11-17.

### Microsoft Project

<a id="msp-xml-schema"></a>
#### Project XML Data Interchange Schema Reference (MSPDI)

Microsoft, *Project XML Data Interchange Schema Reference*, written for the Project 2007 schema `mspdi_pj12.xsd` and since extended with later elements (`AgileMode`, for example). Repository: <https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs>, folder `project-xml-data-interchange/`. Terms: CC BY 4.0 for documentation (`LICENSE`) and MIT for code (`LICENSE-CODE`); the repository doesn't say which covers the schema fragments. Tags: `file-format`, and through its elements all capabilities.

The schema itself is published in sections, one page per top-level element (for example [XML Schema for the Project Element](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/xml-schema-for-the-project-element.md)); no single public `.xsd` download was found. The structure pages:

| Page | Tags |
|---|---|
| [Project XML Data Interchange Schema Reference](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/project-xml-data-interchange-schema-reference.md) | `file-format` |
| [Project Elements and XML Structure](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/project-elements-and-xml-structure.md): project-level settings such as `StatusDate`, `CriticalSlackLimit`, `MultipleCriticalPaths`, `HonorConstraints`, `SplitsInProgressTasks`, the `MoveCompleted…`/`MoveRemaining…` options, `DefaultTaskType`, `NewTasksEffortDriven`, `EarnedValueMethod` (percent complete or physical percent complete), and the default `MinutesPerDay`, `MinutesPerWeek` and `DaysPerMonth` | `file-format`, `out-of-sequence`, `float`, `constraints`, `resource-durations`, `earned-value`, `calendars` |
| [Task Elements and XML Structure](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/task-elements-and-xml-structure.md): task fields such as `TotalSlack`, `FreeSlack`, `Critical`, `Summary`, `ConstraintType`, `Type`, `EffortDriven`, `PredecessorLink` with `LinkLag` and `LagFormat`, `Baseline`, `BCWS`, `BCWP`, `ACWP`, and `PhysicalPercentComplete` (the share of work done, usable for earned value) | `file-format`, `float`, `relationships`, `constraints`, `loe-summary`, `resource-durations`, `baselines`, `earned-value` |
| [Calendar Elements and XML Structure](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/calendar-elements-and-xml-structure.md) | `file-format`, `calendars` |
| [Assignment Elements and XML Structure](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/assignment-elements-and-xml-structure.md): assignment fields such as `Work`, `Units`, `Cost`, `WorkContour` (how work is distributed across the assignment's duration), `BCWS`, `BCWP`, `ACWP` and `TimephasedData` | `file-format`, `resource-durations`, `baselines` |
| [TimephasedDataType Elements and XML Structure](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/timephaseddatatype-elements-and-xml-structure.md) | `file-format`, `earned-value` |
| [Introduction to Project XML Data](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/introduction-to-project-xml-data.md) | `file-format` |
| [Saving and Opening Projects in XML Format](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/saving-and-opening-projects-in-xml-format.md) | `file-format` |
| [Changes in the Project 2007 XML Data Interchange Schema](https://github.com/MicrosoftDocs/office-developer-msproject-xml-docs/blob/main/project-xml-data-interchange/changes-in-the-project-2007-xml-data-interchange-schema.md) | `file-format` |

<a id="msp-vba-reference"></a>
#### Project VBA object model reference: scheduling options

Microsoft, Project VBA reference. Repository: <https://github.com/MicrosoftDocs/VBA-Docs>, folder `api/`. Terms: CC BY 4.0 for documentation (`LICENSE`) and MIT for code (`LICENSE-CODE`). Tags: listed per page. These pages name every option on Project's Schedule, Calculation and Calendar option pages as a parameter, which makes them a checklist of the choices Project lets a planner make.

| Page | Tags |
|---|---|
| [Application.OptionsSchedule method (Project)](https://github.com/MicrosoftDocs/VBA-Docs/blob/main/api/Project.Application.OptionsSchedule.md): for example `AutoSplit`, `HonorConstraints`, `CriticalSlack`, `TaskType`, `EffortDriven` | `out-of-sequence`, `constraints`, `float`, `resource-durations` |
| [Application.OptionsCalculation method (Project)](https://github.com/MicrosoftDocs/VBA-Docs/blob/main/api/Project.Application.OptionsCalculation.md): for example `MoveCompleted`, `MoveRemaining`, `CalcMultipleCriticalPaths`, `CalcInsProjLikeSummTask`, `EVMethod`, `EVBaseline` | `out-of-sequence`, `float`, `loe-summary`, `earned-value`, `baselines` |
| [Application.OptionsCalendar method (Project)](https://github.com/MicrosoftDocs/VBA-Docs/blob/main/api/Project.Application.OptionsCalendar.md) | `calendars` |

<a id="open-source-code-read-for-format-facts"></a>
## Open-source code, read for format facts

The `format-fact` class ([README.md](README.md#provenance-classes)): a file format's table, field and code names, encodings and units, read from open-source projects' code ([CONTRIBUTING.md, Clean room](../../CONTRIBUTING.md#clean-room)).

<a id="mpxj"></a>
#### MPXJ

MPXJ, a Java library that reads and writes project files. Repository: <https://github.com/joniles/mpxj>. Licence: LGPL-2.1; read for facts only. Tags: `file-format`. Facts read from it: the Primavera code lists in `org.mpxj.primavera` (for example duration types, constraint types, activity code scopes, total float types and rate sources), and the MS Project XML currency scale in `org.mpxj.mspdi.DatatypeConverter` (currency values are written as hundredths). Checked on 2026-11-23.

## Capability by source: standards and literature

`✓` means the entry in [Standards and literature](#standards-and-literature) points to the section that covers the capability.

| Source | relationships | constraints | calendars | float | out-of-sequence | loe-summary | resource-durations | baselines | earned-value |
|---|---|---|---|---|---|---|---|---|---|
| [gao-sag](#gao-sag) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| [aace-10s-90](#aace-10s-90) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  | ✓ |  |
| [aace-49r-06](#aace-49r-06) |  | ✓ | ✓ | ✓ |  | ✓ |  |  |  |
| [aace-92r-17](#aace-92r-17) |  |  |  | ✓ |  |  |  |  |  |
| [dcma-14pt](#dcma-14pt) | ✓ | ✓ |  | ✓ |  |  |  | ✓ | ✓ |
| [ndia-paseg](#ndia-paseg) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| [dod-evmsig](#dod-evmsig) | ✓ |  |  | ✓ |  | ✓ |  | ✓ | ✓ |
| [kelley-walker-1959](#kelley-walker-1959) | ✓ |  |  | ✓ |  |  | ✓ |  |  |
| [fondahl-1961](#fondahl-1961) | ✓ |  | ✓ | ✓ |  |  | ✓ |  |  |
| [hendrickson-pmc](#hendrickson-pmc) | ✓ | ✓ | ✓ | ✓ |  |  | ✓ |  |  |
