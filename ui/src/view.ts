// What the grid shows, shared with the Gantt so the two scroll together: the
// arranged lines, the vertical scroll and the active line. The grid publishes
// on each change; the Gantt draws from it on the next frame, without React
// rendering anything, so scrolling stays a canvas repaint.

import type { Rows } from "./api";
import type { Arranged } from "./grid/model";

export interface Shown {
  rows: Rows;
  arranged: Arranged;
  /** The body's scroll offset and height, in CSS pixels. */
  top: number;
  height: number;
  rowHeight: number;
  /** The line the active cell is on. */
  active: number;
  /** Where the grid's body starts on screen, so the Gantt's rows line up with it. */
  bodyTop: number;
}

export class View {
  private shown: Shown | null = null;
  private readonly listeners = new Set<(shown: Shown) => void>();
  private scroller: ((by: number) => void) | null = null;
  private revealer: ((id: number) => void) | null = null;
  private readonly revealed = new Set<(id: number) => void>();

  /** The grid says what it shows now. */
  publish(shown: Shown): void {
    const was = this.shown;
    if (
      was &&
      was.rows === shown.rows &&
      was.arranged === shown.arranged &&
      was.top === shown.top &&
      was.height === shown.height &&
      was.active === shown.active &&
      was.bodyTop === shown.bodyTop
    ) {
      return;
    }
    this.shown = shown;
    for (const listener of this.listeners) listener(shown);
  }

  current(): Shown | null {
    return this.shown;
  }

  subscribe(listener: (shown: Shown) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** The grid takes requests to scroll, from wheels over the Gantt. */
  scrollWith(scroller: ((by: number) => void) | null): void {
    this.scroller = scroller;
  }

  /**
   * Scrolls the grid, and so the Gantt, by `by` pixels from where it is now,
   * so wheel events quicker than a frame each count.
   */
  scrollBy(by: number): void {
    this.scroller?.(by);
  }

  /** The grid takes requests to select an activity. */
  revealWith(revealer: ((id: number) => void) | null): void {
    this.revealer = revealer;
  }

  /** Selects activity `id` (`task_id`) in the grid, scrolling to it; the Gantt scrolls to its bar. */
  reveal(id: number): void {
    this.revealer?.(id);
    for (const listener of this.revealed) listener(id);
  }

  /** Hears each activity revealed, as the Gantt does to bring its bar into view. */
  onReveal(listener: (id: number) => void): () => void {
    this.revealed.add(listener);
    return () => this.revealed.delete(listener);
  }
}
