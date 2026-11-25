import { describe, expect, it } from "vitest";

import { type Graph, type Node, layout, ordered } from "./graph";

/** Changes newest first, each `id<parent` or `id<parent+second`, or a root. */
function nodes(...lines: string[]): Node[] {
  return lines.map((line) => {
    const [id = "", parents = ""] = line.split("<");
    const [parent = "", second = ""] = parents.split("+");
    return { id, parent: parent || null, second_parent: second || null };
  });
}

/** Each row as `id lane`, then its lines: `|n` through, `n>` into, `>n` out. */
function drawn(graph: Graph): string[] {
  return graph.rows.map((row) =>
    [
      `${row.id}@${row.lane}${row.hollow ? "o" : ""}${row.heads.length ? `[${row.heads.join(",")}]` : ""}`,
      ...row.through.map((l) => `|${l.lane}`),
      ...row.into.map((l) => `${l.lane}>`),
      ...row.out.map((l) => `>${l.lane}`),
    ].join(" "),
  );
}

describe("the history graph", () => {
  it("draws a linear history as one straight line", () => {
    const graph = layout(nodes("c<b", "b<a", "a"), [{ name: "main", head: "c" }], { first: "main" });
    expect(graph.lanes).toBe(1);
    expect(drawn(graph)).toEqual(["c@0[main] >0", "b@0 0> >0", "a@0 0>"]);
    expect(graph.rows.every((row) => row.tone === 0)).toBe(true);
  });

  it("starts a branch's line at the change it was made from", () => {
    // main: a, b, c; what-if from a, holding w1 and w2.
    const graph = layout(nodes("w2<w1", "w1<a", "c<b", "b<a", "a"), [
      { name: "what-if", head: "w2" },
      { name: "main", head: "c" },
    ], { first: "main" });
    expect(graph.lanes).toBe(2);
    expect(drawn(graph)).toEqual([
      "w2@1[what-if] >1",
      "w1@1 1> >1",
      "c@0[main] |1 >0",
      "b@0 |1 0> >0",
      "a@0 0> 1>",
    ]);
    // Each branch keeps its colour.
    expect(graph.rows.map((row) => row.tone)).toEqual([1, 1, 0, 0, 0]);
  });

  it("joins two lines at a merge, from the merge across to the branch merged in", () => {
    const graph = layout(nodes("m<c+w2", "w2<w1", "w1<a", "c<b", "b<a", "a"), [
      { name: "main", head: "m" },
      { name: "what-if", head: "w2" },
    ], { first: "main" });
    expect(drawn(graph)).toEqual([
      "m@0[main] >0 >1",
      "w2@1[what-if] |0 1> >1",
      "w1@1 |0 1> >1",
      "c@0 |1 0> >0",
      "b@0 |1 0> >0",
      "a@0 0> 1>",
    ]);
  });

  it("orders lanes with the first branch asked for, then the others by name", () => {
    const heads = [
      { name: "zeta", head: "z" },
      { name: "alpha", head: "x" },
      { name: "main", head: "m" },
    ];
    const graph = layout(nodes("z<r", "x<r", "m<r", "r"), heads, { first: "main" });
    expect(graph.rows.map((row) => [row.id, row.lane])).toEqual([
      ["z", 2],
      ["x", 1],
      ["m", 0],
      ["r", 0],
    ]);
    // The same store shown from another branch puts that one first.
    const shown = layout(nodes("z<r", "x<r", "m<r", "r"), heads, { first: "zeta" });
    expect(shown.rows.map((row) => [row.id, row.lane])).toEqual([
      ["z", 0],
      ["x", 1],
      ["m", 2],
      ["r", 0],
    ]);
  });

  it("names every branch at its head, even one with no changes of its own", () => {
    const graph = layout(nodes("b<a", "a"), [
      { name: "what-if", head: "b" },
      { name: "main", head: "b" },
    ], { first: "main" });
    expect(graph.lanes).toBe(1);
    expect(graph.rows[0]?.heads).toEqual(["main", "what-if"]);
  });

  it("draws a proposal as a hollow dot off its branch, in its branch's colour, with a dashed line", () => {
    const graph = layout(nodes("p<m", "m<a", "a"), [{ name: "main", head: "m" }], { first: "main", hollow: new Set(["p"]) });
    expect(drawn(graph)).toEqual(["p@1o >1", "m@0[main] 1> >0", "a@0 0>"]);
    const [proposal] = graph.rows;
    expect(proposal?.tone).toBe(0);
    expect(proposal?.out[0]?.dashed).toBe(true);
    expect(graph.rows[1]?.out[0]?.dashed).toBe(false);
  });

  it("keeps the changes of a branch whose name was deleted, on a line with no name", () => {
    // what-if merged into main, then deleted; a proposal waits on main.
    const graph = layout(nodes("p<m", "m<c+w2", "w2<w1", "w1<a", "c<a", "a"), [{ name: "main", head: "m" }], {
      first: "main",
      hollow: new Set(["p"]),
    });
    expect(drawn(graph)).toEqual([
      "p@1o >1",
      "m@0[main] 1> >0 >2",
      "w2@2 |0 2> >2",
      "w1@2 |0 2> >2",
      "c@0 |2 0> >0",
      "a@0 0> 2>",
    ]);
    expect(graph.rows.map((row) => row.tone)).toEqual([0, 0, null, null, 0, 0]);
  });

  it("shares a lane between unnamed lines that don't overlap", () => {
    // Two proposals on main, made at different points.
    const graph = layout(nodes("p2<b", "b<p1-parent", "p1<a", "p1-parent<a", "a"), [{ name: "main", head: "b" }], {
      first: "main",
      hollow: new Set(["p2", "p1"]),
    });
    expect(graph.rows.find((row) => row.id === "p2")?.lane).toBe(1);
    expect(graph.rows.find((row) => row.id === "p1")?.lane).toBe(1);
    expect(graph.lanes).toBe(2);
  });

  it("gives proposals waiting on the same change lanes of their own, so no line crosses another's dot", () => {
    const graph = layout(nodes("p2<c", "p1<c", "c<b", "b"), [{ name: "main", head: "c" }], {
      first: "main",
      hollow: new Set(["p1", "p2"]),
    });
    expect(drawn(graph)).toEqual(["p2@1o >1", "p1@2o |1 >2", "c@0[main] 1> 2> >0", "b@0 0>"]);
  });

  it("keeps proposals to a few lanes, however many wait at once", () => {
    const many = Array.from({ length: 6 }, (_, k) => `p${k}<a`);
    const graph = layout(nodes(...many, "b<a", "a"), [{ name: "main", head: "b" }], {
      first: "main",
      hollow: new Set(many.map((line) => line.split("<")[0] ?? "")),
    });
    expect(new Set(graph.rows.filter((row) => row.hollow).map((row) => row.lane))).toEqual(new Set([1, 2, 3]));
    expect(graph.lanes).toBe(4);
  });

  it("keeps unnamed lines to a few lanes, however many a listing cut short leaves", () => {
    // Every change's parent is older than the listing.
    const cut = Array.from({ length: 50 }, (_, k) => `c${k}<old${k}`);
    const graph = layout(nodes(...cut), [], {});
    expect(graph.lanes).toBe(4);
    expect(Math.max(...graph.rows.map((row) => row.through.length))).toBeLessThanOrEqual(4);
  });

  it("keeps a branch's colour while another gains its first change", () => {
    const heads = (b: string) => [
      { name: "main", head: "m" },
      { name: "b", head: b },
      { name: "c", head: "c1" },
    ];
    // b was just made at main's head, so has no line of its own yet.
    const before = layout(nodes("c1<m", "m"), heads("m"), { first: "main" });
    const after = layout(nodes("b1<m", "c1<m", "m"), heads("b1"), { first: "main" });
    expect(before.rows.find((row) => row.id === "c1")?.tone).toBe(2);
    expect(after.rows.find((row) => row.id === "c1")?.tone).toBe(2);
    expect(after.rows.find((row) => row.id === "b1")?.tone).toBe(1);
  });

  it("draws a merge whose second parent isn't listed to the bottom, in a lane of its own", () => {
    const graph = layout(nodes("m<c+gone", "c<b", "b"), [{ name: "main", head: "m" }], { first: "main" });
    expect(drawn(graph)).toEqual(["m@0[main] >0 >1", "c@0 |1 0> >0", "b@0 |1 0>"]);
  });

  it("runs a line whose parent isn't listed to the bottom", () => {
    // Older changes are cut off: c's branch started below what's listed.
    const graph = layout(nodes("w<old", "c<b", "b"), [
      { name: "main", head: "c" },
      { name: "what-if", head: "w" },
    ], { first: "main" });
    expect(drawn(graph)).toEqual(["w@1[what-if] >1", "c@0[main] |1 >0", "b@0 |1 0>"]);
  });
});

