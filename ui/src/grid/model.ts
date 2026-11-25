// The row model the grid draws, and the Gantt will share: which activities
// show, in what order, under which WBS bands. Built for 50,000 activities:
// work is on index arrays, text is lower-cased once, and sort keys are read
// once per sort.

import type { ProjectWbs, Rows, WbsNode } from "../api";
import { COLUMN_BY_ID, COMPARE_COLUMNS, type ColumnId } from "./columns";

export type QuickFilter = "critical" | "not-started" | "in-progress" | "completed";

export interface SortKey {
  column: ColumnId;
  descending: boolean;
}

export interface Arrangement {
  /** Matches the activity ID or name, in any case. */
  text: string;
  /**
   * Critical narrows to critical activities; the statuses widen each other,
   * so Not started with In progress shows open work.
   */
  quick: ReadonlySet<QuickFilter>;
  /** First key first; Shift-click adds another. */
  sort: readonly SortKey[];
  /** Whether bands group the rows: WBS bands, or `by`'s where it's given. */
  grouped: boolean;
  /** Band paths that are collapsed. */
  collapsed: ReadonlySet<string>;
  /** Only these activities, by `task_id`, as for a driving path; null for all. */
  only?: ReadonlySet<number> | null;
  /** Only these too, by `task_id`: what a built filter matched; null for all. */
  matching?: ReadonlySet<number> | null;
  /** Group by one field's values instead of the WBS; null for the WBS. */
  by?: Grouping | null;
  /**
   * Further levels, each splitting every band of the level before it: a
   * field's values, or null for the WBS.
   */
  then?: readonly (Grouping | null)[];
  /**
   * Building the schedule: the WBS's paths. Grouped by the WBS first, each
   * gets a band, empty or not while nothing filters the rows, and an Add
   * activity line at its end.
   */
  building?: ReadonlySet<string> | null;
  /** Building: the WBS path a new node is being typed under, which gets a line after its child nodes. */
  addingWbs?: string | null;
}

/** A field to group by, such as an activity code type, and each activity's value of it. */
export interface Grouping {
  /** The field as `query` names a column: `code:TYPE` or `udf:LABEL`. */
  field: string;
  /** What the bands say the field is, such as the code type's name. */
  title: string;
  /** Each activity's value, by `task_id`; an activity not here has none. */
  values: ReadonlyMap<number, string | number>;
  /** Values' places, such as a code type's own sequence; others follow in natural order. */
  order?: ReadonlyMap<string, number>;
}

/** What a band for activities with no value of the field says. */
export const NO_VALUE = "(no value)";

export const DEFAULT_ARRANGEMENT: Arrangement = {
  text: "",
  quick: new Set(),
  sort: [],
  grouped: true,
  collapsed: new Set(),
};

/** A WBS band. */
export interface Band {
  /**
   * The full dotted path, or for a field's band, the field and value: what
   * collapses it. A band within another's starts with the outer band's path
   * and `\u0001`.
   */
  path: string;
  /** This level's code, or the field's value. */
  label: string;
  /** What the band shows: the WBS path, or `TYPE: VALUE`. */
  title: string;
  /** A WBS node's name, shown before its path, where the WBS gives one. */
  name?: string;
  /** What a screen reader hears before the count: `WBS`, the name and the path, or the title. */
  spoken: string;
  /** 0 for a top node. */
  depth: number;
  /** Set on an Add activity line: the WBS path it adds to. */
  adds?: string;
  /** Set on the line a new WBS node is typed on: the path of the node it goes under. */
  newWbs?: string;
  /** Matching activities under it, at any depth. */
  count: number;
  collapsed: boolean;
  /**
   * The earliest start and latest finish of those activities, actual where
   * there is one, else early: the server's dates, compared as text, which
   * orders `YYYY-MM-DD HH:MM` correctly. Null when none has one.
   */
  start: string | null;
  finish: string | null;
}

/**
 * What shows, top to bottom: an activity's index (0 or more) or a band,
 * encoded as `-(band index + 1)`.
 */
export interface Arranged {
  lines: Int32Array;
  bands: Band[];
  /** Activities that pass the filters. */
  matched: number;
}

