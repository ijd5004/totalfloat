// The History panel's graph: each branch a line, each change a dot on its
// branch's line, a merge a line joining two branches, and a proposal a hollow
// dot off its branch. A pure layout, from changesets to one row each: which
// lane its dot is in, and which lines pass through, come into and leave it.

/** A changeset, as the log lists it. */
export interface Node {
  id: string;
  parent: string | null;
  /** A merge's second parent: the head of the branch merged in. */
  second_parent?: string | null;
}

/** A branch: its name and the changeset it points at. */
export interface Head {
  name: string;
  head: string;
}

/**
 * A line's colour: a branch's, by its place in the lane order, or null for a
 * line whose branch has no name any more.
 */
export type Tone = number | null;

/** A line in a row: the lane it runs in at the row's top or bottom. */
export interface Line {
  lane: number;
  tone: Tone;
  /** It leads to or from a proposal, which isn't on the branch yet. */
  dashed: boolean;
}

export interface Row {
  id: string;
  /** The lane the dot is in. */
  lane: number;
  tone: Tone;
  /** A proposal: a hollow dot. */
  hollow: boolean;
  /** The branches whose head this is. */
  heads: string[];
  /** Lines passing the row, top to bottom, that don't touch the dot. */
  through: Line[];
  /** Lines coming into the dot from the row's top, in the lane they come from. */
  into: Line[];
  /** Lines leaving the dot for the row's bottom, in the lane they go to. */
  out: Line[];
}

export interface Graph {
  rows: Row[];
  /** How many lanes wide it is. */
  lanes: number;
}

/**
 * The order rows are drawn in: newest first as given by `rank` (lower first),
 * except that a change always comes before its parents, so every line runs
 * down the page. Changes recorded at the same moment keep `rank`'s order.
 */
export function ordered<T extends Node>(nodes: readonly T[], rank: (node: T) => number): T[] {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  // How many of each change's children are still to be placed.
  const waiting = new Map<string, number>();
  for (const node of nodes) {
    for (const parent of parentsOf(node)) {
      if (byId.has(parent)) waiting.set(parent, (waiting.get(parent) ?? 0) + 1);
    }
  }
  const ready = nodes.filter((node) => !waiting.get(node.id));
  const out: T[] = [];
  const placed = new Set<string>();
  while (ready.length > 0) {
    // The best ranked of those whose children are all placed.
    let best = 0;
    for (let k = 1; k < ready.length; k++) {
      const node = ready[k];
      const chosen = ready[best];
      if (node && chosen && rank(node) < rank(chosen)) best = k;
    }
    const [node] = ready.splice(best, 1);
    if (!node || placed.has(node.id)) continue;
    placed.add(node.id);
    out.push(node);
    for (const parent of parentsOf(node)) {
      const left = (waiting.get(parent) ?? 0) - 1;
      waiting.set(parent, left);
      const next = byId.get(parent);
      if (left === 0 && next) ready.push(next);
    }
  }
  // A cycle can't be recorded, but nothing listed is ever dropped.
  for (const node of nodes) if (!placed.has(node.id)) out.push(node);
  return out;
}

function parentsOf(node: Node): string[] {
  const parents: string[] = [];
  if (node.parent) parents.push(node.parent);
  if (node.second_parent && node.second_parent !== node.parent) parents.push(node.second_parent);
  return parents;
}

/** The most lanes proposals take; more waiting at once share them. */
const PROPOSAL_LANES = 3;
/** The most lanes other unnamed lines take, so a listing cut short can't widen the graph without end. */
const OTHER_LANES = 4;

interface Chain {
  /** Its rows, top to bottom: a branch's line through first parents. */
  rows: number[];
  named: boolean;
  tone: Tone;
  /** The first and last rows its lane is taken in. */
  top: number;
  end: number;
  lane: number;
}

/**
 * Lays out `nodes`, which are in row order with every change before its
 * parents (see `ordered`). Each branch's line is the changes reached from its
 * head through first parents that no branch before it reached; `first` comes
 * first, then the others by name, and each keeps its own lane. Changes no
 * branch reaches, as those of a branch since deleted, and the proposals in
 * `hollow`, make unnamed lines in the lanes after them, sharing a lane where
 * they don't overlap; proposals take at most `PROPOSAL_LANES` and the others
 * `OTHER_LANES`. A line whose
 * parent isn't listed runs to the bottom.
 */
