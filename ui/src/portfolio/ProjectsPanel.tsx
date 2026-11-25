// The Projects view: a portfolio's EPS with each node's and
// project's figures, as `portfolio show` gives them but calculated as the
// grid shows each schedule, and a capacity chart
// per resource, as `portfolio capacity` gives it. Clicking a project shows
// its schedule in the grid and Gantt, read-only.

import { useEffect, useMemo, useState } from "react";

import { type Api, type CapacityReport, type PortfolioReport, failure } from "../api";
import { amount, barsOf } from "./capacity";
import { dateText } from "../dates";

const WIDTH = 320;
const HEIGHT = 100;

type Figures = PortfolioReport["nodes"][number]["totals"];

/**
 * Whether `shown`, the location served, is the portfolio's `file`: the same,
 * or it after a path separator, so `subplant.xer` isn't `plant.xer`.
 */
export function isShown(shown: string | null, file: string): boolean {
  if (shown == null) return false;
  const name = file.replace(/^\.\//, "");
  return shown === file || shown.endsWith(`/${name}`) || shown.endsWith(`\\${name}`);
}

function span(t: Figures): string {
  return t.start && t.finish ? `${dateText(t.start, false)} to ${dateText(t.finish, false)}` : "no dates";
}

function figures(t: Figures): string {
  return `${t.activities} activities, ${span(t)}, ${t.critical} critical, ${t.negative_float} negative float`;
}

/** What a node's figures say when some of its projects couldn't be calculated. */
export function storedNote(t: Figures): string {
  const n = t.stored_results;
  if (n === 0) return "";
  return ` · includes the stored results of ${n} ${n === 1 ? "project" : "projects"} that couldn't be calculated`;
}

const STORED = " · the file's stored results, since it couldn't be calculated";

export function ProjectsPanel({
  api,
  shown,
  onOpened,
  onClose,
}: {
  api: Api;
  /** The schedule file shown now, as the portfolio names it or as served. */
  shown: string | null;
  /** After a project is opened, so the grid and Gantt read it. */
  onOpened: () => void | Promise<void>;
  onClose: () => void;
}) {
  const [report, setReport] = useState<PortfolioReport | null>(null);
  const [capacity, setCapacity] = useState<CapacityReport | null>(null);
  const [by, setBy] = useState<"week" | "month">("month");
  const [resource, setResource] = useState<string>("");
  const [refused, setRefused] = useState<{ message: string; hint: string } | null>(null);
  const [opening, setOpening] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    api
      .portfolio()
      .then((r) => {
        if (!live) return;
        setReport(r);
        setRefused(null);
      })
      .catch((e: unknown) => live && setRefused(failure(e, "The portfolio couldn't be read.")));
    return () => {
      live = false;
    };
  }, [api]);
  useEffect(() => {
    let live = true;
    api
      .capacity({ by })
      .then((c) => {
        if (!live) return;
        setCapacity(c);
        setRefused(null);
      })
      .catch((e: unknown) => live && setRefused(failure(e, "Capacity couldn't be worked out.")));
    return () => {
      live = false;
    };
  }, [api, by]);

  // Resources with demand, the most over-allocated first, as the one shown
  // until another is chosen.
  const lines = useMemo(
    () =>
      [...(capacity?.resources ?? [])]
        .filter((l) => l.total_demand > 0)
        .sort((a, b) => b.periods_over - a.periods_over || b.total_demand - a.total_demand),
    [capacity],
  );
  const line = lines.find((l) => l.short_name === resource) ?? lines[0] ?? null;
  const bars = useMemo(() => (line ? barsOf(line, WIDTH, HEIGHT) : []), [line]);

  const open = async (file: string) => {
    setOpening(file);
    setRefused(null);
    try {
      await api.openProject(file);
      await onOpened();
    } catch (e) {
      setRefused(failure(e, "The project couldn't be opened."));
    } finally {
      setOpening(null);
    }
  };

  return (
    <section className="projects" aria-label="Projects">
      <header className="explain-head">
        <h2>Projects</h2>
        <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
          ×
        </button>
      </header>
      {refused && (
        <p className="explain-refused" role="alert">
          {refused.message}
          {refused.hint && <span className="explain-hint">{refused.hint}</span>}
        </p>
      )}
      {!report && !refused && <p className="explain-empty">Reading the portfolio…</p>}
      {report && (
        <ul className="eps" aria-label="EPS">
          {report.nodes.map((node) => (
            <li key={node.id} className="eps-node" style={{ paddingLeft: `${node.depth * 12}px` }}>
              <div className="eps-name">
                <strong>{node.name}</strong>{" "}
                <span className="eps-figures">
                  {figures(node.totals)}
                  {storedNote(node.totals)}
                </span>
              </div>
              <ul className="eps-projects">
                {report.projects
                  .filter((p) => p.eps === node.id)
                  .map((p) => (
                    <li key={`${p.file}:${p.id}`}>
                      <button
                        type="button"
                        className="eps-project"
                        aria-current={isShown(shown, p.file) ? "true" : undefined}
                        disabled={opening != null}
                        onClick={() => void open(p.file)}
                        aria-label={`Open ${p.code} in ${p.file}`}
                      >
                        {p.code}
                      </button>{" "}
                      <span className="eps-figures">
                        {p.name ?? ""} · data date {dateText(p.data_date, false) || "none"} · {figures(p.totals)}
                        {p.finish_variance_days != null &&
                          ` · ${p.finish_variance_days >= 0 ? "+" : ""}${p.finish_variance_days.toFixed(1)} d to must finish`}
                        {p.results === "stored" && STORED}
                      </span>
                    </li>
                  ))}
              </ul>
            </li>
          ))}
        </ul>
      )}
      <h3>Capacity</h3>
      <div className="capacity-controls">
        <label>
          Resource
          <select aria-label="Resource" value={line?.short_name ?? ""} onChange={(e) => setResource(e.target.value)}>
            {lines.map((l) => (
              <option key={l.short_name} value={l.short_name}>
                {l.short_name} {l.name}
                {l.periods_over > 0 ? ` (over in ${l.periods_over})` : ""}
              </option>
            ))}
          </select>
        </label>
        <label>
          By
          <select aria-label="By" value={by} onChange={(e) => setBy(e.target.value as "week" | "month")}>
            <option value="week">week</option>
            <option value="month">month</option>
          </select>
        </label>
      </div>
      {capacity && !line && <p className="explain-empty">No resource has demand in the portfolio.</p>}
      {line && (
        <>
          <svg
            className="capacity-chart"
            role="img"
            aria-label={`${line.short_name}: demand by ${capacity?.by ?? by} against supply, over in ${line.periods_over} periods`}
            viewBox={`0 0 ${WIDTH} ${HEIGHT + 14}`}
            width={WIDTH}
            height={HEIGHT + 14}
          >
            {bars.map((b) => (
              <g key={b.start}>
                <rect className={b.over ? "capacity-bar over" : "capacity-bar"} x={b.x} y={b.y} width={b.width} height={HEIGHT - b.y}>
                  <title>{`${dateText(b.start)}: ${amount(b.demand)} ${line.unit} against ${b.supply == null ? "no limit" : amount(b.supply)}`}</title>
                </rect>
                {b.supplyY != null && <line className="capacity-supply" x1={b.x} x2={b.x + b.width + 1} y1={b.supplyY} y2={b.supplyY} />}
              </g>
            ))}
            <text x={0} y={HEIGHT + 12} className="risk-axis">
              {dateText(bars[0]?.start)}
            </text>
            <text x={WIDTH} y={HEIGHT + 12} className="risk-axis" textAnchor="end">
              {dateText(bars[bars.length - 1]?.start)}
            </text>
          </svg>
          {line.peak && (
            <p className="capacity-peak">
              Worst: the {capacity?.by ?? by} of {dateText(line.peak.start)}, {amount(line.peak.demand)} {line.unit} against {amount(line.peak.supply)}
              : {line.peak.projects.map((s) => `${s.project} (${s.file}) ${amount(s.demand)}`).join(", ")}.
            </p>
          )}
        </>
      )}
    </section>
  );
}