/** The rows, with what filtering needs worked out once. */
export interface Model {
  rows: Rows;
  count: number;
  /** `code name`, lower-cased, per activity. */
  search: string[];
  /** Each activity's WBS path, as an index into `paths`. */
  pathOf: Int32Array;
  /** The distinct WBS paths. */
  paths: string[];
  /** Each activity's start and finish as the Gantt shows them: actual, else early. */
  starts: (string | null)[];
  finishes: (string | null)[];
}

export function buildModel(rows: Rows): Model {
  const search = new Array<string>(rows.count);
  const pathOf = new Int32Array(rows.count);
  const paths: string[] = [];
  const known = new Map<string, number>();
  const starts = new Array<string | null>(rows.count);
  const finishes = new Array<string | null>(rows.count);
  for (let i = 0; i < rows.count; i++) {
    search[i] = `${rows.code[i] ?? ""} ${rows.name[i] ?? ""}`.toLowerCase();
    const path = rows.wbs_path[i] ?? "";
    let at = known.get(path);
    if (at === undefined) {
      at = paths.length;
      paths.push(path);
      known.set(path, at);
    }
    pathOf[i] = at;
    starts[i] = rows.actual_start[i] ?? rows.early_start[i] ?? null;
    finishes[i] = rows.actual_finish[i] ?? rows.early_finish[i] ?? null;
  }
  return { rows, count: rows.count, search, pathOf, paths, starts, finishes };
}

/** Whether anything narrows the rows: text, quick filters, a path or a built filter. */
function filtering(arrangement: Arrangement): boolean {
  return arrangement.text.trim() !== "" || arrangement.quick.size > 0 || !!arrangement.only || !!arrangement.matching;
}

/** The activities that pass the text and quick filters, in file order. */
export function filtered(
  model: Model,
  arrangement: Pick<Arrangement, "text" | "quick" | "only" | "matching">,
): Int32Array {
  const only = arrangement.only ?? null;
  const matching = arrangement.matching ?? null;
  const { rows } = model;
  const words = arrangement.text.toLowerCase().split(/\s+/).filter(Boolean);
  const quick = arrangement.quick;
  const critical = quick.has("critical");
  const statuses = (["not-started", "in-progress", "completed"] as const).filter((s) => quick.has(s));
  const out = new Int32Array(model.count);
  let n = 0;
  for (let i = 0; i < model.count; i++) {
    if (only && !only.has(rows.id[i] ?? -1)) continue;
    if (matching && !matching.has(rows.id[i] ?? -1)) continue;
    if (critical && !rows.critical[i]) continue;
    if (statuses.length > 0 && !(statuses as readonly string[]).includes(rows.status[i] ?? "")) continue;
    if (words.length > 0) {
      const text = model.search[i] ?? "";
      let all = true;
      for (const word of words) {
        if (!text.includes(word)) {
          all = false;
          break;
        }
      }
      if (!all) continue;
    }
    out[n++] = i;
  }
  return out.slice(0, n);
}

function isDigit(code: number): boolean {
  return code >= 48 && code <= 57;
}

/** The longest digit run read as a number; longer ones compare as digits. */
const EXACT_DIGITS = 15;

/** A digit run without its leading zeros (`"0"` for all zeros). */
function significant(text: string, start: number, end: number): string {
  let from = start;
  while (from < end - 1 && text.charCodeAt(from) === 48) from++;
  return text.slice(from, end);
}

/** Two digit runs by value, exactly at any length: by length, then digit by digit. */
function compareDigits(a: string, b: string): number {
  if (a.length !== b.length) return a.length - b.length;
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * Natural order: run by run, a value that has ended first, then a number by
 * value (`A9` before `A10`, exactly at any length), then text ignoring case;
 * ties fall to the text as written. Used for the WBS bands; the grid sorts
 * by the same order with [`ranks`], which is built for 50,000 values.
 */
export function natural(a: string, b: string): number {
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i >= a.length) return -1;
    if (j >= b.length) return 1;
    const da = isDigit(a.charCodeAt(i));
    const db = isDigit(b.charCodeAt(j));
    if (da !== db) return da ? -1 : 1;
    let ei = i;
    while (ei < a.length && isDigit(a.charCodeAt(ei)) === da) ei++;
    let ej = j;
    while (ej < b.length && isDigit(b.charCodeAt(ej)) === db) ej++;
    const c = da
      ? compareDigits(significant(a, i, ei), significant(b, j, ej))
      : (() => {
          const x = a.slice(i, ei).toLowerCase();
          const y = b.slice(j, ej).toLowerCase();
          return x < y ? -1 : x > y ? 1 : 0;
        })();
    if (c !== 0) return c;
    i = ei;
    j = ej;
  }
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * `group` (activity indices) in natural order of their values, appended to
 * `out`. A bucket sort, run by run: at each run, values that have ended, then
 * numbers sorted natively as numbers, then each distinct text sorted
 * natively, each bucket ordered further on the next run. `at` holds where
 * each value's next run starts. Codes such as `A1234` cost one numeric sort,
 * with no comparison in JavaScript.
 */