export function layout(
  nodes: readonly Node[],
  heads: readonly Head[],
  options: { first?: string | null; hollow?: ReadonlySet<string> } = {},
): Graph {
  const count = nodes.length;
  const rowOf = new Map(nodes.map((node, k) => [node.id, k]));
  const hollow = options.hollow ?? new Set<string>();
  const chainOf = new Array<number>(count).fill(-1);
  const chains: Chain[] = [];

  // Walks first parents from `start` while no line holds them.
  const walk = (start: number | undefined, named: boolean, tone: Tone) => {
    const rows: number[] = [];
    let at = start;
    while (at !== undefined && chainOf[at] === -1) {
      chainOf[at] = chains.length;
      rows.push(at);
      const parent = nodes[at]?.parent;
      at = parent ? rowOf.get(parent) : undefined;
    }
    if (rows.length > 0) chains.push({ rows, named, tone, top: rows[0] ?? 0, end: 0, lane: 0 });
  };

  const order = [...heads].sort((a, b) => {
    if (a.name === options.first) return -1;
    if (b.name === options.first) return 1;
    return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
  });
  // A branch's colour is its place in that order, so it keeps it while
  // others gain or lose changes of their own.
  order.forEach((branch, k) => walk(rowOf.get(branch.head), true, k));
  const namedCount = chains.length;
  for (let k = 0; k < count; k++) if (chainOf[k] === -1) walk(k, false, null);

  const chainAt = (row: number): Chain | undefined => chains[chainOf[row] ?? -1];
  const isProposal = (chain: Chain) => !chain.named && chain.rows.every((row) => hollow.has(nodes[row]?.id ?? ""));
  // A proposal takes its branch's colour. Proposals are listed above their
  // parents, so a parent's line is coloured before the proposals made on it:
  // walk from the bottom up.
  for (let k = chains.length - 1; k >= namedCount; k--) {
    const chain = chains[k];
    const bottom = chain?.rows.at(-1);
    if (!chain || bottom === undefined || !isProposal(chain)) continue;
    const parent = nodes[bottom]?.parent;
    const below = parent === null || parent === undefined ? undefined : rowOf.get(parent);
    chain.tone = below === undefined ? null : (chainAt(below)?.tone ?? null);
  }

  // How far down each line's lane is taken: to its last change's parent, or
  // the bottom if that isn't listed; and up to any merge that took it in.
  for (const chain of chains) {
    const bottom = chain.rows.at(-1) ?? 0;
    const parent = nodes[bottom]?.parent;
    chain.end = !parent ? bottom : (rowOf.get(parent) ?? count);
  }
  // A merge whose second parent isn't listed takes a lane of its own from
  // the merge to the bottom.
  const unlisted = new Map<number, Chain>();
  nodes.forEach((node, k) => {
    if (!node.second_parent) return;
    const second = rowOf.get(node.second_parent);
    if (second === undefined) {
      unlisted.set(k, { rows: [], named: false, tone: null, top: k, end: count, lane: 0 });
      return;
    }
    const chain = chainAt(second);
    if (chain) chain.top = Math.min(chain.top, k);
  });

  // Branches in their own lanes, in order. Then proposals, each in the first
  // lane free over its rows, in at most a few lanes so that many waiting at
  // once don't widen the graph. Then other unnamed lines, as of a branch
  // since deleted, each in the first lane free over its rows.
  chains.forEach((chain, k) => {
    if (chain.named) chain.lane = k;
  });
  const place = (lines: Chain[], from: number, most = Infinity) => {
    const free: number[] = [];
    for (const chain of [...lines].sort((a, b) => a.top - b.top || a.end - b.end)) {
      let lane = free.findIndex((end) => end < chain.top);
      if (lane === -1 && free.length < most) lane = free.push(0) - 1;
      // All taken: the one that frees first.
      if (lane === -1) lane = free.indexOf(Math.min(...free));
      free[lane] = Math.max(free[lane] ?? 0, chain.end);
      chain.lane = from + lane;
    }
    return free.length;
  };
  const unnamed = chains.slice(namedCount);
  const after = namedCount + place(unnamed.filter(isProposal), namedCount, PROPOSAL_LANES);
  place([...unnamed.filter((chain) => !isProposal(chain)), ...unlisted.values()], after, OTHER_LANES);

  const rows: Row[] = nodes.map((node, k) => {
    const chain = chainAt(k);
    return {
      id: node.id,
      lane: chain?.lane ?? 0,
      tone: chain?.tone ?? null,
      hollow: hollow.has(node.id),
      heads: order.filter((branch) => branch.head === node.id).map((branch) => branch.name),
      through: [],
      into: [],
      out: [],
    };
  });
  const add = (lines: Line[], line: Line) => {
    if (!lines.some((l) => l.lane === line.lane && l.tone === line.tone && l.dashed === line.dashed)) lines.push(line);
  };
  // One edge from a change to a parent, in lane `lane` between their rows.
  const edge = (from: number, to: number, lane: number, tone: Tone, dashed: boolean) => {
    const line = { lane, tone, dashed };
    const start = rows[from];
    if (start) add(start.out, line);
    for (let k = from + 1; k < Math.min(to, count); k++) {
      const row = rows[k];
      if (row) add(row.through, line);
    }
    const end = rows[to];
    if (end) add(end.into, line);
  };
  nodes.forEach((node, k) => {
    const row = rows[k];
    if (!row) return;
    const dashed = row.hollow;
    // To the first parent in the change's own lane: down its line, or down
    // to where its branch started.
    if (node.parent) edge(k, rowOf.get(node.parent) ?? count, row.lane, row.tone, dashed);
    // To a merge's second parent in that parent's lane: across, then down.
    const second = node.second_parent ? rowOf.get(node.second_parent) : undefined;
    const into = second === undefined ? undefined : rows[second];
    if (second !== undefined && into) edge(k, second, into.lane, into.tone, dashed);
    const stub = unlisted.get(k);
    if (stub) edge(k, count, stub.lane, null, dashed);
  });

  const byLane = (a: Line, b: Line) => a.lane - b.lane;
  for (const row of rows) {
    row.through.sort(byLane);
    row.into.sort(byLane);
    row.out.sort(byLane);
  }

  const lanes = rows.reduce((most, row) => Math.max(most, row.lane + 1, ...row.through.map((l) => l.lane + 1)), 0);
  return { rows, lanes };
}
