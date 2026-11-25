// Saving, opening and deleting layouts: the files beside the schedule that
// keep how the grid and Gantt are set out.

import { useEffect, useRef, useState } from "react";

import { type Api, failure } from "../api";

export type LayoutMode = "save" | "open" | "delete";

const TITLES: Record<LayoutMode, string> = {
  save: "Save layout as",
  open: "Open a layout",
  delete: "Delete a layout",
};

export function LayoutDialog({
  api,
  mode,
  current,
  onSave,
  onOpen,
  onDeleted,
  onClose,
}: {
  api: Api;
  mode: LayoutMode;
  /** The layout in use, if it has a name. */
  current: string | null;
  /** Saves what's shown under `name`; rejects with why it couldn't. */
  onSave: (name: string) => Promise<void>;
  onOpen: (name: string) => Promise<void>;
  onDeleted: (name: string) => void;
  onClose: () => void;
}) {
  const [names, setNames] = useState<string[] | null>(null);
  const [name, setName] = useState(current ?? "");
  const [problem, setProblem] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dialog = useRef<HTMLElement>(null);
  // Where focus was as it opened, before it took focus itself, to give it back.
  const [returnTo] = useState(() => document.activeElement);

  useEffect(() => {
    let live = true;
    void api.layouts().then(
      (answer) => live && setNames(answer.layouts.map((l) => l.name)),
      (error: unknown) => {
        if (!live) return;
        setNames([]);
        setProblem(failure(error, "The layouts couldn't be listed.").message);
      },
    );
    return () => {
      live = false;
    };
  }, [api]);

  useEffect(() => {
    dialog.current?.querySelector<HTMLElement>("input, button:not(:disabled)")?.focus();
  }, [names]);

  // Focus stays in the dialog while it's open, and goes back where it was.
  useEffect(() => {
    const before = returnTo;
    const keepIn = (event: FocusEvent) => {
      if (dialog.current && event.target instanceof Node && !dialog.current.contains(event.target)) {
        dialog.current.querySelector<HTMLElement>("input, button:not(:disabled)")?.focus();
      }
    };
    document.addEventListener("focusin", keepIn);
    return () => {
      document.removeEventListener("focusin", keepIn);
      const back =
        before instanceof HTMLElement && before !== document.body && before.isConnected
          ? before
          : document.querySelector<HTMLElement>(".grid-body");
      back?.focus();
    };
  }, [returnTo]);

  // Names are the same however they're cased, as on Windows and macOS.
  const taken = names?.some((saved) => saved.toLowerCase() === name.trim().toLowerCase()) ?? false;

  const act = async (work: () => Promise<void>, failed: string) => {
    setBusy(true);
    setProblem(null);
    try {
      await work();
    } catch (error) {
      const { message, hint } = failure(error, failed);
      setProblem(hint ? `${message} (${hint})` : message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="overlay" onMouseDown={onClose}>
      <section
        ref={dialog}
        className="layout-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={TITLES[mode]}
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      >
        <h2>{TITLES[mode]}</h2>
        <p className="muted">
          Layouts are files beside the schedule, in its <code>.layouts</code> folder, so they can be shared and compared.
        </p>
        {mode === "save" && (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              const wanted = name.trim();
              if (wanted) void act(() => onSave(wanted), "The layout couldn't be saved.");
            }}
          >
            <label>
              <span>Name</span>
              <input value={name} placeholder="Look-ahead" onChange={(event) => setName(event.target.value)} />
            </label>
            {taken && <p className="muted">This replaces the layout of that name.</p>}
            <button type="submit" className="primary" disabled={busy || !name.trim()}>
              Save
            </button>
          </form>
        )}
        {names === null ? (
          <p className="muted">Listing layouts…</p>
        ) : names.length === 0 ? (
          mode !== "save" && <p className="muted">No layouts saved yet.</p>
        ) : (
          <ul className="layout-list" aria-label="Layouts">
            {names.map((saved) => (
              <li key={saved}>
                {mode === "save" ? (
                  <button type="button" onClick={() => setName(saved)}>
                    {saved}
                  </button>
                ) : mode === "open" ? (
                  <button type="button" disabled={busy} onClick={() => void act(() => onOpen(saved), `${saved} couldn't be opened.`)}>
                    {saved}
                    {saved.toLowerCase() === current?.toLowerCase() && <span className="muted"> (in use)</span>}
                  </button>
                ) : deleting === saved ? (
                  <span className="layout-confirm">
                    Delete {saved}?
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        void act(async () => {
                          await api.deleteLayout(saved);
                          setNames((now) => now?.filter((n) => n !== saved) ?? null);
                          setDeleting(null);
                          onDeleted(saved);
                        }, `${saved} couldn't be deleted.`)
                      }
                    >
                      Delete
                    </button>
                    <button type="button" onClick={() => setDeleting(null)}>
                      Keep
                    </button>
                  </span>
                ) : (
                  <button type="button" aria-label={`Delete layout ${saved}`} onClick={() => setDeleting(saved)}>
                    {saved}
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
        {problem && (
          <p className="history-failed" role="alert">
            {problem}
          </p>
        )}
        <div className="history-buttons">
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>
      </section>
    </div>
  );
}