describe("the graph at scale", () => {
  it("lays out 10 branches and 500 changes, each branch in its own lane, quickly", () => {
    // main holds 50 changes; nine branches start from it at different points, 50 changes each.
    const listed: Node[] = [];
    const heads = [{ name: "main", head: "main-49" }];
    for (let k = 0; k < 50; k++) listed.push({ id: `main-${k}`, parent: k ? `main-${k - 1}` : null });
    for (let b = 1; b < 10; b++) {
      for (let k = 0; k < 50; k++) listed.push({ id: `b${b}-${k}`, parent: k ? `b${b}-${k - 1}` : `main-${b * 5}` });
      heads.push({ name: `branch-${b}`, head: `b${b}-49` });
    }
    // Newest first, branches interleaved, as edits on several at once would be.
    const newestFirst = [...listed].reverse();
    const started = performance.now();
    const rows = ordered(newestFirst, (n) => newestFirst.indexOf(n));
    const graph = layout(rows, heads, { first: "main" });
    const took = performance.now() - started;
    expect(graph.rows).toHaveLength(500);
    expect(graph.lanes).toBe(10);
    expect(took).toBeLessThan(500);
  });
});

describe("the graph's row order", () => {
  it("keeps the order asked for when it already puts changes above their parents", () => {
    const listed = nodes("c<b", "b<a", "a");
    expect(ordered(listed, (n) => listed.indexOf(n)).map((n) => n.id)).toEqual(["c", "b", "a"]);
  });

  it("puts a change above its parents even when the rank says otherwise, as for changes made in the same minute", () => {
    const listed = nodes("a", "m<b+w", "b<a", "w<a");
    expect(ordered(listed, (n) => listed.indexOf(n)).map((n) => n.id)).toEqual(["m", "b", "w", "a"]);
  });
});
