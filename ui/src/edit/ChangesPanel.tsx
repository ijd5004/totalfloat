// The Changes panel: the edits typed in the grid and not yet recorded, each
// with its old and new value, a button to drop it, and any refusal beside it;
// what they do together, from the dry run's report; and the message, Apply
// and Discard. It's a view on `EditState`: the grid keeps its focus and its
// keys while it's open.

import { type ReactNode, useMemo } from "react";

import type { EditImpact, Rows } from "../api";
import { describeKey } from "../commands";
import { COLUMN_BY_ID, type ColumnId, valuePair } from "../grid/columns";
import { fieldWords } from "../history/words";
import type { View } from "../view";
import { type CellKey, indexOf, inputText } from "./cells";
import { type EditState, type Editing, waiting } from "./editing";
import { dateText, moveText } from "../dates";

/** A pending cell as the panel lists it. */
export interface Listed {
  key: CellKey;
  id: number;
  code: string;
  name: string;
  field: string;
  from: string;
  to: string;
}

/** A field's name where the column's title carries the units shown, which the values here carry themselves. */
const FIELD: Partial<Record<ColumnId, string>> = { original: "Original duration", remaining: "Remaining duration" };

/** The typed cells in the order they were typed, with what each activity shows now. */
export function listed(state: EditState, rows: Rows | null): Listed[] {
  if (!state.pending) return [];
  return [...state.values].map(([key, to]) => {
    const split = key.indexOf(":");
    const id = Number(key.slice(0, split));
    const column = key.slice(split + 1) as ColumnId;
    const shown = COLUMN_BY_ID.get(column);
    const i = rows ? indexOf(rows, id) : undefined;
    return {
      key,
      id,
      code: (rows && i !== undefined ? rows.code[i] : undefined) ?? `#${id}`,
      name: (rows && i !== undefined ? rows.name[i] : undefined) ?? "",
      field: FIELD[column] ?? shown?.title ?? column,
      // As an input over the cell starts: durations with their unit.
      from: rows && i !== undefined ? inputText(rows, i, column) : "",
      to,
    };
  });
}

/** "and 3 more", when a list holds fewer than its total. */
function more(listedCount: number, total: number): string {
  return total > listedCount ? ` and ${(total - listedCount).toLocaleString()} more` : "";
}

