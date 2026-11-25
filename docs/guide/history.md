# Keep a schedule's history

A project's history usually ends up as a folder of monthly XER files, with baselines and what-ifs kept as copies of the project. The files show what the schedule looked like each month but not which changes happened in between, who made them, or why.

TotalFloat can keep that history instead. A history store is one file beside your schedule, `plant.tfdb` for `plant.xer`. Each change made through it is recorded as a changeset: what changed, who changed it (a person or an agent), why, when, and the file it produced. Nothing in the store is ever deleted.

You don't have to use it. Plain files work as before, and you can [keep XER files in git](git.md).

## At a glance

| To | Run |
|---|---|
| Start a history | `history init plant.xer -m "why"` |
| Record a change | `edit plant.tfdb … -m "why"` |
| See what happened | `history log`, `history show POINT` |
| Compare two points | `history diff plant.tfdb FROM TO` |
| Try a what-if | `history branch`, `history switch`, `history merge` |
| Have an agent's change reviewed | `edit … --propose`, `history proposals`, `history accept`, `history reject` |
| Take a change back | `history undo`, `history revert ID` |
| Bring in a P6 export | `history import plant.tfdb update.xer` |
| Name a baseline | `history baseline plant.tfdb NAME` |
| Read any point | any command on `plant.tfdb@POINT` |
| Write a point as an XER | `history export plant.tfdb -o out.xer` |
| Check it | `history verify plant.tfdb --replay` |

A point is a branch, a baseline, a proposal, or the first six or more characters of a changeset ID. Every one of these is also an MCP tool (`history_log`, `history_merge` and so on), and an agent opening `plant.tfdb` records its edits there as its own, with a reason.

## Start a history

```sh
totalfloat history init plant.xer -m "As submitted"
```

This makes `plant.tfdb` beside the file, on a branch called `main`. The file is read first, so a file TotalFloat can't read never starts a history. Your original file isn't changed.

Who you are comes from `--author`, then the `TOTALFLOAT_AUTHOR` environment variable, then your account's user name. An agent sets `TOTALFLOAT_AUTHOR_KIND=agent`, so its changes are marked as an agent's.

## Read it back

```sh
totalfloat history log plant.tfdb
totalfloat history show plant.tfdb main
```

`log` lists changesets newest first: a short ID, when, who, what kind, and the reason given (or a summary of the commands). `show` gives one in full, named by a branch or by its ID; the first six or more characters of an ID are enough.

## Change a schedule through its history

`edit` works on a store as it does on a file. Instead of writing a new file with `-o`, it records the change on the checked-out branch, with the reason you give:

```sh
totalfloat edit plant.tfdb --duration A1=15d -m "Steel delivery slipped two weeks, RFI 12"
```

The report is the same as for a file and ends with `Recorded 3f9a2c1d0e on main`. A changeset holds the edits as a `totalfloat.command/1` document, anything `--skip-failed` skipped, the impact report and the resulting file. `--dry-run` shows the impact and records nothing. A refused batch records nothing, and nor does an edit that leaves the file as it was.

If two people edit from the same point, the first to finish is recorded and the second is told the branch moved (`branch-moved`), so neither overwrites the other. Run it again on the new head.

## Try a what-if on a branch

A branch is a named line of work. Make one, check it out, and edits record there while `main` stays as it was. Nothing is copied: a branch is only a name for a point in the history.

```sh
totalfloat history branch plant.tfdb what-if/steel-late
totalfloat history switch plant.tfdb what-if/steel-late
totalfloat edit plant.tfdb --duration A1=20d -m "If the steel is three weeks late"
totalfloat history branches plant.tfdb
totalfloat history switch plant.tfdb main
```

`branch --at` starts one from any point, such as a baseline or an earlier changeset. `branches` marks the checked-out branch with `*`. `branch NAME --delete` removes a branch's name; its changesets stay in the store. The checked-out branch can't be deleted. Names use letters, digits, `-`, `_`, `/` and `.`, and can't look like a changeset ID. `edit plant.tfdb@what-if/steel-late` records on that branch without switching.

## Merge a what-if back

```sh
totalfloat history merge plant.tfdb what-if/steel-late -m "Adopt the steel recovery plan"
```

Merging brings another branch's edits onto the checked-out one by replaying them, the same way a revert replays later edits. If the checked-out branch hasn't moved since the other one started, it just moves forward, and nothing new is recorded. Otherwise the result is a merge changeset with both branches as parents. It's recorded even if the other branch's edits leave the file as it was, so the branches share that point from then on. Activities are matched by P6's own IDs, so a rename on one side doesn't hide a clash.

A merge is refused, with every conflict listed at once, when:

- both branches changed the same field of the same activity, or the same link, to different values;
- both branches made the same add or delete, which can't be made twice;
- one branch deleted an activity the other changed or linked;
- the other branch holds an import or a merge, which hold whole files and can't be replayed;
- an edit from the other branch no longer applies here.

`--dry-run` shows what would happen and records nothing. Logic that clashes without touching the same field, such as two new links that together close a loop, is caught when the merged schedule is calculated: the replayed edit is refused like any other.

## Review an agent's proposals

An edit can wait for review instead of landing straight away. That's how an agent should change a schedule someone is answerable for:

