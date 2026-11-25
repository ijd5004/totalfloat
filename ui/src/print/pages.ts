// Laying the grid and Gantt out on printed pages. Everything here is page
// geometry: where a row, a column or a date falls on paper. Every date and
// float printed is the server's own, from the rows.

import type { Unit } from "../gantt/time";

export type PaperSize = "letter" | "a4" | "a3" | "tabloid";
export type Orientation = "portrait" | "landscape";

/** Paper sizes in points (1/72 inch), portrait, and as CSS `@page` sizes. */
export const PAPER: Record<PaperSize, { label: string; width: number; height: number; css: [string, string] }> = {
  letter: { label: "Letter", width: 612, height: 792, css: ["8.5in", "11in"] },
  a4: { label: "A4", width: 595.28, height: 841.89, css: ["210mm", "297mm"] },
  a3: { label: "A3", width: 841.89, height: 1190.55, css: ["297mm", "420mm"] },
  tabloid: { label: "Tabloid", width: 792, height: 1224, css: ["11in", "17in"] },
};

/** How a page is set out, in points. */
export const MARGIN = 28;
export const TITLE_HEIGHT = 44;
export const HEADER_HEIGHT = 26;
export const FOOTER_HEIGHT = 22;
export const ROW_HEIGHT = 12;

/** A page's size in points, turned for its orientation. */
export function pageSize(size: PaperSize, orientation: Orientation): { width: number; height: number } {
  const { width, height } = PAPER[size];
  return orientation === "landscape" ? { width: height, height: width } : { width, height };
}

/** The CSS `@page` size for the paper and orientation. */
export function cssPageSize(size: PaperSize, orientation: Orientation): string {
  const [w, h] = PAPER[size].css;
  return orientation === "landscape" ? `${h} ${w}` : `${w} ${h}`;
}

/** How many rows fit on a page between the header and the footer. */
export function rowsPerPage(height: number): number {
  return Math.max(1, Math.floor((height - 2 * MARGIN - TITLE_HEIGHT - HEADER_HEIGHT - FOOTER_HEIGHT) / ROW_HEIGHT));
}

/** The lines split into pages of whole rows: no row is ever cut across two. */
export function paginate<T>(lines: readonly T[], perPage: number): T[][] {
  const pages: T[][] = [];
  for (let at = 0; at < lines.length; at += perPage) pages.push(lines.slice(at, at + perPage));
  return pages.length > 0 ? pages : [[]];
}

/** The least a column is shrunk on paper, from its screen width, so its text stays readable. */
export const MIN_SCALE = 0.55;

/**
 * The grid's columns' widths on paper, and how many fit. Fitted, the grid
 * takes `share` of the page's width; else the columns keep their screen
 * widths (as points, three quarters of a pixel), up to most of the page.
 * Columns are never shrunk below `MIN_SCALE`: those that then don't fit are
 * left off, from the right, keeping at least one.
 */
export function columnWidths(
  screen: readonly number[],
  contentWidth: number,
  fit: boolean,
  share: number,
): { widths: number[]; kept: number } {
  const total = screen.reduce((a, b) => a + b, 0) || 1;
  const room = fit ? contentWidth * share : contentWidth * 0.75;
  const scale = Math.max(MIN_SCALE, fit ? room / total : Math.min(0.75, room / total));
  let kept = 0;
  let used = 0;
  for (const width of screen) {
    if (kept > 0 && used + width * scale > room + 0.01) break;
    used += width * scale;
    kept++;
  }
  // Fitted, what's kept fills its share; one column wider than the room is squeezed into it.
  const stretch = used > 0 && (fit || used > room) ? room / used : 1;
  return { widths: screen.slice(0, kept).map((w) => w * scale * stretch), kept };
}

/** The time axis's two tiers for a range of days on paper. */
export function tiersFor(days: number): { top: Unit; bottom: Unit } {
  if (days <= 45) return { top: "month", bottom: "day" };
  if (days <= 200) return { top: "month", bottom: "week" };
  if (days <= 1100) return { top: "year", bottom: "month" };
  return { top: "year", bottom: "quarter" };
}

/**
 * Text cut to fit a width, at a font size, ending in an ellipsis when cut.
 * `measure` gives a text's width as it'll print; without one, the width is
 * estimated from an average character, half the size wide.
 */
export function fitText(text: string, width: number, fontSize: number, measure?: (text: string) => number): string {
  const wide = measure ?? ((t: string) => t.length * fontSize * 0.52);
  if (wide(text) <= width) return text;
  // The longest start that fits with its ellipsis.
  let low = 0;
  let high = text.length;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if (wide(`${text.slice(0, mid)}…`) <= width) low = mid;
    else high = mid - 1;
  }
  return low === 0 ? "" : `${text.slice(0, low)}…`;
}

/** A date input's value (`YYYY-MM-DD`) as the server writes dates, at the day's start. */
export function dayOf(value: string): string | null {
  return /^\d{4}-\d{2}-\d{2}$/.test(value) ? `${value} 00:00` : null;
}
