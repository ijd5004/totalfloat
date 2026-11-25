// The activity inspector: everything about the selected activity in the
// schedule's right column. Its dates, durations and logic come from the rows
// and links already fetched, so it follows the selection as fast as the grid
// moves it; why it falls where it does, and its resources, codes and notes,
// are asked of the server. Fields the grid edits are edited here through the
// same flow, so a change previews, waits, records and undoes the same from
// either place.

import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";

import { type Api, type Fields, type Links, type Rows, type Summary, failure } from "../api";
import type { DateFormat } from "../dates";
import { dateText } from "../dates";
import { CONSTRAINT_KINDS, type CellProblem, type Typed, calendarsOf, cellKey, indexOf, inputKind, inputText } from "../edit/cells";
import type { DrivingPath } from "../gantt/draw";
import { type ColumnId, hours, unitsShown } from "../grid/columns";
import { Icon } from "../shell/icons";
import type { View } from "../view";
import { type Details, fetchDetails } from "./details";
import { type LinkEditing, LogicList, READ_ONLY_LINKS } from "./Logic";
import { type Logic, logicOf } from "./links";
import { selectedIn } from "./selected";
import { ASK_DELAY_MS, type Asked, Why } from "./Why";

/** What the inspector's fields need of editing: the grid's flow. */
export interface InspectorEditing {
  /** Whether what's shown can be edited. */
  enabled: boolean;
  /** Typed values waiting, by cell. */
  values: ReadonlyMap<string, string>;
  /** Refused cells, and why. */
  marks: ReadonlyMap<string, CellProblem>;
  commit: (typed: Typed[]) => void;
  /** Link edits; without them, links are read-only here. */
  links?: LinkEditing;
}

/** A span or float with its unit: days of the activity's calendar, or hours. */
export function spanLabel(value: number | null | undefined, perDay: number | null | undefined): string {
  if (value === null || value === undefined) return "";
  if (unitsShown() === "days" && perDay) return `${hours(value / perDay)}d`;
  return `${hours(value)}h`;
}

/**
 * The hours in a day a link's lag is counted in, by its project's lag
 * calendar option, as the engine counts it: the predecessor's calendar, the
 * successor's, or 24 hours. Null where the day isn't known here, as for the
 * project's default calendar, so the lag shows in hours.
 */
export function lagDay(rows: Rows, predecessor: number, successor: number, summary: Summary | null): number | null {
  const after = indexOf(rows, successor);
  const project = after === undefined ? undefined : rows.project[after];
  const option = summary?.inspect.projects.find((p) => p.id === project)?.options.lag_calendar ?? "predecessor";
  const dayOf = (id: number) => {
    const at = indexOf(rows, id);
    return at === undefined ? null : (rows.hours_per_day?.[at] ?? null);
  };
  switch (option) {
    case "predecessor":
      return dayOf(predecessor);
    case "successor":
      return dayOf(successor);
    case "24-hour":
      return 24;
    default:
      return null;
  }
}

/** A link's type and lag, as `FS`, or `SS +2d`, with the lag in days of `perDay` hours. */
export function linkText(logic: Logic, perDay: number | null | undefined): string {
  if (logic.lagHours === 0) return logic.type;
  return `${logic.type} ${logic.lagHours > 0 ? "+" : "−"}${spanLabel(Math.abs(logic.lagHours), perDay)}`;
}

const STATUS: Record<string, string> = {
  "not-started": "Not started",
  "in-progress": "In progress",
  completed: "Completed",
};

/** The details for one selection, once they're back. */
type Fetched = { asked: Asked } & ({ details: Details | null } | { failed: string });

/**
 * One value: as text, or, where the grid edits it, a button that opens an
 * input over it. Enter or leaving the input commits, as in the grid; Escape
 * puts it back.
 */