function naturalOrder(group: number[], written: string[], at: Int32Array, out: number[]): void {
  if (group.length === 1) {
    out.push(group[0] ?? 0);
    return;
  }
  const ended: number[] = [];
  const numbered: number[] = [];
  const values: number[] = [];
  // Runs too long to be exact as numbers, by their significant digits; all
  // are larger than any shorter run.
  const long = new Map<string, number[]>();
  const texts = new Map<string, number[]>();
  // The last text run read, so a run like the last (the `A` of every `A123`)
  // reuses its key instead of making a new string.
  let lastKey: string | null = null;
  let lastText = "";
  let lastStart = 0;
  let lastLength = 0;
  for (const i of group) {
    const text = written[i] ?? "";
    const start = at[i] ?? 0;
    if (start >= text.length) {
      ended.push(i);
      continue;
    }
    let end = start;
    if (isDigit(text.charCodeAt(start))) {
      while (end < text.length && isDigit(text.charCodeAt(end))) end++;
      const digits = significant(text, start, end);
      if (digits.length <= EXACT_DIGITS) {
        numbered.push(i);
        values.push(Number(digits));
      } else {
        const bucket = long.get(digits);
        if (bucket) bucket.push(i);
        else long.set(digits, [i]);
      }
    } else {
      while (end < text.length && !isDigit(text.charCodeAt(end))) end++;
      let run: string;
      if (lastKey !== null && end - start === lastLength && sameRun(text, start, lastText, lastStart, lastLength)) {
        run = lastKey;
      } else {
        run = text.slice(start, end).toLowerCase();
        lastKey = run;
        lastText = text;
        lastStart = start;
        lastLength = end - start;
      }
      const bucket = texts.get(run);
      if (bucket) bucket.push(i);
      else texts.set(run, [i]);
    }
    at[i] = end;
  }
  // Ended values are equal so far: order them as written, then by index.
  ended.sort((x, y) => {
    const a = written[x] ?? "";
    const b = written[y] ?? "";
    return a < b ? -1 : a > b ? 1 : x - y;
  });
  out.push(...ended);
  const m = numbered.length;
  if (m > 0) {
    const fits = values.every((v) => Number.isSafeInteger(v) && v * m + m < Number.MAX_SAFE_INTEGER);
    const order: ArrayLike<number> = fits
      ? Float64Array.from(values, (v, k) => v * m + k)
          .sort()
          .map((f) => f % m)
      : values.map((_, k) => k).sort((a, b) => (values[a] ?? 0) - (values[b] ?? 0) || a - b);
    let start = 0;
    while (start < m) {
      const value = values[order[start] ?? 0];
      let end = start + 1;
      while (end < m && values[order[end] ?? 0] === value) end++;
      if (end - start === 1) {
        out.push(numbered[order[start] ?? 0] ?? 0);
      } else {
        const bucket: number[] = [];
        for (let k = start; k < end; k++) bucket.push(numbered[order[k] ?? 0] ?? 0);
        naturalOrder(bucket, written, at, out);
      }
      start = end;
    }
  }
  for (const key of [...long.keys()].sort(compareDigits)) {
    naturalOrder(long.get(key) ?? [], written, at, out);
  }
  for (const key of [...texts.keys()].sort()) {
    naturalOrder(texts.get(key) ?? [], written, at, out);
  }
}

/** Whether two runs are the same text, ignoring ASCII case. */
function sameRun(a: string, at: number, b: string, bt: number, length: number): boolean {
  for (let k = 0; k < length; k++) {
    let x = a.charCodeAt(at + k);
    let y = b.charCodeAt(bt + k);
    if (x === y) continue;
    if (x >= 65 && x <= 90) x += 32;
    if (y >= 65 && y <= 90) y += 32;
    if (x !== y) return false;
  }
  return true;
}

