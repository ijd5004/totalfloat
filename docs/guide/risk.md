# Run a schedule risk analysis

A deterministic schedule gives one finish date. A risk analysis asks how sure that date is. `risk` draws each unfinished activity's remaining duration from a three-point estimate, calculates the schedule with those durations, and does that a thousand times. The finish dates it collects give the odds of each finish date.

Every date comes from the same engine as `schedule`. The draws only choose durations.

## A quick range

The fastest start is one range for every unfinished activity, in percent of its remaining duration:

```sh
totalfloat risk plant.xer --range=-10,25
```

```text
Project rel-fs-positive (1): 1000 iterations, seed 1, 2 activities sampled
  deterministic finish: 2026-01-13 17:00 (19% of iterations finish by then)
  P10 2026-01-13 16:40, P50 2026-01-13 17:43, P80 2026-01-13 18:27, P90 2026-01-13 18:54
  ...
```

- **Deterministic finish:** the finish with durations as they are, and the share of iterations that finish by it. A low share is common: when paths merge, the latest one wins, so uncertainty pushes the finish later on average.
- **P10, P50, P80, P90:** the finish that many percent of iterations meet. P80 is a common commitment date.
- **Mean, standard deviation and range** of the finish.
- **Milestones:** each unfinished finish milestone's P50 and P80, latest first.
- **Most often critical:** the criticality index, the share of iterations in which an activity was critical under the project's own definition.
- **Durations that move the finish most:** the Spearman rank correlation of each activity's drawn duration with the finish, from -1 to 1. This is the tornado chart's ranking.

`--target` adds the chance of finishing by a date:

```sh
totalfloat risk plant.xer --range=-10,25 --target "2026-01-14 12:00"
```

## Estimates per activity

For real work, give estimates in a JSON file (schema [`totalfloat.risk-inputs/1`](../schemas/risk-inputs-1.schema.json)):

```json
{
  "schema": "totalfloat.risk-inputs/1",
  "default": {"min": -10, "max": 25},
  "estimates": {
    "A1": {"min": "4h", "likely": "8h", "max": "16h", "distribution": "pert"}
  },
  "correlations": [
    {"activities": ["A1", "A2"], "coefficient": 0.5}
  ]
}
```

```sh
totalfloat risk plant.xer --inputs risk.json --iterations 2000 --seed 7
```

- **`estimates`**, by Activity ID: `min`, `likely` and `max` as spans (`8d`, `60h`, `2w`), measured on the activity's calendar like edits. `likely` defaults to the remaining duration. `distribution` is `triangle` (the default), `pert` (BetaPERT, weighted toward the most likely value) or `uniform`.
- **`default`**: a range in percent for unfinished activities without an estimate. Without it, they keep their remaining duration. `--range` sets it from the command line.
- **`udfs`**: P6 users often keep estimates in activity user-defined fields. `{"min": "Optimistic", "likely": "Most Likely", "max": "Pessimistic"}` names number fields holding days on each activity's calendar. `likely` is optional, and an empty most-likely field means the remaining duration. An estimate in `estimates` wins over the fields.
- **`correlations`**: activities whose durations tend to move together, such as work by the same crew. A coefficient of 1 moves them in step, and 0 leaves them independent.

Finished activities, milestones, level of effort and WBS summary activities are never drawn. Neither is an activity whose expected finish sets its duration, when the project uses expected finish dates: the report lists those as not drawn. Every problem in the file is listed at once, by Activity ID.

## Risk events

Uncertain durations aren't the only risk. A risk event may or may not happen: a late crane, a permit that slips. Give each one a chance and the delay it would cause:

```json
{
  "default": {"min": -10, "max": 25},
  "risks": [
    {"code": "R-01", "probability": 0.3,
     "impacts": {"A1": {"min": "2d", "likely": "5d", "max": "10d"}}}
  ]
}
```

In each iteration a risk happens with its probability. If it does, each activity it affects takes longer by a delay drawn from its range, added to the drawn duration (or the remaining duration, if the activity has no estimate). Without `likely`, the delay is uniform between `min` and `max`. A risk can't delay finished work, milestones, spans, or an activity whose expected finish sets its duration.

The report ranks the risks by their effect on P80: P80 with every risk, less P80 with that one risk switched off, in calendar days, with the same seed so nothing else changes. P50 is given the same way, with the share of iterations each risk happened in.

Working out a risk's effect runs again only the iterations it happened in, with it off. A risk that happens 30% of the time adds about 30% to the run.

## Repeatable results

`--iterations` runs from 1 to 100,000. `--seed` (default 1) fixes the random draws, so the same file, inputs and seed always give the same result, on any machine and any number of cores. Change the seed to see how much the numbers move between runs. If they move more than you can accept, raise `--iterations`.

A file with several projects needs `--project`. Links to other projects in the file read their stored dates, as `schedule` does for a project that isn't open.

In the app, the **Risk** view (Alt+Shift+R) runs the same analysis and draws it ([Open the local app](serve.md#schedule-risk)). `--json` gives schema `totalfloat.risk-analysis/1` ([docs/schemas/risk-analysis-1.schema.json](../schemas/risk-analysis-1.schema.json)), with P5 to P95 in steps of 5 and the share finished by the end of each day. The MCP tool `risk` takes the same inputs as an object, up to 10,000 iterations.

## What it doesn't do yet

- Cost risk.
