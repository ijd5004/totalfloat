# Keep projects in an EPS

An enterprise project structure (EPS) files projects under a tree of nodes, such as *Enterprise > North America > Refining*. XER files leave the tree out. Each project's root WBS node keeps only the ID of the EPS node it sat under, not the node's name or where it was in the tree.

TotalFloat keeps the tree in a **portfolio file** beside your schedules: a short text file that names the nodes and puts projects under them. It never changes the schedules.

## Start one from your schedules

```sh
totalfloat portfolio new plant.xer two-projects.xer -o plan.portfolio.toml
```

This writes one node for each EPS node ID the files keep, named `EPS <id>`, under a root called `Enterprise`, and puts every project of every file under its node. A project whose file keeps no EPS ID goes under the root. An existing file is never replaced.

Then open the file and give the nodes their real names, add levels, and move projects:

```toml
schema = "totalfloat.portfolio/1"

[[eps]]
id = "ENT"
name = "Enterprise"

[[eps]]
id = "NA"
name = "North America"
parent = "ENT"

[[project]]
file = "plant.xer"   # relative to this file
project = 9438       # proj_id; leave it out for every project in the file
eps = "NA"
```

Each `[[eps]]` node has an `id`, a `name` and, except at the top, a `parent`. Each `[[project]]` names a `file`, optionally one `project` in it, and the `eps` node it sits under. Every problem is reported at once, with its line: two nodes with one ID, a parent that isn't there or a loop of parents, a file that can't be read, a project the file doesn't hold, or a project placed twice.

## See the tree

```sh
totalfloat portfolio show plan.portfolio.toml
```

Each node is followed by its projects, indented by depth. For each node and project it gives:

- activities, and how many are not started, in progress and finished;
- the earliest start (actual, or early for work not started) and the latest finish (actual for finished work, early for the rest);
- the critical activities, by each project's own definition of critical, and those with negative total float;
- budgeted, actual and remaining cost of assignments and expenses.

A node's figures add up every project at or under it. Each project also shows its data date and, if it has a must-finish-by date, how many calendar days its finish is ahead of it (negative is late).

The dates are the results each schedule stores, as the file was last calculated. Run `schedule` on a file first if it's out of date. So in `--json` every project gives `results: "stored"`, and each node's `stored_results` equals its `projects`. The app calculates them instead: see [In the app](#in-the-app).

`--json` gives schema `totalfloat.portfolio-show/1` ([docs/schemas/portfolio-show-1.schema.json](../schemas/portfolio-show-1.schema.json)), and the MCP tool `portfolio_show` takes the portfolio file's path.

## Capacity across the projects

```sh
totalfloat portfolio capacity plan.portfolio.toml --by month
```

For every resource with work in the portfolio's projects, by week (the default) or month:

- **demand**: its assignments' units, actual over their actual dates and remaining over their remaining dates as the files store them, spread as [`query --usage`](query.md) spreads them, added up across every project;
- **supply**: the most it can do in the period, its maximum units per hour over its calendar's work hours, from the first project in the portfolio that has it;
- the periods where demand is over supply, and the worst one, with each project's share of it.

A resource in two files is one resource when its short name (its Resource ID in P6) is the same: IDs aren't shared between databases. Work staffed by a role with no resource named counts against the role, with the role's maximum units on the project's default calendar.

`--resource CREW` or `--role ENG` shows just that one, period by period. `--json` gives schema `totalfloat.portfolio-capacity/1` ([docs/schemas/portfolio-capacity-1.schema.json](../schemas/portfolio-capacity-1.schema.json)), and the MCP tool `portfolio_capacity` takes the path and the same choices. Leveling isn't done: capacity shows where the over-allocations are, not how to move work to remove them.

## In the app

```text
totalfloat serve plan.portfolio.toml
```

Serving a portfolio file opens its first schedule as usual, plus the project picker: click the project's name in the title block, or press Alt+Shift+E. It shows:

- the EPS, each node with its figures and its projects under it. The app calculates each file as it shows it in the grid, so the figures agree with the grid. A file it can't calculate, such as one with a loop of relationships, keeps the results it stores, and its line says so, as does each node above it. Each project in `GET /api/1/portfolio` gives `results`, `calculated` or `stored`, and each node's figures count its projects with stored results in `stored_results`;
- a project button that shows that project's schedule in the grid and Gantt;
- a capacity chart for each resource with demand, by week or month: demand as bars, supply as a line, and over-allocated periods in red, with the worst one's project shares under it. It opens on the most over-allocated resource.

A portfolio's schedules are shown read-only, one file at a time. To edit one, serve it on its own.

## What it doesn't do yet

- Writing a project's EPS node into the XER.