/** Each activity's place in a column's order: equal values share a rank; -1 is no value. */
const rankCache = new WeakMap<Model, Map<ColumnId, { ranks: Int32Array; top: number }>>();

/**
 * A column's dense ranks over every activity, worked out once per model:
 * text in natural order ([`naturalOrder`]), numbers by value.
 */
export function ranks(model: Model, column: ColumnId): { ranks: Int32Array; top: number } {
  let byColumn = rankCache.get(model);
  if (!byColumn) {
    byColumn = new Map();
    rankCache.set(model, byColumn);
  }
  // Before and After change with each comparison, so they're never kept.
  const kept = !COMPARE_COLUMNS.has(column);
  const cached = kept ? byColumn.get(column) : undefined;
  if (cached) return cached;
  const get = COLUMN_BY_ID.get(column)?.key;
  const out = new Int32Array(model.count).fill(-1);
  let top = 0;
  if (get) {
    const texts: number[] = [];
    const written: string[] = new Array<string>(model.count);
    const numbers: number[] = [];
    const numbered: number[] = [];
    for (let i = 0; i < model.count; i++) {
      const value = get(model.rows, i);
      if (value === null) continue;
      if (typeof value === "number") {
        numbers.push(value);
        numbered.push(i);
      } else {
        texts.push(i);
        written[i] = value;
      }
    }
    if (texts.length > 0) {
      const order: number[] = [];
      naturalOrder(texts, written, new Int32Array(model.count), order);
      let previous: string | null = null;
      let rank = -1;
      for (const i of order) {
        if (written[i] !== previous) {
          rank++;
          previous = written[i] ?? null;
        }
        out[i] = rank;
      }
      top = rank + 1;
    } else if (numbers.length > 0) {
      const values = Float64Array.from(numbers).sort();
      const rankOf = new Map<number, number>();
      for (const value of values) if (!rankOf.has(value)) rankOf.set(value, rankOf.size);
      numbered.forEach((i, k) => {
        out[i] = rankOf.get(numbers[k] ?? 0) ?? -1;
      });
      top = rankOf.size;
    }
  }
  const result = { ranks: out, top };
  if (kept) byColumn.set(column, result);
  return result;
}

/**
 * `indices` sorted by the keys, in place; ties keep file order, and missing
 * values stay last whichever way a column sorts. With up to two keys, each
 * activity's place folds into one number sorted natively.
 */
export function sorted(model: Model, indices: Int32Array, sort: readonly SortKey[]): Int32Array {
  if (sort.length === 0) return indices;
  const keys = sort.map(({ column, descending }) => {
    const { ranks: r, top } = ranks(model, column);
    // Ranks 0..top-1 by direction; no value is `top`, after either way.
    const place = (i: number) => {
      const rank = r[i] ?? -1;
      if (rank < 0) return top;
      return descending ? top - 1 - rank : rank;
    };
    return { place, size: top + 1 };
  });
  const n = model.count;
  // Folding needs every place, times the count, to stay an exact integer.
  const span = keys.reduce((product, key) => product * key.size, 1) * n;
  if (keys.length <= 2 && span < Number.MAX_SAFE_INTEGER) {
    const [first, second] = keys;
    const folded = new Float64Array(indices.length);
    indices.forEach((i, k) => {
      const outer = first ? first.place(i) : 0;
      const inner = second ? second.place(i) : 0;
      folded[k] = (outer * (second?.size ?? 1) + inner) * n + i;
    });
    folded.sort();
    folded.forEach((value, k) => {
      indices[k] = value % n;
    });
    return indices;
  }
  const order = Array.from(indices);
  order.sort((x, y) => {
    for (const { place } of keys) {
      const c = place(x) - place(y);
      if (c !== 0) return c;
    }
    return x - y;
  });
  indices.set(order);
  return indices;
}

interface Node {
  path: string;
  label: string;
  depth: number;
  count: number;
  start: string | null;
  finish: string | null;
  children: Map<string, Node>;
  activities: number[];
}

function node(path: string, label: string, depth: number): Node {
  return { path, label, depth, count: 0, start: null, finish: null, children: new Map(), activities: [] };
}

