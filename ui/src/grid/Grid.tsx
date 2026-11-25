import { type KeyboardEvent, type ReactNode, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import { ApiError, type QueryParams, type QueryReport, ROWS_ARRIVED, type Rows } from "../api";
import { fitCanvas } from "../canvas";
import type { DateFormat } from "../dates";
import type { NewActivity } from "../edit/shape";
import type { View } from "../view";
import {
  CONSTRAINT_KINDS,
  type CellKey,
  type CellProblem,
  type Typed,
  calendarsOf,
  cellKey,
  indexOf,
  inputKind,
  inputText,
} from "../edit/cells";
import { COLUMN_BY_ID, COLUMNS, COMPARE_COLUMNS, type ColumnId, type Compared, type Units, showCompared } from "./columns";
import { INDENT, type Palette, draw, palette } from "./draw";
import { FrameClock, framesWanted } from "./frames";
import { type CodeChoices, FilterBuilder, type SavedFilters, said } from "./FilterBuilder";
import { CHOICE_BY_ID, type Condition, complete, groupsIn, listItem, paramsOf } from "./filters";
import { BUILT_IN_GROUPS, builtInGrouping, isBuiltInGroup } from "./groups";
import { Icon } from "../shell/icons";
import { ToolPopover } from "./ToolPopover";
import {
  type Arrangement,
  DEFAULT_ARRANGEMENT,
  type Grouping,
  type QuickFilter,
  type SortKey,
  arrange,
  bandOf,
  buildModel,
} from "./model";

const GRID_PAINTED = "totalfloat:grid-painted";
/** How long scrolling must pause before the screen-reader mirror follows it. */
const MIRROR_SETTLE_MS = 120;

/** The measure from rows arriving to the grid first painting them. */
export const FIRST_PAINT = "totalfloat:grid-first-paint";

const QUICK: { id: QuickFilter; label: string }[] = [
  { id: "critical", label: "Critical" },
  { id: "not-started", label: "Not started" },
  { id: "in-progress", label: "In progress" },
  { id: "completed", label: "Complete" },
];

/** A cell, by line in the arrangement and position among the columns shown. */
interface Cell {
  line: number;
  column: number;
}

/**
 * A selected cell as kept between arrangements: what's on its line (`a:` and
 * the activity's `task_id`, or `band:` and a WBS path), so it follows that
 * activity through sorting, filtering, collapsing and fresh rows from the
 * server, and the line it was on, for when what was there no longer shows.
 */
interface Spot extends Cell {
  item: string | null;
}

const START: Spot = { item: null, line: 0, column: 0 };

function clamp(value: number, low: number, high: number): number {
  return Math.max(low, Math.min(high, value));
}

/** The next sort after a click on a header: ascending, then descending, then off. Shift adds a key after the others. */
export function nextSort(sort: readonly SortKey[], column: ColumnId, shift: boolean): SortKey[] {
  const at = sort.findIndex((key) => key.column === column);
  const cycle = (key: SortKey | undefined): SortKey | null =>
    !key ? { column, descending: false } : key.descending ? null : { column, descending: true };
  if (!shift) {
    const next = cycle(at === 0 ? sort[0] : undefined);
    return next ? [next] : [];
  }
  const next = cycle(sort[at]);
  const others = sort.filter((key) => key.column !== column);
  if (at >= 0) return next ? sort.map((key) => (key.column === column ? next : key)) : others;
  return [...others, next ?? { column, descending: false }];
}

/**
 * The tab-separated text of a range, activity rows only, as a spreadsheet
 * takes it, with typed values not yet recorded as they show, and dates as
 * the server writes them, `YYYY-MM-DD HH:MM`, whatever format is shown.
 */
export function rangeText(
  rows: Rows,
  lines: Int32Array,
  columns: ColumnId[],
  from: Cell,
  to: Cell,
  typed?: ReadonlyMap<CellKey, string>,
): string {
  const [top, bottom] = from.line <= to.line ? [from.line, to.line] : [to.line, from.line];
  const [l, r] = from.column <= to.column ? [from.column, to.column] : [to.column, from.column];
  const out: string[] = [];
  for (let line = top; line <= bottom; line++) {
    const i = lines[line] ?? -1;
    if (i < 0) continue;
    const cells: string[] = [];
    for (let c = l; c <= r; c++) {
      const column = COLUMN_BY_ID.get(columns[c] ?? "code");
      const shown = column ? (typed?.get(cellKey(rows.id[i] ?? -1, column.id)) ?? column.raw?.(rows, i) ?? column.text(rows, i)) : "";
      cells.push(shown.replace(/[\t\n\r]/g, " "));
    }
    out.push(cells.join("\t"));
  }
  return out.join("\n");
}

/** Tab-separated text as a grid of cells, as spreadsheets copy it. */
export function parseRange(text: string): string[][] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  if (lines.length > 1 && lines.at(-1) === "") lines.pop();
  return lines.map((line) => line.split("\t"));
}

/**
 * Pasted cells from the top-left of the selection: down the activities shown,
 * passing over bands, and across the columns shown. Cells that would fall past
 * the last row or column are left out and counted.
 */
export function pastedCells(
  rows: Rows,
  lines: Int32Array,
  columns: ColumnId[],
  from: Cell,
  range: string[][],
): { typed: Typed[]; outside: number } {
  const typed: Typed[] = [];
  let outside = 0;
  let line = from.line;
  for (const values of range) {
    while (line < lines.length && (lines[line] ?? -1) < 0) line++;
    const i = lines[line];
    values.forEach((text, c) => {
      const column = columns[from.column + c];
      if (i === undefined || i < 0 || !column) {
        outside++;
        return;
      }
      typed.push({ id: rows.id[i] ?? -1, column, text });
    });
    line++;
  }
  return { typed, outside };
}

/** A field the grid can group by: an activity code type or a user-defined field. */
export interface GroupChoice {
  /** As `query` names the column: `code:TYPE` or `udf:LABEL`. */
  field: string;
  title: string;
  /** Activity codes or user fields, for the list. */
  kind: "code" | "udf";
  /** Values' places, such as a code type's own sequence. */
  order?: ReadonlyMap<string, number>;
}

/** What the grid asks the server for its built filters and groups. */
export interface GridFinder {
  /** Fields to group by. */
  groups: readonly GroupChoice[];
  /** Code values a filter can pick. */
  codes: readonly CodeChoices[];
  /** Runs `query` on the server. */
  query(params: QueryParams): Promise<QueryReport>;
  /** Named filters kept beside the schedule, where the server keeps them. */
  saved?: SavedFilters | undefined;
}

/** Group levels at most: the first and three more. */
const MAX_LEVELS = 4;

/** What rows can be grouped by, less the levels `without` names. */
function GroupOptions({ finder, without }: { finder: GridFinder | undefined; without?: ReadonlySet<string> }) {
  const shown = (field: string) => !without?.has(field);
  return (
    <>
      {shown("wbs") && <option value="wbs">WBS</option>}
      <optgroup label="Activity fields">
        {BUILT_IN_GROUPS.filter((g) => shown(g.field)).map((g) => (
          <option key={g.field} value={g.field}>
            {g.title}
          </option>
        ))}
      </optgroup>
      {(["code", "udf"] as const).map((kind) => {
        const choices = finder?.groups.filter((g) => g.kind === kind && shown(g.field)) ?? [];
        return choices.length === 0 ? null : (
          <optgroup key={kind} label={kind === "code" ? "Activity codes" : "User fields"}>
            {choices.map((g) => (
              <option key={g.field} value={g.field}>
                {g.title}
              </option>
            ))}
          </optgroup>
        );
      })}
    </>
  );
}

const NO_CONDITIONS: Condition[] = [];

/** A filter applied, as the toolbar shows it. */
interface Chip {
  key: string;
  label: string;
  title?: string;
  /** Critical's red mark. */
  swatch?: boolean;
  remove: () => void;
}

/** What a grouping field is called: `WBS`, an activity field's title, or a code's or user field's. */
function groupTitle(field: string, finder: GridFinder | undefined): string {
  if (field === "wbs") return "WBS";
  return BUILT_IN_GROUPS.find((g) => g.field === field)?.title ?? finder?.groups.find((g) => g.field === field)?.title ?? field;
}


/** The `task_id`s a query found. */
function idsOf(report: QueryReport): Set<number> {
  const ids = new Set<number>();
  for (const row of report.rows) if (typeof row.id === "number") ids.add(row.id);
  return ids;
}

/** Each activity's value of a field, by `task_id`, from a query of `id` and the field. */
function valuesOf(report: QueryReport, field: string): Map<number, string | number> {
  const values = new Map<number, string | number>();
  for (const row of report.rows) {
    const value = (row as Record<string, unknown>)[field];
    if (typeof row.id === "number" && (typeof value === "string" || typeof value === "number")) values.set(row.id, value);
  }
  return values;
}

/** What the grid needs to edit: typed values, refusals, and where to send cells. */
/** How the grid is set out, as a layout file keeps it. */
export interface GridLayout {
  /** The columns shown, in order, with their widths; Before and After aren't among them. */
  columns: { field: ColumnId; width: number }[];
  sort: readonly SortKey[];
  grouped: boolean;
  /** The field grouped by, as `query` names a column (`code:TYPE`, `udf:LABEL`); null for the WBS. */
  groupBy: string | null;
  /** Further levels, each `wbs` or a field as `groupBy` names one; left out when there are none. */
  thenBy?: readonly string[];
  text: string;
  quick: readonly QuickFilter[];
  /** The filter builder's conditions applied. */
  built: readonly Condition[];
}

/** The columns in order with Before and After placed after the name (or the first column), where a comparison shows them. */
function withCompareColumns(fields: readonly ColumnId[]): ColumnId[] {
  const plain = fields.filter((id) => !COMPARE_COLUMNS.has(id));
  const at = plain.includes("name") ? plain.indexOf("name") + 1 : Math.min(1, plain.length);
  return [...plain.slice(0, at), "before", "after", ...plain.slice(at)];
}

