// The history panel: the branch's changes, newest first, and what can be done
// with one: see what it did, compare it, branch or baseline from it, export
// it, or take it out. Baselines are listed with it, and any two points can be
// compared, in the panel and, if asked, in the grid's Before and After columns.

import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  type Api,
  type ChangesetDocument,
  type HistoryDiff,
  type HistoryLog,
  type Rows,
  type Summary,
  failure,
} from "../api";
import type { Compared } from "../grid/columns";
import type { View } from "../view";
import { Comparison, comparedOf } from "./Comparison";
import { download } from "./download";
import { layout, ordered } from "./graph";
import { GraphCell, toneClass } from "./GraphCell";
import {
  type Baseline,
  type Branch,
  type Entry,
  madeAt,
  pointsOf,
  type Say,
  didWords,
  tagsOf,
} from "./words";


/**
 * "Now" as a point to compare: the branch shown, or its head, worked out as
 * each comparison is asked, so it follows a switch. A ref name can't hold `@`.
 */
export const NOW = "@now";

/** A small form asking for one line of text: a name, or a reason. */
function Ask({
  label,
  placeholder,
  action,
  busy,
  onDone,
  onCancel,
  children,
}: {
  label: string;
  placeholder: string;
  action: string;
  busy: boolean;
  onDone: (text: string) => void;
  onCancel: () => void;
  children?: ReactNode;
}) {
  const [text, setText] = useState("");
  return (
    <form
      className="history-ask"
      onSubmit={(event) => {
        event.preventDefault();
        if (text.trim()) onDone(text.trim());
      }}
    >
      {children}
      <label>
        <span>{label}</span>
        <input
          // The form opens on a click meant to type here.
          autoFocus
          value={text}
          placeholder={placeholder}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              onCancel();
            }
          }}
        />
      </label>
      <div className="history-buttons">
        <button type="submit" className="primary" disabled={busy || !text.trim()}>
          {action}
        </button>
        <button type="button" onClick={onCancel} disabled={busy}>
          Cancel
        </button>
      </div>
    </form>
  );
}

type Asking = { kind: "branch" | "baseline" | "revert"; at: string } | null;

