// Draws the Gantt: one bar per row on screen, from the dates the server sent,
// placed linearly on the time axis. Nothing is drawn off screen, so a frame
// costs the same at 50 or 50,000 activities.

import type { Links, Rows } from "../api";
import { fontOf, themeOf } from "../canvas";
import type { Arranged } from "../grid/model";
import { type LinkType, type Point, edgesOf, route } from "./lines";
import { type Scale, type Tick, type Times, minutesOf, xOf } from "./time";

export interface Colours {
  bg: string;
  band: string;
  chosen: string;
  line: string;
  text: string;
  muted: string;
  bar: string;
  critical: string;
  actual: string;
  float: string;
  summary: string;
  dataDate: string;
  today: string;
  font: string;
  /** Hi-vis amber for a driving path or an explain chain, and the ink drawn on it. */
  hivis: string;
  hivisInk: string;
  /** Survey blue for what the explain panel points at. */
  focus: string;
}

/** How strongly what's off a highlighted path shows. */
const DIMMED = 0.28;

/** The theme's colours, from the CSS variables on `element`. */
/**
 * Colours on paper, which is always light: the light theme's tokens (the top
 * of styles.css), whatever theme the screen is in.
 */
export const PAPER_COLOURS = {
  critical: "#c62828",
  other: "#4a7bae",
  actual: "#59636e",
  band: "#18222d",
  line: "#dce1e7",
  muted: "#5b6774",
  float: "#5b6774",
  dataDate: "#b25e09",
} as const;

export function coloursOf(element: Element): Colours {
  const v = themeOf(element);
  return {
    bg: v("--bg", "#fff"),
    band: v("--bg-raised", "#f6f7f9"),
    chosen: v("--bg-chosen", "#dbe7fe"),
    line: v("--line", "#d9dde3"),
    text: v("--text", "#111827"),
    muted: v("--muted", "#6b7280"),
    bar: v("--steel", "#4a7bae"),
    critical: v("--bad", "#b91c1c"),
    actual: v("--graphite", "#59636e"),
    float: v("--muted", "#6b7280"),
    summary: v("--text", "#111827"),
    dataDate: v("--warn", "#b45309"),
    today: v("--live", "#15803d"),
    font: fontOf(v),
    hivis: v("--hivis", "#ffb000"),
    hivisInk: v("--hivis-ink", "#4a2f00"),
    focus: v("--accent", "#1f6fd1"),
  };
}

export interface Picture {
  rows: Rows;
  arranged: Arranged;
  times: Times;
  scale: Scale;
  /** Scroll offsets: `top` is the grid's, `left` the Gantt's own. */
  top: number;
  left: number;
  width: number;
  height: number;
  rowHeight: number;
  active: number;
  /** Each project's data date, in minutes, by `proj_id`. */
  dataDates: ReadonlyMap<number, number>;
  /** Today's line, when it's turned on. */
  today: number | null;
  /** The bottom tier's cells, for faint gridlines. */
  grid: Tick[];
  /** Relationships to draw, when there are any. */
  logic?: Logic | undefined;
}

/** Which logic lines to draw, and what to pick out. */
export type LineMode = "all" | "selected" | "none";

export interface Logic {
  /** The relationships, when they've been fetched. */
  links: Links | null;
  /** Each activity's relationships, by `task_id`. */
  index: ReadonlyMap<number, readonly number[]> | null;
  /** Where each activity is in the arrangement, by `task_id`. */
  lineOf: ReadonlyMap<number, number>;
  mode: LineMode;
  /** The selected activity's `task_id`, for `selected` mode. */
  selected: number | null;
  /** A highlighted driving path. */
  path: DrivingPath | null;
}

/**
 * A driving path or an explain chain: its activities, and the relationships
 * the chain runs along. Shown in hi-vis amber, with everything else dimmed.
 */
export interface DrivingPath {
  /** The explain panel's numbers for its activities, by `task_id`, shown as badges. */
  numbers?: ReadonlyMap<number, number>;
  /** The activity the explain panel points at, by `task_id`, shown most strongly. */
  focus?: number | null;
  /** By `task_id`. */
  ids: ReadonlySet<number>;
  /** The relationships the chain names, by `task_pred_id`. */
  relationships: ReadonlySet<number>;
  /** Steps between activities that name no relationship, as `pred>succ` by `task_id`. */
  edges: ReadonlySet<string>;
}

