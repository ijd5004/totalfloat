// Frame timing for the grid, on when the page's address has `?debug=frames`.
// A measurement runs from `window.__gridFrames.reset()` to `stop()`, as a
// scripted scroll does; every gap between drawn frames in it counts, stalls
// included.

export interface FrameStats {
  frames: number;
  totalMs: number;
  worstMs: number;
  /** The average gap between drawn frames while measuring. */
  averageMs: number;
}

declare global {
  interface Window {
    __gridFrames?: FrameStats & { reset: () => void; stop: () => void };
  }
}

export function framesWanted(): boolean {
  return typeof window !== "undefined" && new URLSearchParams(window.location.search).get("debug") === "frames";
}

/** Records each gap between drawn frames while a measurement runs. */
export class FrameClock {
  private last: number | null = null;
  private measuring = false;

  constructor(private readonly on: boolean) {
    if (!on) return;
    const stats = {
      frames: 0,
      totalMs: 0,
      worstMs: 0,
      averageMs: 0,
      reset: () => {
        stats.frames = 0;
        stats.totalMs = 0;
        stats.worstMs = 0;
        stats.averageMs = 0;
        this.last = null;
        this.measuring = true;
      },
      stop: () => {
        this.measuring = false;
      },
    };
    window.__gridFrames = stats;
  }

  tick(now: number): void {
    const stats = window.__gridFrames;
    if (!this.on || !stats || !this.measuring) return;
    if (this.last !== null) {
      const gap = now - this.last;
      stats.frames++;
      stats.totalMs += gap;
      stats.worstMs = Math.max(stats.worstMs, gap);
      stats.averageMs = stats.totalMs / stats.frames;
    }
    this.last = now;
  }
}