export interface GridEditing {
  enabled: boolean;
  /** Typed values not yet recorded, shown in their cells. */
  values: ReadonlyMap<CellKey, string>;
  /** Refused cells, and why. */
  marks: ReadonlyMap<CellKey, CellProblem>;
  /** Whether a preview is waiting, so Enter records it. */
  previewed: boolean;
  commit(typed: Typed[]): void;
  apply(): void;
  discard(): void;
  say(text: string): void;
  /** Whether a key press is an app shortcut, which typing mustn't take. */
  isShortcut(event: KeyboardEvent<HTMLElement>): boolean;
  /** Adds an activity typed into a new row; says why not, or null once it's taken. Building only. */
  add?(activity: NewActivity): string | null;
  /** The Activity ID a new activity in WBS `wbs` is offered. */
  nextId?(wbs: string): string;
  /** Adds a WBS node under `parent`; says why not, or null once it's taken. Building only. */
  addWbs?(parent: string, node: WbsTyped): string | null;
  /** Renames the WBS node at `path`; says why not, or null once it's taken. Building only. */
  renameWbs?(path: string, node: WbsTyped): string | null;
  /** The node at `path`'s code and name as they'll be, with renames waiting. */
  wbsNode?(path: string): WbsTyped | undefined;
}

/** A WBS node's code and name, as typed. */
export interface WbsTyped {
  code: string;
  name: string;
}

/** What the frame asks of the grid: adding an activity where the selection is, and which are selected. */
export interface GridActions {
  /** Opens a new row in the selected row's WBS node, or the first. */
  addHere(): void;
  /** Opens a new WBS node under the selected band, or the selected activity's node. */
  addWbsHere(): void;
  /** The `task_id`s of the activities selected. */
  selected(): number[];
}

/** The input over a cell being edited. */
interface Editor {
  id: number;
  column: ColumnId;
  text: string;
  /**
   * What F2 or Enter opened it with: closing with that unchanged sends
   * nothing, so it can't undo a change that arrived meanwhile. Null when
   * typing opened it, which always sends what's there, even nothing.
   */
  opened: string | null;
}

/**
 * The activity grid: a canvas that draws only the rows on screen, under an
 * ordinary header for sorting, resizing and moving columns. An off-screen
 * table mirrors the rows on screen for screen readers. With `editing`, a cell
 * opens an input over it on F2, Enter or typing.
 */
