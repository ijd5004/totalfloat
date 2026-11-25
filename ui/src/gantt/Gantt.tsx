import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import type { Links, Rows, Summary } from "../api";
import { fitCanvas, wheelPixels } from "../canvas";
import { spanWords } from "../grid/columns";
import type { Arranged } from "../grid/model";
import type { Shown, View } from "../view";
import {
  type Colours,
  type DrivingPath,
  type Drawn,
  type LineMode,
  barOf,
  coloursOf,
  drawBody,
  drawHeader,
  endsOf,
} from "./draw";
import { distanceTo, lineIndex, linksByActivity } from "./lines";
import { type Edge, TYPE_WORDS, dropAt, edgeAt, typeOf } from "./linking";
import { indexOf } from "../edit/cells";
import { type Scale, type Times, ZOOMS, type ZoomChoice, fitZoom, minutesAt, minutesOf, scaleOf, stepZoom, ticks, timesOf, unitStart, xOf } from "./time";
import { dateText } from "../dates";

const DAY = 24 * 60;

/** What's under the pointer: an edge arrow, an activity's bar or row, or a line and the row it's in. */
interface Hit {
  now: Shown;
  arrow?: { x: number; y: number; to: number };
  activity?: number;
  link?: number;
  row?: number;
}

/**
 * What a point on the chart is on, by what was drawn there: an edge arrow,
 * then a bar however short, then a line, then the row's activity.
 */
export function pick(drawn: Drawn, x: number, y: number, activity: number | undefined): Omit<Hit, "now"> {
  const arrow = drawn.arrows.find((a) => Math.hypot(a.x - x, a.y - y) <= ARROW_REACH);
  if (arrow) return { arrow };
  const onBar = drawn.bars.some((b) => x >= b.left - 2 && x <= b.right + 2 && y >= b.top && y < b.bottom);
  if (onBar && activity !== undefined) return { activity };
  const line = drawn.lines.find((l) => distanceTo(l.points, x, y) <= LINE_REACH);
  if (line) return { link: line.link, ...(activity === undefined ? {} : { row: activity }) };
  return activity === undefined ? {} : { activity };
}

/** A relationship drawn on the chart: from the predecessor's end to the successor's. */
export interface Drawing {
  /** By `task_id`. */
  predecessor: number;
  successor: number;
  type: "FS" | "SS" | "FF" | "SF";
}

/** How far, in pixels, the pointer moves from a bar's end before a press becomes a drag, so a click still selects. */
const DRAG_START = 4;

/** A link being dragged: the end it started from, the pointer, and the end it would drop on. */
interface Dragging {
  from: Edge;
  x: number;
  y: number;
  to: Edge | null;
  /** The bars as drawn, to outline the one it would drop on. */
  bars: Drawn["bars"];
  /** Where the chart's body starts in its frame, and its size, so the tooltip opens away from the nearer edges. */
  top: number;
  width: number;
  height: number;
}

/** What the tooltip says while a link is dragged. */
export function dragWords(rows: Rows, drag: { from: Edge; to: Edge | null }): string[] {
  const code = (id: number) => {
    const i = indexOf(rows, id);
    return i === undefined ? String(id) : (rows.code[i] ?? String(id));
  };
  if (!drag.to) return [`From ${code(drag.from.id)}'s ${drag.from.end}`, "Drop on another bar's start or finish. Escape cancels."];
  const type = typeOf(drag.from.end, drag.to.end);
  return [`Link ${code(drag.from.id)} to ${code(drag.to.id)}, ${TYPE_WORDS[type]}`, "Release to add. The ends you drag between set the type."];
}

/** A moment the chart keeps at `x` pixels from its left edge. */
interface Held {
  x: number;
  moment: number;
}
/** How near the pointer must be to an edge arrow, or a line, to pick it. */
const ARROW_REACH = 10;
const LINE_REACH = 4;
/** How far a Ctrl+wheel or pinch moves, in pixels, to zoom one level. */
const ZOOM_STEP = 50;

