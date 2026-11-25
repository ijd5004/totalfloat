// The inspector's predecessors and successors: each link's other activity,
// type and lag, driving or not. Where the schedule can be edited, a link's
// type and lag change in place, a link can be removed, and the Add field
// below finds an activity by ID or name and links it.

import { useRef, useState } from "react";

import type { Rows } from "../api";
import { dateText } from "../dates";
import { type CellProblem, indexOf } from "../edit/cells";
import { Icon } from "../shell/icons";
import type { View } from "../view";
import { type LinkChange, LINK_TYPES, type LinkType, addLink, changeLink, lagOf, linkKey, removeLink, spanHours } from "./linkEdits";
import type { Logic } from "./links";
import { matches, split } from "./match";

/** What the logic lists need of editing: the pending-edits flow's link changes. */
export interface LinkEditing {
  /** Whether what's shown can be edited. */
  enabled: boolean;
  /** Link edits waiting to be applied. */
  waiting: readonly LinkChange[];
  /** Refused link edits, by key, and why. */
  marks: ReadonlyMap<string, CellProblem>;
  /** Adds a link edit to what's waiting, in place of one under the same key. */
  change: (change: LinkChange) => void;
  /** Takes a waiting link edit back. */
  drop: (key: string) => void;
}

export const READ_ONLY_LINKS: LinkEditing = { enabled: false, waiting: [], marks: new Map(), change: () => {}, drop: () => {} };

/** A span with its unit, as the inspector writes lags. */
export type SpanLabel = (hours: number, perDay: number | null) => string;

/** The pair of activities a new link's key names, by `task_id`. */
function addedPair(key: string): [number, number] | null {
  // An activity waiting to be added has a negative `task_id`.
  const match = /^link:add:(-?\d+)>(-?\d+)$/.exec(key);
  return match ? [Number(match[1]), Number(match[2])] : null;
}

/** A lag typed in place: committed on Enter or on leaving, put back on Escape. */
function LagInput({ label, shown, onLag }: { label: string; shown: string; onLag: (text: string) => string | null }) {
  const [text, setText] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // Escape blurs the field, and that blur mustn't commit what it put back.
  const cancelling = useRef(false);
  const commit = () => {
    if (cancelling.current) {
      cancelling.current = false;
      return;
    }
    if (text === null) return;
    const now = text;
    setText(null);
    if (now.trim() === shown) return;
    setProblem(onLag(now));
  };
  return (
    <>
      <input
        className={`inspector-lag${problem ? " refused" : ""}`}
        aria-label={label}
        aria-invalid={problem ? true : undefined}
        title={problem ?? undefined}
        value={text ?? shown}
        placeholder="0"
        size={5}
        onFocus={() => setText(shown)}
        onChange={(event) => setText(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === "Enter") {
            event.preventDefault();
            commit();
          } else if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            setText(null);
            setProblem(null);
            cancelling.current = true;
            event.currentTarget.blur();
            cancelling.current = false;
          }
        }}
      />
      {problem && (
        <span className="inspector-problem" role="alert">
          {problem}
        </span>
      )}
    </>
  );
}

/**
 * The field that adds a link: type part of an ID or name, choose from the
 * matches with the arrows, Tab on to the type and lag, Enter to add.
 */
