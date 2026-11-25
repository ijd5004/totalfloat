// The print view: what the grid shows, with the Gantt beside it, as vector
// pages to print or save as PDF through the browser. Each page has a title
// block, the column and timescale headers, whole rows only, a legend and its
// number. Nothing is calculated for it: every date and float is the rows'.

import { useEffect, useMemo, useRef, useState } from "react";

import type { Rows, Summary } from "../api";
import { readable } from "../explain/narrate";
import { PAPER_COLOURS, barOf } from "../gantt/draw";
import { type Unit, labelOf, minutesOf, nextUnit, timesOf, unitStart } from "../gantt/time";
import { COLUMN_BY_ID, type ColumnId } from "../grid/columns";
import type { Band } from "../grid/model";
import {
  HEADER_HEIGHT,
  MARGIN,
  type Orientation,
  PAPER,
  type PaperSize,
  ROW_HEIGHT,
  TITLE_HEIGHT,
  columnWidths,
  cssPageSize,
  dayOf,
  fitText,
  pageSize,
  paginate,
  rowsPerPage,
  tiersFor,
} from "./pages";
import { dateText } from "../dates";

const FONT = 7;
const COLOURS = PAPER_COLOURS;

/** A column printed: which, and its width on screen. */
export interface PrintColumn {
  id: ColumnId;
  width: number;
}

