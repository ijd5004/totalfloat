// Merging another branch into the one shown (ARCHITECTURE.md, History and changesets): first a dry run of
// the merge, which shows what it would record on this branch, then the merge,
// which replays the other branch's edits. A merge that clashes lists each
// clash, with both values and the changes that set them, and changes nothing.

import { useCallback, useEffect, useRef, useState } from "react";

import { type Api, ApiError, type HistoryDiff, type Merged, type Rows, type Summary, failure } from "../api";
import { type Compared, valueText } from "../grid/columns";
import type { View } from "../view";
import { type Clash, type Shown, clashOf, setting, sinceParting } from "./clashes";
import { type Compare, Comparison, comparedOf } from "./Comparison";
import { type Say, fieldWords } from "./words";

/** A refused merge's clashes, and the changes on each side that set them, once read. */
interface Clashes {
  message: string;
  clashes: Clash[];
  ours: Shown[] | null;
  theirs: Shown[] | null;
}

type Outcome = ({ kind: "clashes" } & Clashes) | { kind: "refused"; message: string; hint: string } | null;

/**
 * What merging would do, from its dry run: replay the other branch's edits
 * (with the change that records), move this branch forward to the other's
 * head, or nothing; or the clashes that stop it.
 */
type Preview =
  | { kind: "replay"; diff: Compare | null }
  | { kind: "forward"; diff: Compare | null }
  | { kind: "nothing" }
  | { kind: "clashes"; list: Clashes | null }
  | { kind: "failed"; message: string; hint: string };

