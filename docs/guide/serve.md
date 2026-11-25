# Open the local app

`serve` shows a schedule in your browser: a grid and Gantt chart you can edit, with its history,
branches, an agent's proposals to review, printing and export. It runs on your computer only and
listens on `127.0.0.1`, so nothing leaves the machine.

```sh
totalfloat history init plant.xer -m "As planned"
totalfloat serve plant.tfdb
totalfloat history branch plant.tfdb what-if
totalfloat serve plant.tfdb@what-if
totalfloat serve plant.xer
```

It calculates the schedule first, as `schedule` does, then prints a link. Open the link to sign that
browser in. Add `--open` to open it in your default browser, and `--port 8400` to choose the port
(any free one otherwise). The server runs until you stop it with Ctrl-C.

- A history store (`plant.tfdb`) shows its checked-out branch, or the point after `@`. Changes are recorded on the branch shown; a baseline or changeset is read-only.
- A plain `.xer` is read-only, and `serve` says so when it starts. To edit it, start a history with `totalfloat history init` and serve the store.
- If the schedule can't be calculated, it's shown with the dates stored in the file, and `/api/1/summary` says why.

## Who can use it

- Only this computer can reach the server: it listens on `127.0.0.1`, which other machines can't connect to.
- The link holds a secret made new each time `serve` starts. Opening it signs that browser in, and every request after needs that sign-in. Treat the link like a password, and stop the server when you're done: the link stops working with it.
- Other web pages open in the same browser can't use the app. The server answers only requests addressed to `127.0.0.1` or `localhost` on its port, refuses changes sent from any other site, and gives other sites no access to its answers.
- The page is built into the `totalfloat` binary and fetches nothing from the internet. The schedule stays in the file or store you served.
- Every change is recorded in the store's history under your name, the same way a change from the command line is. Nothing is deleted: a change you regret can be taken out with a revert, which is recorded too.

## Keys

Press `?` for every key the app has, grouped as the Menu is. Ctrl is Cmd on a Mac. The app leaves the keys Microsoft Edge and Google Chrome keep for themselves to the browser, such as F10, Alt+Shift+S and Ctrl+Shift+S.

| Keys | Does |
|---|---|
| Ctrl+K | Search commands: find any command by name |
| Alt+Shift+M, or Alt on its own | Open the Menu |
| Arrows, Page Up/Down, Home, End | Move in the grid; Ctrl+Home and Ctrl+End go to the first and last cell; with Shift, select a range |
| F2, Enter, or typing | Edit the selected cell |
| Enter, Tab / Escape | Commit an edit / cancel it |
| Ctrl+Enter | Apply the edits waiting |
| Ctrl+Z | Take out the last change this page recorded |
| Ctrl+C | Copy the selected cells as tab-separated text |
| Insert / Delete | Add an activity in the selected row's WBS node / delete the selected activities, once you confirm |
| Alt+Insert, F2 on a WBS band, Alt+M | Add a WBS node under the selected one; rename a WBS node; move the selected activities to another node |
| `/` | Filter activities |
| Alt+X, Alt+Shift+D, Alt+L | Explain the selected activity's dates, in the inspector; its driving path; logic lines |
| Alt+I | Move into the inspector, and back to the grid (Escape also goes back) |
| Alt+B, Alt+A | Add a predecessor (before) or a successor (after) to the selected activity, in the inspector |
| Alt+=, Alt+- | Zoom the Gantt in, out |
| Alt+Shift+G, Alt+Shift+W, Alt+Shift+U, Alt+Shift+R, Alt+H | The Schedule, Network, Resources, Risk and History views; a view's key again goes back to Schedule |
| Alt+C, Alt+P, Alt+N | The Changes, Review and network-around-the-selection panels, in the schedule's right column |
| Ctrl+\ | Show or hide the right column (the inspector, unless another panel was chosen last) |
| Alt+Shift+E | The project picker |
| Ctrl+P, Alt+Shift+X | Print, export as XER |
| Alt+R | Reload the schedule |

## The window