/** Today, as a date input's value, from this computer's clock: only a default for the print date the person can change. */
function today(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** A date for a date input: the day part of a server date. */
function inputDay(minutes: number): string {
  if (!Number.isFinite(minutes)) return "";
  return new Date(minutes * 60_000).toISOString().slice(0, 10);
}

export function PrintView({
  rows,
  lines,
  bands,
  columns,
  split,
  summary,
  onClose,
}: {
  rows: Rows;
  /** The grid's lines in order: an activity's index, or a band as -(its index + 1). */
  lines: Int32Array;
  bands: readonly Band[];
  columns: readonly PrintColumn[];
  /** The grid's share of the width on screen, which a fitted page keeps. */
  split: number;
  summary: Summary | null;
  onClose: () => void;
}) {
  // The range the printed lines' dates span, for the timescale's default:
  // what's filtered out doesn't stretch it. With no dates, the data date.
  const span = useMemo(() => {
    let first = Number.POSITIVE_INFINITY;
    let last = Number.NEGATIVE_INFINITY;
    const take = (starts: (string | null | undefined)[], finishes: (string | null | undefined)[]) => {
      for (const d of starts) {
        const m = minutesOf(d);
        if (m < first) first = m;
      }
      for (const d of finishes) {
        const m = minutesOf(d);
        if (m > last) last = m;
      }
    };
    for (const value of lines) {
      if (value < 0) {
        const band = bands[-value - 1];
        if (band) take([band.start], [band.finish]);
      } else {
        take([rows.actual_start[value], rows.early_start[value]], [rows.actual_finish[value], rows.early_finish[value], rows.late_finish[value]]);
      }
    }
    // With one end known, as for a lone milestone, a month from it; with
    // neither, a month from the data date.
    const month = 30 * 24 * 60;
    if (Number.isFinite(first) && !Number.isFinite(last)) last = first + month;
    else if (!Number.isFinite(first) && Number.isFinite(last)) first = last - month;
    else if (!Number.isFinite(first) || !Number.isFinite(last)) {
      const dataDate = minutesOf(summary?.inspect.projects[0]?.data_date);
      first = Number.isFinite(dataDate) ? dataDate : 0;
      last = first + month;
    }
    return { first, last };
  }, [rows, lines, bands, summary]);

  const [size, setSize] = useState<PaperSize>("letter");
  const [orientation, setOrientation] = useState<Orientation>("landscape");
  const [fit, setFit] = useState(true);
  const [from, setFrom] = useState(() => inputDay(span.first));
  const [to, setTo] = useState(() => inputDay(span.last + 24 * 60));
  const [printed, setPrinted] = useState(today);
  const setup = useRef<HTMLElement>(null);
  const root = useRef<HTMLDivElement>(null);
  // Where focus was as it opened, to give it back on closing.
  const [returnTo] = useState(() => document.activeElement);

  // While it's open: the page prints only it, focus stays in it, and Escape
  // anywhere closes it.
  useEffect(() => {
    document.body.classList.add("printing");
    setup.current?.querySelector<HTMLElement>("select")?.focus();
    const keepIn = (event: FocusEvent) => {
      if (root.current && event.target instanceof Node && !root.current.contains(event.target)) {
        setup.current?.querySelector<HTMLElement>("select")?.focus();
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    document.addEventListener("focusin", keepIn);
    window.addEventListener("keydown", escape);
    return () => {
      document.body.classList.remove("printing");
      document.removeEventListener("focusin", keepIn);
      window.removeEventListener("keydown", escape);
      const back =
        returnTo instanceof HTMLElement && returnTo !== document.body && returnTo.isConnected
          ? returnTo
          : document.querySelector<HTMLElement>(".grid-body");
      back?.focus();
    };
  }, [onClose, returnTo]);

  // Text is measured as it'll print, where the browser can; else estimated.
  const measure = useMemo(() => {
    const context = typeof document === "undefined" ? null : document.createElement("canvas").getContext?.("2d");
    if (!context) return undefined;
    return (text: string, size: number, bold = false) => {
      context.font = `${bold ? "bold " : ""}${size}px Helvetica, Arial, sans-serif`;
      return context.measureText(text).width;
    };
  }, []);
  const cut = (text: string, width: number, size: number, bold = false) =>
    fitText(text, width, size, measure ? (t: string) => measure(t, size, bold) : undefined);

  // The paper's size when printed, as an `@page` rule. The server's
  // Content-Security-Policy allows no inline <style>, so it's a constructed
  // style sheet, which the policy leaves alone.
  const pageCss = `@page { size: ${cssPageSize(size, orientation)}; margin: 0; }`;
  useEffect(() => {
    if (typeof CSSStyleSheet === "undefined" || !("adoptedStyleSheets" in document)) return;
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(pageCss);
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
    return () => {
      document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== sheet);
    };
  }, [pageCss]);

  const page = pageSize(size, orientation);
  const content = page.width - 2 * MARGIN;
  const fitted = columnWidths(
    columns.map((c) => c.width),
    content,
    fit,
    Math.min(0.85, Math.max(0.15, split)),
  );
  const widths = fitted.widths;
  // Each column's left edge, once.
  const offsets = widths.map((_, c) => MARGIN + widths.slice(0, c).reduce((a, b) => a + b, 0));
  // Those that fit; the rest are named in the setup, to hide in the grid if wanted.
  const printedColumns = columns.slice(0, fitted.kept);
  const leftOff = columns.slice(fitted.kept).map((c) => COLUMN_BY_ID.get(c.id)?.title ?? c.id);
  const gridWidth = widths.reduce((a, b) => a + b, 0);
  const ganttLeft = MARGIN + gridWidth;
  const ganttWidth = page.width - MARGIN - ganttLeft;
  const asked = [minutesOf(dayOf(from) ?? ""), minutesOf(dayOf(to) ?? "")] as const;
  // A range that ends before it starts, or isn't given, is the dates' own.
  const backwards = Number.isFinite(asked[0]) && Number.isFinite(asked[1]) && asked[1] <= asked[0];
  const start = !backwards && Number.isFinite(asked[0]) ? asked[0] : span.first;
  const end = !backwards && Number.isFinite(asked[1]) ? asked[1] : span.last;
  const range = Math.max(end - start, 24 * 60);
  const xOf = (minutes: number) => ganttLeft + ((minutes - start) / range) * ganttWidth;
  const tiers = tiersFor(range / (24 * 60));
  const pages = paginate([...lines], rowsPerPage(page.height));
  const project = summary?.inspect.projects[0];
  const dataDate = minutesOf(project?.data_date);
  const bodyTop = MARGIN + TITLE_HEIGHT + HEADER_HEIGHT;

  /** The tier's cells across the Gantt. */
  const cells = (unit: Unit, tier: "top" | "bottom") => {
    const out: { x: number; width: number; label: string }[] = [];
    let at = unitStart(start, unit);
    for (let n = 0; n < 1000 && at < start + range; n++) {
      const next = nextUnit(at, unit);
      const x = Math.max(xOf(at), ganttLeft);
      const right = Math.min(xOf(next), ganttLeft + ganttWidth);
      if (right > x) out.push({ x, width: right - x, label: labelOf(at, unit, tier) });
      at = next;
    }
    return out;
  };
  const top = cells(tiers.top, "top");
  const bottom = cells(tiers.bottom, "bottom");

  /** A span on paper, clipped to the Gantt, or null when it's outside it. */
  const clip = (a: number, b: number): [number, number] | null => {
    const from = Math.max(a, ganttLeft);
    const until = Math.min(b, ganttLeft + ganttWidth);
    return Number.isFinite(from) && Number.isFinite(until) && until >= from ? [from, until] : null;
  };
  // The screen's scale, set to paper: the Gantt's left edge is the range's start.
  const scale = { origin: start, pxPerMinute: ganttWidth / range };
  const times = timesOf(rows);

  /** A bar's ends on paper, clipped to the Gantt, or null when it's outside it. */
  const ends = (from: number, until: number): [number, number] | null => {
    if (!Number.isFinite(from) || !Number.isFinite(until)) return null;
    const a = Math.max(xOf(from), ganttLeft);
    const b = Math.min(xOf(until), ganttLeft + ganttWidth);
    return b >= a ? [a, b] : null;
  };

  const title = project?.name || project?.short_name || "Schedule";
  const heading = `${title}${project?.name && project.short_name ? ` (${project.short_name})` : ""}`;
  // The title has the line to itself but for the print date, and a gap.
  const dated = `Printed ${dateText(printed, false)}`;
  const titleRoom = content - (measure ? measure(dated, 8) : dated.length * 8 * 0.52) - 24;
  const where = summary?.head ? `${summary.branch ?? "point"} @ ${summary.head.slice(0, 10)}` : "plain file";

  return (
    <div className="print-view" ref={root} role="dialog" aria-modal="true" aria-label="Print">
      <section
        ref={setup}
        className="print-setup"
        aria-label="Page setup"
      >
        <h2>Print</h2>
        <label>
          <span>Paper</span>
          <select value={size} onChange={(event) => setSize(event.target.value as PaperSize)}>
            {(Object.keys(PAPER) as PaperSize[]).map((key) => (
              <option key={key} value={key}>
                {PAPER[key].label}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>Orientation</span>
          <select value={orientation} onChange={(event) => setOrientation(event.target.value as Orientation)}>
            <option value="landscape">Landscape</option>
            <option value="portrait">Portrait</option>
          </select>
        </label>
        <label className="print-check">
          <input type="checkbox" checked={fit} onChange={(event) => setFit(event.target.checked)} />
          <span>Fit the grid to the page width</span>
        </label>
        <label>
          <span>Timescale from</span>
          <input type="date" value={from} onChange={(event) => setFrom(event.target.value)} />
        </label>
        <label>
          <span>to</span>
          <input type="date" value={to} onChange={(event) => setTo(event.target.value)} />
        </label>
        <label>
          <span>Print date</span>
          <input type="date" value={printed} onChange={(event) => setPrinted(event.target.value)} />
        </label>
        <p className="muted">
          {pages.length} {pages.length === 1 ? "page" : "pages"}. Save as PDF from the browser's print dialog.
        </p>
        {backwards && (
          <p className="print-left-off" role="alert">
            The timescale ends before it starts, so it spans the dates printed.
          </p>
        )}
        {leftOff.length > 0 && (
          <p className="print-left-off">
            Left off to stay readable: {leftOff.join(", ")}. Hide columns in the grid, use wider paper or landscape, or
            turn off fitting, to choose what prints.
          </p>
        )}
        <div className="print-buttons">
          <button type="button" className="primary" onClick={() => window.print()}>
            Print…
          </button>
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>
      </section>
      <div className="print-pages" aria-label="Pages">
        {pages.map((pageLines, p) => (
          <svg
            key={p}
            className="print-page"
            role="group"
            aria-label={`Page ${p + 1} of ${pages.length}`}
            width={`${page.width}pt`}
            height={`${page.height}pt`}
            viewBox={`0 0 ${page.width} ${page.height}`}
            fontFamily="Helvetica, Arial, sans-serif"
            fontSize={FONT}
          >
            <rect width={page.width} height={page.height} fill="#fff" />
            <defs>
              {/* Bars and milestones stay in the Gantt. */}
              <clipPath id={`print-${p}-gantt`}>
                <rect x={ganttLeft} y={MARGIN + TITLE_HEIGHT} width={Math.max(ganttWidth, 0)} height={page.height} />
              </clipPath>
              {/* Each column's text stays in its column. */}
              {printedColumns.map((column, c) => (
                <clipPath key={column.id} id={`print-${p}-c${c}`}>
                  <rect x={offsets[c] ?? MARGIN} y={MARGIN + TITLE_HEIGHT} width={Math.max((widths[c] ?? 0) - 1, 0)} height={page.height} />
                </clipPath>
              ))}
            </defs>
            {/* The title block. */}
            <g className="print-title">
              <text x={MARGIN} y={MARGIN + 13} fontSize={12} fontWeight={700}>
                {cut(heading, titleRoom, 12, true)}
              </text>
              <text x={MARGIN} y={MARGIN + 27} fill={COLOURS.muted} fontSize={8}>
                {`Data date ${project?.data_date ? readable(project.data_date, false) : "none"} · Finish ${
                  project?.finish ? readable(project.finish, false) : "none"
                } · ${where}`}
              </text>
              <text x={page.width - MARGIN} y={MARGIN + 13} textAnchor="end" fontSize={8}>
                {dated}
              </text>
              <line x1={MARGIN} x2={page.width - MARGIN} y1={MARGIN + TITLE_HEIGHT - 6} y2={MARGIN + TITLE_HEIGHT - 6} stroke={COLOURS.band} />
            </g>
            {/* Headers, on every page. */}
            <g className="print-header">
              {printedColumns.map((column, c) => {
                const x = offsets[c] ?? MARGIN;
                const w = widths[c] ?? 0;
                return (
                  <g key={column.id} clipPath={`url(#print-${p}-c${c})`}>
                    <rect x={x} y={MARGIN + TITLE_HEIGHT} width={w} height={HEADER_HEIGHT} fill="#eef1f4" stroke={COLOURS.line} strokeWidth={0.5} />
                    <text x={x + 2} y={MARGIN + TITLE_HEIGHT + HEADER_HEIGHT - 6} fontWeight={700}>
                      {cut(COLUMN_BY_ID.get(column.id)?.title ?? column.id, w - 4, FONT, true)}
                    </text>
                  </g>
                );
              })}
              {[top, bottom].map((tier, t) =>
                tier.map((cell) => (
                  <g key={`${t}-${cell.x}`}>
                    <rect
                      x={cell.x}
                      y={MARGIN + TITLE_HEIGHT + (t * HEADER_HEIGHT) / 2}
                      width={cell.width}
                      height={HEADER_HEIGHT / 2}
                      fill="#eef1f4"
                      stroke={COLOURS.line}
                      strokeWidth={0.5}
                    />
                    <text x={cell.x + 2} y={MARGIN + TITLE_HEIGHT + (t * HEADER_HEIGHT) / 2 + 9} fontSize={6}>
                      {cut(cell.label, cell.width - 3, 6)}
                    </text>
                  </g>
                )),
              )}
            </g>
            {/* The rows: whole rows only. */}
            <g className="print-body">
              {bottom.map((cell) => (
                <line
                  key={`grid-${cell.x}`}
                  x1={cell.x}
                  x2={cell.x}
                  y1={bodyTop}
                  y2={bodyTop + pageLines.length * ROW_HEIGHT}
                  stroke={COLOURS.line}
                  strokeWidth={0.4}
                />
              ))}
              {pageLines.map((value, r) => {
                const y = bodyTop + r * ROW_HEIGHT;
                const textY = y + ROW_HEIGHT - 3.5;
                if (value < 0) {
                  const band = bands[-value - 1];
                  if (!band) return null;
                  const bar = ends(minutesOf(band.start), minutesOf(band.finish));
                  return (
                    <g key={`band-${band.path}`} className="print-row print-band">
                      <rect x={MARGIN} y={y} width={gridWidth} height={ROW_HEIGHT} fill="#eef1f4" />
                      <text x={MARGIN + 2 + band.depth * 6} y={textY} fontWeight={700}>
                        {cut(`${band.name ? `${band.name} ${band.title}` : band.title} (${band.count})`, gridWidth - 4 - band.depth * 6, FONT, true)}
                      </text>
                      {bar && (
                        <rect
                          x={bar[0]}
                          y={y + 4}
                          width={Math.max(bar[1] - bar[0], 1)}
                          height={3}
                          fill={COLOURS.band}
                          clipPath={`url(#print-${p}-gantt)`}
                        />
                      )}
                    </g>
                  );
                }
                const i = value;
                // The same bar the screen draws, from the rows' dates, on paper's scale.
                const bar = barOf(rows, times, i, scale, Number.isFinite(dataDate) ? dataDate : undefined);
                const colour = bar.critical ? COLOURS.critical : COLOURS.other;
                const shift = (pair: [number, number] | undefined) => (pair ? clip(pair[0] + ganttLeft, pair[1] + ganttLeft) : null);
                const actual = shift(bar.actual);
                const remaining = shift(bar.remaining);
                const float = shift(bar.float);
                const at = bar.at === undefined ? null : bar.at + ganttLeft;
                return (
                  <g key={`a-${rows.id[i] ?? i}`} className="print-row">
                    <line x1={MARGIN} x2={page.width - MARGIN} y1={y + ROW_HEIGHT} y2={y + ROW_HEIGHT} stroke={COLOURS.line} strokeWidth={0.3} />
                    {printedColumns.map((column, c) => {
                      const x = offsets[c] ?? MARGIN;
                      const w = widths[c] ?? 0;
                      const spec = COLUMN_BY_ID.get(column.id);
                      const right = spec?.align === "right";
                      return (
                        <text
                          key={column.id}
                          x={right ? x + w - 2 : x + 2}
                          y={textY}
                          textAnchor={right ? "end" : "start"}
                          clipPath={`url(#print-${p}-c${c})`}
                        >
                          {cut(spec?.text(rows, i) ?? "", w - 4, FONT)}
                        </text>
                      );
                    })}
                    <g clipPath={`url(#print-${p}-gantt)`}>
                    {float && <line x1={float[0]} x2={float[1]} y1={y + 6} y2={y + 6} stroke={COLOURS.float} strokeWidth={0.6} />}
                    {remaining && <rect x={remaining[0]} y={y + 3} width={Math.max(remaining[1] - remaining[0], 0.8)} height={6} fill={colour} />}
                    {actual && <rect x={actual[0]} y={y + 3} width={Math.max(actual[1] - actual[0], 0.8)} height={6} fill={COLOURS.actual} />}
                    {bar.kind === "milestone" && at !== null && at >= ganttLeft && at <= ganttLeft + ganttWidth && (
                      <path d={`M ${at} ${y + 2} l 4 4 l -4 4 l -4 -4 z`} fill={colour} />
                    )}
                    </g>
                  </g>
                );
              })}
              {Number.isFinite(dataDate) && dataDate >= start && dataDate <= start + range && (
                <line
                  x1={xOf(dataDate)}
                  x2={xOf(dataDate)}
                  y1={bodyTop}
                  y2={bodyTop + pageLines.length * ROW_HEIGHT}
                  stroke={COLOURS.dataDate}
                  strokeWidth={0.8}
                  strokeDasharray="3 2"
                />
              )}
            </g>
            {/* The legend and the page's number. */}
            <g className="print-footer" transform={`translate(${MARGIN} ${page.height - MARGIN - 6})`}>
              {[
                { label: "Critical", colour: COLOURS.critical, kind: "bar" },
                { label: "Other work", colour: COLOURS.other, kind: "bar" },
                { label: "Work done", colour: COLOURS.actual, kind: "bar" },
                { label: "Milestone", colour: COLOURS.other, kind: "milestone" },
                { label: "Total float", colour: COLOURS.float, kind: "line" },
                { label: "Data date", colour: COLOURS.dataDate, kind: "dash" },
              ].map((item, k) => (
                <g key={item.label} transform={`translate(${k * 70} 0)`}>
                  {item.kind === "bar" && <rect x={0} y={-6} width={12} height={6} fill={item.colour} />}
                  {item.kind === "milestone" && <path d="M 6 -8 l 4 4 l -4 4 l -4 -4 z" fill={item.colour} />}
                  {item.kind === "line" && <line x1={0} x2={12} y1={-3} y2={-3} stroke={item.colour} strokeWidth={0.6} />}
                  {item.kind === "dash" && <line x1={6} x2={6} y1={-8} y2={1} stroke={item.colour} strokeDasharray="3 2" />}
                  <text x={16} y={-1}>
                    {item.label}
                  </text>
                </g>
              ))}
              <text x={page.width - 2 * MARGIN} y={-1} textAnchor="end">
                {`Page ${p + 1} of ${pages.length}`}
              </text>
            </g>
          </svg>
        ))}
      </div>
    </div>
  );
}