const earlier = (a: string | null, b: string | null) => (a === null || (b !== null && b < a) ? b : a);
const later = (a: string | null, b: string | null) => (a === null || (b !== null && b > a) ? b : a);

/**
 * Each WBS node's place in the project's WBS, by its dotted path, in the
 * order the server gives the tree: the WBS's own sequence, then code.
 */
export function wbsOrder(projects: readonly ProjectWbs[]): Map<string, number> {
  const order = new Map<string, number>();
  const walk = (node: WbsNode, parent: string) => {
    const path = parent ? `${parent}.${node.code}` : node.code;
    if (!order.has(path)) order.set(path, order.size);
    for (const child of node.children) walk(child, path);
  };
  for (const project of projects) for (const node of project.nodes) walk(node, "");
  return order;
}

/** How an edit names a WBS node, and the project it's in. */
export interface WbsRef {
  /** `proj_id`. */
  project: number;
  /** Its code, as an edit gives it. */
  code: string;
  /** `wbs_id`; none for a node not yet added. */
  id?: number;
  /** The project's own node, at the top with the project's code, which follows the project and can't be renamed. */
  projectNode?: boolean;
  /**
   * What an edit's `wbs` or `parent` takes for it: its code where no other
   * node in its project has that code, else its `wbs_id` where no node has
   * that as a code; null when neither names it alone.
   */
  ref: string | null;
}

/** Sets each node's `ref` from the codes its project's nodes have, as the server looks nodes up: by code in any case first, then by ID. */
export function withRefs(refs: Map<string, WbsRef>): Map<string, WbsRef> {
  const codes = new Map<number, Map<string, number>>();
  for (const node of refs.values()) {
    const here = codes.get(node.project) ?? new Map<string, number>();
    const code = node.code.toLowerCase();
    here.set(code, (here.get(code) ?? 0) + 1);
    codes.set(node.project, here);
  }
  for (const [path, node] of refs) {
    const here = codes.get(node.project);
    const id = node.id === undefined ? null : String(node.id);
    const ref = here?.get(node.code.toLowerCase()) === 1 ? node.code : id !== null && !here?.has(id.toLowerCase()) ? id : null;
    if (ref !== node.ref) refs.set(path, { ...node, ref });
  }
  return refs;
}

/** Each WBS node's reference for an edit, by its dotted path, as `wbsOrder` gives paths. */
export function wbsRefs(projects: readonly ProjectWbs[]): Map<string, WbsRef> {
  const refs = new Map<string, WbsRef>();
  for (const project of projects) {
    const walk = (node: WbsNode, parent: string) => {
      const path = parent ? `${parent}.${node.code}` : node.code;
      const own = parent === "" && node.code === project.code;
      if (!refs.has(path)) refs.set(path, { project: project.project, code: node.code, id: node.id, ref: null, ...(own ? { projectNode: true } : {}) });
      for (const child of node.children) walk(child, path);
    };
    for (const node of project.nodes) walk(node, "");
  }
  return withRefs(refs);
}

/** Each named WBS node's name, by its dotted path, as `wbsOrder` gives paths. */
export function wbsNames(projects: readonly ProjectWbs[]): Map<string, string> {
  const names = new Map<string, string>();
  const walk = (node: WbsNode, parent: string) => {
    const path = parent ? `${parent}.${node.code}` : node.code;
    if (node.name && !names.has(path)) names.set(path, node.name);
    for (const child of node.children) walk(child, path);
  };
  for (const project of projects) for (const node of project.nodes) walk(node, "");
  return names;
}

/**
 * Lines and bands for an arrangement. Sibling bands follow `order`, the WBS's
 * own sequence, when it's known, else their codes in natural order. Each
 * level's bands hold the next level's, around the activities they hold.
 * WBS bands carry their node's name from `bandNames`, where it has one.
 */
export function arrange(
  model: Model,
  arrangement: Arrangement,
  bandOrder?: ReadonlyMap<string, number>,
  bandNames?: ReadonlyMap<string, string>,
): Arranged {
  const order = sorted(model, filtered(model, arrangement), arrangement.sort);
  if (!arrangement.grouped) return { lines: order, bands: [], matched: order.length };
  const out: Placed = {
    model,
    levels: [arrangement.by ?? null, ...(arrangement.then ?? [])],
    collapsed: arrangement.collapsed,
    bandOrder,
    bandNames,
    building: arrangement.building
      ? { paths: arrangement.building, empty: !filtering(arrangement), addingWbs: arrangement.addingWbs ?? null }
      : null,
    lines: new Int32Array(order.length * 2 + 16),
    used: 0,
    bands: [],
  };
  place(out, order, 0, "", 0);
  return { lines: out.lines.slice(0, out.used), bands: out.bands, matched: order.length };
}

