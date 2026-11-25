# TF-NNN: Rule title

One sentence on what the rule decides.

## Statement

The rule, in our own words, as numbered items a test can check. Say what TotalFloat calculates, from which inputs, and what it refuses. Use TotalFloat's terms ([GLOSSARY](../GLOSSARY.md)), and name file fields only where a file's setting selects an option.

1. First item.
2. Second item.

## Options

The named choices this rule leaves to a scheduling option, where reasonable schedulers differ ([ARCHITECTURE.md, Scheduling rules](../ARCHITECTURE.md#scheduling-rules)). For each: its name, its values, the default and why, and which XER or MS Project XML field selects it. Write "None." when the rule has no options.

- **`option_name`**: `value_a` (default) or `value_b`. A file selects it through `FIELD`; without one, `value_a` applies because …

## Sources

One entry per source. Each entry starts with its class (`standard`, `vendor-doc`, `observed-file`, `format-fact` or `own-choice`; see [README.md](README.md)), then the citation, then what it supports. Link `standard` and `vendor-doc` sources to their anchor in [sources.md](sources.md).

- `standard`: [GAO Schedule Assessment Guide](sources.md#gao-sag), pp. 92–93: what this source supports, and which Statement items it supports.
- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Task element: what this source supports.
- `observed-file`: files in the corpus: what their stored results show, in general words, naming corpus files by ID where it helps, with no match counts or percentages.
- `format-fact`: files in the corpus, or [MPXJ](sources.md#mpxj): a field's name, code or unit, for a Where files hold it item only.
- `own-choice`: Statement item 2; the reasoning is under Rationale.

## Rationale

Why the rule is written this way: how the sources combine, which reading of them we took where they leave room, and the reasoning behind each own choice.

## Tests

The tests written from this rule, by file and name, such as `crates/engine/src/tests.rs`: `a_test_name`. Their expected values come from the Statement.

## Differences

Where another scheduler is known, from public sources, to calculate differently, with the source. Write "None known." when there are none.
