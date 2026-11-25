import { describe, expect, it } from "vitest";

import type { Links, Rows } from "../api";
import { BOX, networkOf } from "./layout";

/** Activities 1 to `n`, A1 to An, starting on the day of their number. */
function rows(n: number, starts: Record<number, string | null> = {}): Rows {
  const ids = Array.from({ length: n }, (_, k) => k + 1);
  const each = <T,>(value: T) => ids.map(() => value);
  return {
    schema: "totalfloat.rows/1",
    file: "x.xer",
    count: n,
    id: ids,
    project: each(1),
    code: ids.map((id) => `A${id}`),
    name: ids.map((id) => `Activity ${id}`),
    early_start: ids.map((id) => (id in starts ? (starts[id] ?? null) : `2026-01-${String(id).padStart(2, "0")} 08:00`)),
    early_finish: each(null),
    total_float_hours: each(0),
    critical: each(false),
  } as unknown as Rows;
}

function links(pairs: [number, number][], driving: number[] = []): Links {
  return {
    schema: "totalfloat.links/1",
    count: pairs.length,
    id: pairs.map((_, k) => 100 + k),
    predecessor: pairs.map(([p]) => p),
    successor: pairs.map(([, s]) => s),
    type: pairs.map(() => "FS"),
    lag_hours: pairs.map(() => 0),
    driving: pairs.map((_, k) => driving.includes(100 + k)),
  } as unknown as Links;
}

// 1 → 3, 2 → 3, 3 → 4, 4 → 5, 4 → 6, 6 → 7; 8 stands apart.
const LOGIC: [number, number][] = [
  [1, 3],
  [2, 3],
  [3, 4],
  [4, 5],
  [4, 6],
  [6, 7],
];

