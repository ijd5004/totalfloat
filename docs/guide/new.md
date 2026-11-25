# Start a new schedule

`new` writes a new XER with a project, a root WBS node and a five-day calendar called Standard 5-Day.

```sh
totalfloat new --code PLANT --name "New Plant" --data-date "2026-03-02 08:00" -o new-plant.xer
```

```text
Project PLANT "New Plant": data date 2026-03-02 08:00, 0 activities, 0 relationships
Wrote new-plant.xer
```

The root WBS node takes the project's code. Build on the file with [`edit`](edit.md), or give the work up front with `--edits` and a JSON list. It's the same list `edit` reads.

```sh
totalfloat new --code PLANT --name "New Plant" --data-date "2026-03-02 08:00" --must-finish "2026-06-30 17:00" --edits my-build.json -o built-plant.xer
```

```json
[
  {"op": "add_activity", "code": "A1", "name": "Excavate", "wbs": "PLANT", "duration": "5d"},
  {"op": "add_activity", "code": "A2", "name": "Pour", "wbs": "PLANT", "duration": "10d"},
  {"op": "add_link", "predecessor": "A1", "successor": "A2"}
]
```

The result depends only on what you give it: the export date is the data date, and generated IDs follow the project code. Any path that exists is refused, and the must-finish date can't be before the data date.

Resources, roles and code types come from edits too: `add_resource`, `add_role` and `add_code_type`, then `add_code_value`, `assign_resource`, `assign_role` and `set_code`.
