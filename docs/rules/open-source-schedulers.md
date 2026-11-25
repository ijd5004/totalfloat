# Open-source schedulers: rules, options and differential tests

A survey of six open-source projects for how they define scheduling rules and options, and of which ones we can run as an independent check on our engine ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules), decision 6). It informs the options model and the differential harness.

Checked on 2026-11-16. Versions come from each project's own release channel, licences from its repository or licence file. Facts about scheduling come from public documentation and from reading source code for facts only; no code or documentation text is copied here, and nothing here is a rule of ours. The test runs used small networks we wrote ourselves, in a scratch directory outside the repository. Results that compare open-source tools may be published; nothing here comes from another vendor's software.

## Summary

| Project | Version checked | Licence | Runtime | CPM engine | Runs headless | Use to us |
|---|---|---|---|---|---|---|
| [TaskJuggler](#taskjuggler) | 3.8.4 (gem, 2025-07-28) | GPL-2.0-only | Ruby | Own: ASAP/ALAP heuristic with resource allocation; no float | Yes, a plain CLI | **Differential tests** (early dates; late dates for FS networks) |
| [ProjectLibre](#projectlibre) | 1.9.8 (2025-04-28) | CPAL-1.0 | Java (Swing) | Own: MS Project–style, with total and free slack | No | Ideas only |
| [GanttProject](#ganttproject) | 3.3.3323 stable; 3.4 beta 3397 (2026-11-15) | GPL-3.0 | Java with JavaFX | Own: day-granular, all four link types, critical flag; no float | Yes, under a virtual display | **Differential tests** (early dates) |
| [LibrePlan](#libreplan) | 1.6.1 (2026-06-11) | AGPL-3.0 | Java web app, PostgreSQL | Own: resource-driven, three link types, no lag | Server only | Ideas only |
| [OpenProject](#openproject) | 17.9.7 (2026-11-09) | GPL-3.0 | Ruby on Rails web app | Own: predecessor/successor only, working-day lag | Server only | Ideas only |
| [MPXJ](#mpxj-format-facts-only) | 16.11.0 (2026-11-02) | LGPL-2.1 | Java, .NET, Python | Not used (see below) | Library | File-format facts only |

**Recommendation:** use **GanttProject** for all four relationship types with signed lags, and **TaskJuggler** for minute-level working hours and calendar-time gaps. Neither reports total float, so float is checked by property tests and, for finish-to-start networks, by TaskJuggler's late-date run. Commands are under [Differential testing](#differential-testing).

## TaskJuggler

- **Licence and version:** GPL-2.0-only (gem metadata, `COPYING`); 3.8.4 is the latest gem on rubygems.org and tag `v3.8.4` on [github.com/taskjuggler/TaskJuggler](https://github.com/taskjuggler/TaskJuggler). Manual: [taskjuggler.org/tj3/manual](https://taskjuggler.org/tj3/manual/), also offline through `tj3man <keyword>`.
- **Scheduling model.** A project is a text file (`.tjp`). Each task is scheduled in one direction: as soon as possible (ASAP) from its start, or as late as possible (ALAP) back from its end. `depends` makes a task ASAP and `precedes` makes it ALAP; the manual warns that mixing them costs time and can surprise. There is no separate forward and backward pass and no late dates for an ASAP task.
  - **Relationships:** `depends x` bounds this task's start by the end of `x` (finish-to-start), or by its start with `{ onstart }` (start-to-start). `precedes` bounds an ALAP task's end the same way. Finish-to-finish and start-to-finish on an ASAP task can't be written.
  - **Lags:** `gaplength` is working time and `gapduration` is calendar time; both are minimum gaps and can't be negative (a negative value is a syntax error).
  - **Constraints:** `start` is a lower bound on the start and makes the task ASAP; `end` is an upper bound on the end and makes it ALAP. Resource availability can move a task off either. `minstart`, `maxstart`, `minend` and `maxend` are checked after scheduling and reported as errors; they don't move anything.
  - **Calendars:** project working hours by weekday, global and per-resource `leaves`, and `shifts` that replace working hours for a period. Time resolution is 5 to 60 minutes (`timingresolution`, default 1 hour). Projects have a time zone.
  - **Durations:** `length` is working time, `duration` is calendar time, `effort` is resource work.
  - **Float:** none. Reports offer `criticalness` and `pathcriticalness`, measures of how strained a task's resources are, not slack.
  - **Progress:** `complete` is a percentage for reports and doesn't affect scheduling. Real progress is bookings (resource time spent); with a `trackingscenario`, scheduling only adds work after `now`.
  - **Resources:** effort tasks are scheduled by allocating resources slot by slot, in task `priority` order, with `limits`, `efficiency` and alternative resources. This is resource levelling built into the scheduler, not a separate step.
- **Options it exposes:** scheduling direction per task, timing resolution, working hours and leaves, time zone, scenarios, `now`, task priority.
- **Formats:** reads `.tjp`/`.tji` and time and status sheets; writes HTML, CSV and Niku XOG reports, iCalendar, `.tjp`/`.tji` exports and MS Project XML (`formats mspxml` on an export report).
- **Headless:** yes. `tj3` is a command-line program; reports are files.

## ProjectLibre

- **Licence and version:** CPAL-1.0 (Common Public Attribution License; header of every source file and `projectlibre_build/license/license.txt`), with the ProjectLibre end-user licence as an alternative. 1.9.8 is the newest folder on [sourceforge.net/projects/projectlibre](https://sourceforge.net/projects/projectlibre/files/ProjectLibre/); source read at [github.com/claur/ProjectLibre](https://github.com/claur/ProjectLibre) (commit 0530be2, 2025-04-30).
- **Scheduling model.** A successor of OpenProj, modelled on MS Project.
  - **Relationships:** FS, SS, FF, SF, with lag that can be a duration, elapsed, or a percent of the predecessor's duration.
  - **Constraints:** ASAP, ALAP, must start on, must finish on, start no earlier/later than, finish no earlier/later than; deadlines.
  - **Direction:** a project schedules forward from its start or backward from its finish.
  - **Calendars:** base and derived calendars on the project, tasks and resources.
  - **Float:** total slack, free slack, start and finish slack, and a critical flag.
  - **Progress:** percent complete, actual dates, and an "update project" command that sets a status date and marks work complete through it.
  - **Resources:** assignments with units, work and task types (fixed units, work, duration; effort-driven). The resource-levelling action exists in the menu, but its dialog is commented out in 1.9.8, so there is no levelling.
- **Options it exposes:** project scheduling direction, task type and effort-driven default, calendars, lag units.
- **Formats:** its own `.pod`; imports and exports MS Project XML and MPX through a bundled copy of MPXJ; imports `.mpp`.
- **Headless:** no. The launcher's only argument is a list of files to open in the window; there is no export or batch mode. Its engine could only be driven by a Java program written against its classes, which we haven't tried.

## GanttProject

- **Licence and version:** GPL-3.0 ([github.com/bardsoftware/ganttproject](https://github.com/bardsoftware/ganttproject), `LICENSE`). Stable is 3.3.3323 (GitHub release assets and [dl.ganttproject.biz](https://dl.ganttproject.biz/)); a 3.4 beta series continues, newest build 3.4.3397 on 2026-11-15. Source read at commit 88dfca7 (2026-11-15).
- **Scheduling model.**
  - **Granularity:** whole days on one project calendar.
  - **Relationships:** FS, SS, FF and SF, each with a lag in days that can be negative. A link is *strong* (the successor sits exactly where the link puts it) or *rubber* (the link is only a minimum, and the successor may sit later). We confirmed both by running it: a strong link pulled a milestone back from a later stored start, and a rubber link left it there. A strong SF link also placed its successor before the project start, so nothing holds tasks after a project start.
  - **Constraints:** an "earliest begin" date (start no earlier than). Tasks without predecessors keep the start date in the file.
  - **Calendars:** a working week, holidays and other non-working or working days, on one project calendar. Lags count working days.
  - **Float:** none reported. A backward pass from the project end marks critical tasks (`CriticalPathAlgorithmImpl`), but no late dates or slack are exposed.
  - **Progress:** percent complete only; no data date.
  - **Resources:** assignments for cost and load charts; they don't drive durations, and there is no levelling.
- **Options it exposes:** default link hardness (strong or rubber), working week and holidays, "critical path always on".
- **Formats:** reads `.gan` (XML); imports MS Project XML, MPX and `.mpp` through MPXJ, CSV and XLS, and iCalendar holidays (all through the GUI). Writes `.gan`, CSV, XLS, HTML, PDF, PNG/JPG, MS Project XML and MPX.
- **Headless:** yes, with limits. `ganttproject -export <ext> -o <out> <file.gan>` exports and exits, where `<ext>` is a file extension the exporter knows (`csv`, `xml`, `mpx`, `html`, `pdf`, …). It still builds a Swing frame, so it needs an X display (a virtual one such as Xvfb is enough) and a Java runtime that includes JavaFX (for example BellSoft Liberica Full). From the command line it only opens `.gan` files: an MS Project XML file given in its place loads as an empty project.

## LibrePlan

- **Licence and version:** AGPL-3.0 ([github.com/LibrePlan/libreplan](https://github.com/LibrePlan/libreplan)); latest release 1.6.1 on 2026-06-11. User guide: [libreplan.sourceforge.net/user-documentation](https://libreplan.sourceforge.net/user-documentation/en/07-planificacion.html).
- **Scheduling model.** Planning starts from orders and hours: task durations follow from the hours estimated and the resources allocated, on calendars that give each day's working hours with exceptions.
  - **Relationships:** end-start, start-start and end-end (`DependencyType`), with no lag.
  - **Constraints:** as soon as possible, as late as possible, start not earlier than, start in fixed date, finish not later than (`PositionConstraintType`). Each order says whether dependencies or constraints win when they conflict.
  - **Float:** a critical path view; no float values documented.
  - **Progress:** several progress types per task, feeding earned value (BCWS, BCWP).
  - **Resources:** generic and specific allocation, and limiting-resource queues that place tasks one after another on a resource.
- **Options it exposes:** constraint versus dependency priority per order, calendars, allocation strategy.
- **Formats:** imports MS Project and other files through MPXJ 9.0.0; REST web services (XML) for most of the entities it imports and exports.
- **Headless:** only as a server: Java, Tomcat or Jetty, and PostgreSQL or MySQL. Too heavy, and too far from lag-based CPM, for differential tests.

## OpenProject

- **Licence and version:** GPL-3.0 ([github.com/opf/openproject](https://github.com/opf/openproject)); latest release 17.9.7 on 2026-11-09. Docs: [scheduling](https://www.openproject.org/docs/user-guide/gantt-chart/scheduling/), [relations](https://www.openproject.org/docs/user-guide/work-packages/work-package-relations-hierarchies/), [working days](https://www.openproject.org/docs/system-admin-guide/calendars-and-dates/).
- **Scheduling model.**
  - **Modes:** each work package is scheduled manually or automatically. Manual is the default from 15.4 and isn't moved by relations.
  - **Relationships:** only predecessor/successor (`follows`/`precedes`) affects dates. The successor starts the working day after the predecessor finishes, plus a lag in working days that can be negative. Other relation types (relates, blocks, includes, requires, duplicates) carry no scheduling meaning.
  - **Hierarchy:** an automatic parent spans its children.
  - **Calendars:** one set of working days and non-working dates for the whole instance, with a set number of hours per day. Durations are in working days.
  - **Float, constraints, resources:** no float, critical path, constraint types or resource levelling.
- **Options it exposes:** manual or automatic per work package; instance working days and hours per day.
- **Formats:** REST API v3 (JSON), CSV, XLS and PDF exports, BCF for BIM; no MS Project XML or XER import found in its application code.
- **Headless:** only as a server (Rails, PostgreSQL), driven through its API. Finish-to-start only, so of little use for differential tests.

## MPXJ (format facts only)

- **Licence and version:** LGPL-2.1 ([github.com/joniles/mpxj](https://github.com/joniles/mpxj)); latest release 16.11.0 on 2026-11-02.
- **Formats:** reads and writes MPX, MS Project XML, Planner, P6 XER and PMXML, and SDEF; reads `.mpp`/`.mpt`, MPD, P3 and SureTrak, Powerproject, Phoenix, FastTrack, GanttProject `.gan`, TurboProject, ConceptDraw, Synchro, Gantt Designer, Schedule Grid, Project Commander, Open Plan and Edraw ([supported formats](https://www.mpxj.org/supported-formats/)). Its source tree also has a ProjectLibre reader. LibrePlan and ProjectLibre use it for import; [ARCHITECTURE.md, License and dependencies](../ARCHITECTURE.md#license-and-dependencies) keeps it out of our core.
- **Scheduling:** MPXJ ships `MicrosoftScheduler` and `PrimaveraScheduler` (package `org.mpxj.cpm`). Its [documentation](https://www.mpxj.org/howto-use-cpm/) describes them as reproducing those two products' results, so TotalFloat's rules don't use them as a source or as a reference to test against. TotalFloat uses MPXJ for format facts only.

## What the survey suggests for our rules and options

These are ideas for the scheduling options model and the rules in `docs/rules/`. None is decided yet:

- **Lag calendar.** TaskJuggler has working-time and calendar-time gaps; OpenProject and GanttProject count working days on one calendar; ProjectLibre adds elapsed and percent lags. Our lag-calendar option already names this choice; percent lag is a candidate value or a documented refusal.
- **A link as a minimum or an exact position.** Standard precedence diagramming treats a link as a minimum. GanttProject's strong link places a task exactly, and that is how it pulls tasks earlier. Our engine should say it uses minimums, which is GanttProject's rubber link plus an as-soon-as-possible start.
- **What holds a task after the project start.** GanttProject put a task before its project start through an SF link. Our rule should say the project start (or data date) bounds every early start.
- **Constraints that move dates or only warn.** TaskJuggler's `min*`/`max*` dates only report; LibrePlan lets each project choose whether links or constraints win. Both are candidates for an option, or for documenting why our constraints always move dates.
- **Manual scheduling.** OpenProject, and MS Project XML files (the `Manual` element), can mark tasks as scheduled by hand. Our import already turns them into logic-driven activities and reports it; a rule should name that.
- **Direction.** ProjectLibre schedules a whole project forward or backward; TaskJuggler picks per task. Backward scheduling is a candidate option.
- **Resource-driven durations and levelling.** TaskJuggler levels as it schedules, by priority; LibrePlan derives durations from hours and allocations; ProjectLibre's levelling is switched off. Worth a separate rule if we schedule resources.
- **Granularity.** Day-granular tools (GanttProject, OpenProject) and TaskJuggler's 5 to 60 minute slots can't express every minute-level case; differential tests must stay on their grid.

## Differential testing

Use the two tools that run without a server:

1. **GanttProject 3.3.3323** for networks with FS, SS, FF and SF links, signed whole-day lags, a working week and holidays. Write `.gan` with strong links, export to MS Project XML, read it with our existing reader (`crates/interop`), and compare early dates. Each run costs about 15 seconds of JVM start-up, so put many independent networks in one file.
2. **TaskJuggler 3.8.4** for FS and SS links with working-time (`gaplength`) or calendar-time (`gapduration`) gaps, working hours within the day, and leaves. It runs in under a second with no display. For late dates on an FS network, run a mirror file with the same tasks as ALAP, `precedes` in place of `depends`, and the final milestone fixed at the early finish; total float is then late start minus early start, in working time.

Neither tool gives total float for SS, FF or SF networks, so those float values are checked by the rule tests and property tests. These tools don't check them.

### Normalizing before comparing

- **Moments.** TaskJuggler gives moments: an ASAP task that starts at the project start begins at midnight, a successor starts at the predecessor's end (17:00), and an ALAP task starts at 08:00. GanttProject's XML puts a milestone at 08:00 on the next working day where we put it at 17:00 the day before. Compare work positions (the same point in working time), not clock spellings.
- **Locale.** GanttProject's CSV writes dates in the JVM's short locale format (`1/5/26`); its MS Project XML export writes ISO date-times, so compare through that.
- **Link meaning.** Use strong links in GanttProject and `depends` in TaskJuggler, so both place each task as early as its links allow.

### Commands

Install into a scratch directory; nothing needs to be system-wide. What was run on 2026-11-16 (Arch Linux; Ruby from conda-forge through micromamba; BellSoft Liberica Full JDK 21.0.13; Xvfb from conda-forge's `xorg-xvfb-server`):

```sh
# TaskJuggler
gem install --no-document taskjuggler -v 3.8.4
tj3 --silent net1.tjp            # writes the CSV named by the taskreport

# GanttProject: zip from the GitHub release ganttproject-3.3.3323
Xvfb :99 -screen 0 1280x1024x24 -nolisten tcp &
JAVA_HOME=/path/to/liberica-full-jdk DISPLAY=:99 \
  ganttproject-3.3.3323/ganttproject -export xml -o net1.xml net1.gan
totalfloat import net1.xml -o net1.xer   # our reader; then schedule and compare
```

In CI on Ubuntu, `sudo apt-get install -y xvfb ruby` and `xvfb-run -a ganttproject …` should do the same; that form wasn't run here. Downloads: about 55 MB for GanttProject and about 120 MB for a Liberica Full JRE (we ran the Full JDK); TaskJuggler is a small gem. Both are cheap enough to run weekly.

### The authored network

Five tasks, starting Monday 2026-01-05, with Saturday and Sunday off and an 8-hour day:

| Task | Work days | Links in |
|---|---|---|
| A | 3 | |
| B | 2 | FS from A |
| C | 4 | SS from A, lag 1 day |
| D | 1 | FF from B, lag 1 day (TaskJuggler: FS from B, which can't be FF) |
| E | milestone | FS from C, FS from D |

By hand: A Mon 5 to Wed 7; B Thu 8 to Fri 9; C Tue 6 to Fri 9; D Mon 12 (it must finish a working day after B); E at the end of Mon 12. Total float is one day for C and zero for the rest.

`net1.gan`, the whole file. Each `<depend>` sits on the predecessor and names its successor's `id`; `type` 1 is SS, 2 FS, 3 FF and 4 SF; `difference` is the lag in days; in `default-week`, 1 marks a day off.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project name="net1" company="" webLink="" view-date="2026-01-05" view-index="0" version="3.3.3323" locale="en">
  <calendars>
    <day-types>
      <day-type id="0"/>
      <day-type id="1"/>
      <default-week id="1" name="default" sun="1" mon="0" tue="0" wed="0" thu="0" fri="0" sat="1"/>
      <only-show-weekends value="false"/>
      <overriden-day-types/>
      <days/>
    </day-types>
  </calendars>
  <tasks empty-milestones="true">
    <task id="1" name="A" meeting="false" start="2026-01-05" duration="3" complete="0" expand="true">
      <depend id="2" type="2" difference="0" hardness="Strong"/>
      <depend id="3" type="1" difference="1" hardness="Strong"/>
    </task>
    <task id="2" name="B" meeting="false" start="2026-01-05" duration="2" complete="0" expand="true">
      <depend id="4" type="3" difference="1" hardness="Strong"/>
    </task>
    <task id="3" name="C" meeting="false" start="2026-01-05" duration="4" complete="0" expand="true">
      <depend id="5" type="2" difference="0" hardness="Strong"/>
    </task>
    <task id="4" name="D" meeting="false" start="2026-01-05" duration="1" complete="0" expand="true">
      <depend id="5" type="2" difference="0" hardness="Strong"/>
    </task>
    <task id="5" name="E" meeting="true" start="2026-01-05" duration="0" complete="0" expand="true"/>
  </tasks>
  <resources/>
  <allocations/>
</project>
```

Every stored start is 2026-01-05 on purpose, to show GanttProject moves tasks to where their links put them.

```text
# net1.tjp
project net1 "net1" 2026-01-05 +3m {
  timezone "UTC"
  timeformat "%Y-%m-%d %H:%M"
  workinghours mon - fri 8:00 - 12:00, 13:00 - 17:00
  workinghours sat, sun off
}
task a "A" { length 3d }
task b "B" { depends a length 2d }
task c "C" { depends a { onstart gaplength 1d } length 4d }
task d "D" { depends b length 1d }
task e "E" { depends c, d milestone }
taskreport out "out" { formats csv columns id, name, start, end hideresource 1 }
```

### Observed results

| Task | Hand | GanttProject (XML export) | TotalFloat (import of that XML) | TaskJuggler |
|---|---|---|---|---|
| A | Mon 5 – Wed 7 | 01-05 08:00 – 01-07 17:00 | 01-05 08:00 – 01-07 17:00, float 0 | 01-05 00:00 – 01-07 17:00 |
| B | Thu 8 – Fri 9 | 01-08 08:00 – 01-09 17:00 | 01-08 08:00 – 01-09 17:00, float 0 | 01-07 17:00 – 01-09 17:00 |
| C | Tue 6 – Fri 9 | 01-06 08:00 – 01-09 17:00 | 01-06 08:00 – 01-09 17:00, float 8 h | 01-05 17:00 – 01-09 17:00 |
| D | Mon 12 | 01-12 08:00 – 01-12 17:00 | 01-12 08:00 – 01-12 17:00, float 0 | 01-09 17:00 – 01-12 17:00 (as FS) |
| E | end of Mon 12 | 01-13 08:00 | 01-12 17:00, float 0 | 01-12 17:00 |

All three agree once moments are read as work positions. Our import reported that GanttProject's tasks are marked manually scheduled and are calculated by their logic, which is what we want here.

Other runs:

- **GanttProject, lag, holidays and SF:** Thursday 2026-01-08 as a holiday, A 5 days, B FS from A with lag −2, C SF from A. A ran Mon 5 to Mon 12, skipping the holiday; B ran Fri 9 to Mon 12; C ran Thu 1 to Fri 2 January, before the project start.
- **GanttProject, strong and rubber:** with E's stored start moved to 2026-01-20, strong links pulled it back to 01-13, and rubber links left it on 01-20.
- **TaskJuggler, late dates:** an FS-only network (A 3 days, then B 2 days and C 1 day after A, D 1 day after B, milestone E after C and D) run ALAP with E fixed at 2026-01-12 17:00. Late starts: A 01-05, B 01-08, C 01-12, D 01-12, against early starts A 01-05, B 01-08, C 01-08, D 01-12. Float: two days on C, none elsewhere, as by hand.
- **TaskJuggler, negative gap:** `gaplength -1d` is refused as a syntax error.

## Sources

- TaskJuggler: [repository](https://github.com/taskjuggler/TaskJuggler), [manual](https://taskjuggler.org/tj3/manual/), [rubygems.org/gems/taskjuggler](https://rubygems.org/gems/taskjuggler); `tj3man` for `depends`, `precedes`, `onstart`, `onend`, `gaplength`, `gapduration`, `scheduling`, `minstart`, `maxend`, `timingresolution`, `leaves`, `columnid`, `complete`, `booking`, `trackingscenario`, `now`, `priority`, `allocate`, `export`.
- ProjectLibre: [downloads](https://sourceforge.net/projects/projectlibre/files/ProjectLibre/), [source mirror by its author](https://github.com/claur/ProjectLibre): `Main.java` (arguments), `DependencyType.java`, `ConstraintType.java`, `Task.java` (slack fields), `DocumentFrame.java` (levelling dialog), `UpdateProjectCommand.java`.
- GanttProject: [repository](https://github.com/bardsoftware/ganttproject), [release 3.3.3323](https://github.com/bardsoftware/ganttproject/releases/tag/ganttproject-3.3.3323), [download bucket](https://dl.ganttproject.biz/), [site](https://www.ganttproject.biz/); `CommandLineExportApplication.java`, `MainApplication.java`, `CriticalPathAlgorithmImpl.java`, `ConstraintType.java` (link codes), `TaskDefaultColumn.java`.
- LibrePlan: [repository](https://github.com/LibrePlan/libreplan), [task planning guide](https://libreplan.sourceforge.net/user-documentation/en/07-planificacion.html); `DependencyType.java`, `PositionConstraintType.java`, `Dependency.java`, `OrderImporterMPXJ.java`, `README.rst`.
- OpenProject: [repository](https://github.com/opf/openproject), [scheduling](https://www.openproject.org/docs/user-guide/gantt-chart/scheduling/), [relations](https://www.openproject.org/docs/user-guide/work-packages/work-package-relations-hierarchies/), [calendars and dates](https://www.openproject.org/docs/system-admin-guide/calendars-and-dates/); `app/models/relation.rb`.
- MPXJ: [repository](https://github.com/joniles/mpxj), [supported formats](https://www.mpxj.org/supported-formats/), [CPM schedulers](https://www.mpxj.org/howto-use-cpm/).
