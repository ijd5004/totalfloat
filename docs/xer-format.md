# XER format and field registry

This is TotalFloat's description of the XER format, written from real files in the corpus. The raw layer is lossless; this registry is deliberately incomplete and version tolerant, and a registered field's meaning may be only partly known.

## Records and preservation

An XER starts with `ERMHDR` and tab-separated header metadata, normally including an export version. Records follow:

| Marker | Content |
|---|---|
| `%T` | Table name |
| `%F` | Ordered field names for this table |
| `%R` | Row cells in `%F` order |
| `%E` | End of file content |

Rows may span physical lines. Lines without record markers continue the previous row; trailing blank lines before a record are separators. A marker must have the expected delimiter (`%Rogue` is text). See [`raw.rs`](../crates/xer/src/raw.rs) for exact recognition and parser limits.

Fields and table order come from the file. Missing trailing cells, extra cells, unknown tables/fields, missing end markers, stray content and mixed line endings are preserved and diagnosed. The reader does not repair the input. A short row's missing trailing cells should be treated as empty by the typed layer, while the raw layer retains the distinction.

UTF-8 (with optional BOM) and Windows-1252 are common. A single file can mix encodings; see [TF-009](rules/TF-009-xer-text-encoding.md). Untouched bytes are copied verbatim by `RawXer::write`; tracked cell edits re-encode only the edited cells. Added rows/tables use the file's selected encoding and first-line ending. Unrepresentable edits are rejected rather than substituted. The writer does not make arbitrary raw tables into a complete P6-importable model export.

## Compiled registry

The public API is [`totalfloat_xer::registry`](../crates/xer/src/registry.rs); the compiled vocabulary is [`registry/data.rs`](../crates/xer/src/registry/data.rs). `tables()`, `table(name)` and `field(table, name)` perform exact case-sensitive lookups without I/O. Unknown names return `None`. `Table::fields` is sorted by name, not by file position: always bind columns using `%F`.

Each `Field` has its name, kind and notes; tables also carry notes. Kinds describe representation, not validation policy:

| Kind | Representation |
|---|---|
| `Id` | Own integer identity |
| `ForeignId(table)` | Integer reference, possibly to another project/file or an unregistered table |
| `Text` | Text or a legacy code whose stronger type is not established |
| `Int` | Integer, including identity-like fields with unresolved targets |
| `Decimal` | Decimal number; duration/float fields remain hours until model mapping |
| `DateTime` | Civil `yyyy-mm-dd hh:mm`; variants need diagnostics, see below |
| `Flag` | `Y` / `N`; empty is distinct from false |
| `Enum(codes)` | Recognized format codes; unknown codes remain raw and become diagnostics |
| `Guid` | P6 GUID, often compressed rather than a hyphenated UUID |
| `Blob` | Structured content such as `clndr_data` or resource curves |

Empty values do not establish a kind. `TASK.cstr_date2` is the known secondary constraint datetime even though it is empty in the present corpus; relationship columns `aref` and `arls` have observed datetime shapes, with their meanings still unresolved. Conservative `Text` definitions and unresolved reference targets are marked in notes. `UDFVALUE.fk_id` and `udf_code_id` depend on `UDFTYPE`; they are not assumed to refer to one fixed table. Text-like `ACTVCODE.color`, `PROJECT.hist_interval`, `hist_level`, `publish_spread_assign_level`, `sum_assign_level`, and `TASKRSRC.cost_per_qty_source_type` must not be parsed as numbers based on their names.

