// The activity network (PERT) view: the selected activity's predecessors and
// successors as boxes and arrows, a few levels each way, with the links that
// drive their successors in hi-vis as the Gantt shows them. Clicking a box
// selects that activity in the grid.

import { type KeyboardEvent, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import type { Links, Rows } from "../api";
import { selectedIn } from "../inspector/selected";
import { hours } from "../grid/columns";
import type { View } from "../view";
import { BOX, box, networkOf, rowIndex } from "./layout";

const DEPTHS = [1, 2, 3] as const;

export function NetworkPanel({ view, links, onClose }: { view: View; links: Links | null; onClose: () => void }) {
  const [depth, setDepth] = useState<number>(2);
  const [selected, setSelected] = useState<{ id: number; rows: Rows } | null>(() => {
    const now = view.current();
    const of = now ? selectedIn(now.rows, now.arranged.lines, now.active) : null;
    return now && of ? { id: of.id, rows: now.rows } : null;
  });
  // Follows the grid's selection, and fresh rows.
  useEffect(
    () =>
      view.subscribe((now) => {
        const of = selectedIn(now.rows, now.arranged.lines, now.active);
        setSelected((was) => {
          if (!of) return null;
          if (was && was.id === of.id && was.rows === now.rows) return was;
          return { id: of.id, rows: now.rows };
        });
      }),
    [view],
  );
  const net = useMemo(
    () => (selected && links ? networkOf(selected.rows, links, selected.id, depth) : null),
    [selected, links, depth],
  );
  const subject = selected ? (selected.rows.code[rowIndex(selected.rows).get(selected.id) ?? -1] ?? "") : "";
  // The panel is narrower than most networks: it opens on the selected activity.
  const scroller = useRef<HTMLDivElement>(null);
  // Only a new selection or depth re-centres it; fresh rows keep the scroll.
  const centred = useRef("");
  useLayoutEffect(() => {
    const centre = net?.nodes.find((n) => n.level === 0);
    const box = scroller.current;
    if (!centre || !box) return;
    const key = `${centre.id}:${depth}`;
    if (centred.current === key) return;
    centred.current = key;
    box.scrollLeft = Math.max(0, centre.x - (box.clientWidth - BOX.width) / 2);
    box.scrollTop = Math.max(0, centre.y - (box.clientHeight - BOX.height) / 2);
  }, [net, depth]);
  const open = (id: number) => view.reveal(id);
  const onKey = (event: KeyboardEvent<SVGGElement>, id: number) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      open(id);
    }
  };
  return (
    <section className="network" aria-label={subject ? `Activity network around ${subject}` : "Activity network"}>
      <header className="explain-head">
        <h2>{subject ? `Network: ${subject}` : "Network"}</h2>
        <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
          ×
        </button>
      </header>
      <label className="network-depth">
        Levels each way
        <select aria-label="Levels each way" value={depth} onChange={(event) => setDepth(Number(event.target.value))}>
          {DEPTHS.map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </select>
      </label>
      {!selected ? (
        <p className="muted">Select an activity in the grid to see its predecessors and successors.</p>
      ) : !net ? (
        <p className="muted">The relationships are still on their way.</p>
      ) : (
        <>
          <p className="muted network-count" aria-live="polite">
            {net.predecessors === 1 ? "1 predecessor" : `${net.predecessors} predecessors`},{" "}
            {net.successors === 1 ? "1 successor" : `${net.successors} successors`} within {depth}{" "}
            {depth === 1 ? "level" : "levels"}
          </p>
          <div className="network-scroll" ref={scroller}>
            <svg className="network-chart" width={net.width} height={net.height} role="group" aria-label="Activities and their links">
              <defs>
                <marker id="network-arrow" viewBox="0 0 8 8" refX="8" refY="4" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
                  <path d="M0,0 L8,4 L0,8 z" className="network-arrowhead" />
                </marker>
                <marker id="network-arrow-driving" viewBox="0 0 8 8" refX="8" refY="4" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                  <path d="M0,0 L8,4 L0,8 z" className="network-arrowhead driving" />
                </marker>
              </defs>
              {net.edges.map((edge) => {
                const lag = edge.lagHours === 0 ? "" : `${edge.lagHours > 0 ? "+" : ""}${hours(edge.lagHours)}h`;
                return (
                  <g key={edge.link} className={edge.driving ? "network-link driving" : "network-link"}>
                    <polyline points={edge.points.map((p) => `${p.x},${p.y}`).join(" ")} markerEnd={edge.driving ? "url(#network-arrow-driving)" : "url(#network-arrow)"} />
                    <text x={edge.label.x} y={edge.label.y} textAnchor="middle">
                      {edge.type}
                      {lag}
                    </text>
                  </g>
                );
              })}
              {net.nodes.map((node) => {
                const shown = box(selected.rows, node);
                const classes = ["network-box", node.level === 0 ? "centre" : "", shown.critical ? "critical" : ""].filter(Boolean).join(" ");
                return (
                  <g
                    key={node.id}
                    className={classes}
                    transform={`translate(${node.x},${node.y})`}
                    role="button"
                    tabIndex={0}
                    aria-label={`${shown.code} ${shown.name}${shown.critical ? ", critical" : ""}: select it in the grid`}
                    onClick={() => open(node.id)}
                    onKeyDown={(event) => onKey(event, node.id)}
                  >
                    <rect width={BOX.width} height={BOX.height} rx={4} />
                    <text x={8} y={18} className="network-code">
                      {shown.code}
                    </text>
                    <text x={BOX.width - 8} y={18} textAnchor="end" className="network-dates">
                      {shown.float}
                    </text>
                    <text x={8} y={35}>
                      {shown.name.length > 24 ? `${shown.name.slice(0, 23)}…` : shown.name}
                    </text>
                    <text x={8} y={53} className="network-dates">
                      {shown.dates}
                    </text>
                  </g>
                );
              })}
            </svg>
          </div>
        </>
      )}
    </section>
  );
}