function Value({
  label,
  column,
  rows,
  i,
  shown,
  edit,
}: {
  label: string;
  column: ColumnId;
  rows: Rows;
  i: number;
  shown: string;
  edit: InspectorEditing;
}) {
  const id = rows.id[i] ?? -1;
  const key = cellKey(id, column);
  const typed = edit.values.get(key);
  const problem = edit.marks.get(key);
  const [text, setText] = useState<string | null>(null);
  const opened = useRef("");
  const button = useRef<HTMLButtonElement>(null);
  const refocus = useRef(false);
  const closing = useRef(false);
  useEffect(() => {
    if (text === null && refocus.current) {
      refocus.current = false;
      button.current?.focus();
    }
  }, [text]);
  const value = typed ?? shown;
  const kind = inputKind(column);
  const described = problem ? `inspector-problem-${column}` : undefined;
  const calendars = useMemo(() => (text !== null && kind === "calendar" ? calendarsOf(rows) : []), [text, kind, rows]);
  if (!kind || !edit.enabled) {
    return <span className={`inspector-value${value ? "" : " none"}`}>{value || "None"}</span>;
  }
  const close = (commit: boolean, back: boolean) => {
    const now = text;
    // Enter closes it, and the input's blur as it goes mustn't commit again.
    if (now === null || closing.current) return;
    closing.current = true;
    refocus.current = back;
    setText(null);
    if (commit && now !== opened.current) edit.commit([{ id, column, text: now }]);
  };
  return (
    <>
      {text === null ? (
        <button
          ref={button}
          type="button"
          className={`inspector-value editable${value ? "" : " none"}${typed !== undefined ? " typed" : ""}${problem ? " refused" : ""}`}
          aria-label={`${label}: ${value || "none"}. Change it`}
          aria-describedby={described}
          onClick={() => {
            opened.current = typed ?? inputText(rows, i, column);
            closing.current = false;
            setText(opened.current);
          }}
        >
          {value || "None"}
        </button>
      ) : (
        <input
          className="inspector-input"
          aria-label={`${label} of ${rows.code[i] ?? "the activity"}`}
          value={text}
          autoFocus
          spellCheck={kind === "text"}
          list={kind === "calendar" ? "inspector-calendars" : kind === "constraint" ? "inspector-constraints" : undefined}
          onChange={(event) => setText(event.target.value)}
          onBlur={() => close(true, false)}
          onKeyDown={(event) => {
            // Enter or Escape that ends an input method's composition is its own.
            if (event.nativeEvent.isComposing || event.keyCode === 229) return;
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              close(false, true);
            } else if (event.key === "Enter") {
              event.preventDefault();
              close(true, true);
            }
          }}
        />
      )}
      {text !== null && kind === "calendar" && (
        <datalist id="inspector-calendars">
          {calendars.map((name) => (
            <option key={name} value={name} />
          ))}
        </datalist>
      )}
      {text !== null && kind === "constraint" && (
        <datalist id="inspector-constraints">
          <option value="none" />
          {CONSTRAINT_KINDS.map((k) => (
            <option key={k} value={k} />
          ))}
        </datalist>
      )}
      {problem && (
        <span className="inspector-problem" id={described} role="note">
          {problem.message}
          {problem.hint && <span className="muted"> ({problem.hint})</span>}
        </span>
      )}
    </>
  );
}

/** A section shut until opened, with its count beside its title. */
function Collapsed({ title, count, children }: { title: string; count: number | null; children: ReactNode }) {
  return (
    <details className="inspector-section inspector-more">
      <summary>
        <Icon name="right" size={13} className="inspector-chevron" />
        {title}
        {count !== null && <span className="count">{count}</span>}
      </summary>
      <div className="inspector-more-body">{children}</div>
    </details>
  );
}

/** A value shown, never edited here. */
function fixedField(label: string, shown: string) {
  return (
    <div className="inspector-field">
      <dt>{label}</dt>
      <dd>
        <span className={`inspector-value${shown ? "" : " none"}`}>{shown || "None"}</span>
      </dd>
    </div>
  );
}

/**
 * Follows the grid's selection; the same activity in the same rows asks
 * nothing new, and fresh rows, as after an edit, ask again.
 */
