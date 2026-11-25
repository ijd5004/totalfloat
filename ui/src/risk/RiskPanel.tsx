// The risk view: a Monte Carlo schedule risk analysis of what's
// shown, run on the server by the same code as `totalfloat risk`. The page
// only draws what comes back: the finish distribution with its cumulative
// curve, the P-values, the durations that move the finish most, the
// activities most often critical (click one to show it selected in the
// schedule) and
// any risk events. No date here is worked out in the page.

import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";

import { type Api, type RiskInputs, type RiskReport, failure } from "../api";
import { dateText } from "../dates";
import type { View } from "../view";
import { histogramOf, percent, tornadoOf, xOf } from "./chart";

const WIDTH = 320;
const HEIGHT = 120;
const BAR = 14;

/** The form's fields, as typed. */
export interface Asked {
  min: string;
  max: string;
  iterations: string;
  seed: string;
  target: string;
  /** A whole `totalfloat.risk-inputs/1` document, which replaces the range when given. */
  inputs: string;
}

const START: Asked = { min: "-10", max: "25", iterations: "1000", seed: "1", target: "", inputs: "" };

/** The request the form describes, or why it can't be sent. */
export function requestOf(asked: Asked): { inputs: RiskInputs; iterations: number; seed: number; target?: string } | string {
  const whole = (text: string) => (/^\d+$/.test(text.trim()) ? Number(text.trim()) : NaN);
  const iterations = whole(asked.iterations);
  const seed = whole(asked.seed);
  if (!(iterations >= 1 && iterations <= 10_000)) return "Iterations run from 1 to 10,000.";
  if (!Number.isSafeInteger(seed)) return "The seed is a whole number from 0 to 9,007,199,254,740,991.";
  let inputs: RiskInputs;
  if (asked.inputs.trim()) {
    try {
      inputs = JSON.parse(asked.inputs) as RiskInputs;
    } catch {
      return "The inputs aren't valid JSON.";
    }
  } else {
    const number = (text: string) => (text.trim() === "" ? NaN : Number(text));
    const min = number(asked.min);
    const max = number(asked.max);
    if (!Number.isFinite(min) || !Number.isFinite(max)) return "The range is two numbers, in percent.";
    inputs = { default: { min, max } };
  }
  const target = asked.target.trim();
  return { inputs, iterations, seed, ...(target ? { target } : {}) };
}

/** The grid's activity ID for an Activity ID code in the report's project. */
function idOf(view: View, report: RiskReport, code: string): number | null {
  const rows = view.current()?.rows;
  if (!rows) return null;
  for (let k = 0; k < rows.count; k++) {
    if (rows.code[k] === code && (rows.project[k] === report.project_id || rows.project[k] === undefined)) {
      return rows.id[k] ?? null;
    }
  }
  return null;
}

/**
 * Which schedule results are of: its location and head. Two plain files
 * have no head, so the location tells them apart.
 */
export function scheduleIdentity(summary: { location: string; head?: string | null } | null): string | null {
  return summary ? `${summary.location}@${summary.head ?? ""}` : null;
}

/** A project the analysis can run on. */
export interface RiskProject {
  id: number;
  short_name: string;
}

/**
 * What the panel holds, kept by the shell so switching side panels, or
 * closing this one, loses nothing: the form, the last results, and a run
 * still going, whose answer lands here even if the panel has closed.
 */
export interface RiskMemory {
  asked: Asked;
  project: number | null;
  report: RiskReport | null;
  refused: { message: string; hint: string } | null;
  stale: boolean;
  running: boolean;
  /** The schedule's head the results are of. */
  head: string | null;
  /** Bumped when the schedule changes, so a run started before is dropped. */
  generation: number;
}

export function riskMemory(): RiskMemory {
  return { asked: START, project: null, report: null, refused: null, stale: false, running: false, head: null, generation: 0 };
}