/** What was drawn where, for hovering and clicking. */
export interface Drawn {
  lines: { points: Point[]; link: number }[];
  /** Arrows at the edge for lines whose other end is off screen, and that end's `task_id`. */
  arrows: { x: number; y: number; to: number }[];
  /** The bars on screen, for clicking: their box and `task_id`. */
  bars: { left: number; right: number; top: number; bottom: number; id: number }[];
}

/** A bar's ends on screen, before scrolling: from its earliest piece to its latest. */
export function endsOf(bar: Bar): { left: number; right: number } | null {
  const xs = [bar.actual, bar.remaining].flatMap((piece) => piece ?? []);
  if (bar.at !== undefined) xs.push(bar.at);
  if (xs.length === 0) return null;
  return { left: Math.min(...xs), right: Math.max(...xs) };
}

/** Where an activity's bar pieces fall, in screen x before scrolling. */
export interface Bar {
  kind: "task" | "milestone";
  critical: boolean;
  /** Work done: actual start to actual finish, or to the data date. */
  actual?: [number, number];
  /** Work left, or all of it if not started: early start to early finish. */
  remaining?: [number, number];
  /** Total float: early finish to late finish. */
  float?: [number, number];
  /** A milestone's point. */
  at?: number;
}

const usable = (x: number) => Number.isFinite(x);

/** The pieces of activity `i`'s bar, all from dates the server sent. */
export function barOf(rows: Rows, times: Times, i: number, scale: Scale, dataDate: number | undefined): Bar {
  const critical = rows.critical[i] === true;
  const type = rows.type[i] ?? "task";
  const as = times.actualStart[i] ?? Number.NaN;
  const af = times.actualFinish[i] ?? Number.NaN;
  const es = times.earlyStart[i] ?? Number.NaN;
  const ef = times.earlyFinish[i] ?? Number.NaN;
  const lf = times.lateFinish[i] ?? Number.NaN;
  const x = (m: number) => xOf(scale, m);
  if (type === "start-milestone" || type === "finish-milestone") {
    const point = type === "start-milestone" ? (usable(as) ? as : es) : usable(af) ? af : ef;
    return usable(point) ? { kind: "milestone", critical, at: x(point) } : { kind: "milestone", critical };
  }
  const bar: Bar = { kind: "task", critical };
  if (usable(as)) {
    // Done up to its finish, or so far, up to the data date.
    const end = usable(af) ? af : dataDate !== undefined && dataDate > as ? dataDate : as;
    bar.actual = [x(as), x(end)];
  }
  if (!usable(af) && usable(es) && usable(ef)) bar.remaining = [x(es), x(ef)];
  if (!usable(af) && usable(ef) && usable(lf) && lf > ef) bar.float = [x(ef), x(lf)];
  return bar;
}

/** The time axis's two tiers. */
export function drawHeader(
  ctx: CanvasRenderingContext2D,
  colours: Colours,
  tiers: { top: Tick[]; bottom: Tick[] },
  left: number,
  width: number,
  height: number,
): void {
  ctx.fillStyle = colours.band;
  ctx.fillRect(0, 0, width, height);
  ctx.font = colours.font;
  ctx.textBaseline = "middle";
  const half = Math.floor(height / 2);
  for (const [cells, y, h] of [
    [tiers.top, 0, half],
    [tiers.bottom, half, height - half],
  ] as const) {
    for (const cell of cells) {
      const x = cell.x - left;
      ctx.fillStyle = colours.line;
      ctx.fillRect(Math.round(x), y, 1, h);
      // A cell cut off at the left edge keeps its label in view, if it fits.
      const from = Math.max(x, 0) + 4;
      if (ctx.measureText(cell.label).width <= x + cell.width - from - 2) {
        ctx.fillStyle = colours.muted;
        ctx.fillText(cell.label, from, y + h / 2);
      }
    }
    ctx.fillStyle = colours.line;
    ctx.fillRect(0, y + h - 1, width, 1);
  }
}

