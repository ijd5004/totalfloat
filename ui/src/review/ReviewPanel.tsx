// The review panel: proposals waiting for a person, as an agent's edits
// arrive (ARCHITECTURE.md, History and changesets). Opening one shows what it does, its impact against the
// point it was made on, and its changed activities lit in the grid and the
// Gantt; then it's accepted, or rejected with a reason.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  type Api,
  ApiError,
  type ChangesetDocument,
  type HistoryDiff,
  type Problem,
  type Proposals,
  type Rows,
  type Summary,
  failure,
} from "../api";
import type { DrivingPath } from "../gantt/draw";
import type { Compared } from "../grid/columns";
import { Comparison, comparedOf, idsOf } from "../history/Comparison";
import { type Say, didWords, madeAt } from "../history/words";
import type { View } from "../view";

export type Proposal = Proposals["proposals"][number];

/** A decision under way on a proposal, or the conflicts its accept met, kept with its ID. */
type Deciding =
  | { kind: "reject"; id: string }
  | { kind: "conflicts"; id: string; message: string; hint: string; problems: Problem[] }
  | null;

export function ReviewPanel({
  api,
  summary,
  rows,
  view,
  proposals,
  onClose,
  onChanged,
  say,
  onCompared,
  onHighlight,
}: {
  api: Api;
  summary: Summary | null;
  rows: Rows | null;
  view: View;
  /** The proposals waiting, newest first, or why they couldn't be listed. */
  proposals: Proposal[] | { message: string; hint: string } | null;
  onClose: () => void;
  /** Loads what's shown, and the proposals, again after a decision. */
  onChanged: () => Promise<unknown>;
  say: Say;
  /** The opened proposal's changes, for the grid's Before and After columns. */
  onCompared: (compared: Compared | null) => void;
  /** The opened proposal's changed activities, lit in the Gantt. */
  onHighlight: (path: DrivingPath | null) => void;
}) {
  const [open, setOpen] = useState<string | null>(null);
  // Answers about a proposal, kept with its ID so another's never shows.
  const [shownOf, setShown] = useState<{ id: string; doc: ChangesetDocument | null; failed?: string } | null>(null);
  const [impactOf, setImpact] = useState<{ id: string; answer: HistoryDiff | { message: string; hint: string } } | null>(null);
  const [inGrid, setInGrid] = useState(true);
  const [deciding, setDeciding] = useState<Deciding>(null);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);

  const list = Array.isArray(proposals) ? proposals : [];
  // A proposal decided elsewhere leaves the list, and so closes here.
  const opened = list.find((p) => p.id === open) ?? null;
  const read = shownOf && shownOf.id === opened?.id ? shownOf : null;
  const shown = read?.doc ?? null;
  const unread = read?.failed ?? null;
  const impact = impactOf && impactOf.id === opened?.id ? impactOf.answer : null;

  // The opened proposal: its commands, and its impact against the point it was made on.
  const asked = useRef(0);
  const openedId = opened?.id ?? null;
  const openedParent = opened?.parent ?? null;
  useEffect(() => {
    if (!openedId) return;
    const ticket = ++asked.current;
    void api.show(openedId).then(
      (doc) => ticket === asked.current && setShown({ id: openedId, doc }),
      (error: unknown) =>
        ticket === asked.current && setShown({ id: openedId, doc: null, failed: failure(error, "Its edits couldn't be read.").message }),
    );
    void api.diff({ ...(openedParent ? { from: openedParent } : {}), to: openedId }).then(
      (answer) => ticket === asked.current && setImpact({ id: openedId, answer }),
      (error: unknown) =>
        ticket === asked.current && setImpact({ id: openedId, answer: failure(error, "Its impact couldn't be worked out.") }),
    );
  }, [api, openedId, openedParent]);

  // Its changes in the grid's Before and After columns and lit in the Gantt, while it's open.
  const diff = impact && "diff" in impact ? impact : null;
  const ids = useMemo(() => (diff && rows ? idsOf(diff, rows, summary) : null), [diff, rows, summary]);
  useEffect(() => {
    onCompared(diff && inGrid ? comparedOf(diff, summary) : null);
  }, [diff, inGrid, summary, onCompared]);
  useEffect(() => {
    onHighlight(ids && ids.size > 0 ? { ids, relationships: new Set(), edges: new Set() } : null);
  }, [ids, onHighlight]);
  useEffect(
    () => () => {
      onCompared(null);
      onHighlight(null);
    },
    [onCompared, onHighlight],
  );

  const decide = useCallback(
    async (proposal: Proposal, decision: "accept" | "reject", why?: string) => {
      setBusy(true);
      try {
        // No head guard: accepting replays onto the branch as it is now,
        // and rejecting changes no branch.
        const { answer } =
          decision === "accept"
            ? await api.accept({ proposal: proposal.id })
            : await api.reject({ proposal: proposal.id, reason: why ?? "" });
        setOpen(null);
        setDeciding(null);
        setReason("");
        await onChanged();
        if (decision === "reject") say(`Rejected ${proposal.short_id}.`, "done");
        else if (answer.replayed) {
          say(
            `Accepted ${proposal.short_id}: ${proposal.for_branch} had moved on, so it was replayed on its head as ${answer.recorded?.slice(0, 10) ?? "a new change"}.`,
            "done",
          );
        } else say(`Accepted ${proposal.short_id} onto ${proposal.for_branch}.`, "done");
      } catch (error) {
        if (decision === "accept" && error instanceof ApiError && error.code === "accept-conflict") {
          // Nothing changed: say what no longer applies.
          setDeciding({ kind: "conflicts", id: proposal.id, message: error.message, hint: error.hint, problems: error.problems });
        } else {
          const { message, hint } = failure(error, `${proposal.short_id} couldn't be ${decision}ed.`);
          say(message, "bad", hint);
        }
      } finally {
        setBusy(false);
      }
    },
    [api, onChanged, say],
  );

  const did = shown ? didWords(shown) : null;
  return (
    <section className="history review" aria-label="Review">
      <header className="explain-head">
        <h2>Proposals to review</h2>
        <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
          ×
        </button>
      </header>
      {!summary?.head ? (
        <p className="muted">
          A plain file has no proposals. Serve a history store, and an agent's edits made with <code>--propose</code> wait here.
        </p>
      ) : proposals && "message" in proposals ? (
        <p className="history-failed" role="alert">
          {proposals.message} {proposals.hint && <span className="muted">{proposals.hint}</span>}
        </p>
      ) : list.length === 0 ? (
        <p className="muted">Nothing waiting. Proposals appear here as they're made.</p>
      ) : (
        <ul className="history-log" aria-label="Proposals waiting">
          {list.map((proposal) => (
            <li key={proposal.id}>
              <button
                type="button"
                className="history-entry"
                aria-pressed={proposal.id === open}
                onClick={() => {
                  setDeciding(null);
                  setReason("");
                  setOpen(proposal.id === open ? null : proposal.id);
                }}
              >
                <span className="history-message">{proposal.reason || "No reason given"}</span>
                <span className="history-meta">
                  <span className={proposal.author_kind === "agent" ? "history-agent" : undefined}>
                    {proposal.author}
                    {proposal.author_kind === "agent" && " (agent)"}
                  </span>
                  {" · "}
                  {madeAt(proposal.created)}
                  {" · for "}
                  <strong>{proposal.for_branch}</strong>
                </span>
                {proposal.impact && <span className="history-summary muted">{proposal.impact}</span>}
              </button>
              {proposal.id === open && (
                <div className="history-detail">
                  {did ? (
                    // The finish it gives is in the comparison below.
                    did.applied.length > 0 && (
                      <ul className="history-applied" aria-label="What it changes">
                        {did.applied.map((line) => (
                          <li key={line}>{line}</li>
                        ))}
                      </ul>
                    )
                  ) : unread ? (
                    <p className="history-failed">{unread}</p>
                  ) : (
                    <p className="muted">Reading the proposal…</p>
                  )}
                  {impact && "message" in impact && (
                    <p className="history-failed" role="alert">
                      {impact.message}
                    </p>
                  )}
                  {diff && (
                    <Comparison diff={diff} rows={rows} summary={summary} view={view} say={say} inGrid={inGrid} onInGrid={setInGrid} />
                  )}
                  {deciding?.kind === "conflicts" && deciding.id === proposal.id && (
                    <div className="review-conflicts" role="alert">
                      <p>
                        <strong>{deciding.message}</strong>
                      </p>
                      <p className="muted">Nothing was changed. {deciding.hint}</p>
                      <ul aria-label="Conflicts">
                        {deciding.problems.map((problem, k) => (
                          <li key={`${problem.code}-${k}`}>
                            {problem.message}
                            {problem.hint && <span className="muted"> · {problem.hint}</span>}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                  {deciding?.kind === "reject" && deciding.id === proposal.id ? (
                    <form
                      className="history-ask"
                      onSubmit={(event) => {
                        event.preventDefault();
                        if (reason.trim()) void decide(proposal, "reject", reason.trim());
                      }}
                    >
                      <label>
                        <span>Why reject it</span>
                        <input
                          // The form opens on a click meant to type here.
                          autoFocus
                          value={reason}
                          placeholder="Reason (required)"
                          onChange={(event) => setReason(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Escape") {
                              event.preventDefault();
                              setDeciding(null);
                              setReason("");
                            }
                          }}
                        />
                      </label>
                      <div className="history-buttons">
                        <button type="submit" className="primary" disabled={busy || !reason.trim()}>
                          Reject
                        </button>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => {
                            setDeciding(null);
                            setReason("");
                          }}
                        >
                          Cancel
                        </button>
                      </div>
                    </form>
                  ) : (
                    <div className="history-buttons">
                      <button
                        type="button"
                        className="primary"
                        disabled={busy || !summary?.writable}
                        onClick={() => void decide(proposal, "accept")}
                      >
                        Accept
                      </button>
                      <button type="button" disabled={busy || !summary?.writable} onClick={() => setDeciding({ kind: "reject", id: proposal.id })}>
                        Reject…
                      </button>
                    </div>
                  )}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