The title block across the top shows the project's name and code, the views, the branch you're looking at, the data date, the finish and the float (the lowest total float of unfinished work). While edits wait to be applied, the finish and float show what they'd become, in a warning colour, with how far the finish moves. Click the project's name for the project picker: the projects in the schedule, or in a portfolio, every project to choose from. The dot shows whether the page is live: it follows changes made from the command line or an agent as they're recorded.

The views are **Schedule** (the grid, the Gantt and a right column), **Network** (the whole project's activity network), **Resources**, **Risk** and **History**. Each takes the whole work area under the toolbar, and each has a key. The schedule keeps its scroll and selection while another view is shown, and what's selected in one view is selected in the others. The right column shows one panel at a time, each with its title and a close button: the inspector, Changes, Review, or the network around the selection.

Every command is in the **Menu** (File, Edit, View, Schedule and Help), with its keys, and greyed out when it can't run. Press Alt+Shift+M, or tap Alt, to open it from the keyboard. **Search or run a command** (Ctrl+K, or Cmd+K on a Mac) finds any command by name. On a history store, **Menu > Export XER** (Alt+Shift+X) saves what you're looking at as an XER file, the same bytes `history export` writes for that point. A plain `.xer` is already the file.

The app needs a window at least 1,024 pixels wide; a 1,366-pixel laptop screen at 125% scaling gives about 1,093. Narrower than 1,280 pixels, Review, the Menu, the command search, **Group by** and **Add activity** show as their icons, with their names in their tooltips, and the filters applied collapse into a count. A long project name is cut short, and its tooltip has it whole.

## The Gantt chart

The Gantt beside the grid draws a bar for each row the grid shows, and scrolls with it:

- early dates as the bar, critical work in red;
- work done in a darker colour, up to the data date;
- total float as a thin line to the late finish;
- milestones as diamonds;
- WBS bands as summary bars.

Hover over a bar for its dates and float.

Dates show as `05 May 26` without times. **Dates as …** in the View commands chooses another way of writing them (`05-May-26`, `2026-05-05`, `05/05/2026` day first, or `05/31/2026` month first), and **Times with dates** shows the hour too. The choice is kept in the browser, and the grid, tooltips, panels and printing all use it. A cell still takes a date typed as `2026-03-02 08:00`.

Logic lines join the bars: solid for a relationship that drives its successor, dashed for one that doesn't. Hover over one for its type and lag. Press Alt+L to show all lines, only the selected activity's, or none. A line whose other end is off screen ends in an arrow at the edge; click the arrow to go to that activity. Clicking a bar selects its activity.

Select an activity and press Alt+Shift+D to highlight its driving path, back to the data date or a constraint, as `totalfloat explain` gives it. **Driving path** in the grid's toolbar then shows only those activities. Press Alt+Shift+D again to clear it. The dashed line is the data date. The chart opens fitted: the first start to the last finish of the rows the grid shows fill its width, and with a filter on, it fits the filtered rows. Zoom with Ctrl and the mouse wheel, or Alt+= and Alt+-, which step from the fit to the nearest fixed scale; **Zoom to fit the dates shown** in the command palette fits again. **Show or hide today's line** is in the command palette. Drag the bar between the grid and the Gantt to share the width.

Bars sit where the dates the server calculated fall. The browser doesn't work out any dates itself.

## Filtering and grouping

The toolbar under the title block holds, left to right: the layout open (its menu opens, saves and deletes layouts), the filters, **Group by**, the Gantt's zoom (**Week**, **Month**, **Quarter** and **Fit**), and toggles for logic lines, the driving path and the right column, each with its key in its tooltip. The box at the start of the filters filters by Activity ID or name. Each filter applied shows as a chip; its × removes it. **Add filter** turns on the quick filters (critical work, or a status) and, under **More conditions…**, opens the filter builder for anything `query` can find (see [Find activities](query.md#filters)): an activity code value, a WBS node and everything under it, a resource, a status or activity type, a range of total float in hours, critical or constrained work, or start and finish dates. A date range takes in the whole day. Add the conditions, then **Apply**. An activity code condition takes a code type and a value; the value can be typed, with the file's values offered as you type. An activity has to meet them all. **Add an “any of” group** adds a group of conditions of its own: an activity then also has to meet every condition of at least one group, as `query --any` finds it. The grid shows what `totalfloat query` would find with the same filters. If the server can't apply them, the builder says why and the filter shown stays. Each condition shows as a chip, and **Clear** in the builder removes them all. Narrower than 1,280 px, the chips show as a count. The rows are found again whenever the schedule changes.

A filter can be kept by name, apart from any layout. In the builder, type a name and choose **Save as**: the conditions shown are saved as a file in a `.filters` folder beside the schedule (`plant.filters/Critical design.json`, `totalfloat.filter/1`, [schema](../schemas/filter-1.schema.json)), holding `query`'s parameters. **Saved filter** lists them; **Apply saved** shows a saved filter's conditions and applies them, and **Delete…** removes it once you confirm. Names follow the same rules as layout names, and a name in another case is the same filter.

**Group by** bands the rows by the WBS, by an activity's own fields (status, activity type, calendar, constraint, or the month it starts or finishes), by any activity code type or user-defined field, or not at all. Status bands go not started, in progress, completed; activity types in this order (task dependent, resource dependent, level of effort, start milestone, finish milestone, WBS summary); months earliest first, from the dates the Gantt draws (actual, else early). A code type's bands follow its values' own order, and other values are in natural order: numbers by value, and text with any numbers in it by those numbers. Activities with no value come last, under **(no value)**. **then** adds a further level, up to four in all: grouped by the WBS, then a code, each WBS band holds a band for each of the code's values among its activities, after its child WBS bands. Each band at each level has its own count and dates, and folds on its own. A WBS band shows the node's name, then its code and how many activities it holds. Click a band to fold it. Printing keeps the bands and their titles.

Click a column's header to sort by it, again to sort the other way, and a third time to stop. Shift-click another header to add it as the next key, as many as you like; each sorted header shows its place.

## The inspector

The inspector, in the schedule's right column, shows everything about the selected activity, and follows whatever you select in the grid, the Gantt or the inspector itself. Ctrl+\ shows or hides it, and Alt+I moves into it and back to the grid.

- **The header**: the Activity ID, the name, the status, and a Critical marker on critical work.
- **Dates and duration**: original and remaining duration, start, finish, total and free float, calendar, and the constraint with its date; actual start and finish once work has started. The values the grid edits (the name, durations, calendar, constraint and its date, actual dates) are edited here too: click one, type, and press Enter, or Escape to put it back. The edit goes through the same preview and Apply as an edit typed in the grid, and waits in both places.
- **Predecessors and successors**: each link's other activity, its type and lag, and a hi-vis dot on the links that drive. Lags are in days of the calendar the project counts lags on. Click one to select that activity. See [Building logic](#building-logic) to add, change and remove them.
- **Why it starts**: see below.
- **Resources**, **Codes and fields** and **Notes**, closed, each with a count; click one to open it.

## Building logic

Relationships are added, changed and removed in the inspector, or drawn on the Gantt. Each change waits with the other edits, previewed, until you apply it, and is recorded as a changeset, as an edit typed in the grid is.

- **Add one by typing.** Alt+B moves into **Add predecessor** and Alt+A into **Add successor**. Type part of an Activity ID or name: the matches list with their finish (for a predecessor) or start (for a successor), and those already linked are marked. Up and Down choose one, Tab goes on to the type (FS, SS, FF or SF) and the lag, and Enter adds it. A bare number of lag is in days of the calendar the project counts lags on, or in hours when hours are shown or when that calendar is the project's default.
- **Change or remove one.** Each listed link's type and lag change in place: changing both makes one change. The × beside it removes it; **Keep** takes the removal back while it waits.
- **Draw one on the Gantt.** Drag from a bar's start or finish to another bar's start or finish. The ends set the type: finish to start is FS, start to start SS, finish to finish FF, and start to finish SF. The tooltip names the link before you let go, and Escape cancels.

A link that would close a loop is refused, with the activities in the loop named.

## Why a date is what it is

Select an activity and press Alt+X, or choose **Explain** in the Menu. The inspector's **Why it starts** section shows how its early start was set, step by step back to the data date or a constraint, as `totalfloat explain` does, numbered as the hi-vis badges on the Gantt. Its switch changes to its finish, latest start, latest finish or float. Click a step to select that activity.

**Menu > Network around selection** (Alt+N) opens it in the right column: the selected activity in the middle, its predecessors in columns to the left and its successors to the right, one to three levels each way (**Levels each way**), each column in order of early start. Each box gives the Activity ID, name, dates and total float, with critical work outlined in red. Each arrow gives the link's type and any lag, and the links that drive their successor's dates are in hi-vis, as in the Gantt. Click a box, or press Enter on it, to select that activity; the panel follows the selection.

The **Network** view (Alt+Shift+W) shows every activity the grid shows. Activities are in columns by logic: one with no predecessor among them in the first column, and every other one a column after its furthest predecessor, so every link runs left to right. Each column is in order of early start. Links that skip columns run along the gaps between rows, so none crosses a box. The grid's filters and folded bands apply, and the selected activity is outlined. Click a box to select it in the grid. Up to 3,000 activities are drawn; above that, filter the grid or fold some bands, or open the network around one activity.

## A portfolio

`totalfloat serve plan.portfolio.toml` serves a portfolio file ([Keep projects in an EPS](portfolio.md)): its first schedule is shown, and the project picker (click the project's name, or Alt+Shift+E) shows the EPS, each node's and project's figures, and a capacity chart by resource. Click a project to show its schedule. A portfolio's schedules are shown read-only. `GET /api/1/portfolio` gives `portfolio show` as JSON with each file calculated as the grid shows it, `GET /api/1/portfolio/capacity?by=month` gives `portfolio capacity`, and `POST /api/1/portfolio/open` with `{"file": "plant.xer"}` shows another of its files.

## Resource usage

The **Resources** view (Alt+Shift+U) shows a resource's units per week or month, from the same report as `query --usage` ([Resource usage](query.md#resource-usage)). Pick a resource from the list, or type its short name, choose **Week** or **Month**, and **Show**. Each period is a bar, actual units at the bottom and remaining units stacked on them; the resource's limit (its maximum units per hour over its calendar's work hours) is a dashed line, and a period over its limit is outlined in red. Above the chart the panel says the limit in force, in units an hour, and from which day when it changes (`Limit 2 units an hour; 3 from 04 Jan 27`). On a schedule you can edit, type units an hour, and a **From** day if it should start then, and choose **Set limit**, or **Clear limit**: the change waits with the other edits, previewed, until you Apply it, and the limit line moves to match. The same change is `edit --resource-limit` or the `set_resource_limit` edit ([Change a schedule](edit.md)). A table under the chart gives the same numbers, and the line above it the totals. When the schedule changes, the panel asks for the resource's usage again.

## Schedule risk

The **Risk** view (Alt+Shift+R) runs a Monte Carlo analysis of what's shown on the server, as `totalfloat risk` runs it ([Run a schedule risk analysis](risk.md)). Give the optimistic and pessimistic range in percent of each activity's remaining duration, the iterations and the seed, and if you like, a date to finish by. Or open **Estimates, correlation and risk events** and paste a `totalfloat.risk-inputs/1` document, which replaces the range. **Run** shows:

- the deterministic finish and the share of iterations that meet it, and the chance of finishing by your date;
- P10, P50, P80 and P90;
- the finish dates as a histogram, one bar a day, with the cumulative curve and the deterministic finish and P80 marked;
- the durations that move the finish most, as a tornado and a list;
- the activities most often critical;
- risk events ranked by their effect on P80, and milestones' P50 and P80.

Click an Activity ID in a list to switch to the Schedule view with it selected and in view. Nothing is changed or recorded: the analysis only reads the schedule. The same inputs and seed give the same answer as the command.

## History, branches and baselines

On a history store, the **History** view (Alt+H) lists the changes on every branch, newest first: who made each (agents are marked), when, its message, and whether it's an import, a revert, a merge or a proposal.

A graph beside the list draws the store. Each branch is a line in its own colour, with its name on the change at its head; the checked-out branch takes the first lane, and the others follow by name. Each change is a dot on its branch's line. A branch's line starts at the change it was made from, and a merge is a line joining the branch merged in to the branch it was merged into. A baseline is a label on the change it was taken at. With **Show proposals, waiting and decided** ticked, proposals are hollow dots off their branch, joined to it by a dashed line. The changes of a branch deleted after it was merged keep a grey line with no name.

Click a change, or its dot, to see what it did and the finish it gave. Then you can:

- **Compare with now**: what changed from that point to the branch as it is;
- **Branch from here** or **Baseline from here**, with a name;
- **Export XER**: the file exactly as it was then;
- **Revert**: take that change out, with a reason. This records a new change; nothing is deleted from the history. Only a change on the branch shown's own line can be taken out.

Baselines are listed below the changes, each with Compare, Export and Delete. **Compare** at the bottom takes any two points (branches, baselines or changes) and shows how the finish moved and which activities and links changed. Click an activity to select it. Tick **Show Before and After in the grid** to see each changed activity's old and new values beside it in the grid.

The branch in the title bar is a menu: pick another branch to switch to it, or create one from what's shown and switch to it. If `serve` was given a branch (`plant.tfdb@main`), the page stays on that branch; serve the store itself to follow switches.

**Merge** beside another branch brings its edits into the branch shown. It first previews what the merge would change on the branch shown, from a dry run of the merge, so work done on the branch shown since the other was made isn't listed as undone. When the branch shown has no changes of its own, merging moves it to the other's head, and the preview shows how the two heads differ. **Merge** then replays the other branch's edits and records a merge change, with an optional message. If both branches changed the same thing differently, the preview lists each clash with both values and the changes that set them, clicking one selects its activity, and there's nothing to merge until one side changes.

## Printing

**Menu > Print** (Ctrl+P, or Cmd+P on a Mac) opens the print view: what the grid shows, in its order, with its columns, beside the Gantt, as pages. Each page has a title block (the project, data date, finish, branch and changeset, and the print date), the column and timescale headers, whole rows only (none is cut across two pages), a legend and its number.

Set the paper (Letter, A4, A3 or Tabloid), the orientation, whether the grid is fitted to the page's width, the timescale's range, and the print date, then choose **Print…** and save as PDF from the browser's print dialog. The PDF is vector: its text can be searched and copied. Columns that wouldn't stay readable are left off the right, and the setup names them; hide columns in the grid, or use wider paper, to choose what prints. Nothing is worked out for printing: every date and float is the one the grid shows.

## Layouts

How the grid and Gantt are set out (the columns, their order and widths, sorting, WBS grouping, filters, the Gantt's zoom, logic lines and today line, and the split between them) can be saved as a layout. A fitted Gantt is saved as `"zoom": "fit"` and opens fitted. A layout keeps grouping by an activity field (`"group": "status"`, `"type"`, `"calendar"`, `"constraint"`, `"start_month"` or `"finish_month"`) or by an activity code or user field (`"group": "code:Phase"`), and further levels in order as `"then_by": ["code:Phase", "status"]`, every sort key in order, and a filter from the builder as `query`'s parameters (`"filters": {"built": {"code": "Phase=DESIGN", "float_max": "40"}}`), with its "any of" groups as `"any"`. If the schedule no longer has that field, the grid says so and groups by the WBS, or at a further level leaves that level out; a code the schedule no longer has is dropped from the filter, saying so, and the rest applies. Layouts are files in a `.layouts` folder beside the schedule (`plant.layouts/` beside `plant.tfdb` or `plant.xer`), one JSON file each, written the same way every time so they can be shared, kept in version control and compared.

From the command palette (Ctrl+K), or the Menu's View section: **Save layout as…** names one, **Save layout** saves over the one in use, **Open layout…** switches to another, and **Delete layout…** removes one. The layout last used on a schedule opens with it next time.

## Reviewing an agent's proposals

An agent (or a person) can propose an edit instead of making it: `totalfloat edit --propose`, or the MCP edit tool's `propose`. The proposal waits for someone to accept or reject it. The **Review** button in the title block counts what's waiting, and it updates as proposals arrive, with no reload. Click it, or choose **Menu > Review** (Alt+P), to open the proposals in the right column.

Each proposal shows who made it (agents are marked), why, when, the branch it's for, and how it moves the finish. Open one to see its edits in words and its full impact against the point it was made on. Its changed activities are lit in the Gantt and shown in the grid's Before and After columns. Then:

- **Accept** records it on its branch. If the branch has moved on since, it's replayed on the newer head, and the status bar says so. If it no longer applies (say, an activity it changes was deleted), each conflict is listed and nothing changes.
- **Reject** needs a reason.

Decided proposals stay in the history: tick **Show proposals, waiting and decided** in the History panel, and open one to see who decided and why.

## Editing in the grid

On a branch, select a cell and press F2 or Enter, or just start typing. Enter or Tab commits it;
Escape cancels. You can edit the name, original and remaining duration, calendar, constraint and
its date, and actual start and finish. Durations take hours or days (`40`, `40h`, `5d`); dates
take `2026-03-02 08:00`. Pasting a tab-separated range from a spreadsheet makes one batch: it is
recorded whole or not at all.

Each commit is previewed first, and nothing is recorded yet. If the right column is closed, the
**Changes** panel opens with it, without taking the grid's focus, so you can go on typing: make
several edits, and each one joins the preview. Another panel you have open stays open. The panel lists each edit waiting, with its activity, field, and
old and new values, and what they do together: each project's finish before and after, the
activities whose dates or float moved (click one to select it in the grid), those that became or
left critical, milestones that moved, and quality checks that newly fail or pass. **Drop** takes
one edit out and previews the rest. While edits wait, the status bar shows how
many and how far they move the finish, with **Review**, which opens the panel (or press Alt+C), and **Apply** (Ctrl+Enter). Otherwise it shows when the schedule shown was calculated and where it's saved.

Press Enter in the grid, Ctrl+Enter (Cmd+Enter), or **Apply** to record every edit waiting as one
changeset, with the panel's optional message. Escape or **Discard** drops them all. Either way, a
Changes panel that opened by itself closes again. A preview that
shows a newly failing check doesn't stop you applying it. Turn off **Preview** in the status bar to
record each commit at once; the panel then doesn't open.

A refused value marks its cell, with the problem and its hint in a tooltip and beside its edit in
the panel. If someone else changed the branch first, nothing is recorded: the page shows their
change and keeps what you typed, so you can apply it again. Ctrl+Z (Cmd+Z) takes out the last
change this page recorded, as a revert.

### Adding and deleting activities

Grouped by WBS, each WBS node's band ends with an **+ Add activity** line, and every node gets a
band, even one with no activities, unless a filter narrows the rows. Click the line, or select it
and press Enter or start typing, to open a new row there. **Add activity** in the toolbar, or
Insert, opens one at the end of the selected row's node: activities in a node show in the file's
order, and a new one comes last. The row offers the next free Activity ID following the node's
IDs: their prefix, then the step they most often take (`RC1350` then `RC1360`), skipping IDs any
activity already has. A node with no numbered IDs follows the whole schedule's, and a node with one
takes the schedule's step. You can type
another ID. Type the name, Tab to the duration, and press Enter: the activity waits with the other
edits, and the next new row opens below it, so you can type a list. Work needs a duration, and
0 adds a milestone. Escape closes the new row.

Press Delete with activities selected to delete them. The app asks first, saying how many
activities go and how many links and resource assignments go with them; Cancel has the focus.
The delete waits with the other edits until you apply it. Each add and delete is listed in the
Changes panel with **Drop**, and recorded in the same changeset as the rest. Ctrl+Z puts deleted
activities back with their links and assignments.

### WBS nodes

Alt+Insert, or **Add WBS node** in the Menu's Edit section, adds a node under the selected WBS
band, or under the selected activity's node. A line opens after the node's own child nodes, where
the new one will go: type its code, Tab to its name, and press Enter. The code can't hold a dot,
which joins codes in a node's path, and no two nodes under one parent can share a code. While it
waits, the new node shows as a band with its own **+ Add activity** line, so you can add its
activities before you apply.

Press F2 on a WBS band to rename the node: its code and name open in place. The project's own node,
at the top, follows the project and can't be renamed here. A new name shows on the band while it
waits; a new code shows once it's recorded.

Alt+M, or **Move to WBS node…**, moves the selected activities to another node: type part of the
node's code or name, choose it, and press Enter. The activities show under the new node while the
move waits. Like the other edits, each WBS change is listed in the Changes panel with **Drop**,
recorded in the changeset you apply, and taken back out by Ctrl+Z.

The page is built into the `totalfloat` binary, so nothing is fetched from the internet. A build
from source without the app shows a page saying how to build it: `cargo xtask ui`, then build
again (see [ui/README.md](../../ui/README.md)).

## The read API

Pages and agents use the same JSON API under `/api/1/`. Each call needs the sign-in cookie and the
token from the link, sent in the `x-totalfloat-session` header. Errors use `totalfloat.error/1`.

| Endpoint | Answer |
|---|---|
| `GET /api/1/summary` | `inspect`'s report, plus the branch, head changeset, and whether it's writable (`totalfloat.summary/1`, [schema](../schemas/summary-1.schema.json)) |
| `GET /api/1/rows` | every activity with its dates, durations, float and constraints, one array per field (`totalfloat.rows/1`, [schema](../schemas/rows-1.schema.json)) |
| `GET /api/1/links` | every relationship: predecessor, successor, type, lag and whether it drives, one array per field (`totalfloat.links/1`, [schema](../schemas/links-1.schema.json)) |
| `GET /api/1/wbs` | each project's WBS tree, under `projects` (`totalfloat.wbs/1`, [schema](../schemas/wbs-1.schema.json)) |
| `GET /api/1/explain?activity=A1000&field=early_start` | `explain`'s report (`totalfloat.explain/1`, [schema](../schemas/explain-1.schema.json)) |
| `GET /api/1/check` | `check`'s report (`totalfloat.check/1`, [schema](../schemas/check-1.schema.json)) |
| `GET /api/1/query?code=Phase%3DDESIGN&columns=id,code:Phase` | `query`'s report (`totalfloat.query/1`, [schema](../schemas/query-1.schema.json)), with `query`'s filters and columns as parameters: lists such as `status` and `columns` with commas between their items (`\,` for a comma within one, such as a user field's label, and `\\` for a backslash), and `critical` and `constrained` as `true`. `any` is the groups of [`--any`](query.md#any-of-several-groups) as a JSON array of objects with text values, as a named filter keeps them: `any=[{"critical":"true"},{"float_max":"8"}]`, URL-encoded. Every match unless `limit` is given. A baseline file can't be given |
| `POST /api/1/risk` | a schedule risk analysis of what's shown, as `risk --json` gives it (`totalfloat.risk-analysis/1`, [schema](../schemas/risk-analysis-1.schema.json)). The body takes `inputs` (`totalfloat.risk-inputs/1`, [schema](../schemas/risk-inputs-1.schema.json)), and `project`, `iterations` (1,000 by default, at most 10,000), `seed` and `target`. It changes nothing. One analysis runs at a time; another meanwhile is refused with `risk-busy` |
| `GET /api/1/fields` | Every activity code type with all its values in sequence, and every activity user-defined field with its type (`totalfloat.fields/1`, [schema](../schemas/fields-1.schema.json)), however many there are; the summary's `inspect` lists at most 100 of each. The grid's filter builder and Group menu read it |
| `GET /api/1/usage?resource=CODE&by=week` | A resource's units per week or month against its limit (`totalfloat.usage/1`, [schema](../schemas/usage-1.schema.json)), as `query --usage` gives them |
| `GET /api/1/events` | server-sent events as the store changes; see [Following changes](#following-changes) |

Add `?at=POINT`, a branch, baseline or changeset, to read another point in the store's history.
It doesn't change what the server shows.

## Changing the schedule

Every change goes through the history store as a changeset, authored by you: the name from
`TOTALFLOAT_AUTHOR`, or your account's, and always marked as a person's, since the app is yours.
Changes are made one at a time. After each one the schedule is calculated again, and the answer's
`x-totalfloat-head` header gives the changeset the page now shows.

| Endpoint | Does |
|---|---|
| `POST /api/1/edit` | Records a batch of edits: `{"edits": [...], "head": "...", "message": "why"}`, plus `dry_run`, `skip_failed`, `propose`, `check`, `project`, `data_date` and `as_planned` as in `edit`. The body is `totalfloat.edit-request/1` ([schema](../schemas/edit-request-1.schema.json)), and its edits are `totalfloat.command/1` ([schema](../schemas/command-1.schema.json)). The answer is `edit --json`'s report (`totalfloat.edit/1`, [schema](../schemas/edit-1.schema.json)). |
| `GET /api/1/history/log`, `/history/show/POINT`, `/history/diff?from=&to=`, `/history/proposals` | Read the history, as `history log`, `show`, `diff` and `proposals` do |
| `POST /api/1/history/revert`, `/history/branch`, `/history/baseline`, `/history/merge`, `/history/accept`, `/history/reject` | Change it, with the same arguments as the MCP tools of the same names, without `store`; `reason` is yours to give or not, and `head` guards as in an edit |
| `GET /api/1/history/export?at=POINT` | Downloads the XER at a point, the same bytes `history export` writes |
| `GET /api/1/filters`, `GET`, `PUT` and `DELETE /api/1/filters/NAME` | Lists, reads, saves and deletes named filters (`totalfloat.filter/1`, [schema](../schemas/filter-1.schema.json)): `{"schema": "totalfloat.filter/1", "built": {"code": "Phase=DESIGN"}}`, with at least one of `query`'s filters, in `built` or in `any`, its "any of" groups: `"any": [{"critical": "true"}, {"float_max": "8"}]`. The list and a deletion are `totalfloat.filters/1` ([schema](../schemas/filters-1.schema.json)). Files and names work as layouts' do. |
| `GET /api/1/layouts`, `GET`, `PUT` and `DELETE /api/1/layouts/NAME` | Lists, reads, saves and deletes layouts (`totalfloat.layout/1`, [schema](../schemas/layout-1.schema.json)); the list and a deletion are `totalfloat.layouts/1` ([schema](../schemas/layouts-1.schema.json)). A saved layout is answered as written: pretty-printed in a fixed key order. A name can't leave the layouts folder, and a layout that doesn't fit the schema is refused with the field named. |

`head` is the changeset the page last saw. If the branch has moved past it, because someone or
something else changed it, the edit is refused with `branch-moved` (409), so a change never lands
on work you haven't seen. The check is made against the store, not the page, and again as the
change is recorded, so a change made from the command line or by an agent counts too. A refusal
catches the page up with the branch, so reload and try again. A refused batch answers 422 with
every problem listed. Reads of the history default to the branch shown, and a new branch or
baseline starts from it unless `at` says otherwise. Revert and merge act on the store's
checked-out branch, as `history revert` and `history merge` do, so they're refused with
`not-checked-out` (409) unless that's the branch shown, and the branch shown can't be deleted. On
a plain `.xer`, every change answers `read-only` (403); start a history with
`totalfloat history init` and serve the store.

## Following changes

While a store is served, the server looks at it twice a second. When anything changed, it tells
every open page through `GET /api/1/events`, whoever made the change: the page, the command line,
or an agent through MCP. A page connects with `EventSource`, which can't set headers, so it gives
the token as `?session=TOKEN`. Each event's data is a `totalfloat.event/1` document
([schema](../schemas/event-1.schema.json)), and the SSE event name is its `event`:

| Event | Says |
|---|---|
| `changeset` | a changeset was recorded: its log entry, and the branch it's on or proposed for |
| `proposal` | a proposal was made (`new`) or accepted or rejected (`decided`), with the changeset |
| `branch` | a branch was `created`, `moved` or `deleted`, with where it points now and before |
| `view` | the schedule shown is now another point, already calculated: fetch what you show again |

The schedule is calculated again only when the point served moved; a look at an unchanged store
reads three small things and nothing more. A page that falls too far behind gets `resync`
instead, and should fetch everything it shows again. A plain `.xer` has no events.
