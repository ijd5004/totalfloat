# Work with an agent

`totalfloat mcp` lets an AI agent use all of the above. [agents.md](../agents.md) covers connecting Claude Code, Claude Desktop and other clients.

The agent calls the engine for every date and float. It doesn't work them out, so ask it to show you the tool results behind anything important.

Try prompts like these, with your own file paths and activity IDs. Each lists the tools it should lead to, which are all tested. The prompts themselves haven't been tried in every client, and agents differ.

## What drives this milestone?

> Open `/data/plant.xer` and schedule it. What drives milestone M1000? Trace it back and tell me which activities have no float.

The agent uses `open`, `schedule`, `query` with `driving_path_to`, and `explain` on the milestone.

## Try a change

> Open `/data/plant.xer`. What happens to Substantial Completion if the foundation takes 15 days instead of 10?

The agent uses `open`, then `edit` with `dry_run`. To keep the change, it edits without `dry_run` and uses `export` to write a new file. Your original never changes.

## Record a status update from field notes

> Here are this week's field notes for `/data/plant.xer`: the slab pour finished Tuesday, framing started Wednesday with about two weeks left, and the fence is half done. Move the data date to Monday 8:00, show me the impact, then save it once I agree.

The agent turns the notes into `set_progress` edits, one per activity, and calls `edit` with `dry_run` and the new `data_date`. It shows you the edits it made and the impact before calling `edit` again without `dry_run`, then `export`. Check the activities and dates it chose, because the notes are yours to interpret: it only translates them into edits. If the notes are already a table, `totalfloat edit --progress-table` takes the CSV directly.

## Compare two updates and draft a narrative

> Open `/data/update-5.xer` and `/data/update-6.xer` and compare them. Draft a monthly narrative covering finish movement, milestone changes, changes to the critical path, and anything flagged. Use only what the comparison shows.

The agent uses `open` twice, then `diff`. `counts` is complete, and each list keeps 100 items, so on a large update ask it to say when a list is cut short.

## Write the month's narrative from the variance

> Open `/data/update-6.xer` and the approved baseline `/data/baseline.xer`. Write this month's schedule narrative: finish movement, milestones that slipped, the biggest slips, critical path changes and where float went. Use only the variance report, and quote its numbers.

The agent uses `open` twice, then `variance` with `current` and `baseline`. Every date, variance and count in the narrative should come from that report. If the narrative mentions a number the report doesn't hold, ask where it came from.

## Check this submission

> Open `/data/contractor-submission.xer` and run the schedule checks. List what failed, with the worst offenders, and tell me which failures you'd raise with the contractor first and why.

The agent uses `open` and `check`, then `explain` or `query` on what it finds.

## Try a date

> Open `/data/plant.xer`, schedule it with a data date of 2 February 2026 at 08:00, and tell me how the finish moved. Don't write a file.

The agent uses `open`, then `schedule`, which keeps results on its handle. Nothing is written unless you ask it to `export`, and that goes to a new file you choose.

## Propose an edit for review

> `/data/plant.tfdb` is the history for our contract schedule. Propose crashing the steel erection to recover the float lost last month, with your reasoning, and leave it for me to review.

The agent uses `open` on the store, tries the change with `edit` and `dry_run`, then calls `edit` again with a `reason` and `propose: true`. That records it as a proposal: the agent's name, its reason and the change's impact are kept, and the schedule itself doesn't change. You see what's waiting with `totalfloat history proposals /data/plant.tfdb` and decide with `history accept` or `history reject`. Who decided, when and why is kept with the proposal. An agent can also `history_review` a proposal, but only do that when you've told it your decision.

## Work while you watch

> `/data/plant.tfdb` is our contract schedule, and I have it open in the app. Propose ways to recover the steel delay, one proposal per idea, each with your reasoning. I'll review them as they arrive.

You serve the store with `totalfloat serve /data/plant.tfdb` and keep the page open ([Open the local app](serve.md)). The agent uses `open` on the store, tries each idea with `edit` and `dry_run`, then records it with `edit`, a `reason` and `propose: true`. Each proposal reaches the page as it's made, with no reload: the **Review** button counts it, and the Review panel (Alt+P) shows who proposed it, why, and how it moves the finish, with its changed activities lit in the Gantt and their old and new values in the grid. Accept or Reject each one there; a rejection needs a reason. The schedule doesn't change until you accept a proposal. The agent sees what's still waiting with `history_proposals`, and what you accepted with `history_log`. Before its next idea it calls `open` on the store again, so it works on the schedule as you left it: a handle keeps the head it was opened at, and once the branch has moved, an edit recorded through an older handle is refused with `branch-moved`.

## Edit the schedule as text

> `/data/plant.tfdb` is the history for our contract schedule. Write it out as text, add the three-week concrete cure the engineer asked for after the slab pour, with its logic, and propose it for my review. Show me the impact first.

The agent uses `open` on the store, then `export` with format `tfs` to a new file, and edits that file with its ordinary file tools: one activity per line, with predecessors on the same line ([Schedules as text](text.md)). It calls `apply_text` with the edited text and `dry_run` to show the impact, then again with a `reason` and `propose: true`. The text becomes ordinary edits, so the proposal shows exactly which commands it made, and you review it as any other. If the text leaves out activities the schedule has, `apply_text` refuses and lists them, unless `missing` is `keep` or `delete`, so a partial file can't delete work by accident.

## Build a schedule from a description

> Draft a schedule for a two-storey steel-frame office from the scope in `/data/scope.md`: a data date of 2 March 2026, a five-day calendar, a WBS by trade, and logic you can justify. Write it to `/data/office.xer`, and keep the text so I can review it.

The agent writes the schedule as text (`totalfloat-schedule 1`, a project line with its `data-date`, then WBS, activities and predecessors) and calls `new` with that `text` and the path. Every problem in the text comes back at its line, so it corrects and tries again. The dates come from TotalFloat's engine, never from the agent: the text holds durations and logic only. Ask it to keep the text file beside the XER, so the schedule can be read top to bottom and changed later with `apply_text`.
