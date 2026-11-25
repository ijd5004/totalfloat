# TotalFloat's rules

This folder holds TotalFloat's own scheduling rules ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules)). A rule says, in our own words, what TotalFloat calculates and why: how a relationship's lag is measured, which activities are critical, where a date constraint holds. The engine and the file codecs implement the rules written here, and each rule records the sources it rests on.

A rule is TotalFloat's rule. It doesn't claim to describe what another product does. XER and MS Project XML files are translated into and out of the model these rules define.

## Files

| File | What it holds |
|---|---|
| `TF-NNN-slug.md` | One rule, written from [template.md](template.md). `NNN` is a three-digit number, never reused; `slug` is a few lowercase words joined by hyphens, such as `TF-012-lag-calendar.md`. |
| [template.md](template.md) | The sections every rule has. Copy it to start a rule. |
| [sources.md](sources.md) | The bibliography: one anchor per source, with where it is and what it covers. Rules link to these anchors. |
| [open-source-schedulers.md](open-source-schedulers.md) | A survey of open-source schedulers and how we run them for differential tests. |

## Provenance classes

Every source a rule cites has one of five classes. [CONTRIBUTING.md](../../CONTRIBUTING.md#calculation-rules-and-their-sources) prefers them in this order, so a rule cites the highest class that supports it. `format-fact` supports format facts only and never how a schedule is calculated. All sources follow [CONTRIBUTING.md, Clean room](../../CONTRIBUTING.md#clean-room).

| Class | What it is | How a rule cites it |
|---|---|---|
| `standard` | Public CPM standards and literature: the GAO Schedule Assessment Guide, DCMA and NDIA guidance, AACE recommended practices, and published CPM papers and textbooks. | A link to its `sources.md` anchor, with the page or section and what it supports. |
| `vendor-doc` | A vendor's openly licensed public documentation or published file schema. `sources.md` says which vendor material qualifies. | A link to its `sources.md` anchor, with the page or element and what it supports. |
| `observed-file` | What real schedule files store: the [corpus](../../CONTRIBUTING.md#the-corpus), and files users donate from their own work ([CONTRIBUTING.md, Donating schedules](../../CONTRIBUTING.md#donating-schedules)). | What the files store, in general words, naming corpus files by their ID in `corpus/manifest.toml` where that helps. |
| `format-fact` | A file format's table, field and code names, encodings and units: read from any corpus file, other tools' test files included, or from an open-source project's code read for facts and never copied ([sources.md, Open-source code](sources.md#open-source-code-read-for-format-facts)). Never how a schedule is calculated. | What the files show, in general words, or a link to the project's `sources.md` anchor with what it shows. |
| `own-choice` | A design choice of ours. | The choice and its reasoning, in the rule's Rationale. |

## The check

`cargo xtask rules` checks every file in this folder as a rule, except the files listed above, and `just check` runs it. A new file here that isn't a rule goes on the list in [`xtask/src/rules.rs`](../../xtask/src/rules.rs) (`NOT_RULES`). The check reports each problem with the file and what to fix, and fails when:

- the file name isn't `TF-NNN-slug.md`, or another rule has the same number;
- the Sources section is missing or has no entries;
- an entry doesn't start with one of the five classes, written as `` `class`: ``;
- a `standard` or `vendor-doc` entry links to no `sources.md` anchor, or any entry links to an anchor `sources.md` doesn't have;
- an `observed-file` entry doesn't mention the corpus;
- a `format-fact` entry neither mentions the corpus nor links to a `sources.md` anchor;
- the rule names a corpus file (`C-` and four digits) that `corpus/manifest.toml` doesn't list;
- a test the Tests section names doesn't exist.