export function ChangesPanel({
  editing,
  state,
  rows,
  view,
  mac,
  onClose,
}: {
  editing: Editing;
  state: EditState;
  rows: Rows | null;
  view: View;
  mac: boolean;
  onClose: () => void;
}) {
  const { pending, busy, notice, marks, message } = state;
  const edits = listed(state, rows);
  const count = waiting(state);
  const report = pending?.preview ?? null;
  const projects = report?.projects ?? [];

  // Each activity's row by project and code, as the report names them.
  const byCode = useMemo(() => new Map((rows?.code ?? []).map((code, k) => [`${rows?.project[k]}:${code}`, k])), [rows]);
  /** The hours in a day for an activity the report names, so spans read in the units the grid shows. */
  const perDay = (project: number, code: string) => {
    const i = byCode.get(`${project}:${code}`);
    return i === undefined ? undefined : rows?.hours_per_day?.[i];
  };
  /** Selects an activity in the grid by its code, in the project the report names. */
  const select = (project: number, code: string) => {
    const i = byCode.get(`${project}:${code}`) ?? -1;
    if (!rows || i < 0) {
      editing.say(`${code} isn't in the schedule shown.`);
      return;
    }
    view.reveal(rows.id[i] ?? -1);
  };
  const activity = (project: number, code: string, label?: string) => (
    <button type="button" className="history-activity changes-activity" onClick={() => select(project, code)}>
      <strong>{code}</strong>
      {label && <span>{label}</span>}
    </button>
  );
  // Each refusal no cell owns, as the notice says it.
  const loose = notice?.tone === "bad" && pending && marks.size === 0 ? notice : null;

  return (
    <section
      className="changes"
      aria-label="Changes"
      onKeyDown={(event) => {
        // Escape anywhere in the panel discards, as it does in the grid; an
        // Escape that ends an input method's composition is its own.
        if (event.key === "Escape" && pending && !event.defaultPrevented && !event.nativeEvent.isComposing) {
          event.preventDefault();
          editing.discard();
        }
      }}
    >
      <header className="explain-head">
        <h2>Changes</h2>
        <button type="button" className="explain-close" aria-label="Close the panel" onClick={onClose}>
          ×
        </button>
      </header>
      {!pending ? (
        <>
          {notice && <p className={`changes-notice ${notice.tone}`}>{notice.text}</p>}
          <p className="muted">
            No edits are waiting. Type in the grid&apos;s cells: each edit is previewed here with what it does, and
            recorded when you apply it.
          </p>
        </>
      ) : (
        <>
          <h3 className="history-heading">
            {count === 1 ? "1 edit" : `${count} edits`} waiting
          </h3>
          <ul className="changes-edits" aria-label="Edits waiting">
            {edits.map((edit) => {
              const refused = marks.get(edit.key);
              return (
                <li key={edit.key} className={refused ? "refused" : undefined}>
                  <div className="changes-edit">
                    <button type="button" className="history-activity changes-activity" onClick={() => view.reveal(edit.id)}>
                      <strong>{edit.code}</strong>
                      <span>{edit.name}</span>
                    </button>
                    <button
                      type="button"
                      className="changes-drop"
                      aria-label={`Drop ${edit.code} ${edit.field}`}
                      title="Drop this edit and preview the rest"
                      disabled={busy}
                      onClick={() => void editing.drop(edit.key)}
                    >
                      Drop
                    </button>
                  </div>
                  <p className="changes-value">
                    {edit.field}: <span className="changes-from">{edit.from || "none"}</span> →{" "}
                    <span className="changes-to">{edit.to || "none"}</span>
                  </p>
                  {refused && (
                    <p className="changes-refusal">
                      {refused.message}
                      {refused.hint && <span className="muted"> · {refused.hint}</span>}
                    </p>
                  )}
                </li>
              );
            })}
            {(pending.changes ?? []).map((change) => (
              <li key={change.key} className={marks.get(change.key as CellKey) ? "refused" : undefined}>
                <div className="changes-edit">
                  <span className="changes-change">{change.words}</span>
                  <button
                    type="button"
                    className="changes-drop"
                    aria-label={`Drop ${change.words}`}
                    title="Drop this change and preview the rest"
                    disabled={busy}
                    onClick={() => void editing.drop(change.key)}
                  >
                    Drop
                  </button>
                </div>
                {marks.get(change.key as CellKey) && (
                  <p className="changes-refusal">
                    {marks.get(change.key as CellKey)?.message}
                    {marks.get(change.key as CellKey)?.hint && <span className="muted"> · {marks.get(change.key as CellKey)?.hint}</span>}
                  </p>
                )}
              </li>
            ))}
          </ul>
          {loose && (
            <p className="changes-refusal" role="alert">
              {loose.text}
              {loose.hint && <span className="muted"> · {loose.hint}</span>}
            </p>
          )}
          <h3 className="history-heading">Effect</h3>
          {busy && !report ? (
            <p className="muted">Previewing…</p>
          ) : !report ? (
            <p className="muted">
              {state.previewing
                ? "Not previewed yet. Apply previews these edits on the schedule as it is now."
                : "Not previewed: previews are off, so Apply records these edits at once."}
            </p>
          ) : (
            <div className="changes-effect" aria-label="Effect">
              {projects.length === 0 && <p className="muted">No project changed.</p>}
              {projects.map((project) => (
                <Effect key={project.id} project={project} named={projects.length > 1} activity={activity} perDay={perDay} />
              ))}
              {report.warnings.length > 0 && (
                <>
                  <h4 className="changes-kind">Warnings</h4>
                  <ul className="changes-warnings" aria-label="Warnings">
                    {report.warnings.map((warning, k) => (
                      <li key={k}>{warning}</li>
                    ))}
                  </ul>
                </>
              )}
            </div>
          )}
          <div className="changes-apply">
            <input
              className="edit-message"
              aria-label="Message for the change (optional)"
              placeholder="Why (optional)"
              value={message}
              onChange={(event) => editing.setMessage(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  void editing.apply();
                }
              }}
            />
            {/* The status bar's Apply is the frame's one filled button. */}
            <button
              type="button"
              disabled={busy}
              onClick={() => void editing.apply()}
              title={describeKey("mod+enter", mac)}
            >
              Apply
            </button>
            <button type="button" disabled={busy} onClick={() => editing.discard()} title="Esc">
              Discard
            </button>
          </div>
        </>
      )}
    </section>
  );
}

