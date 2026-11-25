---
name: plan-milestone
description: Rolling-wave planning for TotalFloat. Break a milestone epic (e.g. "M5") into detailed, agent-ready GitHub issues with labels, dependencies and sub-issue links. Use when the next milestone has no agent-ready issues left, or when asked to plan or re-plan a milestone.
---

# Plan a milestone

Turn a milestone epic into issues an agent with no other context can implement, one PR each.

## 1. Gather context

- Read the milestone's description and its epic (the goal and exit criteria), `docs/ARCHITECTURE.md` (its design decisions), `CONTRIBUTING.md` and `docs/rules/`.
- Read the epic issue: `gh issue list --label type:epic --milestone "<milestone title>"`, then `gh issue view <n>`.
- Look at what exists in code now, and at closed issues from the previous milestone, so new issues build on reality rather than on the plan.
- If the milestone needs a design decision that `docs/ARCHITECTURE.md` doesn't cover, the first issue is a `type:research` + `needs-design` issue to settle it and record it there, and implementation issues depend on it.

## 2. Draft the breakdown

- Every issue is one PR, sized `size:S` or `size:M`. Split anything bigger.
- Order issues so dependencies come first; record them as "Blocked by: #n" in the References section.
- Each body uses the task template sections: **Context**, **Scope**, **Acceptance criteria** (testable checkboxes, ending with "`just check` passes"), **Out of scope**, **References**.
- Name concrete crates, modules, commands, file formats and test names. Vague issues waste whole sessions.
- Work that needs the human (files we don't have, decisions, money, legal) gets its own issue labeled `needs-human` and assigned to the repo owner.
- Together, the issues must cover every exit criterion in the milestone. Check this explicitly before creating anything.

## 3. Create

Labels: one `type:*`, one or more `area:*`, one of `P0`/`P1`/`P2`, one `size:*`, and one status label (`agent-ready`, `blocked`, `needs-design`, or `needs-human`).

```sh
gh issue create --title "<area>: <imperative summary>" --body-file <file> \
  --milestone "<milestone title>" --label "type:feature,area:engine,P1,size:M,agent-ready"
# attach as a sub-issue of the epic (needs the new issue's numeric id, not its number):
id=$(gh api repos/{owner}/{repo}/issues/<new-number> --jq .id)
gh api repos/{owner}/{repo}/issues/<epic-number>/sub_issues -X POST -F sub_issue_id=$id
```

## 4. Close the loop

- Update the epic body's checklist to list the new issues.
- Comment on the epic with a short summary of the plan.