/** What `arrange` builds, level by level. */
interface Placed {
  model: Model;
  levels: readonly (Grouping | null)[];
  collapsed: ReadonlySet<string>;
  bandOrder: ReadonlyMap<string, number> | undefined;
  bandNames: ReadonlyMap<string, string> | undefined;
  /** The WBS paths every band is made for, and Add activity lines too; see `Arrangement.building`. */
  building: { paths: ReadonlySet<string>; empty: boolean; addingWbs: string | null } | null;
  lines: Int32Array;
  used: number;
  bands: Band[];
}

/** A band's path within the band `outer` names, if any. */
const within = (outer: string, path: string) => (outer ? `${outer}\u0001${path}` : path);

function push(out: Placed, line: number): void {
  if (out.used === out.lines.length) {
    const more = new Int32Array(out.lines.length * 2);
    more.set(out.lines);
    out.lines = more;
  }
  out.lines[out.used++] = line;
}

function band(out: Placed, made: Band): boolean {
  out.bands.push(made);
  push(out, -out.bands.length);
  return !made.collapsed;
}

/** Lines for `activities`, in sorted order, grouped from level `level` on. */
function place(out: Placed, activities: ArrayLike<number>, level: number, outer: string, depth: number): void {
  if (level >= out.levels.length) {
    for (let k = 0; k < activities.length; k++) push(out, activities[k] ?? 0);
    return;
  }
  const by = out.levels[level];
  if (by) byField(out, activities, by, level, outer, depth);
  else byWbs(out, activities, level, outer, depth);
}

/** WBS bands for the activities, nested as the WBS is. */
function byWbs(out: Placed, activities: ArrayLike<number>, level: number, outer: string, depth: number): void {
  // A tree of WBS paths, holding activities in sorted order. Each distinct
  // path finds its node once, by its index in the model.
  const { model } = out;
  const root = node("", "", -1);
  // Each WBS path's node, by the path's index: an array when the activities
  // are many beside the paths, else a map, so the WBS within each of many
  // bands costs what the band holds.
  const { pathOf, paths, starts, finishes } = model;
  const many = activities.length * 4 >= paths.length;
  const placed = many ? new Array<Node | undefined>(paths.length) : null;
  const mapped = many ? null : new Map<number, Node>();
  for (let k = 0; k < activities.length; k++) {
    const i = activities[k] ?? 0;
    const at = pathOf[i] ?? 0;
    let found = placed ? placed[at] : mapped?.get(at);
    if (!found) {
      const path = paths[at] ?? "";
      let parent: Node = root;
      let prefix = "";
      for (const part of path === "" ? [] : path.split(".")) {
        prefix = prefix ? `${prefix}.${part}` : part;
        let next: Node | undefined = parent.children.get(part);
        if (!next) {
          next = node(prefix, part, parent.depth + 1);
          parent.children.set(part, next);
        }
        parent = next;
      }
      found = parent;
      if (placed) placed[at] = found;
      else mapped?.set(at, found);
    }
    found.activities.push(i);
  }
  // Building, at the first level: every WBS node gets a band, though empty,
  // while nothing filters the rows.
  const building = level === 0 && outer === "" ? out.building : null;
  if (building?.empty) {
    for (const path of building.paths) {
      let parent: Node = root;
      let prefix = "";
      for (const part of path === "" ? [] : path.split(".")) {
        prefix = prefix ? `${prefix}.${part}` : part;
        let next = parent.children.get(part);
        if (!next) {
          next = node(prefix, part, parent.depth + 1);
          parent.children.set(part, next);
        }
        parent = next;
      }
    }
  }
  const count = (n: Node): number => {
    n.count = n.activities.length;
    const { activities: own } = n;
    for (let k = 0; k < own.length; k++) {
      const i = own[k] ?? 0;
      n.start = earlier(n.start, starts[i] ?? null);
      n.finish = later(n.finish, finishes[i] ?? null);
    }
    for (const child of n.children.values()) {
      n.count += count(child);
      n.start = earlier(n.start, child.start);
      n.finish = later(n.finish, child.finish);
    }
    return n.count;
  };
  count(root);

  const walk = (n: Node) => {
    const at = (child: Node) => out.bandOrder?.get(child.path) ?? Number.POSITIVE_INFINITY;
    const siblings = [...n.children.values()].sort((a, b) => at(a) - at(b) || natural(a.label, b.label));
    for (const child of siblings) {
      const path = within(outer, child.path);
      // A name that only repeats the path says nothing more.
      const named = out.bandNames?.get(child.path);
      const name = named === child.path ? undefined : named;
      const open = band(out, {
        path,
        label: child.label,
        title: child.path,
        ...(name ? { name } : {}),
        spoken: name ? `WBS ${name}, ${child.path}` : `WBS ${child.path}`,
        depth: depth + child.depth,
        count: child.count,
        collapsed: out.collapsed.has(path),
        start: child.start,
        finish: child.finish,
      });
      if (open) walk(child);
    }
    // The node's own activities, split by the next level, if any.
    const inside = n === root ? outer : within(outer, n.path);
    // Building: a new child node is typed after the others, where it will go.
    if (building && n !== root && building.addingWbs === n.path) {
      band(out, {
        path: `${inside}\u0002wbs`,
        label: "",
        title: "New WBS node",
        newWbs: n.path,
        spoken: `New WBS node under ${out.bandNames?.get(n.path) ?? `WBS ${n.path}`}`,
        depth: depth + n.depth + 1,
        count: 0,
        collapsed: false,
        start: null,
        finish: null,
      });
    }
    place(out, n.activities, level + 1, inside, n === root ? depth : depth + n.depth + 1);
    // Building: a line at the end of the node's band that adds an activity to it.
    if (building && n !== root) {
      const named = out.bandNames?.get(n.path);
      band(out, {
        path: `${inside}\u0002add`,
        label: "",
        title: "Add activity",
        adds: n.path,
        spoken: `Add an activity to ${named && named !== n.path ? named : `WBS ${n.path}`}`,
        depth: depth + n.depth + 1,
        count: 0,
        collapsed: false,
        start: null,
        finish: null,
      });
    }
  };
  walk(root);
}

