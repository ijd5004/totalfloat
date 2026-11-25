# XER fixtures

Only synthetic files we authored, or files donated with explicit permission to publish under Apache-2.0, may live here ([CONTRIBUTING.md, Clean room](../../CONTRIBUTING.md#clean-room)). Real-world files are linked from the [corpus](../../corpus/README.md), never copied here.

| File | Origin | Holds computed results? | Header version | Tests / rules | Added in |
|---|---|---|---|---|---|
| `model-mapping.xer` | Authored synthetic data, Apache-2.0 | No; stored results are illustrative mock inputs, never computed claims | Header 23.12; hand-authored with only the columns its tests read, so XER Tools 2.2b reports missing fields and stops | Multi-project model mapping, shared entities, external ties, codes/UDFs | 2026-10-30 |
| `two-projects.xer` | `model-mapping.xer` without its link to a missing project, with project 2 calculated by this tool, Apache-2.0 | No; stored results are illustrative, never computed claims | Header 23.12; hand-authored with only the columns its tests read, so XER Tools 2.2b reports missing fields and stops | Two projects joined by a link, for editing links between projects | 2026-11-01 |
| `anonymize-clean.xer` | Authored synthetic model (one project, five-day calendar, work task, finish milestone, FS tie), Apache-2.0; explicit export date 2026-01-01 and GUID seed 42; no real schedule rows | No; task stored results are empty | Header 23.12; stored results are illustrative | Known-clean anonymizer/XER Tools 2.2b input; unused optional resource-level list absent; evidence in docs/validation/xertools-2.2b.md | 2026-10-30 |
| `anonymize-clean.expected.xer` | Deterministic anonymization of the authored `anonymize-clean.xer` with seed 42, Apache-2.0 | No | Header 23.12 | Exact bytes checked in XER Tools 2.2b with zero errors/warnings; regeneration requires renewed application validation | 2026-10-30 |
| `fresh-writer.xer` | Authored synthetic Rust model, Apache-2.0; `cargo xtask fresh-fixture`, seed 42, export date 2026-01-01 | No; stored results are illustrative | Target 23.12; XER Tools 2.2b: no errors or warnings | Fresh model export, calendar/network, resource assignment, code and UDF round trip | 2026-11-05 |
| `midnight-finish.xer` | Authored synthetic batch model with weekday/continuous calendars and an ALAP constraint, regenerated through the fixed fresh writer (seed 42, export date 2026-01-01), Apache-2.0 | No stored results | Target 23.12; XER Tools 2.2b: no errors or warnings | Midnight-ending shifts; the earlier exact output passed XER Tools 2.2b with zero errors/warnings | 2026-11-09 |
| `roles.xer` | `model-mapping.xer`'s first project with two roles (one under the other), role rates over time, a resource that fills a role, an assignment's role and an assignment by role alone, written by the fresh writer (seed 42, export date 2026-01-01) from `crates/xer/tests/roles.rs`, Apache-2.0 | No; stored results are illustrative mock inputs, never computed claims | Target 23.12; XER Tools 2.2b: no errors or warnings | Mapping, patching and writing roles, including an assignment by role alone | 2026-11-11 |
| `expenses.xer` | `model-mapping.xer`'s first project with two cost accounts (one under the other), an expense category and three expenses (one with an account, category, purchase order and vendor, one part spent, accruing at start, end and uniformly), written by the fresh writer (seed 42, export date 2026-01-01) from `crates/xer/tests/expenses.rs`, Apache-2.0 | No; stored results are illustrative mock inputs, never computed claims | Target 23.12; XER Tools 2.2b: no errors or warnings | Mapping, patching and writing expenses, cost accounts and categories | 2026-11-12 |
| `curves.xer` | `model-mapping.xer`'s first project with two resource curves (linear, and one taking the second half of the work) and its assignment on the second with 40 hours to do, written by the fresh writer (seed 42, export date 2026-01-01) from `crates/xer/tests/curves.rs`, Apache-2.0 | No; stored results are illustrative mock inputs, never computed claims | Target 23.12; not yet loaded in XER Tools | Mapping, patching and writing resource curves, and choosing an assignment's spread by edit | 2026-11-13 |
| `spreads.xer` | `model-mapping.xer`'s first project with a hand-typed spread on its assignment (planned, remaining and actual parts, curve 9 as the marker), written by the fresh writer (seed 42, export date 2026-01-01) from `crates/xer/tests/spreads.rs`, Apache-2.0 | No; stored results are illustrative mock inputs, never computed claims | Target 23.12; XER Tools 2.2b: one warning, for the curve 9 marker, as exports with typed spreads get | Mapping, patching, writing and spreading hand-typed spreads | 2026-11-13 |
| `project-codes.xer` | `model-mapping.xer`'s first project with two project code types, four values (one under another) and the project's Region code, written by the fresh writer (seed 42, export date 2026-01-01) from `crates/xer/tests/project_codes.rs`, Apache-2.0 | No; stored results are illustrative mock inputs, never computed claims | Target 23.12; XER Tools 2.2b: no errors or warnings | Mapping, patching and writing project codes | 2026-11-14 |
| `obs.xer` | `model-mapping.xer`'s first project with a three-node OBS (two under the first) and a responsible manager on each WBS node, written by the fresh writer (seed 42, export date 2026-01-01) from `crates/xer/tests/obs.rs`, Apache-2.0 | No; stored results are illustrative mock inputs, never computed claims | Target 23.12; XER Tools 2.2b: no errors or warnings | Mapping, patching and writing the OBS and WBS managers | 2026-11-14 |
| `notebooks.xer` | `model-mapping.xer`'s first project with three notebook topics, two notes on an activity (one a full HTML page, as exported notes are) and one on a WBS node, written by the fresh writer (seed 42, export date 2026-01-01) from `crates/xer/tests/notebooks.rs`, Apache-2.0 | No; stored results are illustrative mock inputs, never computed claims | Target 23.12; XER Tools 2.2b: no errors or warnings | Mapping, patching and writing notebooks | 2026-11-14 |
| `monitoring.xer` | `fresh-writer.xer` with stored thresholds (`THRSPARM`, `PROJTHRS`: one on activities' total float, one on WBS nodes, one not watched), tables placed and columns ordered as corpus files have them, values authored, Apache-2.0 | No | Header 23.12; XER Tools 2.2b: no errors or warnings (227 of 217 checks, its own count) | `crates/cli/tests/thresholds.rs`: stored thresholds read and measured | 2026-11-23 |
| `steps.xer` | `model-mapping.xer`'s first project measuring physical percent by steps, with three weighted steps on a physical-percent activity (100, 50 and 0 percent, so 50), written by the fresh writer (seed 42, export date 2026-01-01) from `crates/xer/tests/steps.rs`, Apache-2.0 | No; the weighting is TF-042's hypothesis | Target 23.12; XER Tools 2.2b: no errors or warnings | Mapping, patching and writing steps, and the step-weighted percent | 2026-11-14 |
| `usage.xer` | `model-mapping.xer`'s first project with a crew limited to 1 unit an hour, part-done work on it (16h actual, 24h remaining) and a second activity after it loading it over its limit (120h over 80h of work), written by the fresh writer (seed 42, export date 2026-01-01) and calculated by `totalfloat schedule` in `crates/cli/tests/usage.rs`, Apache-2.0 | No; stored dates are TotalFloat's own calculation | Target 23.12; XER Tools 2.2b: no errors or warnings | Resource usage per week and month against the limit | 2026-11-14 |

## Batch 001

All 23 authored TOML specs and corresponding XER inputs in
[`batches/001/`](batches/001/) are synthetic, Apache-2.0, with **no stored results**.
The generated [batch MANIFEST](batches/001/MANIFEST.md) lists every XER with its
spec/output hashes and description. Rebuild with `cargo xtask fixtures build
fixtures/xer/batches/001`; see docs/FIXTURE-SPECS.md. Expected results come from the
written rules in `docs/rules/` ([ARCHITECTURE.md, Scheduling rules](../../docs/ARCHITECTURE.md#scheduling-rules)). The batch was regenerated with the values new rows take (`new_row_defaults` in `crates/xer/src/patch.rs`); the regenerated bytes pass XER
Tools 2.2b with no errors or warnings.

## Batch 002

The authored specs and XERs in [`batches/002/`](batches/002/) are synthetic,
Apache-2.0, with **no stored results**. They hold synthetic inputs for the
calendar-state question in TF-029, zero-duration finishes in TF-014 and
span links in TF-027; those rules decide their expected results.
They pass XER Tools 2.2b with no errors or warnings. Rebuild with `cargo xtask fixtures
build fixtures/xer/batches/002`.

## Patch application validation

The authored baseline and five independent patch outputs in
[`validation/patches/`](validation/patches/) are synthetic, Apache-2.0, with
**no stored results**. Their [MANIFEST](validation/patches/MANIFEST.md) lists each
file, operation and SHA-256, including the source hash. Regenerate into a new
directory with `cargo xtask xertools-fixtures --out <directory>`; the command
does not invoke or claim application validation. The resource-default and row-default changes regenerated them from the new `fresh-writer.xer`; the current bytes pass XER Tools 2.2b with no errors or warnings.

## Supplemental anonymization validation

The authored `source.xer` and seed-42 `anonymized.xer` in
[`validation/anonymize-metadata/`](validation/anonymize-metadata/) are synthetic,
Apache-2.0, with **no stored results**. Their
[MANIFEST](validation/anonymize-metadata/MANIFEST.md) records every operation and
hash. Regenerate with `cargo xtask anonymize-fixture --out <new-directory>`;
The current bytes pass XER Tools' integrity checks with no errors or warnings (docs/validation/xertools-2.2b.md).