/** What the edits do to one project. */
function Effect({
  project,
  named,
  activity,
  perDay,
}: {
  project: EditImpact;
  named: boolean;
  activity: (project: number, code: string, label?: string) => ReactNode;
  perDay: (project: number, code: string) => number | null | undefined;
}) {
  const finish =
    project.scheduled_finish === project.previous_finish
      ? `Finish stays ${dateText(project.scheduled_finish) || "unset"}`
      : `Finish ${moveText(project.previous_finish, project.scheduled_finish).map((d) => d || "unset").join(" → ")}`;
  const moved = project.changes.activities;
  const checks = project.checks;
  return (
    <div className="changes-project">
      {named && <h4>{project.code}</h4>}
      <p className="changes-finish">{finish}</p>
      <p className="muted">
        {project.changed_dates_or_float === 1
          ? "1 activity's dates or float moved"
          : `${project.changed_dates_or_float.toLocaleString()} activities' dates or float moved`}
      </p>
      {moved.length > 0 && (
        <ul className="history-changes" aria-label="Moved activities">
          {moved.map((changed) => {
            const day = perDay(project.id, changed.code);
            return (
              <li key={changed.code}>
                {activity(
                  project.id,
                  changed.code,
                  changed.fields.map((f) => `${fieldWords(f.field)}: ${valuePair(f.from ?? "", f.to ?? "", day).join(" → ")}`).join("; "),
                )}
              </li>
            );
          })}
          {more(moved.length, project.changed_dates_or_float) && (
            <li className="muted">{more(moved.length, project.changed_dates_or_float).trim()}</li>
          )}
        </ul>
      )}
      <Codes label="Became critical" codes={project.became_critical} total={project.became_critical_total} project={project.id} activity={activity} />
      <Codes label="Left critical" codes={project.left_critical} total={project.left_critical_total} project={project.id} activity={activity} />
      {project.milestones_moved.length > 0 && (
        <>
          <h4 className="changes-kind">Milestones moved</h4>
          <ul className="history-changes" aria-label="Milestones moved">
            {project.milestones_moved.map((m) => (
              <li key={m.code}>{activity(project.id, m.code, `${moveText(m.from, m.to).map((d) => d || "no date").join(" → ")}`)}</li>
            ))}
            {more(project.milestones_moved.length, project.milestones_moved_total) && (
              <li className="muted">{more(project.milestones_moved.length, project.milestones_moved_total).trim()}</li>
            )}
          </ul>
        </>
      )}
      {checks && checks.newly_failing.length > 0 && (
        <>
          <h4 className="changes-kind bad">Quality checks newly failing</h4>
          <ul className="changes-checks" aria-label="Quality checks newly failing">
            {checks.newly_failing.map((check) => (
              <li key={check.id}>
                <strong>{check.name}</strong> <span className="muted">{check.measure}</span>
                {check.added.length > 0 && <span className="muted"> · {check.added.join(", ")}</span>}
              </li>
            ))}
          </ul>
        </>
      )}
      {checks && checks.newly_passing.length > 0 && (
        <>
          <h4 className="changes-kind done">Quality checks newly passing</h4>
          <ul className="changes-checks" aria-label="Quality checks newly passing">
            {checks.newly_passing.map((name) => (
              <li key={name}>{name}</li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

/** Activities by code, such as those that became critical, each selecting itself. */
function Codes({
  label,
  codes,
  total,
  project,
  activity,
}: {
  label: string;
  codes: string[];
  total: number;
  project: number;
  activity: (project: number, code: string, label?: string) => ReactNode;
}) {
  if (total === 0) return null;
  return (
    <>
      <h4 className="changes-kind">
        {label}: {total.toLocaleString()}
      </h4>
      <ul className="history-changes changes-codes" aria-label={label}>
        {codes.map((code) => (
          <li key={code}>{activity(project, code)}</li>
        ))}
        {more(codes.length, total) && <li className="muted">{more(codes.length, total).trim()}</li>}
      </ul>
    </>
  );
}