function AddLink({
  side,
  rows,
  self,
  linked,
  keyLabel,
  lagDayOf,
  onAdd,
}: {
  side: "predecessor" | "successor";
  rows: Rows;
  /** The activity inspected, by place in `rows`. */
  self: number;
  /** The activities already linked on this side, by `task_id`. */
  linked: ReadonlySet<number>;
  /** The key that brings focus here, as shown. */
  keyLabel: string;
  /** The hours in a day the new link's lag would count in, by the other activity's `task_id`. */
  lagDayOf: (other: number) => number | null;
  onAdd: (other: number, type: LinkType, lag: string | null) => void;
}) {
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  // The activity chosen with Tab, by `task_id`, so fresh rows can't make it another.
  const [chosen, setChosen] = useState<number | null>(null);
  const [type, setType] = useState<LinkType>("FS");
  const [lag, setLag] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const found = open ? matches(rows, query, self) : [];
  const listId = `inspector-add-${side}-list`;
  const optionId = (k: number) => `inspector-add-${side}-option-${k}`;
  const label = side === "predecessor" ? "Add predecessor" : "Add successor";

  /** Takes the highlighted match as the activity to add, and closes the list. */
  const choose = () => {
    const i = found[at];
    if (i !== undefined) {
      setChosen(rows.id[i] ?? null);
      setQuery(rows.code[i] ?? "");
    }
    setOpen(false);
  };
  const reset = () => {
    setQuery("");
    setChosen(null);
    setType("FS");
    setLag("");
    setProblem(null);
    setOpen(false);
    setAt(0);
  };
  /** Adds the activity chosen, or the one highlighted. */
  const add = (pick?: number) => {
    const fromChosen = chosen === null ? undefined : indexOf(rows, chosen);
    const i = pick ?? fromChosen ?? matches(rows, query, self)[at];
    if (i === undefined) {
      setProblem("Type an Activity ID or part of a name, then choose one.");
      return;
    }
    const other = rows.id[i] ?? -1;
    if (linked.has(other)) {
      setProblem(`${rows.code[i] ?? ""} is already linked.`);
      return;
    }
    const value = lagOf(lag, lagDayOf(other));
    if (value !== null && typeof value !== "string") {
      setProblem(`${value.message}: ${value.hint}`);
      return;
    }
    onAdd(other, type, value);
    reset();
    document.getElementById(`inspector-add-${side}`)?.focus();
  };
  const onEnter = (event: React.KeyboardEvent) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    event.preventDefault();
    add();
  };
  const date = (i: number) =>
    side === "predecessor"
      ? `ends ${dateText(rows.actual_finish[i] ?? rows.early_finish[i]) || "–"}`
      : `starts ${dateText(rows.actual_start[i] ?? rows.early_start[i]) || "–"}`;

  return (
    <div className="inspector-add-wrap">
      <div className={`inspector-add${problem ? " refused" : ""}`}>
        <Icon name={query ? "search" : "plus"} size={13} />
        <input
          id={`inspector-add-${side}`}
          role="combobox"
          aria-label={`${label}: type an Activity ID or part of a name`}
          aria-expanded={found.length > 0}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={found.length > 0 ? optionId(at) : undefined}
          aria-keyshortcuts={keyLabel}
          placeholder={label}
          value={query}
          spellCheck={false}
          onChange={(event) => {
            setQuery(event.target.value);
            setChosen(null);
            setAt(0);
            setProblem(null);
            setOpen(true);
          }}
          // Leaving for the type or lag, by Tab or the pointer, keeps the match highlighted.
          onBlur={choose}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) return;
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              setOpen(true);
              if (found.length > 0) setAt((now) => (now + (event.key === "ArrowDown" ? 1 : found.length - 1)) % found.length);
            } else if (event.key === "Enter") {
              onEnter(event);
            } else if (event.key === "Tab" && !event.shiftKey && found.length > 0) {
              // On to the type and lag, with the highlighted activity chosen.
              choose();
            } else if (event.key === "Escape" && (query || open)) {
              event.preventDefault();
              event.stopPropagation();
              reset();
            }
          }}
        />
        <select aria-label={`Type of the new ${side}'s link`} value={type} onChange={(event) => setType(event.target.value as LinkType)} onKeyDown={onEnter}>
          {LINK_TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
        <input
          className="inspector-lag"
          aria-label={`Lag of the new ${side}'s link`}
          placeholder="0"
          size={5}
          value={lag}
          onChange={(event) => {
            setLag(event.target.value);
            setProblem(null);
          }}
          onKeyDown={onEnter}
        />
        <kbd>{keyLabel}</kbd>
      </div>
      {found.length > 0 && (
        <div className="inspector-options">
          <ul role="listbox" id={listId} aria-label={`Activities to add as a ${side}`}>
            {found.map((i, k) => {
              const name = rows.name[i] ?? "";
              const bold = split(name, query);
              const already = linked.has(rows.id[i] ?? -1);
              return (
                <li
                  key={i}
                  id={optionId(k)}
                  role="option"
                  aria-selected={k === at}
                  aria-disabled={already || undefined}
                  className={`inspector-option${k === at ? " on" : ""}${already ? " linked" : ""}`}
                  // Chosen before the field's blur closes the list.
                  onMouseDown={(event) => {
                    event.preventDefault();
                    add(i);
                  }}
                >
                  <span className="inspector-link-code">{rows.code[i]}</span>
                  <span className="inspector-link-name">
                    {bold ? (
                      <>
                        {bold[0]}
                        <b>{bold[1]}</b>
                        {bold[2]}
                      </>
                    ) : (
                      name
                    )}
                  </span>
                  <span className="inspector-option-date">{already ? "already linked" : date(i)}</span>
                </li>
              );
            })}
          </ul>
          <p className="inspector-add-hint">
            <span>
              <kbd>Enter</kbd> add
            </span>
            <span>
              <kbd>Tab</kbd> type and lag
            </span>
            {found.some((i) => linked.has(rows.id[i] ?? -1)) && <span>Already linked ones can't be added again</span>}
          </p>
        </div>
      )}
      {problem && (
        <p className="inspector-problem" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}

/** A predecessor or successor list, each link selecting the activity at its other end. */
export function LogicList({
  title,
  empty,
  logic,
  rows,
  self,
  side,
  lagDay,
  spanLabel,
  linkText,
  view,
  edit,
  keyLabel,
}: {
  title: string;
  empty: string;
  logic: Logic[];
  rows: Rows;
  /** The activity inspected, by `task_id`. */
  self: number;
  /** Which end of each link the other activity is. */
  side: "predecessor" | "successor";
  /** The hours in a day a link's lag counts in, by its predecessor and successor. */
  lagDay: (predecessor: number, successor: number) => number | null;
  spanLabel: SpanLabel;
  linkText: (logic: Logic, perDay: number | null) => string;
  view: View;
  edit: LinkEditing;
  /** The key that moves into the Add field, as shown. */
  keyLabel: string;
}) {
  const headId = `inspector-${title.toLowerCase()}`;
  const ends = (other: number): [number, number] => (side === "predecessor" ? [other, self] : [self, other]);
  const codeName = (id: number) => {
    const at = indexOf(rows, id);
    return at === undefined ? { code: `#${id}`, name: "Not in the rows shown" } : { code: rows.code[at] ?? "", name: rows.name[at] ?? "" };
  };
  // New links waiting, with this activity at the right end.
  const adding = edit.waiting.flatMap((change) => {
    const pair = addedPair(change.key);
    if (!pair || change.edit.op !== "add_link") return [];
    const [pred, succ] = pair;
    if ((side === "predecessor" ? succ : pred) !== self) return [];
    return [{ change, other: side === "predecessor" ? pred : succ }];
  });
  const place = indexOf(rows, self) ?? -1;
  const linked = new Set([...logic.map((l) => l.other), ...adding.map((a) => a.other)]);
  const count = logic.length + adding.length;
  return (
    <section className="inspector-section" aria-labelledby={headId}>
      <h3 id={headId}>
        {title} <span className="count">{count}</span>
      </h3>
      {count === 0 && <p className="inspector-empty">{empty}</p>}
      {count > 0 && (
        <ul className="inspector-links">
          {logic.map((link) => {
            const { code, name } = codeName(link.other);
            const [before, after] = ends(link.other);
            const perDay = lagDay(before, after);
            const how = linkText(link, perDay);
            const key = linkKey(link.link);
            const waiting = edit.waiting.find((change) => change.key === key);
            const removed = waiting?.edit.op === "remove_link";
            const changed = waiting?.edit.op === "change_link" ? waiting.edit : null;
            const problem = edit.marks.get(key);
            const linkedAs = { link: link.link, predecessor: before, successor: after, type: link.type as LinkType };
            const lagShown = changed?.lag ?? (link.lagHours === 0 ? "0" : spanLabel(link.lagHours, perDay));
            return (
              <li key={link.link} className={`inspector-link-row${removed ? " removed" : waiting ? " typed" : ""}`}>
                <button
                  type="button"
                  className="inspector-link"
                  title={`Select ${code} in the grid and Gantt`}
                  aria-label={`${code} ${name}, ${how}${link.driving ? ", driving" : ""}${removed ? ", to be removed" : ""}`}
                  onClick={() => view.reveal(link.other)}
                >
                  <span className={`inspector-dot${link.driving ? " driving" : ""}`} aria-hidden="true" />
                  <span className="inspector-link-code">{code}</span>
                  <span className="inspector-link-name">{name}</span>
                  {!edit.enabled && <span className="inspector-link-type">{how}</span>}
                </button>
                {edit.enabled && !removed && (
                  <span className="inspector-link-edit">
                    <select
                      aria-label={`Type of the link with ${code}`}
                      value={changed?.new_type ?? link.type}
                      onChange={(event) => {
                        const next = changeLink(rows, linkedAs, waiting, { type: event.target.value as LinkType });
                        if (next) edit.change(next);
                        else edit.drop(key);
                      }}
                    >
                      {LINK_TYPES.map((t) => (
                        <option key={t} value={t}>
                          {t}
                        </option>
                      ))}
                    </select>
                    <LagInput
                      label={`Lag of the link with ${code}`}
                      shown={lagShown}
                      onLag={(text) => {
                        const value = lagOf(text, perDay);
                        if (value !== null && typeof value !== "string") return `${value.message}: ${value.hint}`;
                        // The link's own lag again takes the lag out of the change.
                        const hours = value === null ? 0 : spanHours(value, perDay);
                        const own = hours !== null && Math.round(hours * 60) === Math.round(link.lagHours * 60);
                        const next = changeLink(rows, linkedAs, waiting, { lag: own ? null : (value ?? "0h") });
                        if (next) edit.change(next);
                        else edit.drop(key);
                        return null;
                      }}
                    />
                    <button type="button" className="inspector-remove" aria-label={`Remove the link with ${code}`} title="Remove this link" onClick={() => edit.change(removeLink(rows, linkedAs))}>
                      <Icon name="x" size={14} />
                    </button>
                  </span>
                )}
                {edit.enabled && removed && (
                  <span className="inspector-link-edit">
                    <span className="inspector-pending-tag">Removing</span>
                    <button type="button" className="inspector-undo" aria-label={`Keep the link with ${code}`} onClick={() => edit.drop(key)}>
                      Keep
                    </button>
                  </span>
                )}
                {problem && (
                  <span className="inspector-problem" role="alert">
                    {problem.message}
                    {problem.hint && <span className="muted"> ({problem.hint})</span>}
                  </span>
                )}
              </li>
            );
          })}
          {adding.map(({ change, other }) => {
            const { code, name } = codeName(other);
            const added = change.edit.op === "add_link" ? change.edit : null;
            const problem = edit.marks.get(change.key);
            return (
              <li key={change.key} className="inspector-link-row typed adding">
                <button type="button" className="inspector-link" aria-label={`${code} ${name}, to be added`} onClick={() => view.reveal(other)}>
                  <span className="inspector-dot" aria-hidden="true" />
                  <span className="inspector-link-code">{code}</span>
                  <span className="inspector-link-name">{name}</span>
                </button>
                <span className="inspector-link-edit">
                  <span className="inspector-link-type">
                    {added?.link_type ?? "FS"}
                    {added?.lag && Number.parseFloat(added.lag) !== 0 ? ` ${added.lag.startsWith("-") ? "−" + added.lag.slice(1) : "+" + added.lag}` : ""}
                  </span>
                  <span className="inspector-pending-tag">Adding</span>
                  <button type="button" className="inspector-remove" aria-label={`Don't add the link with ${code}`} title="Don't add this link" onClick={() => edit.drop(change.key)}>
                    <Icon name="x" size={14} />
                  </button>
                </span>
                {problem && (
                  <span className="inspector-problem" role="alert">
                    {problem.message}
                    {problem.hint && <span className="muted"> ({problem.hint})</span>}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {edit.enabled && place >= 0 && (
        <AddLink
          side={side}
          rows={rows}
          self={place}
          linked={linked}
          keyLabel={keyLabel}
          lagDayOf={(other) => lagDay(...ends(other))}
          onAdd={(other, type, lag) => {
            const [before, after] = ends(other);
            edit.change(addLink(rows, before, after, type, lag));
          }}
        />
      )}
    </section>
  );
}