/** Each activity's value of a field, as an index into its distinct values, worked out once. */
interface Keyed {
  /** Per activity: the value's index, or -1 for none. */
  keys: Int32Array;
  /** The distinct values, in band order. */
  values: (string | number)[];
}

const keyCache = new WeakMap<Grouping, WeakMap<Model, Keyed>>();

/**
 * The distinct values of a field, in the order their bands go: a value's
 * place in `order` first, then numbers by value, then text in natural
 * order.
 */
function keyed(model: Model, by: Grouping): Keyed {
  let byModel = keyCache.get(by);
  if (!byModel) {
    byModel = new WeakMap();
    keyCache.set(by, byModel);
  }
  const cached = byModel.get(model);
  if (cached) return cached;
  const first = new Map<string | number, number>();
  const seen: (string | number)[] = [];
  const raw = new Int32Array(model.count).fill(-1);
  for (let i = 0; i < model.count; i++) {
    const value = by.values.get(model.rows.id[i] ?? -1);
    if (value === undefined || value === "") continue;
    let at = first.get(value);
    if (at === undefined) {
      at = seen.length;
      seen.push(value);
      first.set(value, at);
    }
    raw[i] = at;
  }
  const place = (v: string | number) => by.order?.get(String(v)) ?? Number.POSITIVE_INFINITY;
  const sortedValues = seen
    .map((value, at) => ({ value, at }))
    .sort((a, b) => {
      const p = place(a.value) - place(b.value);
      if (p !== 0 && !Number.isNaN(p)) return p;
      if (typeof a.value === "number" && typeof b.value === "number") return a.value - b.value;
      if (typeof a.value === "number") return -1;
      if (typeof b.value === "number") return 1;
      return natural(a.value, b.value);
    });
  const rank = new Int32Array(seen.length);
  sortedValues.forEach(({ at }, k) => {
    rank[at] = k;
  });
  const keys = raw.map((at) => (at < 0 ? -1 : (rank[at] ?? -1)));
  const result = { keys, values: sortedValues.map(({ value }) => value) };
  byModel.set(model, result);
  return result;
}