export function MergeDialog({
  api,
  summary,
  rows,
  view,
  from,
  onClose,
  onChanged,
  say,
  onCompared,
}: {
  api: Api;
  summary: Summary;
  rows: Rows | null;
  view: View;
  /** The branch to merge into the one shown. */
  from: string;
  onClose: () => void;
  onChanged: () => Promise<unknown>;
  say: Say;
  onCompared: (compared: Compared | null) => void;
}) {
  const into = summary.branch ?? "";
  // The preview, with the head of this branch it was made at, and the page's
  // summary it was asked for: each reload, which either branch moving brings,
  // asks again, and until it's answered there's nothing to merge on.
  const [made, setPreview] = useState<{ head: string | null; asked: Summary; answer: Preview } | null>(null);
  const [inGrid, setInGrid] = useState(false);
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<Outcome>(null);
  const [message, setMessage] = useState("");
  const dialog = useRef<HTMLElement>(null);

  /**
   * Lists a refused merge's clashes: their values from the comparison of the
   * two heads, then which changes on each side, since the branches parted,
   * set each value, each step given to `put`. Nothing changed.
   */
  const listClashes = useCallback(
    async (error: ApiError, put: (list: Clashes) => void) => {
      const codes = new Map(summary.inspect.projects.map((p) => [p.id, p.short_name]));
      const heads: HistoryDiff | null = await api.diff({ from: into, to: from }).catch(() => null);
      const clashes = error.problems.map((problem) => clashOf(problem, heads, (id) => codes.get(id)));
      put({ message: error.message, clashes, ours: null, theirs: null });
      try {
        const [mine, other] = await Promise.all([api.log({ branch: into, limit: 200 }), api.log({ branch: from, limit: 200 })]);
        const sides = await sinceParting(
          (id) => api.show(id),
          { head: mine.head ?? "", known: new Set(mine.changesets.map((c) => c.id)) },
          { head: other.head ?? "", known: new Set(other.changesets.map((c) => c.id)) },
        );
        put({ message: error.message, clashes, ...sides });
      } catch {
        // The clashes stand without the changes that set them.
      }
    },
    [api, from, into, summary],
  );

  // The merge's dry run, first, and again whenever the page reloads (either
  // branch moved): a merge is only offered on a preview of this branch as
  // it's shown. A replay previews the change it would record; a fast-forward
  // previews the other head, as this branch would point there.
  const head = summary.head;
  useEffect(() => {
    let live = true;
    const show = (answer: Preview) => live && setPreview({ head, asked: summary, answer });
    const preview = async (): Promise<Preview> => {
      const { answer } = await api.merge({ branch: from, dry_run: true, ...(head ? { head } : {}) });
      return previewOf(answer, () => api.diff({ from: into, to: from }));
    };
    preview().then(show, (error: unknown) => {
      if (error instanceof ApiError && error.code === "merge-conflict") {
        show({ kind: "clashes", list: null });
        void listClashes(error, (list) => show({ kind: "clashes", list }));
      } else {
        show({ kind: "failed", ...failure(error, `${from} couldn't be compared with ${into}.`) });
      }
    });
    return () => {
      live = false;
    };
  }, [api, into, from, head, summary, listClashes]);

  const preview = made && made.asked === summary ? made.answer : null;
  const mergeable = preview?.kind === "replay" || preview?.kind === "forward";
  // The clashes the dry run found, or a merge refused since.
  const listed = outcome?.kind === "clashes" ? outcome : preview?.kind === "clashes" ? preview.list : null;
  const compared = preview && (preview.kind === "replay" || preview.kind === "forward") ? preview.diff : null;
  const waiting =
    preview === null
      ? "Waiting for a preview of the merge of the two branches as they are now"
      : preview.kind === "clashes"
        ? "The branches clash, so the merge would record nothing"
        : preview.kind === "nothing"
          ? "There's nothing to merge"
          : "The merge couldn't be previewed";
  useEffect(() => {
    onCompared(compared && inGrid ? comparedOf(compared, summary) : null);
  }, [compared, inGrid, summary, onCompared]);
  useEffect(() => () => onCompared(null), [onCompared]);

  // Focus starts in the dialog and stays there; it goes back to the branch
  // button when the dialog closes, as the picker that opened it is gone.
  useEffect(() => {
    const before = document.activeElement;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    const keepIn = (event: FocusEvent) => {
      if (dialog.current && event.target instanceof Node && !dialog.current.contains(event.target)) {
        dialog.current.querySelector<HTMLElement>("button")?.focus();
      }
    };
    document.addEventListener("focusin", keepIn);
    return () => {
      document.removeEventListener("focusin", keepIn);
      const back =
        before instanceof HTMLElement && before !== document.body && before.isConnected
          ? before
          : document.querySelector<HTMLElement>(".branch-button");
      back?.focus();
    };
  }, []);

  const merge = useCallback(async () => {
    setBusy(true);
    setOutcome(null);
    try {
      const reason = message.trim();
      const { answer } = await api.merge({
        branch: from,
        ...(reason ? { reason } : {}),
        // Guarded by the head the preview shown was made at.
        ...(made?.head ? { head: made.head } : {}),
      });
      await onChanged();
      if (answer.recorded) say(`Merged ${from} into ${into} as ${answer.recorded.short_id}.`, "done");
      else if (answer.fast_forward) say(`${into} had no changes of its own, so it now points where ${from} does.`, "done");
      else say(`${into} already has everything on ${from}; nothing was recorded.`, "info");
      onClose();
    } catch (error) {
      if (error instanceof ApiError && error.code === "merge-conflict") {
        await listClashes(error, (list) => setOutcome({ kind: "clashes", ...list }));
      } else {
        const { message, hint } = failure(error, `${from} couldn't be merged.`);
        setOutcome({ kind: "refused", message, hint });
      }
    } finally {
      setBusy(false);
    }
  }, [api, made, from, into, message, onChanged, onClose, say, listClashes]);

  /** Selects a clash's activity, if the schedule shown has it. */
  const select = (clash: Clash) => {
    const i = rows ? rows.code.findIndex((c, k) => c === clash.code && (clash.project === null || rows.project[k] === clash.project)) : -1;
    if (!rows || i < 0) {
      say(`${clash.code ?? "That activity"} isn't in the schedule shown.`, "info");
      return;
    }
    view.reveal(rows.id[i] ?? -1);
  };

  /** A clash's value as the grid shows it: spans in days of the activity's calendar, where it's known. */
  const value = (clash: Clash, text: string) => {
    const i = rows ? rows.code.findIndex((c, k) => c === clash.code && (clash.project === null || rows.project[k] === clash.project)) : -1;
    return valueText(text, i >= 0 ? rows?.hours_per_day?.[i] : undefined);
  };

  /** The changes on a side that set a clash's value, once they've been read. */
  const changes = (docs: Shown[] | null, clash: Clash) => {
    if (docs === null) return "finding the changes…";
    const found = setting(docs, clash);
    return found.length === 0 ? "no change found" : found.map((d) => `${d.short_id} ${d.message || d.summary}`).join("; ");
  };

  return (
    <div className="overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section
        ref={dialog}
        className="merge-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={`Merge ${from} into ${into}`}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      >
        <header className="explain-head">
          <h2>
            Merge <code>{from}</code> into <code>{into}</code>
          </h2>
          <button type="button" className="explain-close" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </header>
        <p className="muted">
          Merging replays the edits made on {from} onto {into}. If both changed the same thing differently, nothing is
          changed and each clash is listed.
        </p>
        {preview === null && <p className="muted">Working out what the merge would change…</p>}
        {preview?.kind === "failed" && (
          <p className="history-failed" role="alert">
            {preview.message} {preview.hint && <span className="muted">{preview.hint}</span>}
          </p>
        )}
        {preview?.kind === "nothing" && (
          <p className="history-impact">
            {into} already has everything on {from}, so there's nothing to merge.
          </p>
        )}
        {preview?.kind === "replay" && <h3 className="merge-preview">What the merge would change on {into}</h3>}
        {preview?.kind === "forward" && (
          <h3 className="merge-preview">
            {into} has no changes of its own, so merging moves it to where {from} is
          </h3>
        )}
        {(preview?.kind === "replay" || preview?.kind === "forward") && compared === null && (
          <p className="muted">What it changes couldn't be worked out.</p>
        )}
        {compared && (
          <Comparison diff={compared} rows={rows} summary={summary} view={view} say={say} inGrid={inGrid} onInGrid={setInGrid} />
        )}
        {listed && (
          <div className="review-conflicts" role="alert">
            <p>
              <strong>{listed.message}</strong>
            </p>
            <p className="muted">Nothing was changed on either branch.</p>
            <ul aria-label="Clashes">
              {listed.clashes.map((clash, k) => (
                <li key={`${clash.message}-${k}`}>
                  {clash.code ? (
                    <button type="button" className="history-activity" onClick={() => select(clash)}>
                      <strong>
                        {clash.code}
                        {clash.field && ` ${fieldWords(clash.field).toLowerCase()}`}
                      </strong>
                      {clash.ours !== null && clash.theirs !== null ? (
                        <>
                          <span>
                            {into}: {value(clash, clash.ours)} ({changes(listed.ours, clash)})
                          </span>
                          <span>
                            {from}: {value(clash, clash.theirs)} ({changes(listed.theirs, clash)})
                          </span>
                        </>
                      ) : (
                        <span>{clash.message}</span>
                      )}
                    </button>
                  ) : (
                    clash.message
                  )}
                </li>
              ))}
            </ul>
          </div>
        )}
        {outcome?.kind === "refused" && (
          <p className="history-failed" role="alert">
            {outcome.message} {outcome.hint && <span className="muted">{outcome.hint}</span>}
          </p>
        )}
        <label className="merge-message">
          <span>Message (optional)</span>
          <input value={message} placeholder={`Merge ${from}`} onChange={(event) => setMessage(event.target.value)} />
        </label>
        <div className="history-buttons">
          <button type="button" className="primary" disabled={busy || !summary.writable || !into || !mergeable}
            title={mergeable ? undefined : waiting}
            onClick={() => void merge()}
          >
            Merge {from} into {into}
          </button>
          <button type="button" disabled={busy} onClick={onClose}>
            Cancel
          </button>
        </div>
      </section>
    </div>
  );
}

/**
 * A merge's preview from its dry run. A replay's impact is the change it
 * would record; a fast-forward records nothing new, and this branch then
 * points at the other's head, so the difference between the heads (`heads`)
 * is what changes.
 */
export async function previewOf(answer: Merged, heads: () => Promise<HistoryDiff>): Promise<Preview> {
  if (answer.impact) return { kind: "replay", diff: { diff: answer.impact as HistoryDiff["diff"] } };
  if (answer.fast_forward) return { kind: "forward", diff: await heads().catch(() => null) };
  // Edits replayed with no impact: the merged file couldn't be compared.
  if (answer.replayed.length > 0) return { kind: "replay", diff: null };
  return { kind: "nothing" };
}