export function HistoryPanel({
  api,
  summary,
  rows,
  view,
  onClose,
  onChanged,
  say,
  onCompared,
  proposed,
  active = true,
}: {
  api: Api;
  summary: Summary | null;
  rows: Rows | null;
  view: View;
  onClose: () => void;
  /** Loads what's shown again, after a change here. */
  onChanged: () => Promise<unknown>;
  say: Say;
  /** The comparison to show in the grid, or null for none. */
  onCompared: (compared: Compared | null) => void;
  /**
   * The proposals as last listed: a new listing, as when one is made or
   * decided anywhere, lists the history and reads the chosen change again.
   */
  proposed?: unknown;
  /**
   * Whether it's shown. Kept while hidden, so a comparison it shows in the
   * grid lasts; it lists nothing until shown again, and then shows its
   * comparison in the grid again.
   */
  active?: boolean;
}) {
  const head = summary?.head ?? null;
  const branch = summary?.branch ?? null;
  const [log, setLog] = useState<HistoryLog | { message: string; hint: string } | null>(null);
  const [branches, setBranches] = useState<Branch[]>([]);
  const [baselines, setBaselines] = useState<Baseline[]>([]);
  const [chosen, setChosen] = useState<string | null>(null);
  const [shown, setShown] = useState<ChangesetDocument | null>(null);
  const [asking, setAsking] = useState<Asking>(null);
  const [busy, setBusy] = useState(false);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [diff, setDiff] = useState<HistoryDiff | { message: string; hint: string } | null>(null);
  const [inGrid, setInGrid] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  // Changes listed; more on asking.
  const [limit, setLimit] = useState(200);
  // Proposals, waiting or decided, listed with the branch's changes.
  const [withProposals, setWithProposals] = useState(false);
  // Whether there may be older changes, or proposals, than those listed.
  const [more, setMore] = useState(false);
  // The changes listed that are on no branch: proposals.
  const [off, setOff] = useState<ReadonlySet<string>>(new Set());
  // The changes on the branch shown's own line: those it can take out.
  const [own, setOwn] = useState<ReadonlySet<string>>(new Set());

  // The log, branches and baselines, again whenever the head shown moves.
  const [listed, setListed] = useState(0);
  useEffect(() => {
    if (!head || !active) return;
    let live = true;
    const listing = api.branches().then(
      ({ answer }) => answer.branches,
      (): Branch[] => [],
    );
    void listing.then((listed) => live && setBranches(listed));
    // The branch's own changes, every other branch's, so the graph can draw
    // them all, and every change in the store, newest first: that gives the
    // order, the changes of branches deleted after they were merged, and,
    // when asked for, the proposals.
    const others = listing.then((listed) =>
      Promise.all(
        listed
          .filter((b) => b.name !== branch)
          // A branch deleted since it was listed has nothing to draw.
          .map((b) => api.log({ branch: b.name, limit }).catch(() => null)),
      ),
    );
    const ownLog = api.log({ limit });
    // Without every change, the branches' lines are still drawn.
    const every = api.log({ limit, all: true }).catch(() => null);
    void Promise.all([ownLog, every, others])
      .then(async ([own, listedAll, otherLogs]) => {
        const all = listedAll ?? { ...own, changesets: [] };
        const lines = [own, ...otherLogs.filter((l): l is HistoryLog => l !== null)];
        const byId = new Map(all.changesets.map((c) => [c.id, c]));
        const onBranches = new Map(lines.flatMap((l) => l.changesets.map((c) => [c.id, c] as const)));
        // A branch deleted after it was merged leaves its changes on no
        // branch's line, but its merge still reaches them.
        const reach = [...onBranches.values()].flatMap((c) => (c.second_parent ? [c.second_parent] : []));
        // Whether a merge reaches back past the changes listed.
        let cut = false;
        for (let id = reach.pop(); id !== undefined; id = reach.pop()) {
          if (onBranches.has(id)) continue;
          const found = byId.get(id);
          if (!found) {
            cut = true;
            continue;
          }
          onBranches.set(id, found);
          if (found.parent) reach.push(found.parent);
          if (found.second_parent) reach.push(found.second_parent);
        }
        let proposals: Entry[] = [];
        if (withProposals) {
          // The changes on no branch: proposals waiting or rejected, and the
          // originals of those accepted by replaying, which hold the decision.
          const off = all.changesets.filter((c) => !onBranches.has(c.id));
          // Waiting and rejected ones are proposals. An accepted one on no
          // branch is a proposal accepted by replaying only if it was decided:
          // an edit left by a deleted branch is on no branch too.
          const decided = await Promise.all(
            off.map((c) =>
              c.state !== "accepted"
                ? true
                : api.show(c.id).then(
                    (doc) => (doc.reviews ?? []).length > 0,
                    () => false,
                  ),
            ),
          );
          proposals = off.filter((_, k) => decided[k]);
        }
        // Any listing cut short may hide older changes, or older proposals.
        const older = [...lines, ...(withProposals || cut ? [all] : [])].some((l) => l.changesets.length >= limit);
        // Newest first as the store recorded them; older than all of those,
        // newest first by time. Either way above their parents.
        const recorded = new Map(all.changesets.map((c, k) => [c.id, k]));
        const entries = [...onBranches.values(), ...proposals];
        const byTime = [...entries].sort((a, b) => b.created.localeCompare(a.created));
        const rank = new Map(byTime.map((c, k) => [c.id, recorded.get(c.id) ?? all.changesets.length + k]));
        const changesets = ordered(entries, (c) => rank.get(c.id) ?? 0);
        // The branch's own line, through first parents: what it can take out,
        // including older changes listed only from another branch.
        const line = new Set<string>();
        for (let at = onBranches.get(own.head ?? head ?? ""); at && !line.has(at.id); at = at.parent ? onBranches.get(at.parent) : undefined) {
          line.add(at.id);
        }
        return { own, line, changesets, proposals, more: older };
      })
      .then(
        ({ own, line, changesets, proposals, more: older }) => {
          if (!live) return;
          setMore(older);
          setOff(new Set(proposals.map((c) => c.id)));
          setOwn(line);
          setLog({ ...own, changesets });
        },
        (error: unknown) => live && setLog(failure(error, "The history couldn't be read.")),
      );
    void api.baselines().then(
      ({ answer }) => live && setBaselines(answer.baselines),
      () => live && setBaselines([]),
    );
    return () => {
      live = false;
    };
    // Every reload brings a new summary: a branch made, switched or deleted
    // anywhere lists again, even where the head didn't move.
  }, [api, head, branch, summary, listed, limit, withProposals, proposed, active]);

  // The chosen entry's commands and impact, read again on each reload or
  // proposal listing, so a decision made meanwhile shows.
  useEffect(() => {
    if (!chosen || !active) return;
    let live = true;
    void api.show(chosen).then(
      (answer) => live && setShown(answer),
      () => live && setShown(null),
    );
    return () => {
      live = false;
    };
  }, [api, chosen, summary, proposed, active]);

  const entries = useMemo(() => (log && "changesets" in log ? log.changesets : []), [log]);
  const points = useMemo(
    () => [{ ref: NOW, label: `Now (${branch ?? "the point shown"})` }, ...pointsOf(branches, baselines, entries)],
    [branch, branches, baselines, entries],
  );
  const chosenEntry = entries.find((entry) => entry.id === chosen) ?? null;
  // Opens a change, or closes it if it's open. Kept the same between renders
  // so the graph's rows don't redraw when another change is chosen.
  const choose = useCallback((id: string) => {
    setAsking(null);
    setChosen((open) => (open === id ? null : id));
  }, []);
  // The graph: the checked-out branch in the first lane, else the branch shown.
  const graph = useMemo(
    () => layout(entries, branches, { first: branches.find((b) => b.current)?.name ?? branch, hollow: off }),
    [entries, branches, branch, off],
  );
  const baselinesAt = useMemo(() => {
    const at = new Map<string, string[]>();
    for (const baseline of baselines) at.set(baseline.at, [...(at.get(baseline.at) ?? []), baseline.name]);
    return at;
  }, [baselines]);

  // The grid shows the comparison while asked to, and none once the panel goes.
  useEffect(() => {
    // Hidden, it leaves the grid to what's shown; shown again, it puts its
    // comparison back, over one Review put there meanwhile.
    if (!active) return;
    onCompared(inGrid && diff && "diff" in diff ? comparedOf(diff, summary) : null);
  }, [inGrid, diff, summary, onCompared, active]);
  useEffect(() => () => onCompared(null), [onCompared]);

  /** Runs a change, then shows the result everywhere. */
  const change = useCallback(
    async (work: () => Promise<string>, failed: string) => {
      setBusy(true);
      try {
        const done = await work();
        setAsking(null);
        await onChanged();
        setListed((n) => n + 1);
        say(done, "done");
      } catch (error) {
        const { message, hint } = failure(error, failed);
        say(message, "bad", hint);
      } finally {
        setBusy(false);
      }
    },
    [onChanged, say],
  );

  // Only the latest comparison asked for is shown; one still on its way is
  // `loading`.
  const asked = useRef(0);
  const loading = useRef(false);
  // What "now" is: the branch shown, else its head.
  const now = useRef(branch ?? head ?? "");
  useEffect(() => {
    now.current = branch ?? head ?? "";
  }, [branch, head]);
  const compare = useCallback(
    async (a: string, b: string) => {
      const ticket = ++asked.current;
      loading.current = true;
      setFrom(a);
      setTo(b);
      setDiff(null);
      const point = (ref: string) => (ref === NOW ? now.current : ref);
      let answer: HistoryDiff | { message: string; hint: string };
      try {
        // Every item, so the grid's Before and After cover every changed activity.
        answer = await api.diff({ from: point(a), to: point(b), limit: 0 });
      } catch (error) {
        answer = failure(error, "The two points couldn't be compared.");
      }
      if (ticket !== asked.current) return;
      loading.current = false;
      setDiff(answer);
    },
    [api],
  );

  // A comparison asked for by a button is brought into view: the Compare
  // section at once, and its answer once it comes, as it's below the log.
  const compareHeading = useRef<HTMLHeadingElement>(null);
  const revealing = useRef(false);
  const compareShown = (a: string, b: string) => {
    revealing.current = true;
    compareHeading.current?.scrollIntoView({ block: "start" });
    void compare(a, b);
  };
  useEffect(() => {
    if (!diff || !revealing.current) return;
    revealing.current = false;
    const answer = ":scope > .history-compare ~ .history-result, :scope > .history-compare ~ .history-failed";
    compareHeading.current?.parentElement?.querySelector(answer)?.scrollIntoView({ block: "nearest" });
  }, [diff]);

  // A comparison shown is of points by name, so once the head moves, as
  // after a switch, an edit or a revert, it's asked again of what they are now.
  const shownPair = useRef({ from, to, shown: false });
  useEffect(() => {
    shownPair.current = { from, to, shown: diff !== null };
  }, [from, to, diff]);
  const point = `${branch ?? ""}@${head ?? ""}`;
  const comparedAt = useRef(point);
  useEffect(() => {
    if (comparedAt.current === point) return;
    comparedAt.current = point;
    const pair = shownPair.current;
    if (pair.from && pair.to && (pair.shown || loading.current)) {
      // Shown or on its way: asked again, which drops the older answer.
      void compare(pair.from, pair.to);
    } else {
      asked.current++;
      loading.current = false;
    }
  }, [point, compare]);

  const exportAt = useCallback(
    async (at: string) => {
      try {
        const { name, blob } = await api.exportXer({ at });
        download(name, blob);
        say(`Exported ${name}.`, "done");
      } catch (error) {
        const { message, hint } = failure(error, "That point couldn't be exported.");
        say(message, "bad", hint);
      }
    },
    [api, say],
  );

  if (!head) {
    return (
      <section className="history" aria-label="History">
        <header className="explain-head">
          <h2>History</h2>
          <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
            ×
          </button>
        </header>
        <p className="muted">
          This is a plain file, so it has no history. Start one with <code>totalfloat history init</code>, then serve the store.
        </p>
      </section>
    );
  }

  const did = shown && shown.id === chosen ? didWords(shown) : null;
  return (
    <section className="history" aria-label="History">
      <header className="explain-head">
        <h2>History{branch ? ` of ${branch}` : ""}</h2>
        <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
          ×
        </button>
      </header>

      {log && "message" in log && (
        <p className="history-failed" role="alert">
          {log.message} {log.hint && <span className="muted">{log.hint}</span>}
        </p>
      )}

      <h3 className="history-heading">Changes, newest first</h3>
      <label className="history-option">
        <input type="checkbox" checked={withProposals} onChange={(event) => setWithProposals(event.target.checked)} />
        Show proposals, waiting and decided
      </label>
      <ul className="history-log" aria-label="Changes, newest first">
        {entries.map((entry, k) => {
          // An accepted proposal's original, replayed onto the branch as another change.
          const tags = [...tagsOf(entry), ...(off.has(entry.id) && entry.state === "accepted" ? ["accepted proposal"] : [])];
          const row = graph.rows[k];
          const named = baselinesAt.get(entry.id) ?? [];
          return (
            <li key={entry.id} className="history-row">
              {row && (
                <GraphCell
                  row={row}
                  lanes={graph.lanes}
                  chosen={entry.id === chosen}
                  label={`${entry.short_id}: ${entry.message || entry.summary}`}
                  onChoose={choose}
                />
              )}
              <button type="button" className="history-entry" aria-pressed={entry.id === chosen} onClick={() => choose(entry.id)}>
                {((row?.heads.length ?? 0) > 0 || named.length > 0) && (
                  <span className="history-refs">
                    {row?.heads.map((name) => (
                      <span key={`branch-${name}`} className={`history-ref branch ${toneClass(row.tone)}`} title={`Branch ${name}`}>
                        {name}
                      </span>
                    ))}
                    {named.map((name) => (
                      <span key={`baseline-${name}`} className="history-ref baseline" title={`Baseline ${name}`}>
                        {name}
                      </span>
                    ))}
                  </span>
                )}
                <span className="history-message">
                  {entry.message || entry.summary}
                  {tags.map((tag) => (
                    <span key={tag} className={`history-tag ${tag}`}>
                      {tag}
                    </span>
                  ))}
                </span>
                <span className="history-meta">
                  <span className={entry.author_kind === "agent" ? "history-agent" : undefined}>
                    {entry.author}
                    {entry.author_kind === "agent" && " (agent)"}
                  </span>
                  {" · "}
                  {madeAt(entry.created)}
                  {" · "}
                  <code>{entry.short_id}</code>
                </span>
                {entry.message && <span className="history-summary muted">{entry.summary}</span>}
              </button>
              {entry.id === chosen && chosenEntry && (
                <div className="history-detail">
                  {did ? (
                    <>
                      {did.finishes.map((line) => (
                        <p key={line} className="history-impact">
                          {line.charAt(0).toUpperCase() + line.slice(1)}.
                        </p>
                      ))}
                      {did.applied.length > 0 && (
                        <ul className="history-applied" aria-label="What it changed">
                          {did.applied.map((line) => (
                            <li key={line}>{line}</li>
                          ))}
                        </ul>
                      )}
                      {(shown?.reviews ?? []).length > 0 && (
                        <ul className="history-reviews" aria-label="Decisions">
                          {(shown?.reviews ?? []).map((review) => (
                            <li key={`${review.state}-${review.created}`}>
                              <strong>{review.state === "accepted" ? "Accepted" : "Rejected"}</strong> by {review.author}
                              {review.author_kind === "agent" && " (agent)"}, {madeAt(review.created)}
                              {review.message && <>: {review.message}</>}
                            </li>
                          ))}
                        </ul>
                      )}
                    </>
                  ) : (
                    <p className="muted">Reading the change…</p>
                  )}
                  {asking?.at === entry.id ? (
                    asking.kind === "revert" ? (
                      <Ask
                        label="Why take it out"
                        placeholder="Reason (required)"
                        action="Take it out"
                        busy={busy}
                        onCancel={() => setAsking(null)}
                        onDone={(reason) =>
                          void change(async () => {
                            const { answer } = await api.revert({ at: entry.id, reason, ...(head ? { head } : {}) });
                            return answer.recorded ? `Took out ${entry.short_id}, as ${answer.recorded.id.slice(0, 10)}.` : `Took out ${entry.short_id}.`;
                          }, "The change couldn't be taken out.")
                        }
                      >
                        <p className="history-confirm">
                          Taking out <strong>{entry.message || entry.summary}</strong> records a new change that undoes{" "}
                          {did && did.applied.length > 0 ? "what's listed above" : "it"}. Nothing is deleted from the history.
                        </p>
                      </Ask>
                    ) : (
                      <Ask
                        label={asking.kind === "branch" ? "New branch's name" : "Baseline's name"}
                        placeholder={asking.kind === "branch" ? "what-if" : "submitted"}
                        action={asking.kind === "branch" ? "Create branch" : "Create baseline"}
                        busy={busy}
                        onCancel={() => setAsking(null)}
                        onDone={(name) =>
                          void change(async () => {
                            if (asking.kind === "branch") {
                              await api.branch({ action: "create", name, at: entry.id });
                              return `Created branch ${name} at ${entry.short_id}. Switch to it from the branch menu.`;
                            }
                            await api.baseline({ action: "create", name, at: entry.id });
                            return `Created baseline ${name} at ${entry.short_id}.`;
                          }, `The ${asking.kind} couldn't be created.`)
                        }
                      />
                    )
                  ) : off.has(entry.id) ? (
                    // A proposal isn't on the branch: there's nothing to take out, branch or baseline from.
                    <div className="history-buttons">
                      <button type="button" onClick={() => compareShown(entry.id, NOW)}>
                        Compare with now
                      </button>
                      <button type="button" onClick={() => void exportAt(entry.id)}>
                        Export XER
                      </button>
                    </div>
                  ) : (
                    <div className="history-buttons">
                      <button type="button" onClick={() => compareShown(entry.id, NOW)}>
                        Compare with now
                      </button>
                      <button type="button" disabled={!summary?.writable} onClick={() => setAsking({ kind: "branch", at: entry.id })}>
                        Branch from here…
                      </button>
                      <button type="button" onClick={() => setAsking({ kind: "baseline", at: entry.id })}>
                        Baseline from here…
                      </button>
                      <button type="button" onClick={() => void exportAt(entry.id)}>
                        Export XER
                      </button>
                      <button
                        type="button"
                        disabled={!summary?.writable || entry.kind === "import" || !own.has(entry.id)}
                        title={
                          entry.kind === "import"
                            ? "The import starts the history; there's nothing before it to go back to"
                            : !own.has(entry.id)
                              ? `Only a change on ${branch ? `${branch}'s` : "this point's"} own line can be taken out`
                              : undefined
                        }
                        onClick={() => setAsking({ kind: "revert", at: entry.id })}
                      >
                        Revert…
                      </button>
                    </div>
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ul>
      {more && (
        <button type="button" className="history-more" onClick={() => setLimit((n) => n + 200)}>
          Show older changes
        </button>
      )}

      <h3 className="history-heading">Baselines</h3>
      {baselines.length === 0 ? (
        <p className="muted">None yet. Choose a change above, then Baseline from here.</p>
      ) : (
        <ul className="history-baselines" aria-label="Baselines">
          {baselines.map((baseline) => (
            <li key={baseline.name}>
              <span className="history-message">
                {baseline.name} <code>{baseline.short_id}</code>
              </span>
              <span className="history-meta">
                {baseline.point} · {madeAt(baseline.created)}
              </span>
              <div className="history-buttons">
                <button
                  type="button"
                  aria-label={`Compare baseline ${baseline.name} with now`}
                  onClick={() => compareShown(baseline.name, NOW)}
                >
                  Compare with now
                </button>
                <button type="button" aria-label={`Export baseline ${baseline.name} as XER`} onClick={() => void exportAt(baseline.name)}>
                  Export XER
                </button>
                {deleting === baseline.name ? (
                  <span className="history-confirm-inline">
                    Delete {baseline.name}?
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        void change(async () => {
                          await api.baseline({ action: "delete", name: baseline.name });
                          setDeleting(null);
                          return `Deleted baseline ${baseline.name}. The changes it pointed at are still in the history.`;
                        }, "The baseline couldn't be deleted.")
                      }
                    >
                      Delete
                    </button>
                    <button type="button" onClick={() => setDeleting(null)}>
                      Keep
                    </button>
                  </span>
                ) : (
                  <button
                    type="button"
                    aria-label={`Delete baseline ${baseline.name}`}
                    disabled={busy}
                    onClick={() => setDeleting(baseline.name)}
                  >
                    Delete
                  </button>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}

      <h3 className="history-heading" ref={compareHeading}>
        Compare
      </h3>
      <form
        className="history-compare"
        aria-label="Compare two points"
        onSubmit={(event) => {
          event.preventDefault();
          if (from && to) compareShown(from, to);
        }}
      >
        <label>
          <span>From</span>
          <select aria-label="Compare from" value={from} onChange={(event) => setFrom(event.target.value)}>
            <option value="">Choose…</option>
            {points.map((point) => (
              <option key={`from-${point.ref}`} value={point.ref}>
                {point.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>To</span>
          <select aria-label="Compare to" value={to} onChange={(event) => setTo(event.target.value)}>
            <option value="">Choose…</option>
            {points.map((point) => (
              <option key={`to-${point.ref}`} value={point.ref}>
                {point.label}
              </option>
            ))}
          </select>
        </label>
        <button type="submit" disabled={!from || !to}>
          Compare
        </button>
      </form>
      {diff && "message" in diff && (
        <p className="history-failed" role="alert">
          {diff.message} {diff.hint && <span className="muted">{diff.hint}</span>}
        </p>
      )}
      {diff && "diff" in diff && (
        <Comparison diff={diff} rows={rows} summary={summary} view={view} say={say} inGrid={inGrid} onInGrid={setInGrid} />
      )}
    </section>
  );
}