export function drawBody(ctx: CanvasRenderingContext2D, colours: Colours, p: Picture): Drawn {
  const { rows, arranged, times, scale, top, left, width, height, rowHeight: h } = p;
  ctx.fillStyle = colours.bg;
  ctx.fillRect(0, 0, width, height);
  // Faint lines at the bottom tier's cells.
  ctx.fillStyle = colours.line;
  for (const cell of p.grid) {
    const x = Math.round(cell.x - left);
    if (x >= 0 && x <= width) ctx.fillRect(x, 0, 1, height);
  }
  const first = Math.max(0, Math.floor(top / h));
  const last = Math.min(arranged.lines.length - 1, Math.ceil((top + height) / h));
  const pad = Math.max(3, Math.round(h * 0.25));
  const bars: Drawn["bars"] = [];
  for (let line = first; line <= last; line++) {
    const y = line * h - top;
    const value = arranged.lines[line] ?? 0;
    if (line === p.active) {
      ctx.fillStyle = colours.chosen;
      ctx.fillRect(0, y, width, h);
    }
    if (value < 0) {
      const band = arranged.bands[-value - 1];
      // An Add activity or new WBS node line has nothing to draw.
      if (!band || band.adds !== undefined || band.newWbs !== undefined) continue;
      if (line !== p.active) {
        ctx.fillStyle = colours.band;
        ctx.fillRect(0, y, width, h);
      }
      const from = minutesOf(band.start);
      const to = minutesOf(band.finish);
      if (usable(from) && usable(to)) {
        const x0 = xOf(scale, from) - left;
        const x1 = xOf(scale, to) - left;
        const mid = y + h / 2;
        ctx.fillStyle = colours.summary;
        ctx.fillRect(x0, mid - 2, Math.max(x1 - x0, 1), 4);
        // End caps, as summary bars have.
        for (const x of [x0, x1]) {
          ctx.beginPath();
          ctx.moveTo(x - 4, mid - 2);
          ctx.lineTo(x + 4, mid - 2);
          ctx.lineTo(x, mid + 4);
          ctx.closePath();
          ctx.fill();
        }
      }
      continue;
    }
    const bar = barOf(rows, times, value, scale, p.dataDates.get(rows.project[value] ?? -1));
    const colour = bar.critical ? colours.critical : colours.bar;
    const id = rows.id[value] ?? -1;
    const path = p.logic?.path ?? null;
    const onPath = path?.ids.has(id) === true;
    // Off a highlighted path, faint; on it, as bright as the theme allows.
    ctx.globalAlpha = path && !onPath ? DIMMED : 1;
    if (bar.kind === "milestone") {
      if (bar.at === undefined) continue;
      const x = bar.at - left;
      const mid = y + h / 2;
      const r = h / 2 - pad / 2;
      ctx.fillStyle = colour;
      ctx.beginPath();
      ctx.moveTo(x, mid - r);
      ctx.lineTo(x + r, mid);
      ctx.lineTo(x, mid + r);
      ctx.lineTo(x - r, mid);
      ctx.closePath();
      ctx.fill();
      if (onPath && path) {
        // Outlined in hi-vis, as a bar on the path is.
        ctx.strokeStyle = path.focus === id ? colours.focus : colours.hivis;
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(x, mid - r - 3);
        ctx.lineTo(x + r + 3, mid);
        ctx.lineTo(x, mid + r + 3);
        ctx.lineTo(x - r - 3, mid);
        ctx.closePath();
        ctx.stroke();
        ctx.lineWidth = 1;
        badge(ctx, colours, path, id, x - r, y + h / 2);
      }
      ctx.globalAlpha = 1;
      bars.push({ left: x - r, right: x + r, top: y, bottom: y + h, id });
      continue;
    }
    if (bar.float) {
      ctx.fillStyle = colours.float;
      const [a, b] = bar.float;
      ctx.fillRect(a - left, y + h - pad - 2, b - a, 2);
    }
    if (bar.remaining) {
      ctx.fillStyle = colour;
      const [a, b] = bar.remaining;
      ctx.fillRect(a - left, y + pad, Math.max(b - a, 1), h - 2 * pad);
    }
    if (bar.actual) {
      ctx.fillStyle = colours.actual;
      const [a, b] = bar.actual;
      ctx.fillRect(a - left, y + pad, Math.max(b - a, 1), h - 2 * pad);
    }
    const ends = endsOf(bar);
    if (ends) bars.push({ left: ends.left - left, right: ends.right - left, top: y, bottom: y + h, id });
    // On the highlighted path: outlined in hi-vis, numbered for the explain panel.
    if (onPath && path && ends) {
      ctx.strokeStyle = path.focus === id ? colours.focus : colours.hivis;
      ctx.lineWidth = 3;
      ctx.strokeRect(ends.left - left - 2.5, y + pad - 2.5, ends.right - ends.left + 5, h - 2 * pad + 5);
      ctx.lineWidth = 1;
      badge(ctx, colours, path, id, ends.left - left, y + h / 2);
    }
    ctx.globalAlpha = 1;
  }
  const drawn = p.logic && p.logic.mode !== "none" ? drawLines(ctx, colours, p, p.logic, first, last) : { lines: [], arrows: [], bars: [] };
  drawn.bars = bars;
  // The data dates, and today when asked for.
  const seen = new Set<number>();
  ctx.setLineDash([4, 3]);
  for (const date of p.dataDates.values()) {
    if (seen.has(date)) continue;
    seen.add(date);
    const x = Math.round(xOf(scale, date) - left) + 0.5;
    ctx.strokeStyle = colours.dataDate;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
  }
  ctx.setLineDash([]);
  if (p.today !== null) {
    const x = Math.round(xOf(scale, p.today) - left) + 0.5;
    ctx.strokeStyle = colours.today;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
  }
  return drawn;
}

