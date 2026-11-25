// What the grid's and the Gantt's canvases share: reading the theme, and
// sizing a canvas to its box at the screen's pixel ratio.

/** Reads the theme's CSS variables on `element`, with a fallback for each. */
export function themeOf(element: Element): (name: string, fallback: string) => string {
  const style = getComputedStyle(element);
  return (name, fallback) => style.getPropertyValue(name).trim() || fallback;
}

/** The theme's font, as a canvas takes it, at `weight`. */
export function fontOf(theme: (name: string, fallback: string) => string, weight = ""): string {
  const size = theme("--text-size", "13px");
  const family = theme("--font", "system-ui, sans-serif");
  return `${weight ? `${weight} ` : ""}${size} ${family}`;
}

/**
 * Sizes `canvas` to `width` by `height` CSS pixels at the screen's pixel
 * ratio, only when that changed, and returns its context drawing in CSS
 * pixels. Says whether the size changed, for callers that place it.
 */
export function fitCanvas(
  canvas: HTMLCanvasElement,
  width: number,
  height: number,
): { ctx: CanvasRenderingContext2D | null; resized: boolean } {
  const ratio = window.devicePixelRatio || 1;
  const w = Math.max(width, 1);
  const h = Math.max(height, 1);
  const resized = canvas.width !== Math.round(w * ratio) || canvas.height !== Math.round(h * ratio);
  if (resized) {
    canvas.width = Math.round(w * ratio);
    canvas.height = Math.round(h * ratio);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
  }
  const ctx = canvas.getContext("2d");
  ctx?.setTransform(ratio, 0, 0, ratio, 0, 0);
  return { ctx, resized };
}

/** A wheel's movement in CSS pixels, whatever unit the browser sent it in. */
export function wheelPixels(event: WheelEvent, lineHeight: number, pageHeight: number): { x: number; y: number } {
  const unit = event.deltaMode === 1 ? lineHeight : event.deltaMode === 2 ? pageHeight : 1;
  return { x: event.deltaX * unit, y: event.deltaY * unit };
}