The vocabulary covers the table/field pairs that corpus files hold in the core tables below. It includes unnamed trailing columns in synthetic edge cases; those are represented by the empty field name and retain their positional identity in raw rows. `ROLES`, `ROLERATE` and `RSRCROLE` came later, with their column names taken from the corpus files that hold them; `RSRCRATE` came with resource rates; `PROJCOST`, `ACCOUNT` and `COSTTYPE` came with expenses; `RSRCCURVDATA` came with resource curves, and `TASKRSRC.curv_id` now points at it ([TF-039](rules/TF-039-resource-curves.md)). `OBS` and `PROJWBS.obs_id` are mapped to the model. `TASKPROC` came with activity steps. `MEMOTYPE`, `TASKMEMO` and `WBSMEMO` came with notebooks; a note cell is HTML with each quote doubled and each line break written as two DEL (0x7F) characters, which mapping undoes and the writers redo. `PCATTYPE`, `PCATVAL` and `PROJPCAT` are mapped to the model; new rows follow the column order of the corpus files holding them. `THRSPARM` and `PROJTHRS` are read for reporting only and stay in raw storage. Tables the registry doesn't list stay in raw storage and are written back byte for byte.

| Table | Fields | Role |
|---|---:|---|
| `ACCOUNT` | 6 | Cost accounts |
| `ACTVCODE` | 8 | Activity code values and hierarchy |
| `ACTVTYPE` | 8 | Activity code dictionaries and scope |
| `CALENDAR` | 14 | Calendar metadata and encoded work patterns |
| `COSTTYPE` | 3 | Expense categories |
| `CURRTYPE` | 12 | Currency display and exchange metadata |
| `MEMOTYPE` | 7 | Notebook topics |
| `OBS` | 7 | Organizational hierarchy |
| `PCATTYPE` | 5 | Project code dictionaries |
| `PCATVAL` | 6 | Project code values |
| `PROJCOST` | 20 | Expenses |
| `PROJECT` | 93 | Projects, dates, defaults and options |
| `PROJPCAT` | 3 | Project code assignments |
| `PROJWBS` | 28 | WBS hierarchy and project node |
| `RISKTYPE` | 4 | Risk categories |
| `ROLERATE` | 9 | Role prices over time |
| `ROLES` | 10 | Roles |
| `RSRC` | 36 | Resources |
| `RSRCCURVDATA` | 24 | Resource curves: 21 points of percent units |
| `RSRCROLE` | 9 | The roles each resource fills |
| `SCHEDOPTIONS` | 25 | Scheduling and leveling options |
| `TASK` | 74 | Activities and stored results |
| `TASKACTV` | 4 | Activity code assignments |
| `TASKMEMO` | 5 | Activity notes |
| `TASKPRED` | 12 | Relationships, including external projects |
| `TASKPROC` | 9 | Activity steps |
| `TASKRSRC` | 57 | Resource assignments |
| `UDFTYPE` | 9 | User-defined field definitions |
| `UDFVALUE` | 7 | User-defined field values |
| `WBSMEMO` | 5 | WBS notes |

The synthetic extension `TASK.custom_field_x` is registered because it occurs in the corpus; its presence is not evidence of an Oracle column. Unknown extensions beyond this vocabulary remain supported by raw storage and reported by the inventory.

## Enum evidence

The corpus test checks every nonempty enum cell in registered tables against the compiled list. These are observed format codes, never private schedule values:

| Field | Observed nonempty codes |
|---|---|
| `TASK.task_type` | `TT_Task`, `TT_Rsrc`, `TT_LOE`, `TT_Mile`, `TT_FinMile`, `TT_WBS` |
| `TASK.duration_type` | `DT_FixedDUR2`, `DT_FixedDrtn`, `DT_FixedQty` |
| `TASK.complete_pct_type` | `CP_Drtn`, `CP_Phys`, `CP_Units` |
| `TASK.status_code` | `TK_NotStart`, `TK_Active`, `TK_Complete` |
| `TASK.cstr_type` | `CS_ALAP`, `CS_MANDFIN`, `CS_MEO`, `CS_MEOA`, `CS_MEOB`, `CS_MSO`, `CS_MSOA`, `CS_MSOB` |
| `TASKPRED.pred_type` | `PR_FF`, `PR_FS`, `PR_SF`, `PR_SS` |