/**
 * The relationships of the activities on screen, or of the selected one: a
 * driving relationship solid, the others dashed, those along a highlighted
 * path heavier. A line whose other end is off screen ends in an arrow at the
 * edge.
 */
function drawLines(ctx: CanvasRenderingContext2D, colours: Colours, p: Picture, logic: Logic, first: number, last: number): Drawn {
  const { rows, arranged, times, scale, top, left, height, rowHeight: h } = p;
  const drawn: Drawn = { lines: [], arrows: [], bars: [] };
  const ids: number[] = [];
  if (logic.mode === "selected") {
    if (logic.selected !== null) ids.push(logic.selected);
  } else {
    for (let line = first; line <= last; line++) {
      const value = arranged.lines[line] ?? -1;
      if (value >= 0) ids.push(rows.id[value] ?? -1);
    }
  }
  const { links, index } = logic;
  if (!links || !index) return drawn;
  const done = new Set<number>();
  const underneath: Line[] = [];
  const onTop: Line[] = [];
  const endsAt = (id: number) => {
    const line = logic.lineOf.get(id);
    const i = line === undefined ? undefined : arranged.lines[line];
    // The index is for this arrangement; a mismatch draws nothing rather than the wrong bar.
    if (line === undefined || i === undefined || i < 0 || rows.id[i] !== id) return null;
    const bar = barOf(rows, times, i, scale, p.dataDates.get(rows.project[i] ?? -1));
    const ends = endsOf(bar);
    return ends ? { left: ends.left - left, right: ends.right - left, y: line * h - top + h / 2 } : null;
  };
  for (const id of ids) {
    for (const k of index.get(id) ?? []) {
      if (done.has(k)) continue;
      done.add(k);
      const pred = links.predecessor[k] ?? -1;
      const succ = links.successor[k] ?? -1;
      // A relationship to work filtered out or under a collapsed band has nowhere to go.
      const from = endsAt(pred);
      const to = endsAt(succ);
      if (!from || !to) continue;
      const points = route((links.type[k] ?? "FS") as LinkType, from, to, h);
      // On the path: the relationship the engine's chain names, not one
      // parallel to it; where a step names none, a driving one between its activities.
      const path = logic.path;
      const onPath =
        path !== null &&
        (path.relationships.has(links.id[k] ?? -1) ||
          (links.driving[k] === true && path.edges.has(`${pred}>${succ}`)));
      // Lines on the path go on top, in hi-vis, after the rest.
      (onPath ? onTop : underneath).push({ points, driving: links.driving[k] === true, onPath });
      drawn.lines.push({ points, link: k });
      for (const edge of edgesOf(points, height, p.width)) {
        // The arrow leads to the end the route reaches beyond that edge,
        // unless that end is on screen and the route only strays past the edge.
        const end = edge.toward === "from" ? from : to;
        if (end.y >= 0 && end.y <= height && end.right >= 0 && end.left <= p.width) continue;
        const away = edge.toward === "from" ? pred : succ;
        // A triangle pointing off the screen, its tip on the edge.
        const [dx, dy] = edge.edge === "top" ? [0, -1] : edge.edge === "bottom" ? [0, 1] : edge.edge === "left" ? [-1, 0] : [1, 0];
        const tip = { x: edge.x, y: edge.y };
        ctx.fillStyle = colours.bar;
        ctx.beginPath();
        ctx.moveTo(tip.x, tip.y);
        ctx.lineTo(tip.x - dx * 9 - dy * 6, tip.y - dy * 9 - dx * 6);
        ctx.lineTo(tip.x - dx * 9 + dy * 6, tip.y - dy * 9 + dx * 6);
        ctx.closePath();
        ctx.fill();
        drawn.arrows.push({ x: tip.x - (dx * 9) / 2, y: tip.y - (dy * 9) / 2, to: away });
      }
    }
  }
  ctx.globalAlpha = logic.path ? DIMMED : 1;
  for (const line of underneath) stroke(ctx, colours, line);
  ctx.globalAlpha = 1;
  for (const line of onTop) stroke(ctx, colours, line);
  return drawn;
}

