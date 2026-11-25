---
name: write-rule
description: Write or amend a TotalFloat calculation rule in docs/rules/ with its sources, then implement it. Use when engine or codec work needs a rule that doesn't exist, when a user reports a result they expected, or when a rule is wrong.
---

# Write a rule

A calculation question is a question about TotalFloat's rules ([ARCHITECTURE.md, Scheduling rules](../../../docs/ARCHITECTURE.md#scheduling-rules)). The answer is a rule in `docs/rules/`, written in our own words from its sources, with tests written from the rule and an engine or codec change that follows it. Never fix a case by special-casing a file.

The cycle is done when: the rule exists or is amended, `cargo xtask rules` passes, and the tests cite the rule.

## Clean room

Read `docs/rules/README.md` and [CONTRIBUTING.md, Clean room](../../../CONTRIBUTING.md#clean-room) before starting. This is the short form.

- **Sources, in order of preference:** `standard` (public CPM standards and literature), `vendor-doc` (openly licensed vendor documentation and published schemas), `observed-file` (what files in the corpus store, and files users donate from their own work), `own-choice` (our reasoned design choice). Format facts (names, codes, encodings, units) may also cite `format-fact`: any corpus file, or open-source code read for facts; never for how a schedule is calculated.
- **The corpus** is the public files `corpus/manifest.toml` lists; `cargo xtask corpus fetch` downloads them into `$TOTALFLOAT_CORPUS`. Without them, work from public sources and our own choices, and say in the issue that the corpus wasn't checked.

## 1. Classify

Say which in the issue before changing code.

- **Missing rule:** TotalFloat has no rule for the case, or its rule is wrong.
- **Option:** reasonable schedulers decide the case differently. The rule names the choice as a scheduling option with a documented default, and a file's own setting selects it where the file holds one ([ARCHITECTURE.md, Scheduling rules](../../../docs/ARCHITECTURE.md#scheduling-rules)).
- **Not a rule:** the file can't support the case (corrupt input, rows the mapping had to drop). Report it to the user clearly; no rule changes.

## 2. Find sources

- Start with `docs/rules/sources.md`, the existing `docs/rules/TF-*` rules and `docs/rules/open-source-schedulers.md`.
- A new public source goes into `sources.md` with its anchor, citation, terms and what it covers, in our words.
- An `observed-file` source describes what files in the corpus store, in general words, naming them by ID where that helps.
- When two rules fit the evidence equally well, make the choice an option, or pick one as an own choice and record the other reading under Rationale.

## 3. Write the rule

- Copy `docs/rules/template.md` to `docs/rules/TF-NNN-slug.md` with the next free number, or amend the rule that covers the case.
- Write the Statement as numbered items a test can check, and give each Sources entry its class.
- `cargo xtask rules` passes.

## 4. Implement from the rule

- Add a failing test that cites the rule ID, on a synthetic fixture we write, with expected values taken from the Statement. Then change the engine or codec until it passes.
- Doc comments on the changed code name the rule (`docs/rules/TF-NNN-slug.md`).

## 5. Report

Put the rule ID in the PR and tick "Rules changed or added record their sources". If the case isn't fully settled, open a follow-up `type:research` issue with what you learned, in general terms.