`DT_FixedRate`, `CS_MANDSTART`, `AS_EPS` and `FT_Min` are recognized from MPXJ's code lists ([rules/sources.md, MPXJ](rules/sources.md#mpxj), read for format facts) and have no nonempty examples in the corpus. `FT_SS` appears in one corpus project; its meaning, start float, also comes from MPXJ's list. Other schedule-option evidence and semantic uncertainty are in [TF-008](rules/TF-008-xer-schedule-options.md).

## Version and exporter differences

A header version is evidence, not a schema switch. The corpus includes real P6 exports, third-party exports and deliberately malformed synthetic files. Identical version headers can carry different column sets. For example, samples labeled 8.2 have 59 or 60 `TASK` fields and 66 or 69 `PROJECT` fields; samples labeled 24.12 have 61 or 67 `TASK` fields and 71 or 85 `PROJECT` fields. These counts describe samples, not official version guarantees.

`SCHEDOPTIONS` is present in most real exports and absent from a few older ones (TF-008). Do not infer absence solely from a header version. `TASK.location_id` is absent from sampled 5.0 headers and present in some later ones, but presence is not monotonic by version. `PROJECT.px_enable_publication_flag` is observed with headers 15.1, 23.12 and 24.12; earlier absence in this sample set does not establish its introduction version.

Most scheduling dates have minute precision. The corpus also holds some `*_date` cells shaped `yyyy-mm-dd hh:mm:ss`, a few with nonzero seconds, in exports from several writers, synthetic ones included. They say nothing about calculation precision. Some audit timestamp fields have other text shapes. The raw layer preserves all of them; the typed layer must diagnose unsupported precision without silently losing it. See [TF-010](rules/TF-010-xer-columns-by-name.md).

## Reproducing the inventory

```sh
TOTALFLOAT_CORPUS=/path/to/corpus cargo xtask xer-fields
# An explicit directory overrides the environment variable:
cargo xtask xer-fields --corpus /path/to/corpus
# For large corpora:
cargo run --release -p xtask -- xer-fields --corpus /path/to/corpus
```

The command recursively reads regular `.xer` files (case insensitive), skips symlinks, sorts paths and names, and reports parse totals, rejection reasons, table/column occurrence counts, row counts, missing/extra cells, unknown enum counts and lexical shapes. It prints **no row values, enum codes, header values, example cells or file paths**. Names are escaped so embedded control characters cannot corrupt the report. Occurrences count table/column appearances, including repeated tables; missing cells are counted separately from present empty cells. Duplicate field names aggregate by name.

The fixed shape vocabulary is `empty`, `flag`, `integer`, `decimal`, `datetime`, `guid-like`, `multiline`, and `text`. Classification is lexical, not semantic: `datetime` is the minute-shaped form only, compressed GUID-like strings can also be ordinary text, and second-precision timestamps are currently `text`. Parse rejections and unknown fields/codes are reported without stopping the inventory. Filesystem errors, an unset corpus path or an empty corpus directory return an error.

`TOTALFLOAT_CORPUS=… cargo test --release -p totalfloat-xer --test corpus_raw -- --nocapture` additionally checks byte-identical round-trips, registered-column coverage, and enum vocabulary coverage. Coverage assertions intentionally flag corpus growth that needs registry updates; production reading/inventory remain permissive. Corpus tests skip cleanly when the variable is unset. Do not commit reports containing corpus data.

## Typed rows and exact cell encoding

`typed::TypedXer::decode(&raw)` borrows the raw file and exposes a `TypedTable` variant for each recognized table, with concrete structs such as `TaskRow` and `ProjectRow`. Fields use `Cell<T>`: `i64` identities/integers, exact `Decimal` values, model `DateTime`, booleans, or borrowed strings for validated enum codes, GUIDs, text and opaque blobs. Registry enum codes are validated here; conversion to canonical model enum variants belongs to model mapping.

