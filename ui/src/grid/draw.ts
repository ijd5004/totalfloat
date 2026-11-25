// Draws the rows on screen onto the grid's canvas. Only visible rows and
// columns are drawn; each column's text is clipped to the column rather than
// measured, which keeps a frame to a few milliseconds at any size.

import type { Rows } from "../api";
import { fontOf, themeOf } from "../canvas";
import { type CellKey, type CellProblem, cellKey } from "../edit/cells";
import type { Column } from "./columns";
import type { Arranged } from "./model";

export interface Palette {
  bg: string;
  band: string;
  chosen: string;
  active: string;
  line: string;
  text: string;
  muted: string;
  critical: string;
  typed: string;
  refused: string;
  font: string;
  bold: string;
}

/** The theme's colours, read from the CSS variables on `element`. */
export function palette(element: Element): Palette {
  const v = themeOf(element);
  return {
    bg: v("--bg", "#fff"),
    band: v("--bg-raised", "#f6f7f9"),
    chosen: v("--bg-chosen", "#dbe7fe"),
    active: v("--accent", "#1d4ed8"),
    line: v("--line", "#d9dde3"),
    text: v("--text", "#111827"),
    muted: v("--muted", "#6b7280"),
    critical: v("--bad", "#b91c1c"),
    typed: v("--accent", "#1d4ed8"),
    refused: v("--bad-bg", "#fde8e8"),
    font: fontOf(v),
    bold: fontOf(v, "600"),
  };
}

export interface View {
  rows: Rows;
  arranged: Arranged;
  columns: Column[];
  widths: number[];
  rowHeight: number;
  /** The scroll offsets, in CSS pixels. */
  top: number;
  left: number;
  width: number;
  height: number;
  /** The selected range, by line and column position, and the active cell. */
  selection: { fromLine: number; toLine: number; fromColumn: number; toColumn: number };
  active: { line: number; column: number };
  focused: boolean;
  /** Typed values not yet recorded, by cell, shown in place of the server's. */
  values?: ReadonlyMap<CellKey, string> | undefined;
  /** Refused cells. */
  marks?: ReadonlyMap<CellKey, CellProblem> | undefined;
}

export const INDENT = 14;

export function draw(ctx: CanvasRenderingContext2D, view: View, colours: Palette): void {
  const { rows, arranged, columns, widths, rowHeight: h, top, left, width, height } = view;
  ctx.fillStyle = colours.bg;
  ctx.fillRect(0, 0, width, height);
  const first = Math.max(0, Math.floor(top / h));
  const last = Math.min(arranged.lines.length - 1, Math.ceil((top + height) / h));
  // Where each column starts, and which are on screen.
  const starts: number[] = [];
  let x = 0;
  for (const w of widths) {
    starts.push(x);
    x += w;
  }
  const visible: number[] = [];
  for (let c = 0; c < columns.length; c++) {
    const start = (starts[c] ?? 0) - left;
    if (start + (widths[c] ?? 0) > 0 && start < width) visible.push(c);
  }
  const { selection, active } = view;
  // Looked up per cell only while there's something typed or refused.
  const values = view.values && view.values.size > 0 ? view.values : null;
  const marks = view.marks && view.marks.size > 0 ? view.marks : null;
  ctx.textBaseline = "middle";
  for (let line = first; line <= last; line++) {
    const y = line * h - top;
    const value = arranged.lines[line] ?? 0;
    if (value < 0) {
      const band = arranged.bands[-value - 1];
      if (!band) continue;
      if (band.adds !== undefined) {
        // Add activity: a quiet line, where the band's activities end.
        if (line === active.line) {
          ctx.strokeStyle = colours.active;
          ctx.strokeRect(0.5, y + 0.5, width - 1, h - 1);
        }
        ctx.font = colours.font;
        ctx.fillStyle = colours.muted;
        ctx.fillText("+ Add activity", 6 + band.depth * INDENT - left, y + h / 2);
        continue;
      }
      if (band.newWbs !== undefined) {
        // A new WBS node, typed over this line, where its band will be.
        ctx.fillStyle = colours.band;
        ctx.fillRect(0, y, width, h);
        continue;
      }
      ctx.fillStyle = colours.band;
      ctx.fillRect(0, y, width, h);
      if (line === active.line) {
        ctx.strokeStyle = colours.active;
        ctx.strokeRect(0.5, y + 0.5, width - 1, h - 1);
      }
      ctx.fillStyle = colours.text;
      ctx.font = colours.bold;
      const indent = 6 + band.depth * INDENT - left;
      // A named WBS node: its name, then its path and count, quieter.
      const lead = `${band.collapsed ? "▸" : "▾"} ${band.name ?? band.title}`;
      ctx.fillText(lead, indent, y + h / 2);
      // Measured in the bold font it's drawn in, so what follows clears it.
      let x = indent + 8 + ctx.measureText(lead).width;
      ctx.font = colours.font;
      ctx.fillStyle = colours.muted;
      if (band.name) {
        ctx.fillText(band.title, Math.max(x, 0), y + h / 2);
        x += 8 + ctx.measureText(band.title).width;
      }
      ctx.fillText(`${band.count.toLocaleString()}`, Math.max(x, 0), y + h / 2);
      continue;
    }
    const inRange = line >= selection.fromLine && line <= selection.toLine;
    ctx.font = colours.font;
    for (const c of visible) {
      const column = columns[c];
      if (!column) continue;
      const cellX = (starts[c] ?? 0) - left;
      const w = widths[c] ?? 0;
      if (inRange && c >= selection.fromColumn && c <= selection.toColumn) {
        ctx.fillStyle = colours.chosen;
        ctx.fillRect(cellX, y, w, h);
      }
      const key = values || marks ? cellKey(rows.id[value] ?? -1, column.id) : "";
      const problem = marks?.get(key);
      if (problem) {
        ctx.fillStyle = colours.refused;
        ctx.fillRect(cellX, y, w, h);
        ctx.strokeStyle = colours.critical;
        ctx.strokeRect(cellX + 0.5, y + 0.5, w - 1, h - 1);
      }
      const typed = values?.get(key);
      const text = typed ?? column.text(rows, value);
      if (text) {
        ctx.save();
        ctx.beginPath();
        ctx.rect(cellX + 4, y, w - 8, h);
        ctx.clip();
        const critical = column.id === "float" && rows.critical[value];
        ctx.fillStyle = typed !== undefined ? colours.typed : critical ? colours.critical : colours.text;
        if (typed !== undefined) ctx.font = `italic ${colours.font}`;
        if (column.align === "right") {
          ctx.textAlign = "right";
          ctx.fillText(text, cellX + w - 6, y + h / 2);
          ctx.textAlign = "left";
        } else {
          ctx.fillText(text, cellX + 6, y + h / 2);
        }
        ctx.restore();
      }
      if (line === active.line && c === active.column) {
        ctx.strokeStyle = view.focused ? colours.active : colours.muted;
        ctx.lineWidth = 2;
        ctx.strokeRect(cellX + 1, y + 1, w - 2, h - 2);
        ctx.lineWidth = 1;
      }
    }
    ctx.fillStyle = colours.line;
    ctx.fillRect(0, y + h - 1, width, 1);
  }
  // Column dividers.
  ctx.fillStyle = colours.line;
  for (const c of visible) {
    const edge = (starts[c] ?? 0) + (widths[c] ?? 0) - left;
    ctx.fillRect(Math.round(edge) - 1, 0, 1, height);
  }
}
