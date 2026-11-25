import type { KeyboardEvent, ReactNode } from "react";

import type { Summary } from "../api";
import { describeKey } from "../commands";
import { type EditState, type Editing, waiting } from "../edit/editing";
import type { Connection } from "../events";
import { readable } from "../explain/narrate";
import { Icon, Mark } from "./icons";

/** A ruled cell of the title block: a quiet label over its value. */
function Fact({ label, value, title }: { label: string; value: ReactNode; title?: string | undefined }) {
  return (
    <div className="fact" title={title}>
      <span className="fact-label">{label}</span>
      <span className="fact-value">{value}</span>
    </div>
  );
}

/** A top-level view: what the work area below the toolbar shows. */
export interface ViewTab<Id extends string = string> {
  id: Id;
  label: string;
  /** Its key, as the registry binds it, for the tooltip. */
  key?: string | undefined;
}

/** The finish and float the title block shows, and how waiting edits move them. */
export interface Outlook {
  /** The project's lowest total float among unfinished work, as shown: `0d`. */
  float: string | null;
  /** While edits wait: the finish they give, how far it moves (`+4d`), and the float they give. */
  pending?: {
    finish: string | null;
    moved: string | null;
    float: string | null;
    /** Whether the edits leave less float, which shows in the slip colour. */
    lower?: boolean;
  } | null;
}

/**
 * The title block: slate, as on a drawing. The mark, the project (a picker),
 * the views, where in its history, the data date, finish and float in ruled
 * cells, then the command search, proposals to review, whether the page is
 * live, and the Menu.
 */