```sh
totalfloat edit plant.tfdb --duration A1=12d --propose -m "Crash A1 to recover the float"
totalfloat history proposals plant.tfdb
```

A proposal is recorded with its impact, but the branch doesn't move. `proposals` lists those waiting: who proposed each (marked when it's an agent), why, the branch it's for, and how far each project's finish would move. Then a person decides:

```text
totalfloat history accept plant.tfdb proposal/main/3f9a2c1d0e -m "Agreed with the site team"
totalfloat history reject plant.tfdb proposal/main/9b1e77aa02 -m "Out of sequence with the pour"
```

`accept` moves the branch to the proposal if the branch is where it was proposed from. If the branch has moved on, the proposal's edits are replayed on its head and recorded there, with a fresh impact; a clash refuses it with the reason. `reject` needs a reason. Either way, who decided, when and why is kept with the proposal and shown by `history show`, and a rejected proposal stays in the store: `history log --all` lists every changeset, proposals and rejected ones included.

## Name a baseline

```sh
totalfloat history baseline plant.tfdb contract
totalfloat variance plant.tfdb --baseline plant.tfdb@contract
```

Here a baseline is a name for a point already in the history, so it costs nothing to keep. `--at` names an earlier point. A baseline never moves: it can't be checked out, edited or renamed onto another point. To work from one, make a branch at it (`history branch plant.tfdb rebaseline --at contract`). `history baselines` lists them, and `baseline NAME --delete` removes a name while the point stays. Any command's `--baseline` takes `plant.tfdb@NAME`.

## Undo and revert

```sh
totalfloat history undo plant.tfdb -m "Wrong activity"
```

`undo` takes the branch's latest changeset back out. `revert` takes out any earlier one by its ID: the edits recorded after it are replayed without it, so the branch ends up as if it had never happened. Either way the result is a new changeset, and nothing is deleted, so the record shows the change and its reversal. Undo again to undo the undo.

If a later edit needed the one you're taking out (a link to an activity it added, say), the revert is refused, and the error names that edit and what it needed. Take the later one out first. An import or a merge can't be replayed, so a revert behind one is refused too.

## Bring in next month's P6 export

If you update in P6 and export each month, bring each export into the history:

```sh
totalfloat history import plant.tfdb update-1.xer -m "Update 01 from P6"
```

The export becomes the next point on the checked-out branch, recorded as an import, and you see what changed since the last point. An export identical to the branch's file records nothing. A file that shares no project with the branch is probably the wrong one, so it's refused unless you add `--force`; one that adds or leaves out a project is recorded with a warning.

An import holds a whole file rather than edits, so a revert or merge behind one can't replay it.

## Compare two points

```sh
totalfloat history diff plant.tfdb
```

With no points named, this compares the checked-out head with the point before it. Name two points, by branch, baseline or ID, to compare any pair. Above the comparison it lists the changesets between them, each with its author and reason, so you see both what changed and who changed it and why. Points on different branches are compared as files, and it says so. The comparison is the one [`diff`](diff.md) gives.

It lists at most 25 of each kind per project (changed, added and deleted activities, links, and so on), so the answer stays readable; the counts above each list are always complete. `--limit N` lists at most N, and `--limit 0` lists every one. MCP's `history_diff` and the app's `GET /api/1/history/diff?limit=` take the same `limit`; the app's History panel asks for every one.

## Check a history

```sh
totalfloat history verify plant.tfdb --replay
```

`verify` checks every stored file and changeset against its hash, and that every parent and name points at something that exists. It exits `1` and lists what's wrong if anything is damaged. `--replay` also replays each recorded edit on the file before it and compares the result with the file that was recorded. After an engine fix, today's TotalFloat may calculate a date differently from the version that made the record; those edits are listed with the version that recorded them, and the command exits `3`. The recorded files stand as what was produced at the time.

## Read any point

Every command that reads a schedule reads a store too: `plant.tfdb` is the checked-out branch's latest file, and `plant.tfdb@NAME` is any branch, baseline or changeset ID. Nothing has to be exported first:

```sh
totalfloat query plant.tfdb --columns code,name,total_float_hours
totalfloat diff plant.tfdb@main plant.tfdb
```

A file whose name really holds `@` still opens as a file.

## Export a point in history

```sh
totalfloat history export plant.tfdb -o plant-current.xer
```

This writes the file exactly as it was at that point, byte for byte, ready to submit. `--at` picks another point, by branch or ID.


## What a store costs

Each changeset keeps the whole file it produced, compressed and stored once per distinct file. On a 50,000-activity schedule (a 9 MB XER), that's about 2.6 MB per change, and recording an edit takes about 12% longer than editing the file and writing a new one. `cargo xtask history-cost` measures it on your machine.

## What isn't there yet

- **Pruning.** Every version stays in the store; there's no way yet to drop old files and keep only the commands.
- **Choosing a side in a merge.** A merge with conflicts is refused, with each conflict listed; change one branch to match, then merge again.
- **Baselines inside an exported XER.** A baseline is a point in the history; exporting it as a P6 baseline project in the same file isn't supported yet.
- **A shared server and the app.** A store is a file on your machine. Many people working on one history at once is M8, and the app is M7.