interface Line {
  points: Point[];
  driving: boolean;
  onPath: boolean;
}

/** A logic line and its arrowhead: driving solid, the rest dashed, the path's in hi-vis. */
function stroke(ctx: CanvasRenderingContext2D, colours: Colours, line: Line): void {
  const { points } = line;
  ctx.strokeStyle = line.onPath ? colours.hivis : colours.muted;
  ctx.fillStyle = ctx.strokeStyle;
  ctx.lineWidth = line.onPath ? 3 : 1;
  ctx.setLineDash(line.driving ? [] : [3, 3]);
  ctx.beginPath();
  points.forEach(([x, y], n) => (n === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)));
  ctx.stroke();
  ctx.setLineDash([]);
  ctx.lineWidth = 1;
  // The arrowhead, pointing the way the line enters the successor.
  const [x1, y1] = points[points.length - 1] ?? [0, 0];
  const [x0] = points[points.length - 2] ?? [0, 0];
  const way = Math.sign(x1 - x0) || 1;
  const size = line.onPath ? 7 : 5;
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x1 - way * size, y1 - size * 0.6);
  ctx.lineTo(x1 - way * size, y1 + size * 0.6);
  ctx.closePath();
  ctx.fill();
}

/**
 * The explain panel's number for an activity, in a hi-vis badge just left of
 * its bar, kept on screen. The one the panel points at is inverted.
 */
function badge(ctx: CanvasRenderingContext2D, colours: Colours, path: DrivingPath, id: number, barLeft: number, mid: number): void {
  const n = path.numbers?.get(id);
  if (n === undefined) return;
  const r = 8;
  const x = Math.max(barLeft - r - 5, r + 1);
  const focused = path.focus === id;
  ctx.beginPath();
  ctx.arc(x, mid, r, 0, Math.PI * 2);
  ctx.fillStyle = focused ? colours.focus : colours.hivis;
  ctx.fill();
  ctx.lineWidth = 1.5;
  ctx.strokeStyle = colours.hivisInk;
  ctx.stroke();
  ctx.lineWidth = 1;
  ctx.fillStyle = focused ? "#ffffff" : colours.hivisInk;
  const font = ctx.font;
  ctx.font = `700 11px ${colours.font.split(" ").slice(1).join(" ")}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(String(n), x, mid + 0.5);
  ctx.textAlign = "left";
  ctx.font = font;
}
