// The inspector's "Why it starts" section: why the selected activity's date
// is what it is, in a scheduler's words. The engine's chain becomes numbered
// stops, one per activity, and the Gantt shows the same numbers on the same
// bars, lit in hi-vis, so each line here points at a bar there.

import { useEffect, useMemo, useState } from "react";

import { type Api, ApiError, type Explanation, type Rows } from "../api";
import type { DateFormat } from "../dates";
import { type Narrative, narrate } from "../explain/narrate";
import type { DrivingPath } from "../gantt/draw";
import type { View } from "../view";
import type { Selected } from "./selected";

export const FIELDS = [
  { id: "early_start", label: "Start", title: "Why it starts" },
  { id: "early_finish", label: "Finish", title: "Why it finishes" },
  { id: "late_start", label: "Latest start", title: "Why its latest start" },
  { id: "late_finish", label: "Latest finish", title: "Why its latest finish" },
  { id: "total_float", label: "Float", title: "Why its float" },
] as const;

export type Field = (typeof FIELDS)[number]["id"];

/** An activity to explain, in the rows it was selected in. A new object asks again. */
export interface Asked {
  of: Selected;
  rows: Rows;
}

/** How long the selection rests before it's explained, in milliseconds. */
export const ASK_DELAY_MS = 120;

/** An answer, for the request it answers. */
type Answer = { asked: Asked; field: Field } & (
  | { state: "shown"; report: Explanation }
  | { state: "refused"; message: string; hint: string }
);

/** The chain as the Gantt shows it: its activities and links in hi-vis, numbered as here. */
export function chainOf(told: Narrative, focus: number | null): DrivingPath {
  return {
    ids: new Set(told.stops.map((stop) => stop.id)),
    relationships: told.relationships,
    edges: new Set(),
    numbers: new Map(told.stops.map((stop) => [stop.id, stop.n])),
    focus,
  };
}

const indexes = new WeakMap<Rows, Map<number, number>>();
function indexOf(rows: Rows): Map<number, number> {
  let index = indexes.get(rows);
  if (!index) {
    index = new Map(rows.id.map((id, i) => [id, i]));
    indexes.set(rows, index);
  }
  return index;
}

/** Float in days where the calendar's day is known, as a scheduler reads it. */
function floatWords(report: Explanation): string | null {
  const hours = report.total_float_hours;
  if (hours === null || hours === undefined) return null;
  const perDay = report.chains[0]?.steps[0]?.hours_per_day;
  const amount = perDay ? Math.round((hours / perDay) * 100) / 100 : Math.round(hours * 100) / 100;
  const unit = perDay ? (Math.abs(amount) === 1 ? "day" : "days") : "hours";
  // Float is measured against the activity's own latest dates, which a
  // successor or a constraint sets, not always the project's finish.
  if (amount === 0) return "It has no float: if it slips at all, it passes its latest dates.";
  if (amount < 0) return `It's ${Math.abs(amount)} ${unit} past its latest dates already.`;
  return `It has ${amount} ${unit} of float: it can slip that much before it passes its latest dates.`;
}

/**
 * Explains `asked`, asking again whenever it's a new object, as for fresh
 * rows. Only the latest answer is shown. `onChain` hears the chain shown,
 * for the Gantt, and null when there's none.
 */