Every row has `RowMeta` with a `RowRef { table, row }`, unknown-field `extras`, duplicate column occurrences and overflow cells. Duplicate names bind the first occurrence; all later occurrences retain their index and text. Repeated tables stay distinct. Unrecognized tables remain available through the borrowed raw file and `unknown_tables` indices. No source bytes change during typed decoding.

`Cell::state()` distinguishes `Parsed`, `Empty`, `Missing` (column absent) and `Invalid`. A short row's omitted trailing cells are `Empty`. Missing/empty values do not become zero, false or a default enum. Invalid values remain raw with a warning; callers must inspect cell state before mapping to required model inputs.

`Cell::encode()` reproduces the original token exactly, including leading zeros, signed zero, decimal scale, exponent spelling and unsupported text. The parsed state is immutable, so it cannot silently diverge from the retained token. Replace a cell with `Cell::from_value(value)` to request canonical formatting: integer digits, `Y`/`N`, minute-resolution civil datetime, fixed-point decimal, or unchanged string content. Format validity and tab/newline safety of eventual edits are also checked by the raw mutation API. Fresh string cells are not automatically checked against enum/GUID vocabularies.

`Decimal` uses an `i128` coefficient and a stored scale from 0 through 64. Exponent notation is accepted when it fits; negative input scales expand using checked arithmetic. Out-of-range coefficients/scales, nonfinite values, separators and whitespace are diagnosed and retained without binary floating-point conversion or rounding. This is storage parsing, not schedule math or hour-to-minute conversion.

Minute datetimes are validated by the deterministic civil time type. Timestamps carrying seconds or finer precision remain `Invalid(UnsupportedPrecision)`, even when seconds happen to be zero; they are never silently truncated. GUIDs accept compressed 22-character or standard hexadecimal forms; unfamiliar forms remain raw. Structured blobs are opaque until a dedicated decoder is implemented.

Typed diagnostics have severity, category, table name/index, row index, field and message. All recoverable input issues in this layer are warnings, with no cell values in messages. Details are capped at 1,000 per decoded file; `diagnostic_counts` includes all warnings and `suppressed_diagnostics` reports omitted details. Unknown columns are reported once per declaration rather than once per row. Consumers must keep this summary separate from the raw reader's diagnostics.

The typed corpus test (`corpus_typed_cells_reencode_exactly_with_no_fatal_errors`) decodes every valid corpus file, re-encodes each cell exactly and finds no fatal errors. It reports warnings by kind: unknown tables, overflow rows, invalid integers, decimals and datetimes, datetimes finer than a minute, and unrecognized GUIDs. Malformed and synthetic samples are included; these are codec observations.

## Calendar data

`calendar_data::CalendarData::decode(source)` exposes read-only local weekday definitions, date exceptions and diagnostics, and `encode()` reproduces the original source string exactly. It accepts the observed compact, whitespace/DEL-separated, quoted-multiline and unnamed-root variants. Parsing is bounded to 32 node levels and 100,000 nodes. Unknown structure, unsupported intervals and malformed dates retain their source rather than becoming invented work patterns. [TF-011](rules/TF-011-xer-calendar-data.md) documents the grammar, epoch, midnight endpoints and evidence.

`calendar_data::decode_calendars(&typed)` retains one `CalendarEntry` per raw row and resolves supported entries into model `Calendar` values. Scope, project/base IDs, defaults, conversion factors and source references are retained. Resolution is independent of row order, checks base cycles iteratively and limits valid chains to 64 levels. Inherited exceptions are merged before local overrides. A standard week must be locally specified; it is not copied from the base. Missing or malformed inputs have `calendar: None`, preserving their local definition and diagnostics. This prevents downstream scheduling from silently using fabricated hours.

The source link uses checked 32-bit table/row coordinates packed into the model's opaque `SourceRef`; `typed::RowRef::{to_source_ref,from_source_ref}` provides the conversion. Decimal display hours convert using checked integer arithmetic and the exact export-precision recognition rule in TF-012. Weekly display-hour mismatches are warnings and do not change work intervals.

