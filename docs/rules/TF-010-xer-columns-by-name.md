# TF-010: XER columns by name

How TotalFloat finds a field in an XER table, and what it does with columns and values it doesn't expect.

## Statement

1. A field is found by its name in the table's `%F` row. Its position, and the version in the file's header, never decide which column holds it.
2. A column TotalFloat doesn't know is kept, and a known column a table lacks is normal. Neither is an error.
3. A field whose column is missing and a field whose cell is empty are two different states, and TotalFloat keeps them apart.
4. Where a `%F` row names a column twice, the field reads the first one. The later ones are kept.
5. A date-time cell has the form `yyyy-mm-dd hh:mm`. A cell with seconds or finer is kept as written and reported, and the model doesn't read it as a date. TotalFloat never truncates it to the minute.
6. A cell whose text doesn't fit its field's kind, such as letters in a number field, is kept as written and reported. TotalFloat never rewrites it into a valid value.
7. Fields whose names suggest numbers but whose corpus values are text codes are read as text.

## Options

None.

## Sources

- `observed-file`: in the corpus's XER files, tables carry different column sets in files that give the same header version, and files from writers other than P6 carry the same version with their own columns. Some date-time cells give seconds, a few of them nonzero, mostly in synthetic and third-party files. Some fields with numeric-looking names, such as `ACTVCODE.color` and `TASKRSRC.cost_per_qty_source_type`, hold text codes. Items 1, 2, 5 and 7.
- `own-choice`: items 3 to 7; the reasoning is under Rationale.

## Rationale

The corpus shows that neither a column's position nor the file's version predicts which columns a table holds, so the `%F` row is the only reliable guide (items 1 and 2). TotalFloat must write back what it read ([ARCHITECTURE.md, Lossless round trips](../ARCHITECTURE.md#lossless-round-trips)), so it keeps what it doesn't understand: unknown columns, repeated ones, finer timestamps and malformed cells (items 2, 4, 5 and 6). An empty cell and a missing column can mean different things to the software that wrote the file, so the model keeps them apart (item 3). The model counts time in whole minutes ([ARCHITECTURE.md, Data model](../ARCHITECTURE.md#data-model)), and dropping seconds would change a value without saying so, so a finer timestamp is reported (item 5). Reading a code as a number would lose it, so a field is text until its codes are known (item 7).

## Tests

- `crates/xer/tests/corpus_raw.rs`: `corpus_core_registry_covers_columns_and_enum_codes` (items 1, 2 and 7), `corpus_typed_cells_reencode_exactly_with_no_fatal_errors` (items 2, 5 and 6).
- `crates/xer/src/typed/tests.rs`: `invalid_tokens_are_not_silently_normalized` (items 5 and 6).
- `crates/xer/src/registry.rs`: `vocabulary_is_sorted_unique_and_extensible` (item 2).
- `xtask/src/xer_fields.rs`: `inventory_reports_unknowns_missing_and_extra_cells_without_values` (items 2 and 3).

## Differences

None known.
