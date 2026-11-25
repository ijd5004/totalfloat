# TF-040: Typed spreads in XER

How TotalFloat reads and writes an assignment's units typed by period in an XER file, and what a stored spread that no longer fits its assignment means.

## Statement

1. **The marker.** An assignment whose curve ID names a curve the file doesn't hold, and which holds spread cells, has a typed spread. A curve the file lists but can't read isn't the marker.
2. **Three parts.** The planned, remaining and actual spreads are held apart, each as stretches in order from that part's start. Each stretch is some units over the next so many hours of work.
3. **The stretch format.** A cell is `units:hours`, stretches joined by `;`, with unsigned decimals. Hours are whole minutes, written to three decimal places. A cell that doesn't decode is kept as it was written, with a warning.
4. **Work time.** A stretch's hours are work time on the assignment's calendar ([TF-039](TF-039-resource-curves.md), item 6), and within a stretch units follow work time.
5. **A spread that no longer fits.** When a stored spread's work or units differ from its assignment's, the spread gives the shape: it is stretched over the work between the dates and scaled to the units. A spread that fits gives each stretch exactly its typed units. A spread with no units spreads evenly. Reading reports units that differ, and keeps the assignment's units as the file holds them.
6. **Alternatives.** An assignment holds a curve or a typed spread, never both. A stored spread beside a curve the file holds isn't read, and its cells are left as they are.
7. **Cells with no marker.** Spread cells on an assignment with no curve ID aren't read. The assignment spreads evenly, and the cells are kept as written.
8. **Writing.** The writers mark a typed spread with curve ID 9 and write all three cells. A file that already marks typed spreads with another missing curve ID keeps its own marker. When a typed spread needs the marker written and the model holds a real curve with ID 9, writing is refused, because the marker would name that curve.
9. **Clearing and replacing.** Clearing a typed spread clears the marker and all three cells. Typing a new spread replaces any old cells in the row.
10. **Where files hold it.** In XER: `TASKRSRC.curv_id`, `target_crv` (planned), `remain_crv` (remaining) and `actual_crv` (actual). The cell format is in `docs/xer-format.md`. In MS Project XML, an assignment's `TimephasedData` holds units by period.

## Options

None.

## Sources

- `vendor-doc`: [MSPDI schema](sources.md#msp-xml-schema), Assignment element and `TimephasedDataType`: an assignment's work can be stored by period, apart from its totals. Items 2 and 9.
- `standard`: [NDIA PASEG](sources.md#ndia-paseg), §6.2, pp. 81–85: resource loading is time phased against a task or resource calendar. Item 4.
- `observed-file`: files in the corpus whose typed assignments all name curve ID 9, which no corpus file holds as a curve. Every typed assignment carries a planned cell; a remaining cell is on exactly those with remaining units, and an actual cell only on finished ones. No assignment on a curve its file holds has any of the three. Items 1, 2 and 7.
- `observed-file`: the files in the corpus that hold spread cells. Every stored cell has the form in item 3 and decodes; every stretch's hours are whole minutes; and where a cell's hours add up to anything, they add up to the assignment's work hours, its units over its units per hour. Most stored remaining spreads on marked assignments don't match their assignment's units or work hours. Items 3 to 5.
- `observed-file`: files in the corpus that hold remaining spread cells on assignments with no curve ID. All of them are finished assignments with no remaining units, so the cells hold nothing to spread. Item 7.
- `own-choice`: items 1 and 5 to 9; the reasoning is under Rationale.

## Rationale

- **The marker** (item 1). The corpus pairs typed cells with a curve ID no file holds, and a missing curve can't shape anything else. Requiring the cells as well keeps an assignment with a stale curve ID and no cells on the even spread.
- **Scaling a stale spread** (item 5). Most stored spreads in the corpus were typed before their assignment's units or dates changed. Ignoring them would throw away the planner's shape, and refusing the file would lose everything else in it. Scaling keeps the shape and the assignment's own units, which the file states in its own columns. The report says where they differ.
- **Alternatives** (item 6). A curve and a typed spread both claim to say how the units fall, so holding both would be ambiguous. The file's held curve wins, and the cells beside it stay untouched so writing back changes nothing.
- **Cells with no marker** (item 7). Without the marker, nothing in the file says the cells are in use, and every corpus case is finished work with nothing left to spread. Keeping them as written means writing the file back changes nothing.
- **Writing** (items 8 and 9). Curve 9 is the marker every corpus file uses, so files TotalFloat writes look like them. Keeping a file's own marker means an unchanged assignment writes back unchanged. Clearing all three cells together keeps the parts from disagreeing.

## Tests

- `crates/xer/src/spread_data.rs`: the stretch format, malformed cells by position, and a property test that written cells read back (item 3).
- `crates/xer/tests/spreads.rs`: `the_typed_spread_is_read_with_no_warning`, `an_unreadable_curve_is_not_the_marker` (item 1), `units_that_differ_from_the_assignment_s_are_reported`, `an_unreadable_spread_is_reported_and_kept_raw` (items 3 and 5), `a_stored_spread_beside_a_held_curve_is_not_read`, `a_typed_spread_can_give_way_to_a_held_curve_but_not_sit_beside_it` (item 6), `the_spread_is_written_with_the_marker_p6_files_use`, `another_missing_curve_as_the_marker_is_kept`, `a_file_holding_curve_9_keeps_its_own_marker`, `spreads_the_writers_cannot_write_are_refused` (item 8), `clearing_the_typed_spread_clears_its_cells`, `typing_a_spread_replaces_old_cells_without_the_marker`, `changing_one_period_changes_only_its_cell` (item 9).
- `crates/engine/src/spread_tests.rs`: `a_typed_spread_gives_the_typed_units_per_period`, `a_typed_spread_that_differs_is_scaled_to_the_work_and_units`, `typed_units_sum_to_the_units` (items 4 and 5).
- `crates/xer/src/fresh/tests.rs`: `typed_spreads_round_trip` (item 8).
- `crates/xer/tests/corpus_raw.rs`: `corpus_typed_spreads_read_and_round_trip` (items 1 to 3 and 7, on the corpus).

## Differences

None known.