The public sample set has 367 calendar rows, all with exact original-string re-encoding. There are 350 effective models, 16 unresolved local weeks and one truncated tree. Among resolved models, 176 weekly totals agree with stored week factors, 49 differ, and 125 have no usable week factor. Repeated identical exceptions are harmless and diagnosed; conflicting duplicates, missing bases and cycles remain unsupported. Omitted-week behavior is still to become a TotalFloat rule ([ARCHITECTURE.md, Scheduling rules](ARCHITECTURE.md#scheduling-rules)). These are codec results.

## Hand-typed spreads

`TASKRSRC.target_crv`, `remain_crv` and `actual_crv` hold an assignment's planned, remaining and actual units typed in by hand ([TF-040](rules/TF-040-typed-spreads.md)). `spread_data::decode` reads a cell and `spread_data::encode` writes one:

- A cell is stretches separated by `;`, each `units:hours`: the units in the next `hours` of the assignment's work time, in order from the start of that part (planned, remaining or actual). An empty cell is no spread.
- Units and hours are unsigned decimals: digits, optionally `.` and more digits. No sign, exponent or leading `.`.
- Hours are a whole number of minutes, rounded to three decimal places, with trailing zeros dropped: 2 minutes is `0.033`, 40 minutes `0.667`, 40 hours `40`. The decoder accepts hours only when they are exactly that spelling of a positive whole minute (or the same number with extra trailing zeros). Units are kept as written.
- The encoder writes units as Rust's shortest exact decimal (no `.0` on whole numbers) and refuses negative or non-finite units and work that isn't positive.
- The marker: an assignment has a typed spread when its `curv_id` names a curve the file doesn't list in `RSRCCURVDATA`. Every one in the corpus names 9 (`spread_data::MARKER_CURVE`), which the writers use. A stored spread beside a curve the file holds isn't read; the corpus has none.

In the corpus, every stored cell decodes, and all but a few re-encode to the same spelling (those write units as `0.000`). Every hours value is a whole number of minutes at three decimal places.

## Model mapping

`xer::to_model(&raw)` returns `(Vec<Schedule>, mapping::Diagnostics)` for representable projects, in project ID order. It maps project metadata/options, WBS, activities and stored results, internal/external relationships, resolved calendars, resource hierarchies/assignments, activity-code dictionaries/values and UDF definitions/values. Relationships appear in every imported project containing either endpoint, unless that project's own endpoint activity was omitted; external endpoint identities remain external. Missing predecessor project IDs can be recovered from unambiguous raw `TASK` ownership, including when that task's duration cannot map. Other missing identities are not invented.

Model entities retain packed raw-row `SourceRef` links. Shared entities are copied into schedules that reference them, with parent/base dependencies. A root project WBS discards its external EPS parent in the canonical tree. Unused tables, columns and values remain in raw storage; this projection does not rewrite the file.

Schedule-option fallbacks use the previously documented model defaults and generate diagnostics. `sched_outer_depend_type` remains unconfirmed (TF-008), rather than receiving a fabricated interpretation. Unsupported timestamp precision remains unparsed as in the typed layer. Non-time numeric fields use the model's existing finite `f64` representation; scheduling hour values use exact integer-minute conversion.

**Minute conversion.** Exact minutes and rounded/truncated whole-minute hour spellings at six or more decimal places are recognized (TF-012). Empty relationship lag cells mean zero. Other required values that cannot be represented cause a diagnosed omission; genuinely fractional-minute or malformed tokens are never silently rounded or defaulted. Relationships and assignments whose mapped ends are missing are omitted too, so the model never holds a dangling relationship or assignment. Optional references to missing entities are cleared with a warning. Any remaining `Schedule::validate` finding (an activity whose WBS or calendar is missing) is reported as `Integrity`. Optional unrepresentable floats remain `None` with a warning.

`Diagnostics::omitted` lists every omitted raw row, uncapped; diagnostic details are capped at 1,000 with complete per-kind counts. Callers must inspect the report and validate a schedule before using it; an imported projection is not proof of scheduling readiness.

The authored `fixtures/xer/model-mapping.xer` covers multiple projects, external ties, globals, codes and UDFs. Its stored results are illustrative mock inputs, not P6-computed ground truth. Unit tests validate this fixture's model integrity and raw source references.

The corpus test `corpus_model_accounts_for_every_raw_row` enforces the contract: every raw `PROJECT`, `TASK` and `TASKPRED` row is represented in a schedule or listed in `omitted`; no relationship or assignment dangles; and every remaining integrity finding is reported. Corpus, 2026-10-30: PROJECT 92 represented + 1 omitted; TASK 80,885 + 811; TASKPRED 185,642 + 3,041; 155 `Integrity` findings (153 activities with a missing WBS or calendar). [TF-012](rules/TF-012-xer-hours-and-whole-minutes.md) has the precision aggregates.


## Fresh model export

`xer::fresh::write(&Schedule, FreshOptions)` creates an export without using any
source XER bytes. `FreshOptions` requires an export date, accepts an explicit
GUID seed, and defaults to Windows-1252. Output has CRLF line endings; UTF-8 can
be selected explicitly. No clock or ambient randomness is used.

The target header is **23.12**, the most frequent *exact header spelling* among
the 44 tagged P6 exports in the current corpus (six files). Version spelling is
not normalized: `6.0` and `6.0.0` are separate groups. This selects a reference
schema, not a claim that every file labeled 23.12 has identical columns.
`fresh/schema.rs` records table/column metadata observed in one reference export in the corpus;
no schedule rows or identifying values were copied. A corpus test verifies this
selection and compares generated table/field order with that reference.

The reference has 25 tables. Fresh output emits 24, in the same order with the
same columns: the unused optional `RSRCLEVELLIST` table is omitted because no
resource-leveling configuration is modeled. Including it empty causes XER Tools
2.2b to report a missing blank field; omitting it passes the viewer's checks.
Other optional tables remain empty when no modeled entities use them.

Model IDs are retained, including references between tables. Default currency,
OBS and scheduling-option records use ID 1 in their separate table namespaces.
GUIDs use the first 16 bytes of a SHA-256 digest of an explicit domain, seed,
table name and identity, encoded as 22 unpadded standard-base64 characters. This
matches the observed storage shape; GUIDs are reproducible across runs, differ
by table and seed, and are regenerated independently of source provenance.

The default currency is USD with two decimal digits and an exchange rate of 1;
OBS metadata identifies the synthetic exporter. Required modeled fields use the
same exact scalar formatters as the patch writer. Dates retain minute precision,
flags use Y/N, enums use observed mappings, and minute/hour conversion must be
read-back exact in minutes (nonterminating hours use six decimal places; TF-012). Calendars contain all local
weekday intervals and exceptions using the TF-011 serial-day epoch. No dates,
floats or scheduling results are computed by this writer.

A valid standalone project needs exactly one project WBS root whose name agrees
with the model's project name. External relationships, unconfirmed external
option encodings, unknown enum/UDF types, unsupported UDF code lists,
unencodable strings and any state
that cannot round-trip exactly are refused. The entire output is read and mapped
back; its model must equal the input except for regenerated source links. This
also refuses unused dictionaries that the mapper would omit. Unknown optional
source columns are outside this model-only export; use the patch writer to
preserve an existing file.

Generate the authored fixture into a new path with:

```sh
cargo xtask fresh-fixture --output new-fixture.xer
```

`fixtures/xer/fresh-writer.xer` covers a work task, finish milestone, FS tie,
five-day calendar, resource assignment, activity code and text UDF. Its export
date and GUID seed are explicit; stored task results are empty. XER Tools 2.2b
application evidence is in [validation/xertools-2.2b.md](validation/xertools-2.2b.md).
