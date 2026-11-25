// The resource usage view: a resource's units per week or month,
// actual and remaining stacked, against its limit, from `/api/1/usage`, as
// a histogram and as a table. The units are the server's; the page only
// lays them out. The limit in force is shown above them, and can be set or
// cleared, from a day or throughout, as an edit waiting with the others
//.

import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";

import { type Api, type UsageReport, failure } from "../api";
import { units, usageChart } from "./chart";
import { dateText } from "../dates";
import type { Change } from "../edit/editing";
import { changeKey } from "../edit/shape";

const WIDTH = 320;
const HEIGHT = 140;

/** A resource the list offers. */
export interface UsageResource {
  short_name: string;
  name: string;
}

export interface UsagePanelProps {
  api: Api;
  /** The file's resources, as `inspect` lists them; more can be typed. */
  resources: readonly UsageResource[];
  /** Which schedule is shown, so its usage is asked for again when it changes. */
  head: string | null;
  onClose: () => void;
  /**
   * Setting a limit: a change that waits with the other edits, previewed and
   * applied as they are. Absent, or not enabled, where the schedule is read-only.
   */
  limits?: { enabled: boolean; change: (change: Change) => void };
}

export function UsagePanel({ api, resources, head, onClose, limits }: UsagePanelProps) {
  // What's typed in the Resource field; until something is, the list's first.
  const [typed, setTyped] = useState<string | null>(null);
  const resource = typed ?? resources[0]?.short_name ?? "";
  const [by, setBy] = useState<"week" | "month">("week");
  const [report, setReport] = useState<UsageReport | null>(null);
  const [refused, setRefused] = useState<{ message: string; hint: string } | null>(null);
  const [loading, setLoading] = useState(false);
  // Each ask counts, so an answer to one since replaced is dropped.
  const asked = useRef(0);
  // What was asked last, answered yet or not, so a new schedule or period
  // asks it again.
  const active = useRef<{ resource: string; by: "week" | "month" } | null>(null);

  const show = async (wanted: string, period: "week" | "month") => {
    const name = wanted.trim();
    if (!name) {
      setRefused({ message: "Choose a resource.", hint: "" });
      return;
    }
    const generation = ++asked.current;
    active.current = { resource: name, by: period };
    setLoading(true);
    setRefused(null);
    try {
      const answer = await api.usage({ resource: name, by: period });
      if (generation === asked.current) setReport(answer);
    } catch (error) {
      if (generation === asked.current) {
        setReport(null);
        setRefused(failure(error, "The usage couldn't be found."));
      }
    } finally {
      if (generation === asked.current) setLoading(false);
    }
  };

  // A changed schedule: what was asked, again, if the schedule has that
  // resource. Otherwise the panel starts over from the new list.
  const shown = useRef<string | null>(null);
  useEffect(() => {
    const last = active.current;
    if (shown.current !== null && last) {
      if (resources.length > 0 && !resources.some((r) => r.short_name === last.resource)) {
        asked.current++;
        active.current = null;
        setTyped(null);
        setReport(null);
        setRefused(null);
        setLoading(false);
      } else {
        void show(last.resource, last.by);
      }
    }
    shown.current = head;
    // Only a new schedule asks again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [head]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    void show(resource, by);
  };

  return (
    <section className="usage" aria-label="Resource usage">
      <header className="explain-head">
        <h2>Resource usage</h2>
        <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
          ×
        </button>
      </header>
      <form className="risk-form" onSubmit={submit}>
        <label className="risk-wide">
          Resource
          <input
            aria-label="Resource"
            list="usage-resources"
            value={resource}
            placeholder="Short name"
            onChange={(event) => setTyped(event.target.value)}
          />
          <datalist id="usage-resources">
            {resources.map((r) => (
              <option key={r.short_name} value={r.short_name}>
                {r.name}
              </option>
            ))}
          </datalist>
        </label>
        <label>
          By
          <select
            aria-label="By"
            value={by}
            onChange={(event) => {
              const period = event.target.value === "month" ? "month" : "week";
              setBy(period);
              // The resource in the field, or the one shown if it's been cleared.
              if (active.current) void show(resource.trim() || active.current.resource, period);
            }}
          >
            <option value="week">Week</option>
            <option value="month">Month</option>
          </select>
        </label>
        <button type="submit" className="risk-run" disabled={loading}>
          {loading ? "Finding…" : "Show"}
        </button>
      </form>
      {refused && (
        <p className="explain-refused" role="alert">
          {refused.message}
          {refused.hint && <span className="explain-hint">{refused.hint}</span>}
        </p>
      )}
      {report && <Usage report={report} limits={limits} />}
    </section>
  );
}

/** `1 unit an hour`, `2 units an hour`. */
const perHour = (n: number) => `${units(n)} ${units(n) === "1" ? "unit" : "units"} an hour`;

/** The limit over time, as the panel says it: `2 units an hour; 3 from 04 Jan 27`. */
export function limitText(limits: UsageReport["limits"]): string {
  if (limits.every((l) => l.units_per_hour === null)) return "No limit";
  const each = limits.map((l, k) => {
    const amount = l.units_per_hour === null ? "none" : k === 0 ? perHour(l.units_per_hour) : units(l.units_per_hour);
    return l.from === null || k === 0 ? amount : `${amount} from ${dateText(l.from, false)}`;
  });
  return `Limit ${each.join("; ")}`;
}

/** The limit in force, and a form to set or clear it as an edit waiting with the others. */
function Limit({ report, limits }: { report: UsageReport; limits: UsagePanelProps["limits"] }) {
  const resource = report.resource.short_name;
  const [amount, setAmount] = useState("");
  const [from, setFrom] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const send = (value: number | null) => {
    const day = from.trim();
    const words =
      value === null ? `Clear ${resource}'s limit` : `Limit ${resource} to ${perHour(value)}`;
    limits?.change({
      key: changeKey("limit", `${resource}@${day}`),
      edit: { op: "set_resource_limit", resource, max_units_per_hour: value, ...(day ? { from: day } : {}) },
      words: `${words}${day ? ` from ${dateText(day, false)}` : ""}`,
    });
    setProblem(null);
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const text = amount.trim();
    if (!/^\d+(\.\d+)?$|^\.\d+$/.test(text)) {
      setProblem("Give zero or more units an hour: 2 is two people at once.");
      return;
    }
    send(Number(text));
  };
  return (
    <div className="usage-limit-edit">
      <p className="usage-limit-now">{limitText(report.limits)}.</p>
      {limits?.enabled && (
        <form className="usage-limit-form" aria-label="Set the limit" onSubmit={submit}>
          <label>
            Units an hour
            <input
              aria-label="Limit, units an hour"
              inputMode="decimal"
              value={amount}
              onChange={(event) => setAmount(event.target.value)}
            />
          </label>
          <label>
            From
            <input aria-label="From" type="date" value={from} onChange={(event) => setFrom(event.target.value)} />
          </label>
          <button type="submit">Set limit</button>
          <button type="button" onClick={() => send(null)}>
            Clear limit
          </button>
        </form>
      )}
      {problem && (
        <p className="explain-refused" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}

function Usage({ report, limits }: { report: UsageReport; limits: UsagePanelProps["limits"] }) {
  const chart = useMemo(() => usageChart(report.periods, WIDTH, HEIGHT), [report]);
  const over = report.periods.filter((p) => p.over).length;
  const unit = report.unit;
  const periods = report.periods.length === 1 ? report.by : `${report.by}s`;
  const first = dateText(report.periods[0]?.start);
  const last = dateText(report.periods[report.periods.length - 1]?.start);
  return (
    <div className="usage-results" aria-live="polite">
      <p className="usage-summary">
        {report.resource.short_name} ({report.resource.name}), by {report.by}: {units(report.totals.actual)} {unit} actual and{" "}
        {units(report.totals.remaining)} remaining over {report.periods.length} {periods}
        {over > 0 ? `; over its limit in ${over}` : ""}.
      </p>
      <Limit key={`${report.resource.short_name}:${JSON.stringify(report.limits)}`} report={report} limits={limits} />
      {report.periods.length === 0 ? (
        <p className="risk-note">Nothing is spread to this resource.</p>
      ) : (
        <>
          <svg
            className="usage-histogram"
            role="img"
            aria-label={`${report.resource.short_name}'s ${unit} by ${report.by} from ${first} to ${last}, up to ${units(chart.top)}`}
            viewBox={`0 0 ${WIDTH} ${HEIGHT + 14}`}
            width={WIDTH}
            height={HEIGHT + 14}
          >
            {chart.bars.map((b) => (
              <g key={b.start} className={b.over ? "usage-period usage-over" : "usage-period"}>
                <title>
                  {`${dateText(b.start)}: ${units(b.actual)} actual, ${units(b.remaining)} remaining` +
                    (b.limit === null ? "" : `, limit ${units(b.limit)}`) +
                    (b.over ? ", over the limit" : "")}
                </title>
                <rect className="usage-actual" x={b.x} y={b.actualY} width={b.width} height={HEIGHT - b.actualY} />
                <rect className="usage-remaining" x={b.x} y={b.remainingY} width={b.width} height={b.actualY - b.remainingY} />
              </g>
            ))}
            <path className="usage-limit" d={chart.limitPath} fill="none" />
            <text x={0} y={HEIGHT + 12} className="risk-axis">
              {first}
            </text>
            <text x={WIDTH} y={HEIGHT + 12} className="risk-axis" textAnchor="end">
              {last}
            </text>
          </svg>
          <p className="usage-legend">
            <span className="usage-key usage-actual-key" /> Actual <span className="usage-key usage-remaining-key" /> Remaining{" "}
            <span className="usage-key usage-limit-key" /> Limit <span className="usage-key usage-over-key" /> Over the limit
          </p>
          <table className="usage-table">
            <thead>
              <tr>
                <th scope="col">{report.by === "week" ? "Week of" : "Month"}</th>
                <th scope="col">Actual</th>
                <th scope="col">Remaining</th>
                <th scope="col">Limit</th>
              </tr>
            </thead>
            <tbody>
              {report.periods.map((p) => (
                <tr key={p.start} className={p.over ? "usage-over" : undefined}>
                  <th scope="row">{dateText(p.start)}</th>
                  <td>{units(p.actual)}</td>
                  <td>{units(p.remaining)}</td>
                  <td>
                    {p.limit === null ? "none" : units(p.limit)}
                    {p.over && <span className="usage-flag"> over</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {report.not_spread.length > 0 && (
        <details className="risk-note usage-not-spread">
          <summary>
            {report.not_spread.length} assignment{report.not_spread.length === 1 ? "" : "s"} couldn't be spread
          </summary>
          <ul>
            {report.not_spread.map((n, k) => (
              <li key={k}>
                {n.activity} ({n.reason})
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}
