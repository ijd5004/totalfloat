# TF-009: XER text encoding

How TotalFloat chooses the text encoding of an XER file it reads, and how it writes text back without disturbing the rest of the file.

## Statement

1. A file that starts with a UTF-8 byte-order mark is read as UTF-8. A cell in it that isn't valid UTF-8 is read with replacement characters, and the import reports it once for the file.
2. Without a byte-order mark, a file that is valid UTF-8 and holds any byte outside ASCII is read as UTF-8.
3. Every other file is read as Windows-1252. That includes a file of ASCII alone, which reads the same in either encoding.
4. A file read as Windows-1252 that also holds valid multi-byte UTF-8 sequences is reported as mixed. Its cells are still all read as Windows-1252, so the UTF-8 parts read as garbled text.
5. The raw layer keeps every cell's original bytes. Reading a file and writing it with no edits gives back the same bytes, whatever the encoding.
6. Writing an edit encodes only the edited cells, in the encoding chosen for the file. Rows and tables added to the file use that encoding and the line ending of the file's first line. Every other byte is copied unchanged.
7. An edit whose text the file's encoding can't hold is refused. TotalFloat never substitutes characters.

## Options

None.

## Sources

- `observed-file`: most of the corpus's P6 exports with text beyond ASCII are Windows-1252, and a few are UTF-8. Some, from several P6 versions, hold Windows-1252 bytes and valid UTF-8 sequences in the same file: byte-order marks inside cells, text already garbled once before export, and currency symbols as single Windows-1252 bytes. Items 1 to 4.
- `own-choice`: items 5 to 7; the reasoning is under Rationale.

## Rationale

A file has no header that names its encoding, and the corpus shows one file can mix two, so any choice for the whole file is an approximation. Valid UTF-8 with bytes beyond ASCII is very unlikely to be anything else, while Windows-1252 can decode any byte, so it is the fallback (items 2 and 3). Reporting a mixed file tells the planner why some text looks wrong (item 4).

Since the choice can be wrong for some cells, TotalFloat keeps every cell's bytes and re-encodes only what an edit changed (items 5 and 6). A text the file can't hold would have to be changed to fit, and the planner should decide how, so the edit is refused (item 7).

## Tests

- `crates/xer/src/raw/tests.rs`: `mostly_utf8_with_a_stray_byte_is_flagged` (items 3 and 4).
- `crates/xer/src/raw/writer_tests.rs`: `patch_only_changes_one_cell_even_with_invalid_utf8_and_mixed_endings` (items 5 and 6), `windows1252_edits_and_multiline_values` (items 6 and 7).
- `crates/xer/tests/corpus_raw.rs`: `corpus_files_parse_losslessly` writes every corpus file back byte for byte and summarizes the encoding diagnostics (items 1 to 5).

## Differences

None known.
