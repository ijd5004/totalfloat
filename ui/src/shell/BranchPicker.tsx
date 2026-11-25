// The branch picker in the title bar: which branch is shown, and the others
// to switch to; a new branch from the head shown; and deleting one.

import { useCallback, useEffect, useRef, useState } from "react";

import { type Api, type Summary, failure } from "../api";
import type { Say } from "../history/words";
import type { Branch } from "../history/words";
import { Icon } from "./icons";

export function BranchPicker({
  api,
  summary,
  onSwitch,
  onChanged,
  say,
  onMerge,
}: {
  api: Api;
  summary: Summary;
  /** Switches the store to `name` and shows it; resolves with the branch shown after. */
  onSwitch: (name: string) => Promise<void>;
  /** Loads what's shown again, after a branch is made or deleted. */
  onChanged: () => Promise<unknown>;
  say: Say;
  /** Starts merging branch `name` into the one shown. */
  onMerge?: (name: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [branches, setBranches] = useState<Branch[] | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  // Bumped to list the branches again.
  const [listed, setListed] = useState(0);
  // Placed under the button; the title bar clips what overflows it, so the popup is fixed.
  const [place, setPlace] = useState({ top: 0, left: 0 });
  const button = useRef<HTMLButtonElement>(null);
  const popup = useRef<HTMLDivElement>(null);

  const close = useCallback((refocus: boolean) => {
    setOpen(false);
    setDeleting(null);
    if (refocus) button.current?.focus();
  }, []);

  // The list, each time it opens.
  useEffect(() => {
    if (!open) return;
    let live = true;
    void api.branches().then(
      ({ answer }) => live && setBranches(answer.branches),
      (error: unknown) => {
        if (!live) return;
        setBranches([]);
        const { message, hint } = failure(error, "The branches couldn't be listed.");
        say(message, "bad", hint);
      },
    );
    return () => {
      live = false;
    };
  }, [api, open, say, listed]);

  // A click or focus anywhere else closes it.
  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      const target = event.target as Node;
      if (!popup.current?.contains(target) && !button.current?.contains(target)) close(false);
    };
    window.addEventListener("mousedown", away);
    window.addEventListener("focusin", away);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("focusin", away);
    };
  }, [open, close]);

  // Focus goes to the first branch as it opens.
  useEffect(() => {
    if (open && branches) popup.current?.querySelector<HTMLElement>("button:not(:disabled), input")?.focus();
  }, [open, branches]);

  const act = async (work: () => Promise<void>, failed: string) => {
    setBusy(true);
    try {
      await work();
    } catch (error) {
      const { message, hint } = failure(error, failed);
      say(message, "bad", hint);
    } finally {
      setBusy(false);
    }
  };

  const shown = summary.branch;
  // The branch the store has checked out, which edits made elsewhere record on;
  // a page pinned to a branch can show another.
  const checkedOut = branches?.find((b) => b.current)?.name ?? null;
  return (
    <div className="branch-picker">
      <button
        ref={button}
        type="button"
        className="branch-button"
        aria-haspopup="dialog"
        aria-expanded={open}
        title={`${summary.writable ? "" : "Read-only. "}Head ${summary.head ?? ""}`}
        onClick={() => {
          if (open) {
            close(false);
            return;
          }
          const box = button.current?.getBoundingClientRect();
          if (box) setPlace({ top: box.bottom + 2, left: Math.max(4, Math.min(box.left, window.innerWidth - 344)) });
          setOpen(true);
        }}
      >
        <span className="fact-label">{shown ? "Branch" : "Point in history"}</span>
        <span className="fact-value">
          <Icon name="branch" size={13} className="tb-quiet" />
          {shown ?? (
            <>
              fixed point <code className="head">{summary.head?.slice(0, 10)}</code>
            </>
          )}
          <Icon name="down" size={12} className="tb-quiet" />
        </span>
      </button>
      {open && (
        <div
          ref={popup}
          className="branch-popup"
          style={place}
          role="dialog"
          aria-label="Branches"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              close(true);
            }
          }}
        >
          {branches === null ? (
            <p className="muted">Listing branches…</p>
          ) : (
            <ul className="branch-list" aria-label="Branches">
              {branches.map((b) => (
                <li key={b.name} className={b.name === shown ? "shown" : undefined}>
                  <button
                    type="button"
                    className="branch-switch"
                    aria-current={b.name === shown ? "true" : undefined}
                    disabled={busy || (b.name === shown && b.name === checkedOut)}
                    onClick={() =>
                      void act(async () => {
                        await onSwitch(b.name);
                        close(true);
                      }, `${b.name} couldn't be switched to.`)
                    }
                  >
                    <span className="branch-name">
                      {b.name === shown && <span aria-hidden="true">✓ </span>}
                      {b.name}
                      {b.current && b.name !== shown && <span className="muted"> (checked out)</span>}
                    </span>
                    <span className="muted">
                      {b.last_change} · <code>{b.short_id}</code>
                    </span>
                  </button>
                  {/* A merge records on the checked-out branch, so only into it. */}
                  {onMerge && shown && b.name !== shown && summary.writable && checkedOut === shown && (
                    <button
                      type="button"
                      className="branch-merge"
                      aria-label={`Merge ${b.name} into ${shown}`}
                      title={`Merge ${b.name} into ${shown}`}
                      disabled={busy}
                      onClick={() => {
                        close(false);
                        onMerge(b.name);
                      }}
                    >
                      Merge
                    </button>
                  )}
                  {b.name !== shown && !b.current &&
                    (deleting === b.name ? (
                      <span className="branch-confirm">
                        Delete {b.name}?
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() =>
                            void act(async () => {
                              await api.branch({ action: "delete", name: b.name });
                              setBranches((now) => now?.filter((x) => x.name !== b.name) ?? null);
                              setDeleting(null);
                              await onChanged();
                              say(`Deleted branch ${b.name}. Its changes are still in the history.`, "done");
                            }, `${b.name} couldn't be deleted.`)
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
                        className="branch-delete"
                        aria-label={`Delete branch ${b.name}`}
                        disabled={busy}
                        onClick={() => setDeleting(b.name)}
                      >
                        Delete
                      </button>
                    ))}
                </li>
              ))}
            </ul>
          )}
          <form
            className="branch-new"
            onSubmit={(event) => {
              event.preventDefault();
              const wanted = name.trim();
              if (!wanted) return;
              void act(async () => {
                await api.branch({ action: "create", name: wanted, ...(summary.head ? { at: summary.head } : {}) });
                setName("");
                setBranches(null);
                try {
                  await onSwitch(wanted);
                } catch (error) {
                  // Made, but not switched to: say so, and show it in the list.
                  const { message, hint } = failure(error, "");
                  say(`Created ${wanted}, but couldn't switch to it${message ? `: ${message}` : "."}`, "bad", hint);
                  setListed((n) => n + 1);
                  return;
                }
                close(true);
              }, `${wanted} couldn't be created.`);
            }}
          >
            <label>
              <span>New branch from what's shown</span>
              <input value={name} placeholder="what-if" onChange={(event) => setName(event.target.value)} />
            </label>
            <button type="submit" className="primary" disabled={busy || !name.trim()}>
              Create and switch
            </button>
          </form>
        </div>
      )}
    </div>
  );
}
