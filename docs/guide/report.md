# Write a report

`report` turns a template into a table of activities: filtered, grouped and sorted, as Markdown, CSV or HTML. Two templates are built in, and you can write your own.

The rows come from [`query`](query.md), so a report reads the dates stored in the file: [schedule](schedule.md) it first if it hasn't been calculated.

```sh
totalfloat schedule plant.xer -o scheduled.xer
totalfloat report critical-path scheduled.xer
```

```text
# Critical path

scheduled.xer · data date 2026-01-12 08:00 · 2 activities

| Activity ID | Activity name | Status | Early start | Early finish | Original duration hours | Total float hours |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | Synthetic activity 1 | not-started | 2026-01-12 08:00 | 2026-01-12 17:00 | 8.0 | 0.0 |
| A2 | Synthetic activity 2 | not-started | 2026-01-13 09:00 | 2026-01-13 17:00 | 8.0 | 0.0 |
```

| Built-in | What it lists |
|---|---|
| `look-ahead` | Unfinished work starting within 21 days of each project's data date, grouped by WBS path, by start |
| `critical-path` | Critical unfinished work, by start |

`--format csv` writes one header row of column names and a row per activity, for a spreadsheet. A grouped report's group comes first. Text that a spreadsheet would run as a formula (starting with `=`, `+`, `-` or `@`) gets a leading `'`, so it shows as written. `--format html` writes one self-contained page, with no scripts. `-o` writes to a new file instead of printing; an existing file is never replaced. `--project` reports one project of a file that has several, and its data date starts a look-ahead's window.

```sh
totalfloat report look-ahead scheduled.xer --format csv
totalfloat report look-ahead scheduled.xer --format html -o look-ahead.html
```

## Templates

A template is a small TOML file:

```toml
title = "Work with float"
columns = ["code", "name", "total_float_hours"]
group = "wbs"
sort = [{ column = "total_float_hours", descending = true }]

[filter]
float_min = 1
```

```sh
totalfloat report float.toml scheduled.xer
```

- `title`: the report's heading.
- `columns`: `query`'s columns, in order: `code`, `name`, `status`, `early_start`, `total_float_hours`, `code:TYPE` for an activity code, `udf:LABEL` for a user field, and the rest that `query --columns all` lists. Common ones get the usual headings, such as Activity ID.
- `filter`: `query`'s filters, by the names its options have without the dashes: `status = ["not-started", "in-progress"]`, `critical = true`, `code = "Phase=DESIGN"`, `float_max = 40`, `start_from = "2026-03-01 00:00"`, and so on. An activity has to pass all of them.
- `group` (optional): a column to group by, with a heading and a table per value. Groups are in value order, and activities with no value come last.
- `sort` (optional): columns to sort by, first key first, each with `descending = true` if wanted. Numbers sort by size, text and dates as written, and empty values last.
- `totals` (optional): columns to add up, each one of `columns`. Each group gets a total row, and a grouped report ends with the total for every activity; in CSV, a group's row reads `total: VALUE` in the first column and the report's reads `total`. Durations, units, costs, earned value amounts and number user fields add up; floats, percents and indexes don't, and are refused. At least one column has to be left untotalled, to label the total rows.
- `days_from_data_date` (optional, up to 36500): only work starting no later than this many days after its project's data date, each project with its own. It can't be combined with `filter.start_to`.

A template with a mistake, such as a filter `query` doesn't have or a column it doesn't know, is refused with what's wrong. A built-in name comes first, so a template file named `look-ahead` is given by its path, as `./look-ahead`.

```toml
title = "Durations by WBS"
columns = ["code", "name", "original_duration_hours", "budgeted_cost"]
group = "wbs"
totals = ["original_duration_hours", "budgeted_cost"]
```

Markdown escapes text that would read as markup with a backslash, so a name like `Pipe & fittings <DN50>` shows as written once rendered.

## Over MCP

The MCP `report` tool takes an open handle and either `template`, a built-in name, or `text`, a template's TOML as a file would hold it, with `project` and `format` as above. It returns the report itself, as `report` prints it: Markdown unless `format` is `csv` or `html`. A template file's path isn't accepted; read the file and pass its text.
