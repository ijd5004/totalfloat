import { describe, expect, it } from "vitest";

import type { Links, Rows } from "../api";
import { BOX } from "./layout";
import { projectNetworkOf } from "./project";

/** Activities 1 to `n`, A1 to An, starting on a day set by their number. */
function rows(n: number): Rows {
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
    early_start: ids.map((id) => `2026-${String((id % 12) + 1).padStart(2, "0")}-${String((id % 28) + 1).padStart(2, "0")} 08:00`),
    early_finish: each(null),
    total_float_hours: each(0),
    critical: each(false),
  } as unknown as Rows;
}

function links(pairs: [number, number][]): Links {
  return {
    schema: "totalfloat.links/1",
    count: pairs.length,
    id: pairs.map((_, k) => 100 + k),
    predecessor: pairs.map(([p]) => p),
    successor: pairs.map(([, s]) => s),
    type: pairs.map(() => "FS"),
    lag_hours: pairs.map(() => 0),
    driving: pairs.map(() => false),
  } as unknown as Links;
}

/** A seeded network of `n` activities: each links to one to three later ones, some far ahead. */
function synthetic(n: number): { rows: Rows; links: Links } {
  let seed = 7;
  const next = () => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
  };
  const pairs: [number, number][] = [];
  for (let id = 1; id < n; id++) {
    const count = 1 + Math.floor(next() * 3);
    for (let k = 0; k < count; k++) {
      const ahead = next() < 0.8 ? 1 + Math.floor(next() * 4) : 1 + Math.floor(next() * 60);
      const to = Math.min(n, id + ahead);
      if (to !== id) pairs.push([id, to]);
    }
  }
  return { rows: rows(n), links: links(pairs) };
}

const all = (n: number) => Array.from({ length: n }, (_, k) => k);

describe("the whole-project network", () => {
  it("puts each activity one column after its furthest predecessor", () => {
    // 1 → 2 → 4, 1 → 3 → 4 → 5, 1 → 5; 6 stands apart.
    const net = projectNetworkOf(rows(6), links([[1, 2], [2, 4], [1, 3], [3, 4], [4, 5], [1, 5]]), all(6));
    const level = Object.fromEntries(net.nodes.map((n) => [n.id, n.level]));
    expect(level).toEqual({ 1: 0, 2: 1, 3: 1, 4: 2, 5: 3, 6: 0 });
    expect(net.columns).toBe(4);
  });

  it("gives the same picture for the same input, whatever order the activities come in", () => {
    const { rows: r, links: l } = synthetic(400);
    const once = projectNetworkOf(r, l, all(400));
    const again = projectNetworkOf(r, l, all(400));
    const shuffled = projectNetworkOf(r, l, all(400).reverse());
    expect(again).toEqual(once);
    const byId = (net: typeof once) => [...net.nodes].sort((a, b) => a.id - b.id);
    expect(byId(shuffled)).toEqual(byId(once));
    expect([...shuffled.edges].sort((a, b) => a.link - b.link)).toEqual([...once.edges].sort((a, b) => a.link - b.link));
  });

  it("runs every link left to right, and never through a box", () => {
    const { rows: r, links: l } = synthetic(2000);
    const net = projectNetworkOf(r, l, all(2000));
    expect(net.nodes).toHaveLength(2000);
    const level = new Map(net.nodes.map((n) => [n.id, n.level]));
    for (const edge of net.edges) expect(level.get(edge.from) ?? 0).toBeLessThan(level.get(edge.to) ?? 0);
    // Each segment is level or upright; none passes inside a box.
    const inside = (x: number, y: number) =>
      net.nodes.some((n) => x > n.x + 0.5 && x < n.x + BOX.width - 0.5 && y > n.y + 0.5 && y < n.y + BOX.height - 0.5);
    let crossings = 0;
    for (const edge of net.edges) {
      for (let k = 1; k < edge.points.length; k++) {
        const a = edge.points[k - 1] ?? { x: 0, y: 0 };
        const b = edge.points[k] ?? { x: 0, y: 0 };
        expect(a.x === b.x || a.y === b.y).toBe(true);
        // Sample the segment at a few points between its ends.
        for (const t of [0.25, 0.5, 0.75]) if (inside(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)) crossings++;
      }
    }
    expect(crossings).toBe(0);
  }, 30_000);

  it("draws only the activities given, and links between them", () => {
    const net = projectNetworkOf(rows(5), links([[1, 2], [2, 3], [3, 4], [4, 5]]), [0, 2, 4]);
    expect(net.nodes.map((n) => n.id).sort()).toEqual([1, 3, 5]);
    expect(net.edges).toEqual([]);
    expect(net.nodes.every((n) => n.level === 0)).toBe(true);
  });

  it("orders a column by early start, then Activity ID", () => {
    const r = rows(3);
    (r.early_start as (string | null)[])[0] = "2026-05-01 08:00";
    (r.early_start as (string | null)[])[1] = null;
    (r.early_start as (string | null)[])[2] = "2026-01-01 08:00";
    const net = projectNetworkOf(r, links([]), all(3));
    expect([...net.nodes].sort((a, b) => a.y - b.y).map((n) => n.id)).toEqual([3, 1, 2]);
  });

  it("cuts a loop instead of hanging, in the same place whatever the order", () => {
    const loop = links([[1, 2], [2, 3], [3, 1]]);
    const net = projectNetworkOf(rows(3), loop, all(3));
    expect(net.nodes).toHaveLength(3);
    const levels = (n: typeof net) => Object.fromEntries(n.nodes.map((node) => [node.id, node.level]));
    expect(levels(projectNetworkOf(rows(3), loop, [2, 1, 0]))).toEqual(levels(net));
    expect(levels(projectNetworkOf(rows(3), loop, [1, 2, 0]))).toEqual(levels(net));
  });

  it("keeps every link inside the picture, even into the top row", () => {
    // A1 → A2 → A3, and A1 → A3 skips a column into the top row.
    const net = projectNetworkOf(rows(3), links([[1, 2], [2, 3], [1, 3]]), all(3));
    const { rows: r, links: l } = synthetic(400);
    for (const n of [net, projectNetworkOf(r, l, all(400))]) {
      for (const e of n.edges) {
        for (const p of e.points) {
          expect(p.y).toBeGreaterThanOrEqual(0);
          expect(p.y).toBeLessThanOrEqual(n.height);
          expect(p.x).toBeGreaterThanOrEqual(0);
          expect(p.x).toBeLessThanOrEqual(n.width);
        }
      }
    }
  });

  it("spreads links that share a channel, so each can be followed", () => {
    // A1 and A2 start; A5 skips to A3's column... both A1 and A2 skip two columns into A6's row.
    const net = projectNetworkOf(rows(6), links([[1, 3], [3, 4], [4, 5], [1, 6], [2, 6], [4, 6]]), all(6));
    const into6 = net.edges.filter((e) => e.to === 6 && e.points.length === 6);
    expect(into6.length).toBeGreaterThanOrEqual(2);
    const channels = new Set(into6.map((e) => e.points[2]?.y));
    expect(channels.size).toBe(into6.length);
  });
});