export function Why({
  api,
  view,
  asked,
  onChain,
  dates,
}: {
  api: Api;
  view: View;
  asked: Asked | null;
  onChain?: ((chain: DrivingPath | null) => void) | undefined;
  /** How dates are shown; the steps are told again when it changes. */
  dates?: DateFormat | undefined;
}) {
  const [field, setField] = useState<Field>("early_start");
  const [answer, setAnswer] = useState<Answer | null>(null);
  const [focus, setFocus] = useState<number | null>(null);

  useEffect(() => {
    if (!asked) return;
    let live = true;
    const { of } = asked;
    // Asked once the selection rests, so a held arrow key asks for the row
    // it stops on, and the rows passed over cost nothing.
    const timer = setTimeout(() => {
      api
        .explain(of.code, field, of.project === undefined ? {} : { project: of.project })
        .then((report) => live && setAnswer({ asked, field, state: "shown", report }))
        .catch((error: unknown) => {
          if (!live) return;
          const refused = error instanceof ApiError ? error : null;
          setAnswer({
            asked,
            field,
            state: "refused",
            message: refused?.message ?? "The explanation couldn't be fetched.",
            hint: refused?.hint ?? "",
          });
        });
    }, ASK_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [api, asked, field]);

  // An answer for an older request is loading the current one.
  const current = answer && answer.asked === asked && answer.field === field ? answer : null;
  const told = useMemo(() => {
    if (current?.state !== "shown") return [];
    const index = indexOf(current.asked.rows);
    return current.report.chains.map((_, k) => narrate(current.report, k, current.asked.rows, index));
    // The steps' dates are written in the format shown.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current, dates]);

  // The Gantt mirrors the chain pointed into, the first until another is,
  // numbered as here.
  const [onChainIndex, setOnChainIndex] = useState(0);
  const mapped = told[onChainIndex] ?? told[0];
  useEffect(() => {
    onChain?.(mapped ? chainOf(mapped, focus) : null);
  }, [mapped, focus, onChain]);
  useEffect(() => () => onChain?.(null), [onChain]);

  const title = FIELDS.find((f) => f.id === field)?.title ?? "Why";
  return (
    <section className="inspector-section explain" id="inspector-why" aria-labelledby="inspector-why-title">
      <div className="inspector-section-head">
        <h3 id="inspector-why-title">{title}</h3>
        <div
          className="explain-tabs"
          role="tablist"
          aria-label="Which date"
          onKeyDown={(event) => {
            // Arrows, Home and End move between the tabs, choosing as they go.
            const at = FIELDS.findIndex((f) => f.id === field);
            const to =
              event.key === "ArrowRight"
                ? (at + 1) % FIELDS.length
                : event.key === "ArrowLeft"
                  ? (at - 1 + FIELDS.length) % FIELDS.length
                  : event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? FIELDS.length - 1
                      : -1;
            const next = FIELDS[to];
            if (!next) return;
            event.preventDefault();
            setField(next.id);
            document.getElementById(`explain-tab-${next.id}`)?.focus();
          }}
        >
          {FIELDS.map((f) => (
            <button
              key={f.id}
              id={`explain-tab-${f.id}`}
              type="button"
              role="tab"
              aria-selected={field === f.id}
              aria-controls="explain-panel"
              tabIndex={field === f.id ? 0 : -1}
              className="explain-tab"
              onClick={() => setField(f.id)}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>
      <div className="explain-body" role="tabpanel" id="explain-panel" aria-labelledby={`explain-tab-${field}`}>
        {!asked && <p className="explain-empty">Select an activity to see what drives its dates.</p>}
        {asked && !current && <p className="explain-empty">Tracing {asked.of.code}…</p>}
        {current?.state === "refused" && (
          <p className="explain-refused">
            {current.message}
            {current.hint && <span className="explain-hint">{current.hint}</span>}
          </p>
        )}
        {current?.state === "shown" && field === "total_float" && <p className="explain-float">{floatWords(current.report)}</p>}
        {told.map((chain, k) => (
          <div key={k} className="explain-chain">
            <h4 className="explain-question">{chain.question}</h4>
            <ol className="explain-stops" onMouseLeave={() => setFocus(null)}>
              {chain.stops.map((stop) => (
                <li key={stop.n}>
                  <button
                    type="button"
                    className="explain-stop"
                    aria-describedby={`explain-why-${k}-${stop.n}`}
                    onMouseEnter={() => {
                      setOnChainIndex(k);
                      setFocus(stop.id);
                    }}
                    onFocus={() => {
                      setOnChainIndex(k);
                      setFocus(stop.id);
                    }}
                    onBlur={() => setFocus(null)}
                    onClick={() => view.reveal(stop.id)}
                    title="Select this activity in the grid and Gantt"
                  >
                    <span className="explain-n" aria-hidden="true">
                      {stop.n}
                    </span>
                    <span className="explain-what">
                      <span className="explain-activity">
                        <b>{stop.code}</b> {stop.name}
                      </span>
                      <span className="explain-date">{stop.date}</span>
                      <span className="explain-why" id={`explain-why-${k}-${stop.n}`}>
                        {stop.why}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ol>
          </div>
        ))}
      </div>
    </section>
  );
}