/** One band per value of the field, in band order, and one last for activities with none. */
function byField(
  out: Placed,
  activities: ArrayLike<number>,
  by: Grouping,
  level: number,
  outer: string,
  depth: number,
): void {
  const { model } = out;
  const { keys, values } = keyed(model, by);
  const none = values.length;
  if (none < activities.length * 4) {
    denseByField(out, activities, keys, values, by, level, outer, depth);
    return;
  }
  // Many more values than activities, as within another band: only the
  // values these activities hold, each with its activities in sorted order,
  // so the band costs what it holds.
  const buckets = new Map<number, number[]>();
  for (let k = 0; k < activities.length; k++) {
    const i = activities[k] ?? 0;
    const key = keys[i] ?? -1;
    const b = key < 0 ? none : key;
    let bucket = buckets.get(b);
    if (!bucket) {
      bucket = [];
      buckets.set(b, bucket);
    }
    bucket.push(i);
  }
  for (const b of [...buckets.keys()].sort((x, y) => x - y)) {
    const held = buckets.get(b) ?? [];
    let start: string | null = null;
    let finish: string | null = null;
    for (const i of held) {
      start = earlier(start, model.starts[i] ?? null);
      finish = later(finish, model.finishes[i] ?? null);
    }
    fieldBand(out, by, b < none ? String(values[b]) : null, held, start, finish, level, outer, depth);
  }
}

/** One field band, for `value` or for no value, and what's within it. */
function fieldBand(
  out: Placed,
  by: Grouping,
  value: string | null,
  held: ArrayLike<number>,
  start: string | null,
  finish: string | null,
  level: number,
  outer: string,
  depth: number,
): void {
  const label = value ?? NO_VALUE;
  const path = within(outer, `${by.field}\u0000${value ?? ""}`);
  const title = `${by.title}: ${label}`;
  const open = band(out, {
    path,
    label,
    title,
    spoken: title,
    depth,
    count: held.length,
    collapsed: out.collapsed.has(path),
    start,
    finish,
  });
  if (open) place(out, held, level + 1, path, depth + 1);
}

/** `byField` where values are few beside the activities: one counting sort over them all. */
function denseByField(
  out: Placed,
  activities: ArrayLike<number>,
  keys: Int32Array,
  values: readonly (string | number)[],
  by: Grouping,
  level: number,
  outer: string,
  depth: number,
): void {
  const { model } = out;
  const none = values.length;
  const counts = new Int32Array(none + 1);
  for (let k = 0; k < activities.length; k++) {
    const key = keys[activities[k] ?? 0] ?? -1;
    const b = key < 0 ? none : key;
    counts[b] = (counts[b] ?? 0) + 1;
  }
  // Each band's activities, kept in sorted order, by counting sort.
  const starts = new Int32Array(none + 2);
  for (let b = 0; b <= none; b++) starts[b + 1] = (starts[b] ?? 0) + (counts[b] ?? 0);
  const next = starts.slice(0, none + 1);
  const grouped = new Int32Array(activities.length);
  const starting = new Array<string | null>(none + 1).fill(null);
  const finishing = new Array<string | null>(none + 1).fill(null);
  for (let k = 0; k < activities.length; k++) {
    const i = activities[k] ?? 0;
    const key = keys[i] ?? -1;
    const b = key < 0 ? none : key;
    const at = next[b] ?? 0;
    grouped[at] = i;
    next[b] = at + 1;
    starting[b] = earlier(starting[b] ?? null, model.starts[i] ?? null);
    finishing[b] = later(finishing[b] ?? null, model.finishes[i] ?? null);
  }
  for (let b = 0; b <= none; b++) {
    if ((counts[b] ?? 0) === 0) continue;
    const held = grouped.subarray(starts[b] ?? 0, starts[b + 1] ?? 0);
    const value = b < none ? String(values[b]) : null;
    fieldBand(out, by, value, held, starting[b] ?? null, finishing[b] ?? null, level, outer, depth);
  }
}

/** The band a line shows, if it's a band. */
export function bandOf(arranged: Arranged, line: number): Band | undefined {
  const value = arranged.lines[line];
  return value !== undefined && value < 0 ? arranged.bands[-value - 1] : undefined;
}
