# TF-012: XER hours and whole minutes

How TotalFloat turns the decimal hours an XER file stores into the whole minutes it calculates with, and writes minutes back as hours.

## Statement

1. TotalFloat counts time in whole minutes ([ARCHITECTURE.md, Data model](../ARCHITECTURE.md#data-model)). An hours value that is an exact whole number of minutes is that number of minutes.
2. An hours value with six to 40 decimal places that equals a whole number of minutes divided by 60, rounded to nearest (half away from zero) or cut off toward zero at the value's own number of places, is that number of minutes. The check uses the value's decimal digits and integer arithmetic only, with no tolerance.
3. Any other hours value is not read. A value the model needs, such as an activity's original duration, leaves its row out of the model, and the import lists every row it left out. A value the model can do without, such as a stored float, is left empty, and the import reports it. In both cases the file keeps the value as written.
4. An empty `TASKPRED.lag_hr_cnt` cell is a lag of zero. A lag cell that holds text that isn't a number leaves the relationship out, as item 3 says.
5. A number of minutes whose hours end in a finite decimal is written as those exact hours. Any other number is written at six decimal places, rounded to nearest with integer arithmetic. Every value written this way reads back as the same number of minutes under items 1 and 2.
6. Cells an edit doesn't change keep their original spelling, including cut-off values and empty lags.

## Options

None.

## Sources

- `observed-file`: hours values in the corpus's P6 exports that aren't an exact number of minutes are all a whole number of minutes divided by 60, written at six or more decimal places: most rounded, the rest cut off. In the activity checked against its stored dates, the dates span the same whole number of minutes. Items 1 and 2.
- `observed-file`: some corpus P6 exports hold relationships with an empty lag cell. Read as a lag of zero, those relationships give the stored early dates of the activities they lead to; left out, they don't. Item 4.
- `own-choice`: items 3, 5 and 6; the reasoning is under Rationale.

## Rationale

Minutes divided by 60 rarely terminate in decimal, so a file that stores hours must round or cut off most of them. Recognizing exactly those two spellings at the precision the file used recovers the minutes the file meant, and nothing else (item 2). A value that is neither is a real fraction of a minute, which TotalFloat doesn't represent, so it reports the value and keeps it in the file as written (item 3).

An empty lag is how the files write no lag, and reading it that way restores the relationship (item 4). Item 2 accepts six decimal places or more, up to 40, so six is the shortest spelling that reads back, and a file TotalFloat writes always reads back the same (item 5). Values nobody edited keep their spelling, so a file changes only where the planner changed it (item 6).

## Tests

- `crates/xer/src/mapping/tests.rs`: `export_precision_recognition_is_exact_at_the_tokens_scale` (items 1 and 2), `strict_minute_conversion_never_rounds_or_invents_required_duration` and `optional_nonminute_float_remains_raw_instead_of_fabricated_minutes` (item 3), `empty_relationship_lag_is_zero_but_malformed_lag_is_omitted` (item 4).
- `crates/xer/src/patch/tests.rs`: `every_minute_formatter_reads_back_including_model_limits` and `nonterminating_lag_is_written_and_reads_back_exactly` (item 5), `precision_and_empty_lag_source_spellings_survive_unrelated_edits` (item 6).
- `crates/xer/tests/corpus_raw.rs`: `corpus_model_accounts_for_every_raw_row` (item 3).

## Differences

- MS Project XML stores durations and work as ISO 8601 durations, such as `PT8H0M0S`, and lag in tenths of a minute ([MSPDI schema](sources.md#msp-xml-schema), Task elements, `Duration` and `LinkLag`).
