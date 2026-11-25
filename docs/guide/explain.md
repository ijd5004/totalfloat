# Find out why a date is what it is

`explain` walks a date back to what started it. Ask about an activity by its Activity ID:

```sh
totalfloat explain plant.xer A2 --field early_finish
```

```text
A2 early finish 2026-01-13 17:00: its early start and its duration
A2 early start 2026-01-13 09:00: FS with 1h lag on the predecessor calendar from A1 early finish
A1 early finish 2026-01-12 17:00: its early start and its duration
A1 early start 2026-01-12 08:00: data date
```

Read from the top: the finish comes from the start, the start from a link to A1, and so on back to the data date.

`--field` takes `early_start`, `early_finish`, `late_start`, `late_finish` or `total_float`. Total float shows both dates it compares:

```sh
totalfloat explain plant.xer A1 --field total_float
```

Where a calendar gap leaves time that total float doesn't count ([TF-032](../rules/TF-032-float-across-calendar-gaps.md)), `total_float` gives the true value as well, and `check` lists those activities.

Explanations come from a fresh calculation, not the stored dates. If `explain` disagrees with a date in the file, that's worth knowing: see [Limitations](limitations.md).