/** Each project's data date, in minutes, by `proj_id`. */
function dataDatesOf(summary: Summary | null): Map<number, number> {
  const out = new Map<number, number>();
  for (const project of summary?.inspect.projects ?? []) {
    const date = minutesOf(project.data_date);
    if (Number.isFinite(date)) out.set(project.id, date);
  }
  return out;
}

/** What hovering over a line says: its ends, type, lag and whether it drives. */
export function linkText(
  rows: Rows,
  links: Links,
  k: number,
  lineOf: ReadonlyMap<number, number>,
  lines: Int32Array,
): string[] {
  const code = (id: number) => {
    const line = lineOf.get(id);
    const i = line === undefined ? undefined : lines[line];
    return i === undefined || i < 0 ? String(id) : (rows.code[i] ?? String(id));
  };
  const lag = links.lag_hours[k] ?? 0;
  // In hours: a lag's day depends on the project's lag calendar setting,
  // which the links don't carry, so days here could be wrong.
  return [
    `${code(links.predecessor[k] ?? -1)} → ${code(links.successor[k] ?? -1)}`,
    `${links.type[k] ?? "FS"}${lag ? `, lag ${spanWords(lag, null)}` : ", no lag"}${links.driving[k] ? ", driving" : ""}`,
  ];
}

/**
 * The first start and last finish of what the grid shows: each activity's
 * bar, actual where there is one, else early, out to its late finish where
 * the float line runs, and each band's dates, which cover activities a
 * collapsed band hides. Null when nothing has a date.
 */
export function spanShown(times: Times, arranged: Arranged): { first: number; last: number } | null {
  let first = Number.POSITIVE_INFINITY;
  let last = Number.NEGATIVE_INFINITY;
  const { lines, bands } = arranged;
  for (let k = 0; k < lines.length; k++) {
    const value = lines[k] ?? 0;
    let start: number;
    let finish: number;
    if (value < 0) {
      const band = bands[-value - 1];
      start = minutesOf(band?.start);
      finish = minutesOf(band?.finish);
    } else {
      const actual = times.actualStart[value] ?? Number.NaN;
      start = Number.isNaN(actual) ? (times.earlyStart[value] ?? Number.NaN) : actual;
      const done = times.actualFinish[value] ?? Number.NaN;
      const early = times.earlyFinish[value] ?? Number.NaN;
      const late = times.lateFinish[value] ?? Number.NaN;
      finish = !Number.isNaN(done) ? done : Number.isNaN(late) ? early : Math.max(early, late);
    }
    if (start < first) first = start;
    if (finish > last) last = finish;
  }
  return Number.isFinite(first) && Number.isFinite(last) ? { first, last } : null;
}

/** What hovering over a bar says: the rows' own text, nothing worked out. */
export function hoverText(rows: Rows, i: number): string[] {
  const lines = [`${rows.code[i] ?? ""} · ${rows.name[i] ?? ""}`];
  const pair = (label: string, start: string | null | undefined, finish: string | null | undefined) => {
    if (start || finish) lines.push(`${label}: ${dateText(start) || "–"} to ${dateText(finish) || "–"}`);
  };
  pair("Actual", rows.actual_start[i], rows.actual_finish[i]);
  pair("Early", rows.early_start[i], rows.early_finish[i]);
  pair("Late", rows.late_start[i], rows.late_finish[i]);
  const float = rows.total_float_hours[i];
  if (float !== null && float !== undefined) {
    lines.push(`Total float: ${spanWords(float, rows.hours_per_day?.[i])}${rows.critical[i] ? ", critical" : ""}`);
  }
  return lines;
}

/**
 * The Gantt chart: a canvas drawing a bar for each row the grid shows, on a
 * two-tier timescale. It follows the grid's vertical scroll through `view`
 * and scrolls through time on its own.
 */