export function Grid({
  rows,
  filterRef,
  editing,
  view,
  path = null,
  bandOrder,
  bandNames,
  compared = null,
  layout = null,
  onLayout,
  finder,
  toolbar,
  status,
  actions,
}: {
  rows: Rows | null;
  filterRef?: React.RefObject<HTMLInputElement | null>;
  editing?: GridEditing;
  view?: View;
  /** A highlighted driving path, by `task_id`, which the grid can filter to. */
  path?: ReadonlySet<number> | null;
  /** Each WBS node's place in the WBS, by path, so bands follow the WBS sequence. */
  bandOrder?: ReadonlyMap<string, number> | undefined;
  /** Each named WBS node's name, by path, which its band shows before the path. */
  bandNames?: ReadonlyMap<string, string> | undefined;
  /** How durations and float are shown; the grid draws again when it changes. */
  units?: Units;
  /** How dates are shown; the grid draws again when it changes. */
  dates?: DateFormat;
  /** A comparison to show in the Before and After columns, which show only while there is one. */
  compared?: Compared | null;
  /** A layout to set the grid out by, applied each time a new one is given. */
  layout?: GridLayout | null;
  /** Hears how the grid is set out, each time that changes. */
  onLayout?: (layout: GridLayout) => void;
  /** The server's `query`, for built filters and grouping by codes and fields. */
  finder?: GridFinder | undefined;
  /** Where the filters and grouping go, a row of the frame's own; above the grid without one. */
  toolbar?: HTMLElement | null;
  /** Where the counts of activities shown and selected go, in the frame's status bar; in the toolbar without one. */
  status?: HTMLElement | null;
  /** Filled with what the frame can ask of the grid. */
  actions?: { current: GridActions | null };
}) {
  // Set as the grid renders, so its text, sorting and drawing all read it.
  showCompared(compared);
  const comparing = compared !== null;
  const model = useMemo(() => (rows ? buildModel(rows) : null), [rows]);
  const [arrangement, setArrangement] = useState<Arrangement>(DEFAULT_ARRANGEMENT);
  // The built filter applied and the activities the server found for it,
  // set together; the builder open; and the field grouped by (null for the
  // WBS), found anew when the rows change.
  const [filter, setFilter] = useState<{ conditions: Condition[]; ids: ReadonlySet<number> } | null>(null);
  const built = filter?.conditions ?? NO_CONDITIONS;
  const [building, setBuilding] = useState(false);
  // The Add filter menu and the grouping panel, open or not.
  const [adding, setAdding] = useState(false);
  const [grouping, setGrouping] = useState(false);
  // The chips, listed from their count in a narrow window.
  const [listing, setListing] = useState(false);
  const [groupField, setGroupField] = useState<string | null>(null);
  const [filterProblem, setFilterProblem] = useState<string | null>(null);
  // Why a level couldn't be grouped by, by its field.
  const [groupProblems, setGroupProblems] = useState<ReadonlyMap<string, string>>(new Map());
  const clearGroupProblems = () => setGroupProblems(new Map());
  // Further levels, `wbs` or a field; and each code or user field's values,
  // with the rows they were found for.
  const [thenFields, setThenFields] = useState<readonly string[]>([]);
  const [fetched, setFetched] = useState<ReadonlyMap<string, { rows: Rows; grouping: Grouping }>>(new Map());
  // Counts each apply and clear, so an answer to an apply since cleared or
  // replaced is dropped; and holds the conditions applied, for the refetch.
  const filtering = useRef<{ generation: number; conditions: Condition[] }>({ generation: 0, conditions: [] });
  // A layout's built filter, waiting for the server's `query` to find it.
  // Clearing or applying another filter replaces it.
  const [pendingBuilt, setPendingBuilt] = useState<readonly Condition[] | null>(null);
  // The conditions last asked for, while the server's answer is on its way,
  // so a chip removed meanwhile takes from them rather than from what's shown.
  const wantedBuilt = useRef<Condition[] | null>(null);
  const dropFilter = useCallback(() => {
    wantedBuilt.current = [];
    setPendingBuilt(null);
    filtering.current = { generation: filtering.current.generation + 1, conditions: [] };
    setFilter(null);
    setFilterProblem(null);
  }, []);
  // Asks for the applied filter's activities again: after an apply whose
  // rows changed while it waited.
  const [refind, setRefind] = useState(0);
  const latestRows = useRef(rows);
  useEffect(() => {
    latestRows.current = rows;
  }, [rows]);
  useEffect(() => {
    // The rows changed: find the applied filter's activities again. Applying
    // finds them itself, so this runs only for new rows. An answer for
    // conditions since cleared or replaced is dropped.
    const { generation, conditions } = filtering.current;
    if (!finder || conditions.length === 0) return;
    let current = true;
    const still = () =>
      current && filtering.current.generation === generation && filtering.current.conditions === conditions;
    finder.query({ ...paramsOf(conditions), columns: "id" }).then(
      (report) => {
        if (!still()) return;
        setFilter({ conditions, ids: idsOf(report) });
        setFilterProblem(null);
      },
      (error: unknown) => {
        if (still()) {
          setFilterProblem(`The filter couldn't be found again, so it shows the activities it found before: ${said(error)}`);
        }
      },
    );
    return () => {
      current = false;
    };
  }, [finder, rows, refind]);
  useEffect(() => {
    if (pendingBuilt === null || !finder) return;
    const generation = ++filtering.current.generation;
    const asked = latestRows.current;
    let current = true;
    const still = () => current && filtering.current.generation === generation;
    const find = (conditions: Condition[]) => finder.query({ ...paramsOf(conditions), columns: "id" });
    void (async () => {
      if (pendingBuilt.length === 0) {
        setPendingBuilt(null);
        dropFilter();
        return;
      }
      let conditions = [...pendingBuilt];
      let problem: string | null = null;
      let report: QueryReport;
      try {
        report = await find(conditions);
      } catch (error) {
        // A code the schedule no longer has: that condition goes, the rest
        // apply. In an "any of" group, the group goes, since it can't be met.
        const codes = conditions.filter((c) => c.field === "code");
        const message = error instanceof Error ? error.message : "";
        const typeGone = /^no activity code type is named (.*)$/.exec(message)?.[1];
        const gone =
          codes.find((c) => message === `no activity code matches ${c.value}`) ??
          codes.find((c) => typeGone !== undefined && c.value.startsWith(`${typeGone}=`)) ??
          (codes.length === 1 ? codes[0] : undefined);
        const missing = error instanceof ApiError && (error.code === "no-such-code" || error.code === "no-such-code-type");
        if (!still()) return;
        if (!missing || !gone) {
          setPendingBuilt(null);
          setFilterProblem(`The layout's filter couldn't be applied: ${said(error)}`);
          return;
        }
        const inGroup = gone.group !== undefined;
        conditions = conditions.filter((c) => (inGroup ? c.group !== gone.group : c !== gone));
        problem = inGroup
          ? `This schedule has no activity code ${gone.value}, so the layout's filter goes without the "any of" group that needs it.`
          : `This schedule has no activity code ${gone.value}, so the layout's filter goes without it.`;
        if (conditions.length === 0) {
          setPendingBuilt(null);
          dropFilter();
          setFilterProblem(problem);
          return;
        }
        try {
          report = await find(conditions);
        } catch (again) {
          if (still()) {
            setPendingBuilt(null);
            setFilterProblem(`The layout's filter couldn't be applied: ${said(again)}`);
          }
          return;
        }
      }
      if (!still()) return;
      filtering.current = { generation, conditions };
      // A layout's filter is what's asked for now.
      wantedBuilt.current = null;
      setFilter({ conditions, ids: idsOf(report) });
      setPendingBuilt(null);
      setFilterProblem(problem);
      if (latestRows.current !== asked) setRefind((n) => n + 1);
    })();
    return () => {
      current = false;
    };
  }, [finder, pendingBuilt, dropFilter]);
  const grouped = arrangement.grouped;
  // A field this schedule doesn't have, as a layout can name: WBS bands
  // instead at the first level, and none at a later one.
  const known = (field: string) =>
    field === "wbs" || isBuiltInGroup(field) || !finder || finder.groups.some((g) => g.field === field);
  const groupGone = groupField !== null && !known(groupField) ? groupField : null;
  const groupBy = groupGone === null ? groupField : null;
  const thenKey = thenFields.join("\u0000");
  const thenBy = useMemo(
    () => {
      const seen = new Set([groupBy ?? "wbs"]);
      return thenFields.filter((field) => known(field) && !seen.has(field) && Boolean(seen.add(field)));
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [thenKey, groupBy, finder],
  );
  const thenGone = thenFields.filter((field) => !known(field));
  const levelFields = useMemo(
    () => (grouped ? [groupBy, ...thenBy].filter((f): f is string => f !== null && f !== "wbs") : []),
    [grouped, groupBy, thenBy],
  );
  const levelKey = levelFields.join("\u0000");
  // Status, type, calendar, constraint and months come from the rows themselves.
  const builtIns = useMemo(
    () => {
      const made = new Map<string, Grouping>();
      for (const field of levelFields) {
        const grouping = model && isBuiltInGroup(field) ? builtInGrouping(model, field) : null;
        if (grouping) made.set(field, grouping);
      }
      return made;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [model, levelKey],
  );
  // Codes and user fields come from the server, found again when the rows
  // change. Each field is asked for once per rows, and an answer still
  // counts if the levels change while it's coming.
  const asking = useRef(new Map<string, Rows>());
  useEffect(() => {
    if (!finder || !rows) return;
    for (const field of levelKey.split("\u0000")) {
      if (field === "" || isBuiltInGroup(field) || fetched.get(field)?.rows === rows) continue;
      if (asking.current.get(field) === rows) continue;
      const choice = finder.groups.find((g) => g.field === field);
      if (!choice) continue;
      asking.current.set(field, rows);
      const settled = () => {
        if (asking.current.get(field) !== rows) return false;
        asking.current.delete(field);
        return latestRows.current === rows;
      };
      finder.query({ columns: `id,${listItem(choice.field)}` }).then(
        (report) => {
          if (!settled()) return;
          const grouping: Grouping = {
            field: choice.field,
            title: choice.title,
            values: valuesOf(report, choice.field),
            ...(choice.order ? { order: choice.order } : {}),
          };
          setFetched((now) => new Map(now).set(field, { rows, grouping }));
          setGroupProblems((now) => {
            const next = new Map(now);
            next.delete(field);
            return next;
          });
        },
        (error: unknown) => {
          if (!settled()) return;
          setGroupProblems((now) =>
            new Map(now).set(field, `The rows couldn't be grouped by ${choice.title}: ${said(error)}`),
          );
        },
      );
    }
    // `fetched` changes as answers come; `asking` keeps each field to one question per rows.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [finder, levelKey, rows]);
  const groupingOf = (field: string | null): Grouping | null | undefined =>
    field === null || field === "wbs" ? null : (builtIns.get(field) ?? (finder ? fetched.get(field)?.grouping : undefined));
  // Filtering to a driving path lasts while there's one; clearing the path shows everything.
  const [onPath, setOnPath] = useState(false);
  const [pathWas, setPathWas] = useState(path);
  if (pathWas !== path) {
    setPathWas(path);
    // A cleared path leaves nothing to filter to; the next one starts unfiltered.
    if (!path && onPath) setOnPath(false);
  }
  const matching = finder && filter ? filter.ids : null;
  // A level whose values haven't come yet waits: the WBS at the first level.
  const by = groupingOf(groupBy) ?? null;
  // Levels show in order up to the first still waiting for its values, so
  // bands keep their paths as the rest arrive.
  const thenShown = useMemo(
    () => {
      const shown: (Grouping | null)[] = [];
      if (groupBy !== null && groupingOf(groupBy) === undefined) return shown;
      for (const field of thenBy) {
        const grouping = groupingOf(field);
        if (grouping === undefined) break;
        shown.push(grouping);
      }
      return shown;
    },
    // `groupingOf` reads these.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [thenBy, builtIns, fetched, finder, groupBy],
  );
  // A WBS node being typed in place: a new one under `path`, or `path` renamed.
  const [wbsRow, setWbsRow] = useState<({ mode: "add" | "rename"; path: string } & WbsTyped) | null>(null);
  const [wbsProblem, setWbsProblem] = useState<string | null>(null);
  // Building: the WBS's paths, each with a band and an Add activity line,
  // when activities can be added and the rows are grouped by the WBS first.
  const canAdd = !!editing?.add && editing.enabled;
  const wbsPaths = useMemo(
    () => (canAdd && arrangement.grouped && !by && bandOrder ? new Set(bandOrder.keys()) : null),
    [canAdd, arrangement.grouped, by, bandOrder],
  );
  const shownArrangement = useMemo(
    () => ({
      ...arrangement,
      only: onPath && path ? path : null,
      matching,
      by,
      then: thenShown,
      building: wbsPaths,
      addingWbs: wbsRow?.mode === "add" ? wbsRow.path : null,
    }),
    [arrangement, onPath, path, matching, by, thenShown, wbsPaths, wbsRow],
  );
  // A sort on Before or After goes when they do.
  const [comparingWas, setComparingWas] = useState(comparing);
  if (comparingWas !== comparing) {
    setComparingWas(comparing);
    if (!comparing && arrangement.sort.some((key) => COMPARE_COLUMNS.has(key.column))) {
      setArrangement((now) => ({ ...now, sort: now.sort.filter((key) => !COMPARE_COLUMNS.has(key.column)) }));
    }
  }
  const arranged = useMemo(
    () => (model ? arrange(model, shownArrangement, bandOrder, bandNames) : null),
    // A new comparison sorts Before and After anew.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [model, shownArrangement, bandOrder, bandNames, compared],
  );
  const [allColumns, setOrder] = useState<ColumnId[]>(COLUMNS.map((c) => c.id));
  // Before and After show only while there's a comparison.
  const order = useMemo(
    () => allColumns.filter((id) => comparing || !COMPARE_COLUMNS.has(id)),
    [allColumns, comparing],
  );
  const [widths, setWidths] = useState<Record<ColumnId, number>>(
    () => Object.fromEntries(COLUMNS.map((c) => [c.id, c.width])) as Record<ColumnId, number>,
  );
  // A layout given: its columns, widths, sort, grouping and filters.
  const [layoutWas, setLayoutWas] = useState<GridLayout | null>(null);
  if (layout && layoutWas !== layout) {
    setLayoutWas(layout);
    setOrder(withCompareColumns(layout.columns.map((c) => c.field)));
    setWidths((now) => ({ ...now, ...Object.fromEntries(layout.columns.map((c) => [c.field, c.width])) }));
    setGroupField(layout.groupBy);
    setThenFields(layout.thenBy ?? []);
    clearGroupProblems();
    setFilterProblem(null);
    setPendingBuilt(layout.built);
    setArrangement((now) => ({
      ...now,
      sort: [...layout.sort],
      grouped: layout.grouped,
      text: layout.text,
      quick: new Set(layout.quick),
    }));
  }
  // How it's set out now, for whoever keeps layouts.
  useEffect(() => {
    onLayout?.({
      columns: allColumns.filter((id) => !COMPARE_COLUMNS.has(id)).map((field) => ({ field, width: widths[field] })),
      sort: arrangement.sort.filter((key) => !COMPARE_COLUMNS.has(key.column)),
      grouped: arrangement.grouped,
      groupBy: arrangement.grouped ? groupBy : null,
      ...(arrangement.grouped && thenBy.length > 0 ? { thenBy: [...thenBy] } : {}),
      text: arrangement.text,
      quick: [...arrangement.quick],
      built: [...(pendingBuilt ?? built)],
    });
  }, [
    onLayout,
    allColumns,
    widths,
    arrangement.sort,
    arrangement.grouped,
    arrangement.text,
    arrangement.quick,
    groupBy,
    thenBy,
    pendingBuilt,
    built,
  ]);
  const [active, setActive] = useState<Spot>(START);
  const [anchor, setAnchor] = useState<Spot>(START);
  // The selection stays on its columns when columns show, hide or move.
  const [orderWas, setOrderWas] = useState(order);
  if (orderWas !== order) {
    setOrderWas(order);
    const follow = (spot: Spot): Spot => {
      const at = order.indexOf(orderWas[spot.column] ?? "code");
      return { ...spot, column: at >= 0 ? at : Math.min(spot.column, order.length - 1) };
    };
    setActive(follow(active));
    setAnchor(follow(anchor));
  }
  const [scroll, setScroll] = useState({ top: 0, left: 0 });
  // Where the screen-reader mirror starts: the scroll position once scrolling
  // settles, so a fast scroll doesn't rebuild the mirror's rows every frame,
  // which forced a layout each frame and halved the frame rate at 50k.
  const [mirrorTop, setMirrorTop] = useState(0);
  useEffect(() => {
    if (mirrorTop === scroll.top) return;
    const settle = setTimeout(() => setMirrorTop(scroll.top), MIRROR_SETTLE_MS);
    return () => clearTimeout(settle);
  }, [scroll.top, mirrorTop]);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [focused, setFocused] = useState(false);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [hover, setHover] = useState<Cell | null>(null);
  const calendars = useMemo(
    () => (rows && editor?.column === "calendar" ? calendarsOf(rows) : []),
    [rows, editor?.column],
  );

  const scroller = useRef<HTMLDivElement | null>(null);
  // The body only exists once rows arrive, so it's tracked as state too, for
  // the effect that measures it.
  const [body, setBody] = useState<HTMLDivElement | null>(null);
  const attachBody = useCallback((element: HTMLDivElement | null) => {
    scroller.current = element;
    setBody(element);
  }, []);
  const canvas = useRef<HTMLCanvasElement>(null);
  const colours = useRef<Palette | null>(null);
  const painted = useRef<Rows | null>(null);
  const clock = useMemo(() => new FrameClock(framesWanted()), []);
  const [rowHeight] = useState(rowHeightToken);

  const columns = useMemo(() => order.map((id) => COLUMN_BY_ID.get(id)).filter((c) => c !== undefined), [order]);
  const columnWidths = useMemo(() => order.map((id) => widths[id]), [order, widths]);
  const totalWidth = columnWidths.reduce((a, b) => a + b, 0);
  const lineCount = arranged?.lines.length ?? 0;

  // Where each activity and band is now, so the selection can follow them.
  const lineOf = useMemo(() => {
    const map = new Map<string, number>();
    if (!arranged || !rows) return map;
    arranged.lines.forEach((value, line) => {
      map.set(value >= 0 ? `a:${rows.id[value] ?? value}` : `band:${arranged.bands[-value - 1]?.path ?? ""}`, line);
    });
    return map;
  }, [arranged, rows]);
  const itemAt = (line: number): string | null => {
    const value = arranged?.lines[line];
    if (value === undefined || !rows) return null;
    return value >= 0 ? `a:${rows.id[value] ?? value}` : `band:${arranged?.bands[-value - 1]?.path ?? ""}`;
  };
  const place = (spot: Spot): Cell => {
    const found = spot.item === null ? undefined : lineOf.get(spot.item);
    return {
      line: clamp(found ?? spot.line, 0, Math.max(lineCount - 1, 0)),
      column: clamp(spot.column, 0, columns.length - 1),
    };
  };
  const activeCell = place(active);
  const anchorCell = place(anchor);

  // Copy and paste go to the grid whenever its body has the focus. Browsers
  // send these events to where the page's selection is, which needn't be the
  // focused body: Firefox sends them to <body>, and Chromium to a caret left in
  // a text field. So they're heard on the document, and taken only when
  // the body itself is focused; a cell's input or any text field keeps its own.
  const clipboard = useRef<{ copy: (event: ClipboardEvent) => void; paste: (event: ClipboardEvent) => void } | null>(null);
  useLayoutEffect(() => {
    clipboard.current =
      rows && arranged
        ? {
            copy: (event) => {
              event.clipboardData?.setData("text/plain", rangeText(rows, arranged.lines, order, anchorCell, activeCell, editing?.values));
              event.preventDefault();
            },
            paste: (event) => {
              if (!editing) return;
              event.preventDefault();
              if (!editing.enabled) {
                editing.say("This schedule is read-only here.");
                return;
              }
              const from = {
                line: Math.min(anchorCell.line, activeCell.line),
                column: Math.min(anchorCell.column, activeCell.column),
              };
              // Nothing to paste, as when the clipboard holds only an image.
              const text = event.clipboardData?.getData("text/plain") ?? "";
              if (text === "") return;
              const { typed, outside } = pastedCells(rows, arranged.lines, order, from, parseRange(text));
              if (outside > 0) {
                editing.say(`The pasted range runs ${outside} cell${outside === 1 ? "" : "s"} past the grid's edge, so nothing was pasted.`);
                return;
              }
              if (typed.some((cell) => cell.id < 0)) {
                editing.say("Activities waiting to be added can't take pasted values until you apply them.");
                return;
              }
              if (typed.length > 0) editing.commit(typed);
            },
          }
        : null;
  });
  useEffect(() => {
    if (!body) return;
    const document = body.ownerDocument;
    const handler = (kind: "copy" | "paste") => (event: ClipboardEvent) => {
      if (document.activeElement !== body || event.defaultPrevented) return;
      clipboard.current?.[kind](event);
    };
    const copy = handler("copy");
    const paste = handler("paste");
    document.addEventListener("copy", copy);
    document.addEventListener("paste", paste);
    return () => {
      document.removeEventListener("copy", copy);
      document.removeEventListener("paste", paste);
    };
  }, [body]);

  // The canvas follows the scroller's size; the theme follows the OS.
  useLayoutEffect(() => {
    const element = body;
    if (!element) return;
    const measure = () => setSize({ width: element.clientWidth, height: element.clientHeight });
    measure();
    // Both are missing where there's no layout, as in tests.
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(element);
    const scheme = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
    const recolour = () => {
      colours.current = palette(element);
      measure();
    };
    recolour();
    scheme?.addEventListener("change", recolour);
    return () => {
      observer?.disconnect();
      scheme?.removeEventListener("change", recolour);
    };
  }, [body]);

  // Draw on the next frame whenever anything shown changes.
  useEffect(() => {
    const element = canvas.current;
    if (!element || !rows || !arranged || !colours.current) return;
    const frame = requestAnimationFrame((now) => {
      clock.tick(now);
      const width = Math.max(size.width, 1);
      const height = Math.max(size.height, 1);
      const { ctx, resized } = fitCanvas(element, width, height);
      // The spacer sets the scroll size; the sticky canvas mustn't add to it.
      if (resized) element.style.marginBottom = `${-height}px`;
      if (!ctx || !colours.current) return;
      if (painted.current !== rows) {
        painted.current = rows;
        // How long after the rows arrived the grid first showed them; only
        // the latest is kept, so a long session doesn't pile them up.
        performance.clearMarks(GRID_PAINTED);
        performance.clearMeasures(FIRST_PAINT);
        performance.mark(GRID_PAINTED);
        if (performance.getEntriesByName(ROWS_ARRIVED).length > 0) {
          performance.measure(FIRST_PAINT, ROWS_ARRIVED, GRID_PAINTED);
        }
      }
      draw(
        ctx,
        {
          rows,
          arranged,
          columns,
          widths: columnWidths,
          rowHeight,
          top: scroll.top,
          left: scroll.left,
          width,
          height,
          selection: {
            fromLine: Math.min(anchorCell.line, activeCell.line),
            toLine: Math.max(anchorCell.line, activeCell.line),
            fromColumn: Math.min(anchorCell.column, activeCell.column),
            toColumn: Math.max(anchorCell.column, activeCell.column),
          },
          active: activeCell,
          focused,
          values: editing?.values,
          marks: editing?.marks,
        },
        colours.current,
      );
      // The Gantt draws the same rows, in this same frame.
      view?.publish({
        rows,
        arranged,
        top: scroll.top,
        height,
        rowHeight,
        active: activeCell.line,
        bodyTop: element.getBoundingClientRect().top,
      });
    });
    return () => cancelAnimationFrame(frame);
  });

  // The Gantt asks for an activity: the active cell goes to it, on screen.
  // Registered once; the handler reads this render's state through a ref.
  // An activity a filter or a collapsed band hides is shown first: the
  // filters are cleared and its bands opened, then it's selected once it shows.
  const reveal = useRef<(id: number) => void>(() => {});
  const revealing = useRef<number | null>(null);
  useLayoutEffect(() => {
    reveal.current = (id) => {
      const line = lineOf.get(`a:${id}`);
      if (line !== undefined) {
        moveTo({ line, column: activeCell.column }, false);
        return;
      }
      const i = rows?.id.indexOf(id) ?? -1;
      if (i < 0) return;
      revealing.current = id;
      const wbs = rows?.wbs_path[i] ?? "";
      setOnPath(false);
      dropFilter();
      setArrangement((now) => ({
        ...now,
        text: "",
        quick: new Set(),
        // Its WBS bands open, and every band of a field grouped by.
        collapsed: new Set(
          [...now.collapsed].filter(
            (band) => !band.includes("\u0000") && wbs !== band && !wbs.startsWith(`${band}.`),
          ),
        ),
      }));
    };
    const waiting = revealing.current;
    if (waiting !== null) {
      const line = lineOf.get(`a:${waiting}`);
      if (line !== undefined) {
        revealing.current = null;
        moveTo({ line, column: activeCell.column }, false);
      }
    }
  });
  useEffect(() => {
    if (!view) return;
    view.revealWith((id) => reveal.current(id));
    return () => view.revealWith(null);
  }, [view]);

  // Wheels over the Gantt scroll the grid, which the Gantt follows.
  useEffect(() => {
    if (!view) return;
    view.scrollWith((by) => {
      if (scroller.current) scroller.current.scrollTop = Math.max(0, scroller.current.scrollTop + by);
    });
    return () => view.scrollWith(null);
  }, [view]);

  const update = (change: Partial<Arrangement>) => setArrangement((now) => ({ ...now, ...change }));

  // A new activity being typed in place, over its WBS node's Add activity line.
  // Its ID is null while it's the one offered, which follows the rows as they change.
  const [newRow, setNewRow] = useState<(Omit<NewActivity, "code"> & { code: string | null }) | null>(null);
  const [newProblem, setNewProblem] = useState<string | null>(null);
  const newName = useRef<HTMLInputElement>(null);
  // Worked out when the row opens or the rows change, not for each key typed in it.
  const offerFor = newRow && newRow.code === null ? newRow.wbs : null;
  const nextIdOf = editing?.nextId;
  // Counts the rows added, so the next row is offered the ID after them.
  const [addedCount, setAddedCount] = useState(0);
  const offered = useMemo(
    () => (offerFor !== null ? (nextIdOf?.(offerFor) ?? "") : ""),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [offerFor, nextIdOf, rows, addedCount],
  );
  const newCode = newRow ? (newRow.code ?? offered) : "";
  const openNewRow = (wbs: string, name = "") => {
    if (!editing?.add || !editing.enabled) return;
    setEditor(null);
    setWbsRow(null);
    setWbsProblem(null);
    setNewProblem(null);
    // Its band, and those above it, open so its Add activity line shows.
    const parts = wbs.split(".");
    const above = parts.map((_, k) => parts.slice(0, k + 1).join("."));
    setArrangement((now) =>
      above.some((path) => now.collapsed.has(path))
        ? { ...now, collapsed: new Set([...now.collapsed].filter((path) => !above.includes(path))) }
        : now,
    );
    setNewRow({ wbs, code: null, name, duration: "" });
  };
  const submitNewRow = () => {
    if (!newRow || !editing?.add) return;
    const problem = editing.add({ ...newRow, code: newCode });
    if (problem) {
      setNewProblem(problem);
      return;
    }
    // The next row, below it, offers the next ID, with its name to type.
    setAddedCount((n) => n + 1);
    setNewProblem(null);
    setNewRow({ wbs: newRow.wbs, code: null, name: "", duration: "" });
    newName.current?.focus();
  };
  const closeNewRow = () => {
    setNewRow(null);
    setNewProblem(null);
    scroller.current?.focus();
  };

  /** Opens the bands down to WBS `path`, so a line in it shows. */
  const openTo = (path: string) => {
    const parts = path.split(".");
    const above = parts.map((_, k) => parts.slice(0, k + 1).join("."));
    setArrangement((now) =>
      above.some((p) => now.collapsed.has(p)) ? { ...now, collapsed: new Set([...now.collapsed].filter((p) => !above.includes(p))) } : now,
    );
  };
  // A WBS node typed in place: a new one under `path`, or `path` renamed.
  const openWbsRow = (mode: "add" | "rename", path: string, typed: WbsTyped = { code: "", name: "" }) => {
    if (!editing?.enabled || !(mode === "add" ? editing.addWbs : editing.renameWbs)) return;
    setEditor(null);
    setNewRow(null);
    setWbsProblem(null);
    openTo(path);
    setWbsRow({ mode, path, ...typed });
  };
  const submitWbsRow = () => {
    if (!wbsRow || !editing) return;
    const typed = { code: wbsRow.code, name: wbsRow.name };
    const problem = wbsRow.mode === "add" ? (editing.addWbs?.(wbsRow.path, typed) ?? null) : (editing.renameWbs?.(wbsRow.path, typed) ?? null);
    if (problem) {
      setWbsProblem(problem);
      return;
    }
    closeWbsRow();
  };
  const closeWbsRow = () => {
    setWbsRow(null);
    setWbsProblem(null);
    scroller.current?.focus();
  };

  const toggleBand = useCallback(
    (line: number) => {
      if (!arranged) return;
      const band = bandOf(arranged, line);
      if (!band || band.adds !== undefined || band.newWbs !== undefined) return;
      setArrangement((now) => {
        const collapsed = new Set(now.collapsed);
        if (collapsed.has(band.path)) collapsed.delete(band.path);
        else collapsed.add(band.path);
        return { ...now, collapsed };
      });
    },
    [arranged],
  );

  /** Moves the active cell, keeping it on screen; with Shift, the range grows from the anchor. */
  const moveTo = (cell: Cell, extend: boolean) => {
    const next = { line: clamp(cell.line, 0, Math.max(lineCount - 1, 0)), column: clamp(cell.column, 0, columns.length - 1) };
    const spot = { ...next, item: itemAt(next.line) };
    setActive(spot);
    if (!extend) setAnchor(spot);
    const element = scroller.current;
    if (!element) return;
    const y = next.line * rowHeight;
    if (y < element.scrollTop) element.scrollTop = y;
    else if (y + rowHeight > element.scrollTop + element.clientHeight) element.scrollTop = y + rowHeight - element.clientHeight;
    const x = columnWidths.slice(0, next.column).reduce((a, b) => a + b, 0);
    const w = columnWidths[next.column] ?? 0;
    if (x < element.scrollLeft) element.scrollLeft = x;
    else if (x + w > element.scrollLeft + element.clientWidth) element.scrollLeft = x + w - element.clientWidth;
  };

  /** The activity on a line, by index into the rows, or undefined on a band. */
  const activityAt = (line: number): number | undefined => {
    const value = arranged?.lines[line];
    return value === undefined || value < 0 ? undefined : value;
  };

  /** Opens an input over the active cell, holding `initial` or the cell's value. */
  const openEditor = (initial?: string): boolean => {
    if (!editing || !rows) return false;
    const i = activityAt(activeCell.line);
    const column = columns[activeCell.column];
    if (i === undefined || !column) return false;
    if (!inputKind(column.id)) {
      editing.say(`${column.title} can't be changed here.`);
      return true;
    }
    if (!editing.enabled) {
      editing.say("This schedule is read-only here.");
      return true;
    }
    const id = rows.id[i] ?? -1;
    // An activity waiting to be added has no row on the server to edit yet.
    if (id < 0) {
      editing.say(`${rows.code[i] ?? "It"} is added once you apply the edits waiting; drop it and add it again to change it.`);
      return true;
    }
    const typed = editing.values.get(cellKey(id, column.id));
    const text = initial ?? typed ?? inputText(rows, i, column.id);
    setWbsRow(null);
    setWbsProblem(null);
    setEditor({ id, column: column.id, text, opened: initial === undefined ? text : null });
    return true;
  };

  /** Closes the input; with `commit`, sends its value and moves on by `step`. */
  const closeEditor = (commit: boolean, step?: Cell, refocus = true) => {
    const open = editor;
    setEditor(null);
    if (refocus) scroller.current?.focus();
    if (!open) return;
    if (commit && (open.opened === null || open.text !== open.opened)) editing?.commit([{ id: open.id, column: open.column, text: open.text }]);
    if (step) moveTo({ line: activeCell.line + step.line, column: activeCell.column + step.column }, false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // Keys typed in the cell's input are its own.
    if (event.target !== event.currentTarget) return;
    const mod = event.ctrlKey || event.metaKey;
    // F2 on a WBS band renames its node, typed in place.
    const onBand = arranged ? bandOf(arranged, activeCell.line) : undefined;
    if (
      event.key === "F2" &&
      !mod &&
      onBand &&
      onBand.adds === undefined &&
      onBand.newWbs === undefined &&
      !by &&
      !onBand.path.includes("\u0001") &&
      editing?.renameWbs
    ) {
      event.preventDefault();
      if (!editing.enabled) {
        editing.say("This schedule is read-only here.");
        return;
      }
      // As it'll be: a new code waiting shows here, though the band keeps its path.
      openWbsRow("rename", onBand.path, editing.wbsNode?.(onBand.path) ?? { code: onBand.label, name: onBand.name ?? "" });
      return;
    }
    // On an Add activity line, Enter, Space, F2 or typing a name opens a new row.
    const adds = onBand?.adds;
    if (adds !== undefined && editing?.add && !mod && !event.altKey) {
      if (event.key === "Enter" || event.key === " " || event.key === "F2") {
        event.preventDefault();
        openNewRow(adds);
        return;
      }
      if (event.key.length === 1 && !editing.isShortcut(event)) {
        event.preventDefault();
        openNewRow(adds, event.key);
        return;
      }
    }
    if (editing && activityAt(activeCell.line) !== undefined) {
      if (event.key === "F2" || (event.key === "Enter" && !mod && !event.shiftKey && !event.altKey)) {
        event.preventDefault();
        if (event.key === "Enter" && editing.previewed) editing.apply();
        else openEditor();
        return;
      }
      if (event.key.length === 1 && !mod && !event.altKey && !editing.isShortcut(event) && event.key !== " ") {
        if (openEditor(event.key)) {
          event.preventDefault();
          event.stopPropagation();
        }
        return;
      }
    }
    if (event.key === "Escape" && editing && (editing.values.size > 0 || editing.marks.size > 0)) {
      event.preventDefault();
      editing.discard();
      return;
    }
    const page = Math.max(1, Math.floor((scroller.current?.clientHeight ?? rowHeight) / rowHeight) - 1);
    const extend = event.shiftKey;
    const at = activeCell;
    const band = arranged ? bandOf(arranged, at.line) : undefined;
    let next: Cell;
    switch (event.key) {
      case "ArrowUp":
        next = { ...at, line: at.line - 1 };
        break;
      case "ArrowDown":
        next = { ...at, line: at.line + 1 };
        break;
      case "ArrowLeft":
        if (band && !band.collapsed && !extend) {
          toggleBand(at.line);
          event.preventDefault();
          return;
        }
        next = { ...at, column: at.column - 1 };
        break;
      case "ArrowRight":
        if (band?.collapsed && !extend) {
          toggleBand(at.line);
          event.preventDefault();
          return;
        }
        next = { ...at, column: at.column + 1 };
        break;
      case "PageUp":
        next = { ...at, line: at.line - page };
        break;
      case "PageDown":
        next = { ...at, line: at.line + page };
        break;
      case "Home":
        next = mod ? { line: 0, column: 0 } : { ...at, column: 0 };
        break;
      case "End":
        next = mod ? { line: lineCount - 1, column: columns.length - 1 } : { ...at, column: columns.length - 1 };
        break;
      case "Enter":
      case " ":
        // Space would otherwise scroll the body a page under the active cell.
        event.preventDefault();
        if (band) toggleBand(at.line);
        return;
      default:
        return;
    }
    event.preventDefault();
    moveTo(next, extend);
  };

  const cellAt = (clientX: number, clientY: number): Cell | null => {
    const element = scroller.current;
    if (!element) return null;
    const box = element.getBoundingClientRect();
    const y = clientY - box.top + element.scrollTop;
    const x = clientX - box.left + element.scrollLeft;
    const line = Math.floor(y / rowHeight);
    if (line < 0 || line >= lineCount) return null;
    let column = 0;
    let edge = 0;
    for (; column < columnWidths.length; column++) {
      edge += columnWidths[column] ?? 0;
      if (x < edge) break;
    }
    return { line, column: Math.min(column, columns.length - 1) };
  };

  // Header: drag a header to move it, its right edge to resize it.
  const dragging = useRef<ColumnId | null>(null);
  // Set while a resize ends, so the click it makes on the header doesn't sort.
  const resized = useRef(false);
  const startResize = (id: ColumnId, startX: number) => {
    const startWidth = widths[id];
    const onMove = (event: PointerEvent) => {
      setWidths((now) => ({ ...now, [id]: Math.max(40, startWidth + event.clientX - startX) }));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      resized.current = true;
      // The click follows the pointerup in the same input event; clear after it.
      setTimeout(() => (resized.current = false), 0);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  const moveColumn = (from: ColumnId, to: ColumnId) => {
    if (from === to) return;
    setOrder((now) => {
      const without = now.filter((id) => id !== from);
      without.splice(without.indexOf(to) + (now.indexOf(from) < now.indexOf(to) ? 1 : 0), 0, from);
      return without;
    });
  };

  // What the frame can ask: a new row where the selection is, and the activities selected.
  if (actions) {
    actions.current = {
      addHere: () => {
        if (!editing?.add) return;
        if (!editing.enabled) {
          editing.say("This schedule is read-only here.");
          return;
        }
        // The selected activity's WBS node, or the band's or Add line's, or the first node.
        const value = arranged?.lines[activeCell.line] ?? -1;
        const band = value < 0 ? arranged?.bands[-value - 1] : undefined;
        const wbs =
          (value >= 0
            ? rows?.wbs_path[value]
            : (band?.adds ?? band?.newWbs ?? (band && !band.path.includes("\u0001") && !band.path.includes("\u0002") ? band.path : null))) ??
          [...(bandOrder?.keys() ?? [])][0] ??
          rows?.wbs_path[0] ??
          null;
        if (!wbs) return;
        // New rows are typed in WBS bands: grouped by the WBS first.
        if (!arrangement.grouped || by) {
          setArrangement((now) => ({ ...now, grouped: true }));
          setGroupField(null);
        }
        openNewRow(wbs);
      },
      addWbsHere: () => {
        if (!editing?.addWbs) return;
        if (!editing.enabled) {
          editing.say("This schedule is read-only here.");
          return;
        }
        // Under the selected WBS band, or the selected activity's or Add line's node.
        const value = arranged?.lines[activeCell.line] ?? -1;
        const band = value < 0 ? arranged?.bands[-value - 1] : undefined;
        const parent =
          (value >= 0 ? rows?.wbs_path[value] : (band?.adds ?? band?.newWbs ?? (band && !band.path.includes("\u0001") ? band.path : null))) ??
          [...(bandOrder?.keys() ?? [])][0] ??
          null;
        if (!parent) return;
        if (!arrangement.grouped || by) {
          setArrangement((now) => ({ ...now, grouped: true }));
          setGroupField(null);
        }
        openWbsRow("add", parent);
      },
      selected: () => {
        const ids: number[] = [];
        if (!arranged || !rows) return ids;
        for (let line = Math.min(anchorCell.line, activeCell.line); line <= Math.max(anchorCell.line, activeCell.line); line++) {
          const value = arranged.lines[line] ?? -1;
          if (value >= 0) ids.push(rows.id[value] ?? 0);
        }
        return ids;
      },
    };
  }

  if (!rows || !arranged) {
    return <p className="muted grid-empty">Loading activities…</p>;
  }

  const first = Math.max(0, Math.floor(mirrorTop / rowHeight));
  const shown = Math.ceil(size.height / rowHeight) + 1;
  // The rows on screen, and the active and anchor rows wherever they are, so
  // the active cell always has an element for assistive technology to read.
  const mirrored = Array.from({ length: Math.max(0, Math.min(shown, lineCount - first)) }, (_, k) => first + k);
  for (const line of [activeCell.line, anchorCell.line]) {
    if (lineCount > 0 && !mirrored.includes(line)) mirrored.push(line);
  }
  mirrored.sort((a, b) => a - b);
  const activeId = `grid-cell-${activeCell.line}-${bandOf(arranged, activeCell.line) ? 0 : activeCell.column}`;

  /** Where a cell is in the body's content, in CSS pixels. */
  const boxOf = (cell: Cell) => ({
    left: columnWidths.slice(0, cell.column).reduce((a, b) => a + b, 0),
    top: cell.line * rowHeight,
    width: columnWidths[cell.column] ?? 0,
    height: rowHeight,
  });
  // A WBS node typed in place: over the line for a new one, or its own band.
  const wbsRowAt = (() => {
    if (!wbsRow || !arranged) return undefined;
    const at = arranged.bands.findIndex((band) =>
      wbsRow.mode === "add" ? band.newWbs === wbsRow.path : band.path === wbsRow.path && band.adds === undefined && band.newWbs === undefined,
    );
    const line = at < 0 ? -1 : arranged.lines.indexOf(-(at + 1));
    return line < 0 ? undefined : { line, depth: arranged.bands[at]?.depth ?? 0 };
  })();
  // A new row sits over its WBS node's Add activity line.
  const newRowLine = (() => {
    if (!newRow || !arranged) return undefined;
    const at = arranged.bands.findIndex((band) => band.adds === newRow.wbs);
    const line = at < 0 ? -1 : arranged.lines.indexOf(-(at + 1));
    return line < 0 ? undefined : line;
  })();
  // The input sits over its cell while the activity and column still show.
  const editorLine = editor ? lineOf.get(`a:${editor.id}`) : undefined;
  const editorColumn = editor ? order.indexOf(editor.column) : -1;
  const editorBox = editorLine !== undefined && editorColumn >= 0 ? boxOf({ line: editorLine, column: editorColumn }) : null;
  // Why a cell was refused: under the hovered cell, else the active one, else
  // the first refused cell on screen, so a refusal shows where it is at once.
  const tip = (() => {
    if (!editing || editing.marks.size === 0 || editor) return null;
    // From the rows on screen now, not the mirror's, which waits for scrolling to settle.
    const top = Math.max(0, Math.floor(scroll.top / rowHeight));
    const firstMarked = (): Cell | null => {
      for (let line = top; line < Math.min(lineCount, top + shown); line++) {
        const i = activityAt(line);
        if (i === undefined) continue;
        const column = columns.findIndex((c) => editing.marks.has(cellKey(rows.id[i] ?? -1, c.id)));
        if (column >= 0) return { line, column };
      }
      return null;
    };
    for (const cell of [hover, activeCell, firstMarked()]) {
      if (!cell) continue;
      const i = activityAt(cell.line);
      const column = columns[cell.column];
      if (i === undefined || !column) continue;
      const problem = editing.marks.get(cellKey(rows.id[i] ?? -1, column.id));
      if (!problem) continue;
      const box = boxOf(cell);
      return { problem, style: { left: box.left, top: box.top + box.height + 2 } };
    }
    return null;
  })();

  /**
   * Applies built conditions: the server's `query` finds their activities. A
   * refused filter is said, and the one applied stays; an answer to an apply
   * since cleared or replaced is dropped.
   */
  const applyBuilt = async (conditions: Condition[]): Promise<string | null> => {
    if (conditions.length === 0 || !finder) {
      dropFilter();
      return null;
    }
    const generation = ++filtering.current.generation;
    wantedBuilt.current = conditions;
    setPendingBuilt(null);
    const asked = latestRows.current;
    let ids: Set<number>;
    try {
      ids = idsOf(await finder.query({ ...paramsOf(conditions), columns: "id" }));
    } catch (error) {
      if (filtering.current.generation !== generation) return null;
      wantedBuilt.current = null;
      return said(error);
    }
    if (filtering.current.generation !== generation) return null;
    filtering.current = { generation, conditions };
    // Answered: what's shown is what was asked for.
    wantedBuilt.current = null;
    setFilter({ conditions, ids });
    setFilterProblem(null);
    // Rows that came while it waited may hold more, or fewer.
    if (latestRows.current !== asked) setRefind((n) => n + 1);
    return null;
  };

  const toggleQuick = (id: QuickFilter) =>
    // From the arrangement as it is when this runs, not as rendered, so quick
    // clicks don't undo each other.
    setArrangement((now) => {
      const quick = new Set(now.quick);
      if (quick.has(id)) quick.delete(id);
      else quick.add(id);
      return { ...now, quick };
    });

  // Each filter applied, as a chip that removes it: Critical, the statuses
  // together, each condition that must hold, and each "any of" group.
  const chips: Chip[] = [];
  if (arrangement.quick.has("critical")) chips.push({ key: "critical", label: "Critical", swatch: true, remove: () => toggleQuick("critical") });
  const statuses = QUICK.filter((q) => q.id !== "critical" && arrangement.quick.has(q.id));
  if (statuses.length > 0) {
    chips.push({
      key: "statuses",
      label: statuses.map((q, k) => (k === 0 ? q.label : q.label.toLowerCase())).join(" or "),
      remove: () =>
        setArrangement((now) => ({ ...now, quick: new Set([...now.quick].filter((q) => q === "critical")) })),
    });
  }
  // A built chip removed applies the rest of what was last asked for, closes
  // the builder, which holds the conditions it opened with, and says if the
  // server refuses.
  const removeBuilt = (keep: (condition: Condition) => boolean) => {
    setBuilding(false);
    void applyBuilt((wantedBuilt.current ?? built).filter(keep)).then((problem) => problem && setFilterProblem(problem));
  };
  const reads = (condition: Condition) => CHOICE_BY_ID.get(condition.field)?.reads(condition.value.trim()) ?? condition.field;
  built.forEach((condition, k) => {
    if (condition.group !== undefined || !complete(condition)) return;
    const words = reads(condition);
    chips.push({
      key: `built-${k}`,
      label: words.charAt(0).toUpperCase() + words.slice(1),
      remove: () => removeBuilt((c) => !(c.field === condition.field && c.value === condition.value && c.group === undefined)),
    });
  });
  groupsIn(built).forEach((group, k) => {
    const words = built.filter((c) => c.group === group && complete(c)).map(reads);
    if (words.length === 0) return;
    chips.push({
      key: `any-${group}`,
      label: `Group ${k + 1}: ${words.join(" and ")}`,
      title: "An activity meets every condition of at least one group",
      remove: () => removeBuilt((c) => c.group !== group),
    });
  });
  // Activities in the selection, for the status bar.
  let selected = 0;
  for (let line = Math.min(anchorCell.line, activeCell.line); line <= Math.max(anchorCell.line, activeCell.line); line++) {
    if ((arranged.lines[line] ?? -1) >= 0) selected++;
  }
  const counts = (
    <>
      <span className="status-count">
        <span className="muted grid-count" aria-live="polite">
          {arranged.matched.toLocaleString()} of {rows.count.toLocaleString()}
        </span>{" "}
        {rows.count === 1 ? "activity" : "activities"}
      </span>
      {selected > 1 && <span className="status-selected">{selected.toLocaleString()} selected</span>}
    </>
  );

  const chipList = chips.map((chip) => (
    <span key={chip.key} className="chip" role="listitem" title={chip.title ?? chip.label}>
      {chip.swatch && <span className="chip-swatch" aria-hidden="true" />}
      <span className="chip-label">{chip.label}</span>
      <button type="button" className="chip-remove" aria-label={`Remove the filter ${chip.label}`} onClick={chip.remove}>
        <span aria-hidden="true">×</span>
      </button>
    </span>
  ));

  const groupWords = !arrangement.grouped
    ? "none"
    : [groupBy ?? "wbs", ...thenBy].map((field) => groupTitle(field, finder)).join(", then ");

  /** The filters and grouping: above the grid, or in the frame's toolbar row when it gives one. */
  const inToolbar = (node: ReactNode) => (toolbar ? createPortal(node, toolbar) : node);

  return (
    <div className={toolbar ? "grid grid-bare" : "grid"}>
      {status && createPortal(counts, status)}
      {inToolbar(
      <div className="grid-top">
        <div className="grid-tools" role="toolbar" aria-label="Filters">
          <input
            ref={filterRef}
            className="grid-filter"
            type="search"
            placeholder="Filter by ID or name ( / )"
            aria-label="Filter activities by ID or name"
            value={arrangement.text}
            onChange={(event) => update({ text: event.target.value })}
            onKeyDown={(event) => {
              if (event.key === "Escape" || event.key === "Enter") scroller.current?.focus();
            }}
          />
          <span className="chips" role="list" aria-label="Filters applied">
            {chipList}
          </span>
          {chips.length > 0 && (
            <span className="chips-more">
              <ToolPopover
                label={`${chips.length} ${chips.length === 1 ? "filter" : "filters"}`}
                name={`Filters applied: ${chips.length}`}
                role="dialog"
                open={listing}
                onOpen={setListing}
              >
                <span className="chips-listed" role="list" aria-label="Filters applied">
                  {chipList}
                </span>
              </ToolPopover>
            </span>
          )}
          {path && (
            <button type="button" className="quick" aria-pressed={onPath} onClick={() => setOnPath((on) => !on)}>
              Driving path
            </button>
          )}
          <ToolPopover
            label={
              <>
                <Icon name="plus" size={13} />
                Add filter
              </>
            }
            name="Add filter"
            role="menu"
            open={adding}
            onOpen={setAdding}
          >
            {QUICK.map(({ id, label }) => (
              <button
                key={id}
                type="button"
                role="menuitemcheckbox"
                className="tool-item"
                aria-checked={arrangement.quick.has(id)}
                onClick={() => toggleQuick(id)}
              >
                {label}
              </button>
            ))}
            {finder && (
              <button
                type="button"
                role="menuitem"
                className="tool-item tool-more"
                onClick={() => {
                  setAdding(false);
                  setBuilding(true);
                }}
              >
                More conditions…
              </button>
            )}
          </ToolPopover>
          <span className="tool-sep" aria-hidden="true" />
          <ToolPopover
            label={
              <>
                <Icon name="group" size={14} />
                <span className="tool-words">Group by {groupWords}</span>
              </>
            }
            name={`Grouping: ${groupWords}`}
            title={`Group by ${groupWords}`}
            role="dialog"
            open={grouping}
            onOpen={setGrouping}
          >
            <label className="tool-field">
              Group by
              <select
                aria-label="Group by"
                value={!arrangement.grouped ? "none" : (groupBy ?? "wbs")}
                onChange={(event) => {
                  const value = event.target.value;
                  clearGroupProblems();
                  if (value === "none") {
                    update({ grouped: false });
                    setThenFields([]);
                  } else {
                    update({ grouped: true });
                    setGroupField(value === "wbs" ? null : value);
                    setThenFields((now) => now.filter((field) => field !== value));
                  }
                }}
              >
                <option value="none">None</option>
                <GroupOptions finder={finder} />
              </select>
            </label>
            {arrangement.grouped &&
              [...thenBy, ...(thenBy.length + 1 < MAX_LEVELS ? [""] : [])].map((field, k) => {
                // Every other level is taken.
                const before = new Set([groupBy ?? "wbs", ...thenBy.filter((_, j) => j !== k)]);
                return (
                  <label key={`${k}:${field}`} className="tool-field">
                    then
                    <select
                      aria-label={field === "" ? "Then group by" : `Then by, level ${k + 2}`}
                      value={field}
                      onChange={(event) => {
                        const value = event.target.value;
                        clearGroupProblems();
                        setThenFields(() => {
                          const next = [...thenBy];
                          if (value === "") next.splice(k, 1);
                          else next.splice(k, field === "" ? 0 : 1, value);
                          return next;
                        });
                      }}
                    >
                      <option value="">{field === "" ? "…" : "No further level"}</option>
                      <GroupOptions finder={finder} without={before} />
                    </select>
                  </label>
                );
              })}
          </ToolPopover>
          {!status && counts}
        </div>
        {[
          filterProblem,
          ...(groupGone === null ? [] : [`This schedule has no ${groupGone}, so rows are grouped by the WBS.`]),
          ...[...groupProblems].filter(([field]) => levelFields.includes(field)).map(([, problem]) => problem),
          ...thenGone.map((field) => `This schedule has no ${field}, so rows aren't grouped by it.`),
        ].map(
          (problem) =>
            problem && (
              <p key={problem} className="grid-find-problem" role="alert">
                {problem}
              </p>
            ),
        )}
        {building && finder && (
          <FilterBuilder
            applied={built}
            codes={finder.codes}
            saved={finder.saved}
            onApply={applyBuilt}
            onClear={dropFilter}
            onClose={() => {
              setBuilding(false);
              scroller.current?.focus();
            }}
          />
        )}
      </div>
      )}
      <div className="grid-header" role="toolbar" aria-label="Columns">
        <div className="grid-header-row" style={{ transform: `translateX(${-scroll.left}px)`, width: totalWidth }}>
          {columns.map((column, c) => {
            const sorting = arrangement.sort.findIndex((key) => key.column === column.id);
            const key = arrangement.sort[sorting];
            return (
              <div
                key={column.id}
                className={`grid-head ${column.align}`}
                style={{ width: columnWidths[c] }}
                draggable
                onDragStart={() => (dragging.current = column.id)}
                onDragOver={(event) => event.preventDefault()}
                onDrop={() => {
                  if (dragging.current) moveColumn(dragging.current, column.id);
                  dragging.current = null;
                }}
              >
                <button
                  type="button"
                  className="grid-head-button"
                  aria-label={`${column.title}${key ? `, sorted ${key.descending ? "descending" : "ascending"}${arrangement.sort.length > 1 ? `, key ${sorting + 1}` : ""}` : ""}`}
                  aria-keyshortcuts="Enter Shift+Enter Alt+ArrowLeft Alt+ArrowRight Alt+Shift+ArrowLeft Alt+Shift+ArrowRight"
                  title={`${column.title}. Sort: click or Enter; Shift for a second key. Drag, or Alt+Shift+arrows, to move; Alt+arrows to resize.`}
                  onClick={(event) => {
                    const shift = event.shiftKey;
                    if (!resized.current) {
                      setArrangement((now) => ({ ...now, sort: nextSort(now.sort, column.id, shift) }));
                    }
                  }}
                  onKeyDown={(event) => {
                    if (!event.altKey || (event.key !== "ArrowLeft" && event.key !== "ArrowRight")) return;
                    event.preventDefault();
                    const step = event.key === "ArrowLeft" ? -1 : 1;
                    if (event.shiftKey) {
                      const target = order[c + step];
                      if (target) moveColumn(column.id, target);
                    } else {
                      setWidths((now) => ({ ...now, [column.id]: Math.max(40, now[column.id] + step * 16) }));
                    }
                  }}
                >
                  <span className="grid-head-title">{column.short ?? column.title}</span>
                  {key && (
                    <span className="sort-mark" aria-hidden="true">
                      {key.descending ? "▼" : "▲"}
                      {arrangement.sort.length > 1 ? sorting + 1 : ""}
                    </span>
                  )}
                </button>
                <span
                  className="grid-resize"
                  aria-hidden="true"
                  onPointerDown={(event) => {
                    event.stopPropagation();
                    event.preventDefault();
                    startResize(column.id, event.clientX);
                  }}
                  onClick={(event) => event.stopPropagation()}
                />
              </div>
            );
          })}
        </div>
      </div>
      <div
        ref={attachBody}
        className="grid-body"
        tabIndex={0}
        role="application"
        aria-roledescription="activity grid"
        aria-label="Activities"
        aria-activedescendant={lineCount > 0 ? activeId : undefined}
        onScroll={(event) => setScroll({ top: event.currentTarget.scrollTop, left: event.currentTarget.scrollLeft })}
        onKeyDown={onKeyDown}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onMouseDown={(event) => {
          // A press in the cell's input is the input's.
          if (event.target instanceof HTMLInputElement) return;
          // A press elsewhere in the grid commits the cell being edited.
          if (editor) closeEditor(true, undefined, false);
          // A press on the body's own scrollbars isn't on a cell.
          const box = event.currentTarget.getBoundingClientRect();
          if (
            event.clientX - box.left >= event.currentTarget.clientWidth ||
            event.clientY - box.top >= event.currentTarget.clientHeight
          ) {
            return;
          }
          const cell = cellAt(event.clientX, event.clientY);
          if (!cell) return;
          const clicked = bandOf(arranged, cell.line);
          if (clicked?.adds !== undefined && !event.shiftKey) {
            moveTo(cell, false);
            openNewRow(clicked.adds);
            return;
          }
          if (clicked?.newWbs !== undefined) return;
          if (clicked && !event.shiftKey) toggleBand(cell.line);
          moveTo(cell, event.shiftKey);
        }}
        onMouseMove={(event) => {
          if (!editing || editing.marks.size === 0) return;
          const cell = cellAt(event.clientX, event.clientY);
          setHover((now) => (now?.line === cell?.line && now?.column === cell?.column ? now : cell));
        }}
        onMouseLeave={() => setHover(null)}
      >
        <canvas ref={canvas} className="grid-canvas" />
        <table id="grid-mirror" className="visually-hidden" role="grid" aria-multiselectable="true" aria-rowcount={lineCount + 1} aria-colcount={columns.length} aria-label="Activities">
          <thead>
            <tr role="row" aria-rowindex={1}>
              {columns.map((column) => {
                const key = arrangement.sort.find((k) => k.column === column.id);
                return (
                  <th
                    key={column.id}
                    role="columnheader"
                    scope="col"
                    aria-sort={key ? (key.descending ? "descending" : "ascending") : undefined}
                  >
                    {column.title}
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {mirrored.map((line) => {
              const value = arranged.lines[line] ?? 0;
              const band = value < 0 ? arranged.bands[-value - 1] : undefined;
              return (
                <tr
                  key={line}
                  role="row"
                  aria-rowindex={line + 2}
                  aria-expanded={band && band.adds === undefined && band.newWbs === undefined ? !band.collapsed : undefined}
                >
                  {band ? (
                    <td role="gridcell" colSpan={columns.length} id={`grid-cell-${line}-0`} aria-selected={line === activeCell.line}>
                      {band.newWbs !== undefined
                        ? `${band.spoken}: type its code and name`
                        : band.adds !== undefined
                        ? `${band.spoken}: Enter adds one`
                        : `${band.spoken}, ${band.count} ${band.count === 1 ? "activity" : "activities"}${band.collapsed ? ", collapsed" : ""}`}
                    </td>
                  ) : (
                    columns.map((column, c) => (
                      <td
                        key={column.id}
                        role="gridcell"
                        id={`grid-cell-${line}-${c}`}
                        aria-invalid={editing?.marks.has(cellKey(rows.id[value] ?? -1, column.id)) || undefined}
                        title={problemText(editing?.marks.get(cellKey(rows.id[value] ?? -1, column.id)))}
                        aria-selected={
                          line >= Math.min(anchorCell.line, activeCell.line) &&
                          line <= Math.max(anchorCell.line, activeCell.line) &&
                          c >= Math.min(anchorCell.column, activeCell.column) &&
                          c <= Math.max(anchorCell.column, activeCell.column)
                        }
                      >
                        {editing?.values.get(cellKey(rows.id[value] ?? -1, column.id)) ?? column.text(rows, value)}
                      </td>
                    ))
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>

        {editorBox && editor && (
          <input
            className={`cell-input ${COLUMN_BY_ID.get(editor.column)?.align ?? "left"}`}
            style={editorBox}
            aria-label={`${COLUMN_BY_ID.get(editor.column)?.title ?? editor.column} of ${rows.code[indexOf(rows, editor.id) ?? -1] ?? "the activity"}`}
            value={editor.text}
            list={inputKind(editor.column) === "calendar" ? "grid-calendars" : inputKind(editor.column) === "constraint" ? "grid-constraints" : undefined}
            autoFocus
            spellCheck={inputKind(editor.column) === "text"}
            onChange={(event) => setEditor({ ...editor, text: event.target.value })}
            onBlur={(event) => {
              // Leaving for somewhere else on the page commits, as a spreadsheet does.
              if (editor && event.relatedTarget !== scroller.current) closeEditor(true, undefined, false);
            }}
            onKeyDown={(event) => {
              // Enter or Escape that ends an input method's composition is its own.
              if (event.nativeEvent.isComposing || event.keyCode === 229) return;
              if (event.key === "Escape") {
                event.preventDefault();
                closeEditor(false);
              } else if (event.key === "Enter") {
                event.preventDefault();
                closeEditor(true, { line: event.shiftKey ? -1 : 1, column: 0 });
              } else if (event.key === "Tab") {
                event.preventDefault();
                closeEditor(true, { line: 0, column: event.shiftKey ? -1 : 1 });
              }
            }}
          />
        )}
        {newRow && newRowLine !== undefined && (
          <div
            className="new-row"
            role="group"
            aria-label="New activity"
            style={{ top: newRowLine * rowHeight, width: totalWidth, height: rowHeight }}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing || event.keyCode === 229) return;
              if (event.key === "Enter") {
                event.preventDefault();
                submitNewRow();
              } else if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                closeNewRow();
              }
            }}
          >
            {(["code", "name", "original"] as const).map((column) => {
              const c = order.indexOf(column);
              const field = column === "original" ? "duration" : column;
              const box =
                c >= 0
                  ? { left: columnWidths.slice(0, c).reduce((a, b) => a + b, 0), width: columnWidths[c] ?? 0 }
                  : column === "name"
                    ? { left: 0, width: 260 }
                    : null;
              if (!box) return null;
              return (
                <input
                  key={column}
                  className={`new-row-input${column === "original" ? " right" : ""}`}
                  style={{ ...box, height: rowHeight }}
                  aria-label={column === "code" ? "New activity's ID" : column === "name" ? "New activity's name" : "New activity's duration"}
                  placeholder={column === "name" ? "Name" : ""}
                  title={column === "original" ? "A duration of 0 adds a milestone" : undefined}
                  value={field === "code" ? newCode : newRow[field]}
                  ref={column === "name" ? newName : undefined}
                  autoFocus={column === "name"}
                  spellCheck={column === "name"}
                  onChange={(event) => setNewRow({ ...newRow, [field]: event.target.value })}
                />
              );
            })}
            {newProblem && (
              <span className="cell-tip new-row-problem" role="alert">
                {newProblem}
              </span>
            )}
          </div>
        )}
        {wbsRow && wbsRowAt && (
          <div
            className="new-row wbs-row"
            role="group"
            aria-label={wbsRow.mode === "add" ? "New WBS node" : "Rename the WBS node"}
            style={{ top: wbsRowAt.line * rowHeight, width: totalWidth, height: rowHeight }}
            onKeyDown={(event) => {
              if (event.nativeEvent.isComposing || event.keyCode === 229) return;
              if (event.key === "Enter") {
                event.preventDefault();
                submitWbsRow();
              } else if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                closeWbsRow();
              }
            }}
          >
            <input
              className="new-row-input"
              style={{ left: 6 + wbsRowAt.depth * INDENT, width: 110, height: rowHeight }}
              aria-label={wbsRow.mode === "add" ? "New WBS node's code" : "WBS node's code"}
              placeholder="Code"
              value={wbsRow.code}
              autoFocus={wbsRow.mode === "add"}
              spellCheck={false}
              onChange={(event) => setWbsRow({ ...wbsRow, code: event.target.value })}
            />
            <input
              className="new-row-input"
              style={{ left: 6 + wbsRowAt.depth * INDENT + 114, width: 300, height: rowHeight }}
              aria-label={wbsRow.mode === "add" ? "New WBS node's name" : "WBS node's name"}
              placeholder="Name"
              value={wbsRow.name}
              autoFocus={wbsRow.mode === "rename"}
              onChange={(event) => setWbsRow({ ...wbsRow, name: event.target.value })}
            />
            {wbsProblem && (
              <span className="cell-tip new-row-problem" role="alert">
                {wbsProblem}
              </span>
            )}
          </div>
        )}
        {tip && (
          <div className="cell-tip" role="tooltip" style={tip.style}>
            <strong>{tip.problem.message}</strong>
            {tip.problem.hint && <span>{tip.problem.hint}</span>}
          </div>
        )}
        <datalist id="grid-calendars">
          {calendars.map((name) => (
            <option key={name} value={name} />
          ))}
        </datalist>
        <datalist id="grid-constraints">
          <option value="none" />
          {CONSTRAINT_KINDS.map((kind) => (
            <option key={kind} value={kind} />
          ))}
        </datalist>
        <div style={{ height: lineCount * rowHeight, width: totalWidth }} />
      </div>
    </div>
  );
}

/**
 * The `--row-height` token, in pixels, read once: the stylesheet loads before
 * the app's script runs.
 */
function rowHeightToken(): number {
  const value = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--row-height"));
  return value > 0 ? value : 22;
}

function problemText(problem: CellProblem | undefined): string | undefined {
  return problem ? `${problem.message}${problem.hint ? `. ${problem.hint}` : ""}` : undefined;
}
