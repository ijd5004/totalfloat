// A comparison of two points, as the History and Review panels show it: how
// the finish moved, what was counted, and each changed, added and deleted
// activity and link. Clicking an activity selects it in the grid and Gantt.

import type { HistoryDiff, Rows, Summary } from "../api";
import { type Compared, comparedKey, valuePair } from "../grid/columns";
import type { View } from "../view";
import { type Say, countWords, fieldWords, finishWords } from "./words";

/** How many of each kind a comparison lists; its counts say how many there were. */
const LISTED = 25;

/** A comparison as a `totalfloat.diff/1` report: a history diff's, or a merge's impact. */
export type Compare = Pick<HistoryDiff, "diff">;

/** Whether a comparison listed fewer than it counted. */
function cutShort(project: HistoryDiff["diff"]["projects"][number]): boolean {
  const counts = project.counts ?? {};
  return (
    (counts.changed ?? 0) > (project.changed ?? []).length ||
    (counts.added ?? 0) > (project.added ?? []).length ||
    (counts.deleted ?? 0) > (project.deleted ?? []).length ||
    (counts.links_added ?? 0) > (project.links_added ?? []).length ||
    (counts.links_deleted ?? 0) > (project.links_deleted ?? []).length ||
    (counts.links_changed ?? 0) > (project.links_changed ?? []).length
  );
}

/** A comparison as the grid's Before and After columns read it. */
export function comparedOf(diff: Compare, summary: Summary | null): Compared {
  const projects = new Map((summary?.inspect.projects ?? []).map((p) => [p.id, p.short_name]));
  const activities = new Map<string, { added: boolean; fields: { field: string; from: string; to: string }[] }>();
  for (const project of diff.diff.projects) {
    const code = project.project?.code ?? "";
    for (const added of project.added ?? []) activities.set(comparedKey(code, added.code), { added: true, fields: [] });
    for (const changed of project.changed ?? []) {
      activities.set(comparedKey(code, changed.code), {
        added: false,
        fields: changed.changes.map(({ field, from, to }) => ({ field, from, to })),
      });
    }
  }
  return { projects, activities };
}

/** An activity's index in `rows`, by its code, in a project by its code; -1 if it isn't there. */
export function indexIn(rows: Rows | null, summary: Summary | null, project: string, code: string): number {
  if (!rows) return -1;
  const projects = summary?.inspect.projects ?? [];
  const projectId = projects.find((p) => p.short_name === project)?.id;
  // A project the schedule shown doesn't have holds none of its activities.
  if (projectId === undefined && projects.length > 0) return -1;
  return rows.code.findIndex((c, k) => c === code && (projectId === undefined || rows.project[k] === projectId));
}

/** The `task_id`s of the activities a comparison changed or added that the schedule shown has. */
export function idsOf(diff: Compare, rows: Rows, summary: Summary | null): Set<number> {
  const ids = new Set<number>();
  for (const project of diff.diff.projects) {
    const code = project.project?.code ?? "";
    for (const item of [...(project.changed ?? []), ...(project.added ?? [])]) {
      const i = indexIn(rows, summary, code, item.code);
      if (i >= 0) ids.add(rows.id[i] ?? -1);
    }
  }
  return ids;
}

export function Comparison({
  diff,
  rows,
  summary,
  view,
  say,
  inGrid,
  onInGrid,
}: {
  diff: Compare;
  rows: Rows | null;
  summary: Summary | null;
  view: View;
  say: Say;
  /** Whether the grid shows it in its Before and After columns. */
  inGrid: boolean;
  onInGrid: (inGrid: boolean) => void;
}) {
  const indexOf = (project: string, code: string) => indexIn(rows, summary, project, code);

  /** Selects an activity in the grid by its code, in a project by its code. */
  const select = (project: string, code: string) => {
    const i = indexOf(project, code);
    if (!rows || i < 0) {
      say(`${code} isn't in the schedule shown.`, "info", "it may have been deleted since");
      return;
    }
    view.reveal(rows.id[i] ?? -1);
  };

  return (
    <div className="history-result" aria-label="Comparison">
      <label className="history-in-grid">
        <input type="checkbox" checked={inGrid} onChange={(event) => onInGrid(event.target.checked)} />
        Show Before and After in the grid
      </label>
      {diff.diff.projects.map((project) => {
        const code = project.project?.code ?? "";
        return (
          <div key={code} className="history-project">
            {diff.diff.projects.length > 1 && <h4>{code}</h4>}
            <p className="history-impact">{finishWords(project.project)}</p>
            <p className="muted">{countWords(project.counts)}</p>
            {cutShort(project) && (
              <p className="history-cut">
                Only the first {LISTED} of each kind are listed here and in the grid. Compare points closer together to
                see the rest.
              </p>
            )}
            {(project.changed ?? []).length > 0 && (
              <ul className="history-changes" aria-label="Changed activities">
                {(project.changed ?? []).map((changed) => {
                  // Spans in the units the grid shows, by the activity's day as it is now.
                  const at = indexOf(code, changed.code);
                  const perDay = at >= 0 ? rows?.hours_per_day?.[at] : undefined;
                  return (
                    <li key={changed.code}>
                      <button type="button" className="history-activity" onClick={() => select(code, changed.code)}>
                        <strong>{changed.code}</strong>
                        {changed.changes.map((c) => (
                          <span key={c.field}>
                            {fieldWords(c.field)}: {valuePair(c.from, c.to, perDay).join(" → ")}
                          </span>
                        ))}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
            {(project.added ?? []).length > 0 && (
              <ul className="history-changes" aria-label="Added activities">
                {(project.added ?? []).map((added) => (
                  <li key={added.code}>
                    <button type="button" className="history-activity" onClick={() => select(code, added.code)}>
                      <strong>{added.code}</strong> <span>Added: {added.name}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {(project.deleted ?? []).length > 0 && (
              <ul className="history-changes" aria-label="Deleted activities">
                {(project.deleted ?? []).map((deleted) => (
                  <li key={deleted.code}>
                    <strong>{deleted.code}</strong> <span>Deleted: {deleted.name}</span>
                  </li>
                ))}
              </ul>
            )}
            {[
              ["Links added", project.links_added],
              ["Links deleted", project.links_deleted],
              ["Links changed", project.links_changed],
            ].map(([label, links]) =>
              Array.isArray(links) && links.length > 0 ? (
                <ul key={String(label)} className="history-changes" aria-label={String(label)}>
                  {links.map((link) => (
                    <li key={`${link.predecessor}>${link.successor}`}>
                      {String(label).replace("Links ", "")}: {link.predecessor} → {link.successor} {link.kind}
                      {link.from_lag !== link.to_lag && (link.from_lag || link.to_lag)
                        ? `, lag ${link.from_lag ?? "none"} → ${link.to_lag ?? "none"}`
                        : ""}
                    </li>
                  ))}
                </ul>
              ) : null,
            )}
          </div>
        );
      })}
    </div>
  );
}