export function RiskPanel({
  api,
  view,
  projects,
  head,
  memory,
  onClose,
  onShow,
}: {
  api: Api;
  view: View;
  /** The projects shown; with more than one, the panel asks which. */
  projects: RiskProject[];
  /** The schedule shown, as [`scheduleIdentity`] gives it; results are of one. */
  head: string | null;
  memory: RiskMemory;
  onClose: () => void;
  /** After an activity is selected in the grid, so the schedule shows it. */
  onShow: () => void;
}) {
  // The memory is the state; this only redraws when it changes.
  const [, redraw] = useState(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const update = (change: Partial<RiskMemory>) => {
    Object.assign(memory, change);
    if (mounted.current) redraw((n) => n + 1);
  };
  // Results are of the schedule they were run on: another head puts them
  // away, and a run still going is dropped, saying so.
  useEffect(() => {
    if (memory.head === head) return;
    const had = memory.report !== null || memory.running;
    update({
      head,
      generation: memory.generation + 1,
      report: null,
      refused: null,
      running: false,
      stale: memory.stale || had,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [head]);
  const { asked, report, refused, stale, running } = memory;
  // The project chosen, or the first while none is: the list can arrive after
  // the panel opens.
  const project = projects.some((p) => p.id === memory.project) ? memory.project : (projects[0]?.id ?? null);
  const field = (name: keyof Asked) => ({
    value: asked[name],
    onChange: (event: { target: { value: string } }) => update({ asked: { ...memory.asked, [name]: event.target.value } }),
  });

  const run = async (event: FormEvent) => {
    event.preventDefault();
    const request = requestOf(asked);
    if (typeof request === "string") {
      update({ refused: { message: request, hint: "" } });
      return;
    }
    update({ running: true, refused: null, stale: false });
    const asOf = memory.generation;
    const many = projects.length > 1 && project != null;
    try {
      const answer = await api.risk(many ? { ...request, project } : request);
      if (asOf === memory.generation) update({ report: answer, running: false });
    } catch (error) {
      if (asOf === memory.generation) update({ refused: failure(error, "The analysis couldn't be run."), running: false });
    }
  };

  return (
    <section className="risk" aria-label="Schedule risk">
      <header className="explain-head">
        <h2>Schedule risk</h2>
        <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
          ×
        </button>
      </header>
      <form className="risk-form" onSubmit={(e) => void run(e)}>
        {projects.length > 1 && (
          <label className="risk-wide">
            Project
            <select aria-label="Project" value={project ?? ""} onChange={(e) => update({ project: Number(e.target.value) })}>
              {projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.short_name}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          Optimistic %
          <input aria-label="Optimistic %" inputMode="decimal" {...field("min")} />
        </label>
        <label>
          Pessimistic %
          <input aria-label="Pessimistic %" inputMode="decimal" {...field("max")} />
        </label>
        <label>
          Iterations
          <input aria-label="Iterations" inputMode="numeric" {...field("iterations")} />
        </label>
        <label>
          Seed
          <input aria-label="Seed" inputMode="numeric" {...field("seed")} />
        </label>
        <label className="risk-wide">
          Finish by (optional)
          <input aria-label="Finish by" placeholder="YYYY-MM-DD HH:MM" {...field("target")} />
        </label>
        <details className="risk-wide">
          <summary>Estimates, correlation and risk events</summary>
          <textarea
            aria-label="Risk inputs"
            rows={6}
            placeholder='{"estimates": {"A1": {"min": "4d", "max": "9d"}}, "risks": [...]}'
            {...field("inputs")}
          />
          <p className="risk-note">A totalfloat.risk-inputs/1 document; it replaces the range above (docs/guide/risk.md).</p>
        </details>
        <button type="submit" className="risk-run" disabled={running}>
          {running ? "Running…" : "Run"}
        </button>
      </form>
      {refused && (
        <p className="explain-refused" role="alert">
          {refused.message}
          {refused.hint && <span className="explain-hint">{refused.hint}</span>}
        </p>
      )}
      {stale && <p className="risk-note">The schedule changed, so earlier results were put away: run again for its odds.</p>}
      {report && <Results report={report} view={view} onShow={onShow} />}
    </section>
  );
}

function Results({ report, view, onShow }: { report: RiskReport; view: View; onShow: () => void }) {
  const histogram = useMemo(() => histogramOf(report, WIDTH, HEIGHT), [report]);
  // The same ten the list under it names.
  const tornado = useMemo(() => tornadoOf(report, WIDTH).slice(0, 10), [report]);
  // Marks are placed by the dates as sent; text shows them in the format chosen.
  const finishAt = (n: number) => report.percentiles.find((x) => x.p === n)?.finish ?? "?";
  const p = (n: number) => dateText(finishAt(n)) || "?";
  const marks: [string, string, string][] = [
    ["deterministic", report.deterministic_finish, "Deterministic finish"],
    ["p80", finishAt(80), "P80"],
  ];
  const open = (code: string) => {
    const id = idOf(view, report, code);
    if (id == null) return;
    view.reveal(id);
    onShow();
  };
  return (
    <div className="risk-results" aria-live="polite">
      <p className="risk-summary">
        {report.project}: {report.iterations} iterations, seed {report.seed}, {report.sampled_activities} activities drawn.
        The deterministic finish, {dateText(report.deterministic_finish)}, is met in {percent(report.deterministic_probability)}.
        {report.target && ` Finish by ${dateText(report.target.date)}: ${percent(report.target.probability)}.`}
      </p>
      <dl className="risk-p">
        {[10, 50, 80, 90].map((n) => (
          <div key={n}>
            <dt>P{n}</dt>
            <dd>{p(n)}</dd>
          </div>
        ))}
      </dl>
      <svg
        className="risk-histogram"
        role="img"
        aria-label={`Finish dates from ${dateText(report.earliest_finish)} to ${dateText(report.latest_finish)}; P50 ${p(50)}, P80 ${p(80)}`}
        viewBox={`0 0 ${WIDTH} ${HEIGHT + 14}`}
        width={WIDTH}
        height={HEIGHT + 14}
      >
        {histogram.bars.map((b) => (
          <rect key={b.date} className="risk-bar" x={b.x} y={b.y} width={b.width} height={HEIGHT - b.y}>
            <title>{`${dateText(b.date)}: ${percent(b.share)} finish, ${percent(b.cumulative)} by its end`}</title>
          </rect>
        ))}
        <path className="risk-curve" d={histogram.curve} fill="none" />
        {marks.map(([kind, finish, label]) => {
          const x = xOf(report, finish, WIDTH);
          return x == null ? null : (
            <line key={kind} className={`risk-mark risk-mark-${kind}`} x1={x} x2={x} y1={0} y2={HEIGHT}>
              <title>{`${label}: ${dateText(finish)}`}</title>
            </line>
          );
        })}
        <text x={0} y={HEIGHT + 12} className="risk-axis">
          {dateText(report.cumulative[0]?.date)}
        </text>
        <text x={WIDTH} y={HEIGHT + 12} className="risk-axis" textAnchor="end">
          {dateText(report.cumulative[report.cumulative.length - 1]?.date)}
        </text>
      </svg>
      {tornado.length > 0 && (
        <>
          <h3>Durations that move the finish most</h3>
          <svg
            className="risk-tornado"
            role="img"
            aria-label="Sensitivity of the finish to each duration"
            viewBox={`0 0 ${WIDTH} ${tornado.length * BAR}`}
            width={WIDTH}
            height={tornado.length * BAR}
          >
            {tornado.map((bar, k) => (
              <rect key={bar.code} className="risk-sensitivity" x={bar.x} y={k * BAR + 2} width={bar.width} height={BAR - 4}>
                <title>{`${bar.code} ${bar.name}: ${bar.value.toFixed(2)}`}</title>
              </rect>
            ))}
            <line className="risk-axis-line" x1={WIDTH / 2} x2={WIDTH / 2} y1={0} y2={tornado.length * BAR} />
          </svg>
          <ol className="risk-list">
            {report.sensitivity.slice(0, 10).map((r) => (
              <li key={r.code}>
                <button type="button" onClick={() => open(r.code)} aria-label={`${r.code} ${r.name}: show it in the schedule`}>
                  {r.code}
                </button>{" "}
                {r.name} <span className="risk-value">{r.value.toFixed(2)}</span>
              </li>
            ))}
          </ol>
        </>
      )}
      {report.criticality.length > 0 && (
        <>
          <h3>Most often critical ({report.criticality_total} critical at least once)</h3>
          <ol className="risk-list">
            {report.criticality.slice(0, 10).map((r) => (
              <li key={r.code}>
                <button type="button" onClick={() => open(r.code)} aria-label={`${r.code} ${r.name}: show it in the schedule`}>
                  {r.code}
                </button>{" "}
                {r.name} <span className="risk-value">{percent(r.value)}</span>
              </li>
            ))}
          </ol>
        </>
      )}
      {report.risks.length > 0 && (
        <>
          <h3>Risk events, by their effect on P80</h3>
          <table className="risk-events">
            <thead>
              <tr>
                <th>Risk</th>
                <th>Happened</th>
                <th>P80</th>
                <th>P50</th>
              </tr>
            </thead>
            <tbody>
              {report.risks.map((r) => (
                <tr key={r.code}>
                  <td>{r.code}</td>
                  <td>{percent(r.occurred)}</td>
                  <td>{`${r.p80_effect_days >= 0 ? "+" : ""}${r.p80_effect_days.toFixed(1)} d`}</td>
                  <td>{`${r.p50_effect_days >= 0 ? "+" : ""}${r.p50_effect_days.toFixed(1)} d`}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {report.milestones.length > 0 && (
        <>
          <h3>Milestones</h3>
          <table className="risk-events">
            <thead>
              <tr>
                <th>Milestone</th>
                <th>P50</th>
                <th>P80</th>
              </tr>
            </thead>
            <tbody>
              {report.milestones.slice(0, 10).map((m) => (
                <tr key={m.code}>
                  <td>
                    <button type="button" onClick={() => open(m.code)} aria-label={`${m.code} ${m.name}: show it in the schedule`}>
                      {m.code}
                    </button>
                  </td>
                  <td>{m.p50}</td>
                  <td>{m.p80}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {report.set_by_expected_finish.length > 0 && (
        <p className="risk-note">Not drawn, as their expected finish sets their duration: {report.set_by_expected_finish.join(", ")}.</p>
      )}
    </div>
  );
}