export function Gantt({
  view,
  summary,
  zoom,
  onZoom,
  onFitted,
  today,
  links = null,
  lines = "all",
  path = null,
  onLink,
}: {
  view: View;
  summary: Summary | null;
  zoom: ZoomChoice;
  onZoom: (zoom: ZoomChoice) => void;
  /** Hears the pixels a day `fit` gives, each time it fits anew, so a zoom step from it can go to the nearest level. */
  onFitted?: (pxPerDay: number) => void;
  today: boolean;
  links?: Links | null;
  lines?: LineMode;
  /** A highlighted driving path. */
  path?: DrivingPath | null;
  /** Hears a link drawn from one bar's end to another's; without it, bars can't be linked here. */
  onLink?: ((link: Drawing) => void) | undefined;
}) {
  const root = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const head = useRef<HTMLCanvasElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const colours = useRef<Colours | null>(null);
  const [shown, setShown] = useState<Shown | null>(() => view.current());
  const [hover, setHover] = useState<{ x: number; y: number; top: number; flip: [boolean, boolean]; lines: string[] } | null>(
    null,
  );
  const dataDates = useMemo(() => dataDatesOf(summary), [summary]);
  // The chart's elements exist only once the grid has shown rows.
  const ready = shown !== null;

  // The time axis: from a month before the first date to a month after the last.
  const rows = shown?.rows ?? null;
  const times = useMemo(() => (rows ? timesOf(rows) : null), [rows]);
  // Fit: the dates of the rows the grid shows, filtered or not, across the chart's width.
  const [paneWidth, setPaneWidth] = useState(0);
  const arranged = shown?.arranged ?? null;
  const span = useMemo(() => (zoom === "fit" && times && arranged ? spanShown(times, arranged) : null), [zoom, times, arranged]);
  // On the span's minutes, so a sort or a fold that keeps the dates keeps the scroll.
  const [spanFirst, spanLast] = [span?.first ?? Number.NaN, span?.last ?? Number.NaN];
  const fitted = useMemo(() => fitZoom(spanFirst, spanLast, paneWidth), [spanFirst, spanLast, paneWidth]);
  const level = zoom === "fit" ? (fitted?.zoom ?? ZOOMS[3]) : (ZOOMS[zoom] ?? ZOOMS[ZOOMS.length - 1]);
  const origin = useMemo(() => {
    const first = Math.min(times?.first ?? Number.POSITIVE_INFINITY, ...dataDates.values());
    const start = Number.isFinite(first) ? unitStart(first - 30 * DAY, "month") : minutesOf("2026-01-01 00:00");
    // A wide margin can start before a month ahead of the first date.
    return fitted ? Math.min(start, fitted.left) : start;
  }, [times, dataDates, fitted]);
  const scale: Scale = useMemo(() => scaleOf(origin, level ?? { pxPerDay: 8, top: "month", bottom: "week" }), [origin, level]);
  const last = Math.max(times?.last ?? origin, ...dataDates.values());
  const width = Math.max(xOf(scale, last + 60 * DAY), fitted ? xOf(scale, fitted.left) + paneWidth : 0, 0);

  // Keeps what draw() reads current without re-subscribing.
  const latest = useRef({ scale, times, dataDates, today, level, links, lines, path });
  // What the last frame drew, for hovering and clicking.
  const drawn = useRef<Drawn>({ lines: [], arrows: [], bars: [] });
  // The moment in the middle of the chart, as last scrolled or drawn.
  const seen = useRef<Held | null>(null);
  const remember = useCallback(() => {
    const element = body.current;
    if (!element) return;
    const x = element.clientWidth / 2;
    seen.current = { x, moment: minutesAt(latest.current.scale, element.scrollLeft + x) };
  }, []);
  useLayoutEffect(() => {
    latest.current = { scale, times, dataDates, today, level, links, lines, path };
  }, [scale, times, dataDates, today, level, links, lines, path]);

  /** Paints both canvases from the grid's latest view. */
  const draw = useCallback(() => {
    const now = view.current();
    const element = body.current;
    const { scale, times, dataDates, today, level, links, lines, path } = latest.current;
    if (!element || !canvas.current || !head.current || !root.current || !colours.current || !level) return;
    // The middle as drawn, which a resize changes without a scroll.
    remember();
    // Rows line up with the grid's: the timescale fills the height above its body.
    const headHeight = now ? Math.max(now.bodyTop - root.current.getBoundingClientRect().top, 24) : 44;
    if (root.current.style.getPropertyValue("--gantt-head") !== `${headHeight}px`) {
      root.current.style.setProperty("--gantt-head", `${headHeight}px`);
    }
    const width = element.clientWidth;
    const height = element.clientHeight;
    const left = element.scrollLeft;
    const tiers = {
      top: ticks(scale, level.top, "top", left, left + width),
      bottom: ticks(scale, level.bottom, "bottom", left, left + width),
    };
    const headCtx = fitCanvas(head.current, width, headHeight).ctx;
    if (headCtx) drawHeader(headCtx, colours.current, tiers, left, width, headHeight);
    const { ctx } = fitCanvas(canvas.current, width, height);
    if (!ctx || !now || !times) return;
    const active = now.arranged.lines[now.active];
    const selected = active === undefined || active < 0 ? null : (now.rows.id[active] ?? null);
    drawn.current = drawBody(ctx, colours.current, {
      rows: now.rows,
      arranged: now.arranged,
      times,
      scale,
      top: now.top,
      left,
      width,
      height,
      rowHeight: now.rowHeight,
      active: now.active,
      dataDates,
      today: today ? Date.now() / 60_000 - new Date().getTimezoneOffset() : null,
      grid: tiers.bottom,
      // Read from the arrangement being drawn, which can be newer than the last render.
      logic: {
        links,
        index: links ? linksByActivity(links) : null,
        lineOf: lineIndex(now.rows.id, now.arranged.lines),
        mode: lines,
        selected,
        path,
      },
    });
  }, [view, remember]);

  const frame = useRef(0);
  const later = useCallback(() => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(draw);
  }, [draw]);

  // The grid's view: drawn at once, in the frame the grid draws in, so the
  // two never show different rows. New rows re-render, for the axis.
  useEffect(
    () =>
      view.subscribe((next) => {
        setShown((was) => (was?.rows === next.rows && was?.arranged === next.arranged ? was : next));
        // What was under a still pointer has scrolled away.
        setHover((was) => (was && was.top !== next.top ? null : was));
        draw();
      }),
    [view, draw],
  );

  // Colours follow the OS theme; size follows the pane.
  useLayoutEffect(() => {
    const element = root.current;
    if (!element) return;
    colours.current = coloursOf(element);
    const resized = () => {
      setPaneWidth(body.current?.clientWidth ?? 0);
      later();
    };
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resized);
    observer?.observe(element);
    const scheme = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
    const recolour = () => {
      colours.current = coloursOf(element);
      later();
    };
    scheme?.addEventListener("change", recolour);
    return () => {
      observer?.disconnect();
      scheme?.removeEventListener("change", recolour);
      cancelAnimationFrame(frame.current);
    };
  }, [later, ready]);

  // Anything else that changes the picture: zoom, the axis, today's line.
  useEffect(later, [later, scale, times, dataDates, today, links, lines, path]);

  // When the axis changes, by a zoom from any source or a new earliest date,
  // the moment under the pointer (for a wheel zoom) or at the middle stays
  // where it was. The moment is read before the change, since a narrower
  // axis can clamp the scroll before this runs, and the scroll is set once
  // the new width is in the page.
  const pending = useRef<Held | null>(null);
  const shownScale = useRef<Scale | null>(null);
  useLayoutEffect(() => {
    const element = body.current;
    const was = shownScale.current;
    shownScale.current = scale;
    const held = pending.current ?? seen.current;
    pending.current = null;
    if (!element || !was || !held || (was.origin === scale.origin && was.pxPerMinute === scale.pxPerMinute)) return;
    element.scrollLeft = Math.max(0, xOf(scale, held.moment) - held.x);
    remember();
  }, [scale, ready, remember]);

  // Fitted, the fitted dates start at the left edge: on opening, and when
  // the rows shown or the width change.
  useLayoutEffect(() => {
    const element = body.current;
    if (zoom !== "fit" || !fitted || !element) return;
    element.scrollLeft = Math.max(0, xOf(scale, fitted.left));
    remember();
  }, [zoom, fitted, scale, remember]);

  const fittedPx = fitted?.zoom.pxPerDay;
  useEffect(() => {
    if (fittedPx !== undefined) onFitted?.(fittedPx);
  }, [fittedPx, onFitted]);

  const zoomTo = useCallback(
    (by: 1 | -1, anchorX?: number) => {
      const target = stepZoom(zoom, fittedPx ?? level?.pxPerDay ?? 1, by);
      const element = body.current;
      if (target === zoom || !element) return;
      if (anchorX !== undefined) {
        pending.current = { x: anchorX, moment: minutesAt(latest.current.scale, element.scrollLeft + anchorX) };
      }
      onZoom(target);
    },
    [zoom, onZoom, fittedPx, level],
  );

  // Wheels: Ctrl zooms; up and down scroll the grid, so the two move together.
  useEffect(() => {
    const element = body.current;
    if (!element) return;
    // A pinch sends many small wheel events: a zoom step takes a notch's worth.
    let pinched = 0;
    const onWheel = (event: WheelEvent) => {
      const now = view.current();
      const moved = wheelPixels(event, now?.rowHeight ?? 22, element.clientHeight);
      if (event.ctrlKey || event.metaKey) {
        event.preventDefault();
        pinched += moved.y;
        if (Math.abs(pinched) < ZOOM_STEP) return;
        const box = element.getBoundingClientRect();
        zoomTo(pinched < 0 ? 1 : -1, event.clientX - box.left);
        pinched = 0;
        return;
      }
      if (Math.abs(moved.y) > Math.abs(moved.x) && !event.shiftKey) {
        event.preventDefault();
        view.scrollBy(moved.y);
      }
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [view, zoomTo, ready]);

  // An activity revealed, from an explain step or an edge arrow, has its bar
  // brought into view across time, as the grid brings its row into view.
  useEffect(
    () =>
      view.onReveal((id) => {
        const element = body.current;
        const now = view.current();
        const { scale, times, dataDates } = latest.current;
        if (!element || !now || !times) return;
        const i = now.rows.id.indexOf(id);
        if (i < 0) return;
        const ends = endsOf(barOf(now.rows, times, i, scale, dataDates.get(now.rows.project[i] ?? -1)));
        if (!ends) return;
        const margin = 40;
        const { scrollLeft, clientWidth } = element;
        if (ends.left < scrollLeft + margin || ends.left > scrollLeft + clientWidth - margin) {
          element.scrollLeft = Math.max(0, ends.left - margin);
        }
      }),
    [view, ready],
  );

  // Opens on the data date, a week in from the left.
  const placed = useRef(false);
  useEffect(() => {
    const element = body.current;
    if (placed.current || !element || !times) return;
    const [date] = dataDates.values();
    const start = date ?? times.first;
    if (!Number.isFinite(start)) return;
    placed.current = true;
    element.scrollLeft = Math.max(0, xOf(scale, start - 7 * DAY));
  }, [times, dataDates, scale, ready]);

  // A link being dragged from a bar's end, and whether the pointer moved, so
  // the click that ends a drag doesn't also select.
  const [drag, setDrag] = useState<Dragging | null>(null);
  const dragging = useRef<Dragging | null>(null);
  // A press on a bar's end, until it moves far enough to be a drag.
  const pressed = useRef<Dragging | null>(null);
  const swallowClick = useRef(false);
  const place = (event: { clientX: number; clientY: number }) => {
    const box = body.current?.getBoundingClientRect();
    return box ? { x: event.clientX - box.left, y: event.clientY - box.top } : null;
  };
  const endDrag = () => {
    pressed.current = null;
    dragging.current = null;
    setDrag(null);
  };
  // Escape cancels a drag, before anything else hears it.
  useEffect(() => {
    if (!drag) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      swallowClick.current = true;
      endDrag();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [drag]);

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    swallowClick.current = false;
    if (!onLink || event.button !== 0) return;
    const at = place(event);
    const from = at ? edgeAt(drawn.current.bars, at.x, at.y) : null;
    if (!at || !from) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const body = event.currentTarget;
    pressed.current = { from, x: at.x, y: at.y, to: null, bars: drawn.current.bars, top: body.offsetTop, width: body.clientWidth, height: body.clientHeight };
  };
  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const was = dragging.current ?? pressed.current;
    if (!was) return;
    const at = place(event);
    if (!at) return;
    if (!dragging.current) {
      if (Math.hypot(at.x - was.x, at.y - was.y) < DRAG_START) return;
      // Far enough: a drag, and the click that ends it won't also select.
      swallowClick.current = true;
      setHover(null);
    }
    dragging.current = { ...was, ...at, to: dropAt(drawn.current.bars, at.x, at.y, was.from.id), bars: drawn.current.bars };
    setDrag(dragging.current);
  };
  const onPointerUp = () => {
    const was = dragging.current;
    // A press that never became a drag ends here too, so moving on can't start one.
    endDrag();
    if (!was) return;
    if (was.to && onLink) onLink({ predecessor: was.from.id, successor: was.to.id, type: typeOf(was.from.end, was.to.end) });
  };

  /** What's under the pointer: an edge arrow, a line, or an activity's row. */
  const hit = (event: React.MouseEvent<HTMLDivElement>): Hit | null => {
    const now = view.current();
    const element = body.current;
    if (!now || !element) return null;
    const box = element.getBoundingClientRect();
    const x = event.clientX - box.left;
    const y = event.clientY - box.top;
    const i = now.arranged.lines[Math.floor((y + now.top) / now.rowHeight)];
    return { now, ...pick(drawn.current, x, y, i === undefined || i < 0 ? undefined : i) };
  };

  const onMove = (event: React.MouseEvent<HTMLDivElement>) => {
    if (dragging.current) return;
    const found = hit(event);
    const element = body.current;
    let text: string[] | null = null;
    if (found?.arrow) {
      const line = lineIndex(found.now.rows.id, found.now.arranged.lines).get(found.arrow.to);
      const i = line === undefined ? undefined : found.now.arranged.lines[line];
      text = [`Go to ${i === undefined ? "the other activity" : (found.now.rows.code[i] ?? "")}`];
    } else if (found?.link !== undefined && links) {
      text = linkText(
        found.now.rows,
        links,
        found.link,
        lineIndex(found.now.rows.id, found.now.arranged.lines),
        found.now.arranged.lines,
      );
    } else if (found?.activity !== undefined) {
      text = hoverText(found.now.rows, found.activity);
    }
    // A bar's end, where a link can be dragged from, takes a crosshair.
    const at = onLink ? place(event) : null;
    const end = at ? edgeAt(drawn.current.bars, at.x, at.y) : null;
    if (element) element.style.cursor = end ? "crosshair" : found?.arrow || found?.activity !== undefined ? "pointer" : "";
    if (!found || !text || !root.current) {
      setHover(null);
      return;
    }
    // Placed in the chart's frame, which doesn't scroll.
    const frame = root.current.getBoundingClientRect();
    const x = event.clientX - frame.left;
    const y = event.clientY - frame.top;
    // Opens away from the nearer edges, so it isn't cut off.
    setHover({ x, y, top: found.now.top, flip: [x > frame.width / 2, y > frame.height / 2], lines: text });
  };

  // A click on an edge arrow goes to the other end; on a bar, selects it.
  const onClick = (event: React.MouseEvent<HTMLDivElement>) => {
    if (swallowClick.current) {
      swallowClick.current = false;
      return;
    }
    const found = hit(event);
    if (found?.arrow) view.reveal(found.arrow.to);
    else if (found?.activity !== undefined) view.reveal(found.now.rows.id[found.activity] ?? -1);
    // A click on a line selects the row it's in, as a click beside it would.
    else if (found?.row !== undefined) view.reveal(found.now.rows.id[found.row] ?? -1);
  };

  if (!shown) return <p className="muted gantt-empty">Gantt chart</p>;

  return (
    <div className="gantt" ref={root}>
      <canvas ref={head} className="gantt-head" aria-hidden="true" />
      <div
        ref={body}
        className="gantt-body"
        aria-label="Gantt chart: bars for the activities the grid shows"
        role="img"
        tabIndex={0}
        // The scale drawn, for tests in a browser: where a date falls is (minutes - origin) * px a day / 1440.
        data-zoom={zoom}
        data-origin={scale.origin}
        data-px-per-day={scale.pxPerMinute * 1440}
        onScroll={() => {
          remember();
          later();
        }}
        onMouseMove={onMove}
        onClick={onClick}
        onMouseLeave={() => setHover(null)}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => {
          // No click follows a cancelled pointer, so nothing is left to swallow.
          swallowClick.current = false;
          endDrag();
        }}
      >
        <canvas ref={canvas} className="gantt-canvas" />
        <div className="gantt-extent" style={{ width }} />
      </div>
      {drag && <DragLine drag={drag} rows={shown.rows} />}
      {hover && (
        <div
          className="gantt-tip"
          role="tooltip"
          style={{
            ...(hover.flip[0] ? { right: `calc(100% - ${hover.x - 12}px)` } : { left: hover.x + 12 }),
            ...(hover.flip[1] ? { bottom: `calc(100% - ${hover.y - 12}px)` } : { top: hover.y + 12 }),
          }}
        >
          {hover.lines.map((line) => (
            <span key={line}>{line}</span>
          ))}
        </div>
      )}
    </div>
  );
}

