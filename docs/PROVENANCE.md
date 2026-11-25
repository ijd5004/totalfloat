# Where TotalFloat's knowledge comes from

Every part of TotalFloat that encodes knowledge about scheduling or about other products' files has a public source or is TotalFloat's own design. This page lists each kind, its sources, and what checks it. [CONTRIBUTING.md, Clean room](../CONTRIBUTING.md#clean-room) and [Calculation rules and their sources](../CONTRIBUTING.md#calculation-rules-and-their-sources) set the rules.

| Area | Sources | Checked by |
|---|---|---|
| Calculation rules: work time, the forward and backward passes, float, constraints, progress, paths, units and costs, earned value | The written rules in [`docs/rules/`](rules/README.md), each citing its sources by class: public CPM standards and literature, openly licensed vendor documentation and schemas, what files in the corpus store, and our own reasoned choices | `cargo xtask rules`: every rule cites a valid source class, its links exist, and every test it lists exists. Tests are written from each rule. |
| XER format: tables, fields, codes, encodings, layout | The files themselves: the `%F` rows and values of real third-party exports in the corpus ([xer-format.md](xer-format.md)). Codes no file holds come from MPXJ's code lists, read for facts ([sources.md, MPXJ](rules/sources.md#mpxj)). | The corpus test `corpus_backs_every_registered_table_field_and_code`: every table, field and code TotalFloat knows appears in a corpus file or is listed as read from MPXJ. |
| Values a new XER row takes | For a setting, the value most rows in the corpus hold; for a quantity or cost, zero ([TF-034](rules/TF-034-activity-edits.md), [TF-036](rules/TF-036-codes-resources-and-roles.md)) | The corpus test `new_row_defaults_are_what_corpus_rows_hold` |
| MS Project XML format | Microsoft's *Project XML Data Interchange Schema Reference*, its documentation under CC BY 4.0 and its code under MIT ([sources.md](rules/sources.md#msp-xml-schema)); MPXJ for the currency scale ([TF-003](rules/TF-003-msproject-xml-assignments.md)) | `cargo xtask validate msproject` checks written files against that schema |
| Schedule quality checks | DCMA's *EVMS Program Analysis Pamphlet* (DCMA-EA PAM 200.1), a U.S. government work, and the GAO Schedule Assessment Guide; the further checks are our own ([CHECKS.md](CHECKS.md)) | Tests per check |
| Earned value | The DoD EVMS Interpretation Guide, DCMA-EA PAM 200.1 and the GAO Schedule Assessment Guide ([TF-041](rules/TF-041-earned-value.md)) | `cargo xtask rules` |
| Schedule risk (Monte Carlo) | Public statistics: three-point estimates with triangular and BetaPERT distributions, a one-factor Gaussian copula for correlation, the xoshiro256** generator (Blackman and Vigna, public domain) and Acklam's normal quantile approximation. How they're combined is our own design. | Tests in `crates/risk` |
| Text format, history store, local app, CLI and MCP, and how each file format's records map onto TotalFloat's model | TotalFloat's own design ([ARCHITECTURE.md](ARCHITECTURE.md#design-decisions)) | Tests, including round trips through each format |
| Test fixtures | Synthetic files we wrote, or files donated with permission to publish, each listed with its origin in `fixtures/*/MANIFEST.md`. Expected results come from the written rules. | `.gitignore` refuses `*.xer` outside the fixture folders |
| Third-party code | Rust crates and npm packages under permissive licenses, used as dependencies and never copied in | `cargo deny check` and `cargo xtask ui --check --licenses` in CI |
| File validation | XER Tools and MPXJ, run as separate programs to check files TotalFloat writes ([validation/validators.md](validation/validators.md)). | |

## The corpus

The corpus is real schedule files posted publicly. [`corpus/manifest.toml`](../corpus/README.md) lists each one with where it's posted, the license stated there, the date we downloaded it and its SHA-256, and `cargo xtask corpus fetch` downloads them, so anyone can check what a rule says the files show. Other tools' own test and demo files are tagged `codec-only` and used for format facts and for testing reading and writing only. See [CONTRIBUTING.md, The corpus](../CONTRIBUTING.md#the-corpus).