export function TitleBlock<Id extends string>({
  summary,
  connection,
  mac,
  onSearch,
  project,
  views,
  view,
  onView,
  branch,
  outlook,
  review,
  menu,
}: {
  summary: Summary | null;
  connection: Connection;
  mac: boolean;
  onSearch: () => void;
  /** The project's name and code, as a button that opens the project picker. */
  project: ReactNode;
  views: readonly ViewTab<Id>[];
  view: Id;
  onView: (id: Id) => void;
  /** The branch picker, for a point in a history store; in place of the plain fact. */
  branch?: ReactNode;
  outlook: Outlook;
  /** The proposals waiting, as a button that opens them. */
  review?: ReactNode;
  menu: ReactNode;
}) {
  const first = summary?.inspect.projects[0];
  const pending = outlook.pending;
  const tabs = (event: KeyboardEvent<HTMLDivElement>) => {
    // Arrows move between the views, as WAI-ARIA tabs do; the view opens as focus moves.
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft" && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const at = views.findIndex((v) => v.id === view);
    const count = views.length;
    const to =
      event.key === "Home" ? 0 : event.key === "End" ? count - 1 : (at + (event.key === "ArrowRight" ? 1 : -1) + count) % count;
    const next = views[to];
    if (!next) return;
    onView(next.id);
    event.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]')[to]?.focus();
  };
  return (
    <header className="title-block">
      <div className="tb-cell tb-project">
        <Mark />
        {project}
      </div>
      <div className="tb-cell tb-views" role="tablist" aria-label="Views" onKeyDown={tabs}>
        {views.map((tab) => (
          <button
            key={tab.id}
            type="button"
            role="tab"
            id={`view-tab-${tab.id}`}
            aria-selected={tab.id === view}
            aria-controls={tab.id === view ? `view-${tab.id}` : undefined}
            tabIndex={tab.id === view ? 0 : -1}
            title={tab.key ? `${tab.label} (${describeKey(tab.key, mac)})` : tab.label}
            onClick={() => onView(tab.id)}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <span className="tb-grow" />
      {summary &&
        (branch ??
          (summary.head ? (
            <Fact
              label={summary.branch ? "Branch" : "Point in history"}
              title={`${summary.writable ? "" : "Read-only. "}Head ${summary.head}`}
              value={
                <>
                  {summary.branch ?? "fixed point"} <code className="head">{summary.head.slice(0, 10)}</code>
                </>
              }
            />
          ) : (
            <Fact label="Source" value="Plain file, read-only" title={summary.location} />
          )))}
      {summary && !summary.writable && summary.head && (
        <span className="read-only" title="Changes can't be recorded here: serve a branch of a history store to edit">
          Read-only
        </span>
      )}
      {first?.data_date && <Fact label="Data date" value={readable(first.data_date, false)} title={first.data_date} />}
      {(first?.finish || pending?.finish) && (
        <Fact
          label="Finish"
          title={pending ? `With the edits waiting; ${first?.finish ?? "none"} now` : (first?.finish ?? undefined)}
          value={
            pending ? (
              <>
                {readable(pending.finish ?? first?.finish, false)}
                {pending.moved && <span className="slip">{pending.moved}</span>}
              </>
            ) : (
              readable(first?.finish, false)
            )
          }
        />
      )}
      {outlook.float !== null && (
        <Fact
          label="Float"
          title={
            !pending
              ? "The lowest total float of unfinished work"
              : pending.float === null
                ? `${outlook.float} now; with the edits waiting, it shows once they're applied`
                : `With the edits waiting; ${outlook.float} now`
          }
          value={
            pending?.float != null && pending.float !== outlook.float ? (
              <span className={pending.lower ? "slip" : undefined}>{pending.float}</span>
            ) : (
              outlook.float
            )
          }
        />
      )}
      {summary && !summary.calculated && (
        <Fact label="Dates" value="As stored, not calculated" title={summary.calculation_refused?.message} />
      )}
      <div className="tb-cell tb-tools">
        <button type="button" className="search" aria-label="Search or run a command" title={`Search or run a command (${describeKey("mod+k", mac)})`} onClick={onSearch}>
          <Icon name="search" size={14} />
          <span>Search or run a command</span>
          <kbd>{describeKey("mod+k", mac)}</kbd>
        </button>
        {review}
        {/* Below 1,360 px only the dot shows, so its words are its tooltip too. */}
        <span className={`connection ${connection}`} role="status" aria-live="polite" title={CONNECTION_TEXT[connection]}>
          <span className="dot" aria-hidden="true" />
          <span className="connection-text">{CONNECTION_TEXT[connection]}</span>
        </span>
        {menu}
      </div>
    </header>
  );
}

const CONNECTION_TEXT: Record<Connection, string> = {
  connecting: "Connecting…",
  live: "Live",
  disconnected: "Disconnected",
  "signed-out": "Signed out: open the link `serve` printed",
};

/** What's going on: the grid's counts (put in `onSlot`'s element), and the edit in hand. */
export function StatusBar({ onSlot, children }: { onSlot: (slot: HTMLSpanElement | null) => void; children?: ReactNode }) {
  return (
    <footer className="status-bar">
      <span className="status-counts" ref={onSlot} />
      <span className="spacer" />
      {children}
    </footer>
  );
}

/** A time of day as the status bar says it: 14:05. */
function clock(at: Date): string {
  return `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
}

/**
 * Editing in the status bar: one short line saying what the last edit did;
 * while edits wait, how many, how far they move the finish, Review (the
 * Changes panel) and Apply, the frame's one filled button; otherwise when the
 * schedule shown was calculated and where it's saved. Then the Preview toggle.
 */
export function EditBar({
  editing,
  state,
  onOpen,
  moved = null,
  calculated = null,
  saved = null,
  mac = false,
}: {
  editing: Editing;
  state: EditState;
  onOpen: () => void;
  /** How far the edits waiting move the finish: `+4d`. */
  moved?: string | null;
  calculated?: Date | null;
  saved?: string | null;
  mac?: boolean;
}) {
  const { notice, previewing } = state;
  const count = waiting(state);
  return (
    <span className="edit-bar">
      <span
        className={`notice ${notice?.tone ?? ""}${count > 0 ? " opens" : ""}`}
        role="status"
        aria-live="polite"
        title={notice?.hint}
        onClick={count > 0 ? onOpen : undefined}
      >
        {notice?.text}
        {notice?.hint && count === 0 && <span className="muted"> · {notice.hint}</span>}
      </span>
      {count > 0 ? (
        <span className="pending" role="group" aria-label="Edits pending">
          <span className="pending-count">
            {count} {count === 1 ? "edit" : "edits"} pending
          </span>
          {moved && <span className="pending-finish">Finish {moved}</span>}
          <button type="button" className="pending-review" onClick={onOpen} title="Show the edits waiting, and what they do together">
            Review
          </button>
          <button
            type="button"
            className="pending-apply"
            disabled={state.busy}
            title={`Record the edits waiting (${describeKey("mod+enter", mac)})`}
            onClick={() => void editing.apply()}
          >
            Apply
          </button>
        </span>
      ) : (
        <>
          {calculated && <span className="status-calculated">Calculated at {clock(calculated)}</span>}
          {saved && <span className="status-saved">{saved}</span>}
        </>
      )}
      <label className="preview-toggle" title="Preview each edit's impact before recording it">
        <input type="checkbox" checked={previewing} onChange={(event) => editing.setPreviewing(event.target.checked)} />
        Preview
      </label>
    </span>
  );
}
