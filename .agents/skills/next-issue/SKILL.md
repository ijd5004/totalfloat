---
name: next-issue
description: Pick up the next agent-ready TotalFloat issue and carry it end to end (branch, implement, test, PR, CI, merge, update the status notes). Use when asked to work the next issue, continue the build, or run a build session, attended or unattended. Accepts an optional number N to work up to N issues in sequence.
---

# Work the next issue

You are building TotalFloat one issue at a time. AGENTS.md is the contract; this skill is the step-by-step procedure. The completion condition for one cycle is: **the issue's PR is merged (or labeled `needs-human` with a precise question), the issue is closed or handed off, and `main` is green.**

Work up to N issues if a number was given with the invocation, otherwise one. Keep a checklist (todo tool) of the steps below and tick them off; an unticked step means the cycle isn't done.

## 1. Orient

```sh
gh api repos/{owner}/{repo}/milestones --jq '.[] | "\(.title): \(.open_issues) open"'
git status && git log --oneline -15
gh issue list --label agent-ready --state open --limit 50 --json number,title,labels,milestone
```

If the working tree is dirty or you're on a feature branch from an earlier session, figure out what it is (check its issue and PR) and either finish it or hand it off before starting anything new.

## 2. Verify the baseline

```sh
git switch main && git pull --ff-only && just check
```

If `main` is red, fixing it is your issue: open one if none exists, then fix it.

## 3. Pick

- Eligible: open, labeled `agent-ready`, not labeled `blocked` / `needs-human` / `needs-design`, and every "Blocked by: #n" in its body is closed (`gh issue view n --json state`).
- Order: lowest milestone (M0 before M1 …), then `P0` > `P1` > `P2`, then lowest issue number.
- Read the whole issue, its epic, and every doc or design decision it references before writing code.
- Comment: `gh issue comment <n> --body "Starting work on this."`

## 4. Branch and implement

```sh
git switch -c <n>-<short-slug>
```

- If the issue adds a feature or closes a gap, update `docs/guide/supported.md` and `docs/guide/limitations.md` in this PR.
- Build exactly what the acceptance criteria ask. Things you notice outside scope become new issues (AGENTS.md "Creating issues"), not extra code.
- Write tests alongside the code. For codec and engine work, include corpus-driven tests that skip cleanly when `$TOTALFLOAT_CORPUS` is unset.
- Engine and codec work implements a written rule: name its `docs/rules/` rule in the PR and in doc comments. When no rule covers the case, write or amend one first with the `write-rule` skill. Cite only the sources `docs/rules/README.md` allows.
- Commit in logical steps: `git commit -s -m "feat(xer): …"`.

## 5. Verify

- `just check` must pass. With `$TOTALFLOAT_CORPUS` set, the corpus tests run too; corpus names, content and results never go in the PR.
- Re-read the acceptance criteria and tick each one with evidence (test name, command output).
- Review your own full diff (`git diff main...HEAD`). For `size:M` or larger, also run `/code-review` and address real findings.

## 6. Record

Comment on the issue with what changed and anything the next session should know, and file follow-up issues for what's left.

## 7. PR, CI, merge

```sh
git push -u origin HEAD
gh pr create --title "<conventional-commit style title>" --body-file <body following .github/pull_request_template.md>
gh pr checks --watch
```

- Fix CI failures. Never skip or weaken tests.
- Merge with `gh pr merge --squash --delete-branch` if the AGENTS.md merge policy allows it. Otherwise label the PR and issue `needs-human`, comment with what needs review, and move on.
- Afterwards: `git switch main && git pull --ff-only`.

## 8. Hand off when blocked

If you can't finish (needs files we don't have, a decision, or a legal judgment; or acceptance criteria are ambiguous). A question no public source settles becomes a documented option or our own choice (`write-rule`):
1. Push your branch, open a draft PR if there is useful work.
2. Label the issue `needs-human`, and comment with the precise question, the options, and your recommendation.
3. Continue with the next eligible issue.

## Unattended sessions

When no human is watching, don't end your turn early. Specifically, don't end with a summary that announces the next step instead of taking it; don't offer to continue and wait for an answer; don't list decisions that don't actually block the remaining work; and don't stop just because a milestone or a long stretch of work finished. Put status notes in the same message as your next tool call and keep going.

Stop only when: N issues are done; no eligible issue remains; every remaining path needs the human; or something is broken that you cannot diagnose after a real attempt (say exactly what). Never take destructive or irreversible actions (force-push, deleting branches you didn't create, closing issues you didn't finish) to get unstuck.
