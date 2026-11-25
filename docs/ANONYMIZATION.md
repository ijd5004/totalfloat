# Field-aware anonymization

```sh
totalfloat anonymize input.xer -o new-output.xer --seed 42 --report --json
```

The maintainer chose **field-aware privacy verification**. Every supported,
nonempty identifying or free-text cell is replaced and verified after writing
and reparsing. Equal original strings share a replacement within the file.
Fixed syntax and retained structural metadata may contain coincidental original
substrings: activity-code value `A` does not require changing `CALENDAR`.

The source and existing output files are never overwritten. Validation occurs
in memory before a temporary output is created; installation refuses an existing
path. Errors use `totalfloat.error/1`. Successful JSON uses
`totalfloat.anonymize/1`, with counts and no private mapping or original values.
Without `--report`, per-field counts are omitted.

The seed defaults to 0. Identical input and seed produce identical output. Labels are handed out in an order hashed from each value, so a label says nothing about where its source sorted.
Cross-file label stability is not promised when the vocabulary changes. Labels
are unique within a file and differ from original values, ignoring ASCII case.
Activity-code labels respect their retained dictionary length limits. Unit
abbreviations never grow beyond their original character width; shared values
inherit the smallest applicable limit. Exhausted label spaces are refused.

## Replaced content

Recognized names and identifying codes, descriptive text, UDF text, header
user/database metadata, row user fields, calendar labels and supported memo text
are replaced with deterministic labels. Optional contact fields are treated as
text, not as deliverable addresses. Labels are generated from the seed and
vocabulary position rather than copying source text.

The field policy lives in `crates/xer/src/anonymize.rs`. Registry text fields are
scrubbed except explicit operational metadata. Supplemental identifying field
suffixes cover supported unregistered memo/name columns. Populated unclassified
columns, unsupported blobs, UDF expressions, malformed calendar payloads,
unsupported headers, extra row cells and unclassified stray bytes cause refusal.
Calendar payloads must also contain only recognized pattern/display metadata;
ignored node labels cannot carry arbitrary text through the anonymizer.

## Supplemental metadata

The policy explicitly scrubs `RCATTYPE.rsrc_catg_type` category labels and
`UMEASURE.unit_abbrev` unit labels. It retains MEMOTYPE's `eps_flag`, `proj_flag`,
`wbs_flag`, `task_flag` and `ROLES.def_cost_qty_link_flag` only as exact Y/N tokens.
Empty cells remain empty. Other populated flag spellings are refused.

`SCHEDOPTIONS.LevelPriorityList` is retained only when every record contains
recognized field names (`priority_type`, `early_start_date`) and an exact ASC or
DESC direction. Supported layouts are `field,ASC`, `field,/ASC`, and comma-joined
known fields followed by `/ASC` (or DESC); DEL-separated records and empty DEL
padding are preserved exactly. Unknown identifiers, private text, malformed
separators and unknown directions are refused. This is not a blanket exemption
for arbitrary scheduling-option text. New calendar dialects and opaque curves
remain outside this extension.

The reviewed corpus layouts include the two known field names, both comma and
slash variants, and DEL padding. Some lists contain additional unclassified
identifiers; those remain refused. Only format facts were used to define the
policy; no private values or source rows were copied into the repository.

## Retained content and limits

IDs, GUIDs, dates, durations, logic, options, stored results, costs, currency
settings and operational codes are retained. Calendar work patterns and date
exceptions remain unchanged. Optional cost scaling is not implemented. All
non-designated cells must remain equal after reparsing.

This is pseudonymization. Retained metadata, network shape, vocabulary ordering
and label reuse can still identify a project. Review the result before sharing.
Unknown or unsupported files are refused rather than partially anonymized. That
includes any retained value that doesn't parse as its column's kind, and a header
whose retained fields (version, date, product, module, currency) don't have their
usual form, because text there would otherwise reach the output.
No real schedules or private mappings belong in this repository or public
issue/PR attachments.

## Validation

Synthetic tests cover short-code overlaps on the unchanged model fixture,
dictionary lengths, repeated values, seed determinism, header and memo text,
legacy encoding, protected cells/results, exact round trips and safe output
creation. A corpus test checks the exports it accepts and reports refused files
separately, without printing file values. Run it with:

```sh
TOTALFLOAT_CORPUS=/path/to/corpus cargo test --release -p totalfloat-xer --test corpus_anonymize -- --nocapture
```

The corpus test accepts a few exports and refuses the rest for unsupported
layouts, fields or calendar content. Every accepted file keeps its stored
results and protected cells unchanged. Coverage is limited: it doesn't mean
every real file anonymizes cleanly.

A known-clean synthetic input and its seed-42 output passed all 107 checks in
XER Tools 2.2b with zero errors and warnings. A negative control triggered two
failing checks. See [application evidence](validation/xertools-2.2b.md); broader
external verification is still to come. The CLI
reports `external_validation: not-run` because it does not run XER Tools for the
user. A fixture-level application pass does not certify every future output.

The authored supplemental case is in
`fixtures/xer/validation/anonymize-metadata/`, with source/output hashes and no
stored results. Regenerate into a new directory using:

```sh
cargo xtask anonymize-fixture --out new-metadata-fixtures
```

This command creates fixtures through the library; it does not run XER Tools.
The exact committed source and anonymized output were separately loaded in
XER Tools 2.2b without errors or warnings. As always, this fixture pass does not
certify arbitrary future schedules or remove the need to review retained data.