describe("the network layout", () => {
  it("puts predecessors left and successors right, a column per level", () => {
    const net = networkOf(rows(8), links(LOGIC, [102]), 4, 2);
    const levels = Object.fromEntries(net.nodes.map((n) => [n.id, n.level]));
    expect(levels).toEqual({ 1: -2, 2: -2, 3: -1, 4: 0, 5: 1, 6: 1, 7: 2 });
    expect(net.predecessors).toBe(3);
    expect(net.successors).toBe(3);
    // Columns step right by a box and a gap; the centre's column is the third.
    const x = (id: number) => net.nodes.find((n) => n.id === id)?.x;
    expect(x(1)).toBe(BOX.margin);
    expect(x(4)).toBe(BOX.margin + 2 * (BOX.width + BOX.gapX));
    expect(x(7)).toBe(BOX.margin + 4 * (BOX.width + BOX.gapX));
    expect(net.width).toBe(BOX.margin * 2 + 5 * BOX.width + 4 * BOX.gapX);
  });

  it("orders each column by early start, then Activity ID", () => {
    const net = networkOf(rows(8, { 5: "2026-01-20 08:00", 6: "2026-01-02 08:00" }), links(LOGIC), 4, 1);
    const column = net.nodes.filter((n) => n.level === 1).sort((a, b) => a.y - b.y);
    expect(column.map((n) => n.id)).toEqual([6, 5]);
  });

  it("draws each link between activities shown, the driving ones marked", () => {
    const net = networkOf(rows(8), links(LOGIC, [102]), 4, 1);
    expect(net.edges.map((e) => [e.from, e.to, e.driving])).toEqual([
      [3, 4, true],
      [4, 5, false],
      [4, 6, false],
    ]);
    const into = net.edges[0];
    const from = net.nodes.find((n) => n.id === 3);
    expect(into?.points[0]).toEqual({ x: (from?.x ?? 0) + BOX.width, y: (from?.y ?? 0) + BOX.height / 2 });
  });

  it("goes only as far as asked, and shows an activity with no links alone", () => {
    expect(networkOf(rows(8), links(LOGIC), 4, 1).nodes.map((n) => n.id).sort()).toEqual([3, 4, 5, 6]);
    const alone = networkOf(rows(8), links(LOGIC), 8, 3);
    expect(alone.nodes.map((n) => [n.id, n.level])).toEqual([[8, 0]]);
    expect(alone.edges).toEqual([]);
  });

  it("gives the same picture however the links come", () => {
    const shuffled = [...LOGIC].reverse();
    const a = networkOf(rows(8), links(LOGIC), 4, 3);
    const b = networkOf(rows(8), links(shuffled), 4, 3);
    const placed = (net: typeof a) => net.nodes.map((n) => [n.id, n.x, n.y]).sort((x, y) => (x[0] ?? 0) - (y[0] ?? 0));
    expect(placed(a)).toEqual(placed(b));
  });

  it("runs a link that skips a column round the boxes between, its label clear of them", () => {
    // A1 → A2 → A3, and A1 → A3 too: with A2 selected, all three sit in a row.
    const net = networkOf(rows(3), links([[1, 2], [2, 3], [1, 3]]), 2, 1);
    expect(new Set(net.nodes.map((n) => n.y)).size).toBe(1);
    const skip = net.edges.find((e) => e.from === 1 && e.to === 3);
    expect(skip?.points).toHaveLength(6);
    const lane = skip?.points[2]?.y ?? 0;
    const boxes = net.nodes.map((n) => n.y + BOX.height);
    expect(lane).toBeGreaterThan(Math.max(...boxes));
    expect(skip?.label.y).toBeGreaterThan(Math.max(...boxes));
    expect(net.height).toBeGreaterThan(lane);
    // The links to the next column still run straight across.
    expect(net.edges.filter((e) => e !== skip).every((e) => e.points.length === 2)).toBe(true);
  });

  it("keeps a skipping link's every segment off every box, however many rows", () => {
    // A1 → A2 → A3 and A1 → A3, with A4 → A2 and A2 → A5 putting boxes below A1 and A3.
    const net = networkOf(rows(5), links([[1, 2], [2, 3], [1, 3], [4, 2], [2, 5]]), 2, 1);
    const skip = net.edges.find((e) => e.from === 1 && e.to === 3);
    const points = skip?.points ?? [];
    expect(points.length).toBe(6);
    const inside = (x: number, y: number) =>
      net.nodes.some((n) => x > n.x + 0.5 && x < n.x + BOX.width - 0.5 && y > n.y + 0.5 && y < n.y + BOX.height - 0.5);
    // Sample each segment finely: none of it may pass through a box.
    for (let k = 1; k < points.length; k++) {
      const a = points[k - 1];
      const b = points[k];
      if (!a || !b) continue;
      for (let t = 0; t <= 1; t += 0.02) {
        const x = a.x + (b.x - a.x) * t;
        const y = a.y + (b.y - a.y) * t;
        expect(inside(x, y), `segment ${k} at ${x},${y}`).toBe(false);
      }
    }
  });

  it("runs every link left to right and clear of every box, even with redundant logic", () => {
    const cases: [string, [number, number][], number, number][] = [
      // P1 → C directly and through P2 → P3: P1 goes left of P2, not right.
      ["a predecessor reached two ways", [[1, 5], [1, 2], [2, 3], [3, 5]], 5, 2],
      // P2 → P1 → C and P2 → C: P2 left of P1, not in its column.
      ["two predecessors linked to each other", [[1, 5], [2, 5], [2, 1]], 5, 2],
      // The same on the successor side.
      ["a successor reached two ways", [[5, 1], [5, 2], [2, 3], [3, 1]], 5, 2],
    ];
    for (const [name, pairs, centre, depth] of cases) {
      const net = networkOf(rows(6), links(pairs), centre, depth);
      for (const edge of net.edges) {
        const from = net.nodes.find((n) => n.id === edge.from);
        const to = net.nodes.find((n) => n.id === edge.to);
        expect((to?.x ?? 0) > (from?.x ?? 0), `${name}: ${edge.from} → ${edge.to}`).toBe(true);
        const inside = (x: number, y: number) =>
          net.nodes.some((n) => x > n.x + 0.5 && x < n.x + BOX.width - 0.5 && y > n.y + 0.5 && y < n.y + BOX.height - 0.5);
        for (let k = 1; k < edge.points.length; k++) {
          const a = edge.points[k - 1];
          const b = edge.points[k];
          if (!a || !b) continue;
          for (let t = 0; t <= 1; t += 0.02) {
            expect(inside(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t), `${name}: ${edge.from} → ${edge.to}`).toBe(false);
          }
        }
      }
    }
  });
});
