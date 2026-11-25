# Keep schedules in git

A history store isn't the only way to keep a schedule's history. If your team already uses git, keep the XER files there.

TotalFloat's own edits change only the cells that changed, so `git diff` on its output is already small. Two exports of the same schedule can differ throughout, with rows in a different order, so a plain diff between them is mostly noise. `totalfloat textconv` gives git a readable view of any XER instead:

```sh
totalfloat textconv plant.xer
```

It prints each project as sorted lines: one per activity with its inputs (name, type, WBS, calendar, durations, constraints, actuals), one per link (`link A1 FS A2 lag 1h`), each calendar with its exceptions, each resource, and then each activity's stored dates last, so input changes read first. The rows come out in the same order whatever order the file holds them in.

## Set up git to use it

Tell git which files to show this way, in `.gitattributes` at the top of the repository:

```text
*.xer diff=totalfloat
```

and how, once per clone:

```text
git config diff.totalfloat.textconv "totalfloat textconv"
```

Now `git diff`, `git log -p` and `git show` print the readable view. A duration change shows as one changed `activity` line, and the `dates` lines below it show what recalculating moved:

```text
-activity A1 | Pour slab | task | wbs CIV | calendar Standard | original 40h | remaining 40h | …
+activity A1 | Pour slab | task | wbs CIV | calendar Standard | original 80h | remaining 80h | …
```

The files in the repository stay exactly as P6 or TotalFloat wrote them. This only changes how git shows the difference. For the full comparison, with float, the critical path and anything suspicious flagged, use [`totalfloat diff`](diff.md) between two versions.

## Git or a history store?

Git gives you what your team already knows, but it knows nothing about schedules: who changed what is whoever committed, and a merge is a text merge. A [history store](history.md) records each edit as a typed command with its reason and impact, keeps proposals waiting for review, and merges by replaying edits with schedule-aware conflicts. Either works, and so do plain files.