/** The link being dragged, over the chart: a line from the end it left, and the bar it would drop on outlined. */
function DragLine({ drag, rows }: { drag: Dragging; rows: Rows }) {
  const { from, to, bars, top } = drag;
  const [x, y] = to ? [to.x, to.y] : [drag.x, drag.y];
  const bend = Math.max(24, Math.abs(x - from.x) / 2);
  const out = from.end === "finish" ? 1 : -1;
  const back = to ? (to.end === "start" ? -1 : 1) : -out;
  const target = to ? bars.find((bar) => bar.id === to.id) : undefined;
  const words = dragWords(rows, drag);
  return (
    <>
      <svg className="gantt-drag" style={{ top }} aria-hidden="true">
        <path d={`M${from.x} ${from.y} C ${from.x + out * bend} ${from.y}, ${x + back * bend} ${y}, ${x} ${y}`} />
        <circle className="from" cx={from.x} cy={from.y} r={4.5} />
        {to && <circle className="to" cx={to.x} cy={to.y} r={4.5} />}
        {target && <rect x={target.left - 3} y={target.top + 2} width={target.right - target.left + 6} height={target.bottom - target.top - 4} rx={3} />}
      </svg>
      <div
        className="gantt-tip gantt-drag-tip"
        role="status"
        style={{
          ...(drag.x > drag.width / 2 ? { right: `calc(100% - ${drag.x - 14}px)` } : { left: drag.x + 14 }),
          ...(drag.y > drag.height / 2 ? { bottom: `calc(100% - ${top + drag.y - 14}px)` } : { top: top + drag.y + 14 }),
        }}
      >
        {words.map((line) => (
          <span key={line}>{line}</span>
        ))}
      </div>
    </>
  );
}
