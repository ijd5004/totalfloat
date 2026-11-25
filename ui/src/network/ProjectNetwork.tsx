// The whole project's activity network, full width in place of the Gantt
//: every activity the grid shows (its filters and folded bands
// applied), boxes in columns by logic and links between them, driving links
// in hi-vis. Only what's near the visible area is drawn, so scrolling a few
// thousand activities stays smooth. Clicking a box selects it in the grid.

import { type KeyboardEvent, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import type { Links, Rows } from "../api";
import { selectedIn } from "../inspector/selected";
import { hours } from "../grid/columns";
import type { Arranged } from "../grid/model";
import type { View } from "../view";
import { BOX, box } from "./layout";
import { PROJECT_LIMIT, projectNetworkOf } from "./project";

/** How far past the visible area boxes and links are drawn, in CSS pixels. */
const OVERDRAW = 600;

interface Members {
  rows: Rows;
  arranged: Arranged;
  /** The activities the grid shows, as indexes into `rows`, in its order. */
  members: number[];
}

/** The activities the grid shows now. */
function shownBy(view: View): Members | null {
  const now = view.current();
  if (!now) return null;
  const members: number[] = [];
  for (const line of now.arranged.lines) if (line >= 0) members.push(line);
  return { rows: now.rows, arranged: now.arranged, members };
}

/** The grid's selected activity, by `task_id`. */
function selectedBy(view: View): number | null {
  const now = view.current();
  return now ? (selectedIn(now.rows, now.arranged.lines, now.active)?.id ?? null) : null;
}

export function ProjectNetwork({
  view,
  links,
  onNeighbourhood,
}: {
  view: View;
  links: Links | null;
  /** Opens the side panel's network around the selection instead. */
  onNeighbourhood: () => void;
}) {
  const [shown, setShown] = useState(() => shownBy(view));
  const [selected, setSelected] = useState(() => selectedBy(view));
  // Follows what the grid shows (new rows, filters and folded bands, not its
  // scrolling) and its selection.
  useEffect(
    () =>
      view.subscribe((now) => {
        setShown((was) => (was && was.rows === now.rows && was.arranged === now.arranged ? was : shownBy(view)));
        setSelected(selectedBy(view));
      }),
    [view],
  );
  const tooMany = (shown?.members.length ?? 0) > PROJECT_LIMIT;
  const net = useMemo(
    () => (shown && links && !tooMany ? projectNetworkOf(shown.rows, links, shown.members) : null),
    [shown, links, tooMany],
  );

  // The visible area, kept as the box scrolls, a frame at a time.
  const scroller = useRef<HTMLDivElement>(null);
  const [area, setArea] = useState({ left: 0, top: 0, width: 1600, height: 1200 });
  useLayoutEffect(() => {
    const box = scroller.current;
    if (!box) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      setArea({ left: box.scrollLeft, top: box.scrollTop, width: box.clientWidth, height: box.clientHeight });
    };
    const later = () => {
      if (!frame) frame = requestAnimationFrame(measure);
    };
    measure();
    box.addEventListener("scroll", later, { passive: true });
    const resized = new ResizeObserver(later);
    resized.observe(box);
    return () => {
      box.removeEventListener("scroll", later);
      resized.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  }, [net]);
  // Each link's bounds, and how many links each activity has coming in, worked out once per layout.
  const shape = useMemo(() => {
    if (!net) return null;
    const bounds = net.edges.map((e) => {
      let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
      for (const p of e.points) {
        x0 = Math.min(x0, p.x);
        x1 = Math.max(x1, p.x);
        y0 = Math.min(y0, p.y);
        y1 = Math.max(y1, p.y);
      }
      return { x0, x1, y0, y1 };
    });
    const incoming = new Map<number, number>();
    for (const e of net.edges) incoming.set(e.to, (incoming.get(e.to) ?? 0) + 1);
    return { bounds, incoming };
  }, [net]);
  // A focused box stays drawn, so scrolling it away doesn't drop the focus.
  const [focused, setFocused] = useState<number | null>(null);
  const near = useMemo(() => {
    if (!net || !shape) return null;
    const left = area.left - OVERDRAW;
    const right = area.left + area.width + OVERDRAW;
    const top = area.top - OVERDRAW;
    const bottom = area.top + area.height + OVERDRAW;
    const nodes = net.nodes.filter(
      (n) => n.id === focused || (n.x + BOX.width >= left && n.x <= right && n.y + BOX.height >= top && n.y <= bottom),
    );
    const edges = net.edges.filter((_, k) => {
      const b = shape.bounds[k];
      return !!b && b.x1 >= left && b.x0 <= right && b.y1 >= top && b.y0 <= bottom;
    });
    return { nodes, edges };
  }, [net, shape, area, focused]);

  // A new selection, made in the grid or here, scrolls its box into view;
  // a rebuilt network (new rows, a filter) leaves the scroll where it is.
  const shownFor = useRef<number | null>(null);
  useLayoutEffect(() => {
    // A band selected, or nothing: the next activity selected is new, even the last one again.
    if (selected === null) {
      shownFor.current = null;
      return;
    }
    const node = net?.nodes.find((n) => n.id === selected);
    const box = scroller.current;
    if (!node || !box || shownFor.current === selected) return;
    shownFor.current = selected;
    const inside =
      node.x >= box.scrollLeft && node.x + BOX.width <= box.scrollLeft + box.clientWidth && node.y >= box.scrollTop && node.y + BOX.height <= box.scrollTop + box.clientHeight;
    if (!inside) {
      box.scrollLeft = Math.max(0, node.x - (box.clientWidth - BOX.width) / 2);
      box.scrollTop = Math.max(0, node.y - (box.clientHeight - BOX.height) / 2);
    }
  }, [net, selected]);

  const open = (id: number) => view.reveal(id);
  const onKey = (event: KeyboardEvent<SVGGElement>, id: number) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      open(id);
    }
  };
  const count = shown?.members.length ?? 0;
  return (
    <div className="project-network">
      {!shown ? (
        <p className="muted">The activities are still on their way.</p>
      ) : tooMany ? (
        <div className="project-network-limit" role="status">
          <p>
            The grid shows {count.toLocaleString()} activities. The whole-project network draws up to {PROJECT_LIMIT.toLocaleString()}: filter the grid
            or fold some bands to see fewer, or look at the network around one activity.
          </p>
          <button type="button" onClick={onNeighbourhood}>
            Network around the selection
          </button>
        </div>
      ) : !net || !near ? (
        <p className="muted">The relationships are still on their way.</p>
      ) : (
        <>
          <p className="muted network-count" aria-live="polite">
            {count === 1 ? "1 activity" : `${count.toLocaleString()} activities`} in {net.columns === 1 ? "1 column" : `${net.columns} columns`} by logic
          </p>
          <div className="network-scroll project-network-scroll" ref={scroller}>
            <svg className="network-chart" width={net.width} height={net.height} role="group" aria-label="Every activity shown and its links">
              <defs>
                <marker id="project-arrow" viewBox="0 0 8 8" refX="8" refY="4" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
                  <path d="M0,0 L8,4 L0,8 z" className="network-arrowhead" />
                </marker>
                <marker id="project-arrow-driving" viewBox="0 0 8 8" refX="8" refY="4" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                  <path d="M0,0 L8,4 L0,8 z" className="network-arrowhead driving" />
                </marker>
              </defs>
              {near.edges.map((edge) => {
                const lag = edge.lagHours === 0 ? "" : `${edge.lagHours > 0 ? "+" : ""}${hours(edge.lagHours)}h`;
                // Labels of several links into one box would print over each other: those say it on hover.
                const alone = (shape?.incoming.get(edge.to) ?? 0) <= 1;
                return (
                  <g key={edge.link} className={edge.driving ? "network-link driving" : "network-link"}>
                    <title>{`${edge.type}${lag}`}</title>
                    <polyline
                      points={edge.points.map((p) => `${p.x},${p.y}`).join(" ")}
                      markerEnd={edge.driving ? "url(#project-arrow-driving)" : "url(#project-arrow)"}
                    />
                    {alone && (
                      <text x={edge.label.x} y={edge.label.y} textAnchor="middle">
                        {edge.type}
                        {lag}
                      </text>
                    )}
                  </g>
                );
              })}
              {near.nodes.map((node) => {
                const text = box(shown.rows, node);
                const classes = ["network-box", node.id === selected ? "centre" : "", text.critical ? "critical" : ""].filter(Boolean).join(" ");
                return (
                  <g
                    key={node.id}
                    className={classes}
                    transform={`translate(${node.x},${node.y})`}
                    role="button"
                    tabIndex={0}
                    aria-label={`${text.code} ${text.name}${text.critical ? ", critical" : ""}: select it in the grid`}
                    onClick={() => open(node.id)}
                    onKeyDown={(event) => onKey(event, node.id)}
                    onFocus={() => setFocused(node.id)}
                    onBlur={() => setFocused((was) => (was === node.id ? null : was))}
                  >
                    <rect width={BOX.width} height={BOX.height} rx={4} />
                    <text x={8} y={18} className="network-code">
                      {text.code}
                    </text>
                    <text x={BOX.width - 8} y={18} textAnchor="end" className="network-dates">
                      {text.float}
                    </text>
                    <text x={8} y={35}>
                      {text.name.length > 24 ? `${text.name.slice(0, 23)}…` : text.name}
                    </text>
                    <text x={8} y={53} className="network-dates">
                      {text.dates}
                    </text>
                  </g>
                );
              })}
            </svg>
          </div>
        </>
      )}
    </div>
  );
}
