# See what's in a file

`inspect` shows what a file holds, without scheduling it.

```sh
totalfloat inspect plant.xer
```

```text
plant.xer (8227 bytes, windows-1252, CRLF)
XER version 23.12, exported 2026-01-01

projects (1):
  1 rel-fs-positive "rel-fs-positive": data date 2026-01-12 08:00; 2 activities, 1 relationships, 1 WBS nodes
    options: retained-logic; lag calendar predecessor; finish float; critical total-float<=0h
```

A project with expenses gets a line adding them up: how many, and their budgeted, actual and remaining cost. A project with project codes gets a line naming each code's value (`codes` in JSON). It goes on to list tables, calendars, activity codes, project codes, resources, roles and diagnostics. Look at `rows left out of the model` in the diagnostics. If it isn't zero, some rows couldn't be read exactly, and the calculated results for those projects may be refused.

`calendar` shows what one calendar works. Add `--from` and `--to` to see each day in a range, which tells you whether a date is worked:

```sh
totalfloat calendar plant.xer "Synthetic weekdays" --from 2026-12-24 --to 2026-12-27
```

`roundtrip` confirms TotalFloat can write a file back exactly as it read it, byte for byte. It exits `3` if it can't.

```sh
totalfloat roundtrip plant.xer
```

To share a file for support, `anonymize` replaces identifying text and keeps the scheduling data. IDs, dates and costs stay, so review the result first.

```sh
totalfloat anonymize plant.xer --output plant-anonymous.xer
```