export function Inspector({
  api,
  view,
  links,
  fields,
  summary = null,
  edit,
  dates,
  onClose,
  onChain,
}: {
  api: Api;
  view: View;
  links: Links | null;
  fields: Fields | null;
  /** For each project's lag calendar option. */
  summary?: Summary | null;
  edit: InspectorEditing;
  /** How dates are shown; set again when it changes. */
  dates?: DateFormat | undefined;
  onClose: () => void;
  onChain?: ((chain: DrivingPath | null) => void) | undefined;
}) {
  const [asked, setAsked] = useState<Asked | null>(() => {
    const now = view.current();
    const of = now ? selectedIn(now.rows, now.arranged.lines, now.active) : null;
    return now && of ? { of, rows: now.rows } : null;
  });
  useEffect(
    () =>
      view.subscribe((now) => {
        const of = selectedIn(now.rows, now.arranged.lines, now.active);
        setAsked((was) => {
          // Nothing selected, as when a filter hides everything.
          if (!of) return null;
          if (was && was.of.id === of.id && was.rows === now.rows) return was;
          return { of, rows: now.rows };
        });
      }),
    [view],
  );

  // Resources, codes and notes, once the selection rests.
  const [fetched, setFetched] = useState<Fetched | null>(null);
  useEffect(() => {
    // An activity added and waiting has nothing on the server to ask about yet.
    if (!asked || asked.of.id < 0) return;
    let live = true;
    const timer = setTimeout(() => {
      fetchDetails(api, asked.of, fields).then(
        (details) => live && setFetched({ asked, details }),
        (error: unknown) => live && setFetched({ asked, failed: failure(error, "The details couldn't be fetched.").message }),
      );
    }, ASK_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [api, asked, fields]);
  const details = fetched && fetched.asked === asked ? fetched : null;

  const rows = asked?.rows;
  const i = rows && asked ? indexOf(rows, asked.of.id) : undefined;
  const logic = useMemo(() => (links && asked ? logicOf(links, asked.of.id) : null), [links, asked]);

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    // Escape from anywhere in it but an input goes back to the grid.
    if (event.key !== "Escape" || event.defaultPrevented || event.target instanceof HTMLInputElement) return;
    event.preventDefault();
    document.querySelector<HTMLElement>(".grid-body")?.focus();
  };

  if (!asked || !rows || i === undefined) {
    return (
      <section className="inspector" tabIndex={-1} aria-label="Activity inspector" onKeyDown={onKeyDown}>
        <header className="inspector-head">
          <div className="inspector-top">
            <span className="inspector-code">No activity selected</span>
            <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
              <Icon name="x" />
            </button>
          </div>
        </header>
        <p className="inspector-empty pad">Select an activity in the grid or the Gantt to see its dates, logic and why it falls where it does.</p>
      </section>
    );
  }

  const perDay = rows.hours_per_day?.[i];
  if (asked.of.id < 0) {
    // Added, waiting with the other edits: the grid shows it, the server doesn't have it yet.
    return (
      <section className="inspector" tabIndex={-1} aria-label={`Activity ${asked.of.code}`} onKeyDown={onKeyDown}>
        <header className="inspector-head">
          <div className="inspector-top">
            <span className="inspector-code">{asked.of.code}</span>
            <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
              <Icon name="x" />
            </button>
          </div>
          <h2 className="inspector-name">{rows.name[i] ?? ""}</h2>
          <div className="inspector-tags">
            <span className="inspector-tag">Waiting to be added</span>
          </div>
        </header>
        <section className="inspector-section" aria-labelledby="inspector-dates">
          <h3 id="inspector-dates">Dates and duration</h3>
          <dl className="inspector-grid">{fixedField("Original duration", spanLabel(rows.original_duration_hours[i], perDay))}</dl>
        </section>
        <p className="inspector-empty pad">
          It&apos;s added when you apply the edits waiting. Its dates, logic and details show once it&apos;s recorded.
        </p>
      </section>
    );
  }
  const status = rows.status[i] ?? "";
  const field = (label: string, column: ColumnId, shown: string) => (
    <div className="inspector-field">
      <dt>{label}</dt>
      <dd>
        <Value key={`${asked.of.id}:${column}`} label={label} column={column} rows={rows} i={i} shown={shown} edit={edit} />
      </dd>
    </div>
  );
  const fixed = fixedField;
  const constraint = rows.constraint[i] ?? "";
  const resources = details && "details" in details ? details.details : null;
  const resourceCount = resources ? resources.resources.length + resources.roles.length : null;
  const codeCount = resources ? resources.codes.length + resources.udfs.length : null;
  const unknown =
    details && "failed" in details
      ? details.failed
      : details?.details === null
        ? `${asked.of.code} wasn't found when its details were asked for; reload to see it as it is now.`
        : null;
  const loading = !details;
  const later = (list: ReactNode) =>
    unknown ? <p className="explain-refused">{unknown}</p> : loading ? <p className="inspector-empty">Loading…</p> : list;

  return (
    <section className="inspector" tabIndex={-1} aria-label={`Activity ${asked.of.code}`} onKeyDown={onKeyDown}>
      <header className="inspector-head">
        <div className="inspector-top">
          <span className="inspector-code">{asked.of.code}</span>
          <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
            <Icon name="x" />
          </button>
        </div>
        <h2 className="inspector-name">
          <Value key={`${asked.of.id}:name`} label="Activity name" column="name" rows={rows} i={i} shown={rows.name[i] ?? ""} edit={edit} />
        </h2>
        <div className="inspector-tags">
          <span className="inspector-tag">{STATUS[status] ?? status}</span>
          {rows.critical[i] && (
            <span className="inspector-tag critical">
              <i aria-hidden="true" />
              Critical
            </span>
          )}
        </div>
      </header>

      <section className="inspector-section" aria-labelledby="inspector-dates">
        <h3 id="inspector-dates">Dates and duration</h3>
        <dl className="inspector-grid">
          {field("Original duration", "original", spanLabel(rows.original_duration_hours[i], perDay))}
          {field("Remaining", "remaining", spanLabel(rows.remaining_duration_hours[i], perDay))}
          {fixed("Start", dateText(rows.actual_start[i] ?? rows.early_start[i]))}
          {fixed("Finish", dateText(rows.actual_finish[i] ?? rows.early_finish[i]))}
          {field("Actual start", "actual_start", dateText(rows.actual_start[i]))}
          {field("Actual finish", "actual_finish", dateText(rows.actual_finish[i]))}
          {fixed("Total float", spanLabel(rows.total_float_hours[i], perDay))}
          {fixed("Free float", spanLabel(rows.free_float_hours[i], perDay))}
          {field("Calendar", "calendar", rows.calendar[i] ?? "")}
          {field("Constraint", "constraint", constraint)}
          {field("Constraint date", "constraint_date", dateText(rows.constraint_date[i]))}
          {rows.secondary_constraint[i] &&
            fixed("Second constraint", `${rows.secondary_constraint[i] ?? ""} ${dateText(rows.secondary_constraint_date[i])}`.trim())}
        </dl>
      </section>

      {logic ? (
        <>
          <LogicList
            // Each activity's own fields, so a half-typed add doesn't follow the selection.
            key={`${asked.of.id}:predecessors`}
            title="Predecessors"
            empty="None: nothing comes before it, so its start is open."
            logic={logic.predecessors}
            rows={rows}
            self={asked.of.id}
            side="predecessor"
            lagDay={(before, after) => lagDay(rows, before, after, summary)}
            spanLabel={spanLabel}
            linkText={linkText}
            view={view}
            edit={edit.links ?? READ_ONLY_LINKS}
            keyLabel="Alt+B"
          />
          <LogicList
            key={`${asked.of.id}:successors`}
            title="Successors"
            empty="None: nothing follows it, so its finish is open."
            logic={logic.successors}
            rows={rows}
            self={asked.of.id}
            side="successor"
            lagDay={(before, after) => lagDay(rows, before, after, summary)}
            spanLabel={spanLabel}
            linkText={linkText}
            view={view}
            edit={edit.links ?? READ_ONLY_LINKS}
            keyLabel="Alt+A"
          />
        </>
      ) : (
        <p className="inspector-empty pad">Loading its logic…</p>
      )}

      <Why api={api} view={view} asked={asked} onChain={onChain} dates={dates} />

      <Collapsed title="Resources" count={resourceCount}>
        {later(
          resources && resourceCount ? (
            <ul className="inspector-list">
              {resources.resources.map((name) => (
                <li key={`r:${name}`}>{name}</li>
              ))}
              {resources.roles.map((name) => (
                <li key={`o:${name}`}>
                  {name} <span className="muted">(role)</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="inspector-empty">No resources or roles assigned.</p>
          ),
        )}
      </Collapsed>
      <Collapsed title="Codes and fields" count={codeCount}>
        {later(
          resources && codeCount ? (
            <dl className="inspector-pairs">
              {resources.codes.map(({ type, value }) => (
                <div key={`c:${type}:${value}`}>
                  <dt>{type || "Code"}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
              {resources.udfs.map(({ label, value, date }) => (
                <div key={`u:${label}`}>
                  <dt>{label}</dt>
                  {/* Dates in the format chosen, as it changes. */}
                  <dd>{date ? dateText(value) : value}</dd>
                </div>
              ))}
            </dl>
          ) : (
            <p className="inspector-empty">No activity codes or user fields set.</p>
          ),
        )}
      </Collapsed>
      <Collapsed title="Notes" count={resources ? resources.notes.length : null}>
        {later(
          resources && resources.notes.length > 0 ? (
            <dl className="inspector-notes">
              {resources.notes.map(({ topic, text }, k) => (
                <div key={k}>
                  <dt>{topic || "Note"}</dt>
                  <dd>{text}</dd>
                </div>
              ))}
            </dl>
          ) : (
            <p className="inspector-empty">No notes.</p>
          ),
        )}
      </Collapsed>
    </section>
  );
}
