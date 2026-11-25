// Editing the schedule from the page: each commit is previewed with a dry
// run, then recorded as a changeset on a second Enter or Apply. Refusals mark
// the cells they came from; a branch moved by someone else keeps what was
// typed so it can be applied again; Ctrl+Z takes out the last changeset this
// page made.

import { type Api, ApiError, type Edit, type EditReport, type Problem, type Rows } from "../api";
import { type Batch, type CellKey, type CellProblem, type Typed, buildBatch, cellKey, shownText, withUnits } from "./cells";
import { dateText, moveText } from "../dates";
import type { NewActivity } from "./shape";

/** What the status bar says about editing. */
export interface Notice {
  tone: "info" | "bad" | "preview" | "done";
  text: string;
  hint?: string;
}

/**
 * A change to the schedule's shape, not typed in a cell: an activity or WBS
 * node added or deleted, an activity moved to another node.
 */
export interface Change {
  /** Stable while it waits, so it can be dropped: `change:…`. */
  key: string;
  edit: Edit;
  /** What it does, as the Changes panel lists it. */
  words: string;
  /** For an activity added: what was typed, so the grid can show it while it waits. */
  adds?: NewActivity;
  /** The project it's in (`proj_id`): a request names one at most. */
  project?: number;
  /** For a WBS node added or renamed: its path, and its code and name as they'll be. */
  wbs?: { path: string; code?: string; name?: string };
  /** For an activity moved to another WBS node: the node's path. */
  moves?: string;
  /** Made again, it stays where the first was, as a rename does for the edits after it that use its new name. */
  keepPlace?: boolean;
}

/** Values typed and changes made, not yet recorded. */
export interface Pending {
  typed: readonly Typed[];
  /** Changes to the schedule's shape, sent after the typed cells' edits. */
  changes?: readonly Change[];
  batch: Batch;
  /** The head the preview was made at; recording needs the branch still there. */
  head: string;
  /** The dry run's report, once there is one. */
  preview: EditReport | null;
}

/** A changeset this page recorded, which Ctrl+Z can take out. */
export interface Made {
  id: string;
  short: string;
  /** What it did, as the report put it. */
  what: string;
}

export interface EditState {
  pending: Pending | null;
  /** Typed values to show in their cells until they're recorded or dropped. */
  values: ReadonlyMap<CellKey, string>;
  /** Refused cells, and why. */
  marks: ReadonlyMap<CellKey, CellProblem>;
  notice: Notice | null;
  /** Shown over the grid when another writer moved the branch. */
  banner: string | null;
  busy: boolean;
  /** Whether each commit is previewed before it's recorded. */
  previewing: boolean;
  /** The message to record with the edits waiting; cleared when none are left. */
  message: string;
  /** What this page recorded, latest last. */
  made: readonly Made[];
  /** Changes taken while a request was out, sent once it's done: shown, and their IDs taken, meanwhile. */
  queued: readonly Change[];
  /** Changes taken back while a request was out, by key: no longer shown, and dropped once it's done. */
  dropping: ReadonlySet<string>;
}

/** What the page shows, as far as editing needs it. */
export interface Shown {
  rows: Rows | null;
  /** The head shown, or null for something that can't change. */
  head: string | null;
  writable: boolean;
}

/** Fetches what's shown again; resolves with it once it's shown, or null if a newer fetch won or it failed. */
export type Reload = () => Promise<Shown | null>;

const PREVIEW_SETTING = "totalfloat.preview-edits";

function storedPreview(storage: Storage | null): boolean {
  return storage?.getItem(PREVIEW_SETTING) !== "off";
}

const NONE = new Map<CellKey, never>();

/** How many typed cells and changes wait to be recorded: what the Changes panel lists. */
export function waiting(state: EditState): number {
  return (state.pending ? state.values.size + (state.pending.changes?.length ?? 0) : 0) + state.queued.length;
}

/**
 * Changes with one under each key: a later one takes the earlier's place, as
 * a second edit to a link does. It goes last, after anything it may name,
 * unless it keeps its place, as a rename does for the edits that use its name.
 */
export function latest(changes: readonly Change[]): Change[] {
  const kept = new Map<string, Change>();
  for (const change of changes) {
    if (!(change.keepPlace && kept.has(change.key))) kept.delete(change.key);
    kept.set(change.key, change);
  }
  return [...kept.values()];
}

/** The WBS code an edit gives a node, as a node added or a new code. */
function codeMade(edit: Edit): string | null {
  if (edit.op === "add_wbs") return edit.code;
  if (edit.op === "rename_wbs") return edit.code ?? null;
  return null;
}

/** The WBS node an edit names. */
export function wbsNamed(edit: Edit): string | null {
  if (edit.op === "add_activity" || edit.op === "set_wbs" || edit.op === "rename_wbs") return edit.wbs;
  if (edit.op === "add_wbs") return edit.parent;
  return null;
}

/**
 * `keys` with the changes after them that name a WBS node by a code one of
 * them gives it, and theirs in turn: dropped alone, those would name a node
 * that isn't there.
 */
export function withDependents(keys: readonly string[], changes: readonly Change[]): string[] {
  const gone = new Set(keys);
  const made = new Set<string>();
  for (const change of changes) {
    const named = wbsNamed(change.edit);
    if (!gone.has(change.key) && named !== null && made.has(named.toLowerCase())) gone.add(change.key);
    const code = gone.has(change.key) ? codeMade(change.edit) : null;
    if (code !== null) made.add(code.toLowerCase());
  }
  return [...gone];
}

/** Every change to the schedule's shape waiting, those queued behind a request too. */
export function changesWaiting(state: Pick<EditState, "pending" | "queued"> & Partial<Pick<EditState, "dropping">>): readonly Change[] {
  const pending = (state.pending?.changes ?? []).filter((change) => !state.dropping?.has(change.key));
  return state.queued.length === 0 ? pending : latest([...pending, ...state.queued]);
}

/** Says what an edit did to each project's finish and dates. */
export function impactText(report: EditReport): string {
  const projects = report.projects ?? [];
  const parts = projects.map((project) => {
    const named = projects.length > 1 ? `${project.code}: ` : "";
    const finish =
      project.scheduled_finish === project.previous_finish
        ? `finish stays ${dateText(project.scheduled_finish) || "unset"}`
        : `finish ${moveText(project.previous_finish, project.scheduled_finish).map((d) => d || "unset").join(" → ")}`;
    const changed = project.changed_dates_or_float;
    const critical =
      project.became_critical_total || project.left_critical_total
        ? `, ${project.became_critical_total} became critical, ${project.left_critical_total} left`
        : "";
    return `${named}${finish}, ${changed.toLocaleString()} ${changed === 1 ? "activity" : "activities"} changed${critical}`;
  });
  const failing = projects.reduce((sum, project) => sum + (project.checks?.newly_failing.length ?? 0), 0);
  const checks = failing ? `; ${failing} check${failing === 1 ? "" : "s"} newly ${failing === 1 ? "fails" : "fail"}` : "";
  const warnings = report.warnings?.length ? `; ${report.warnings.length} warning${report.warnings.length === 1 ? "" : "s"}` : "";
  return `${parts.join("; ") || "no project changed"}${checks}${warnings}`;
}

/** The cells a refusal's problems point at, and the problems no cell owns. */
export function marksOf(batch: Batch, problems: readonly Problem[]): { marks: Map<CellKey, CellProblem>; loose: Problem[] } {
  const marks = new Map<CellKey, CellProblem>();
  const loose: Problem[] = [];
  for (const problem of problems) {
    const cells = problem.edit === undefined ? undefined : batch.cells[problem.edit - 1];
    if (!cells || cells.length === 0) {
      loose.push(problem);
      continue;
    }
    for (const cell of cells) {
      if (!marks.has(cell)) marks.set(cell, { message: problem.message, hint: problem.hint });
    }
  }
  return { marks, loose };
}

function valuesOf(typed: readonly Typed[]): Map<CellKey, string> {
  // A bare duration shows the unit it was typed in, so it reads, and copies,
  // the same whatever units are shown later.
  return new Map(typed.map((cell) => [cellKey(cell.id, cell.column), shownText(cell)]));
}

function asApiError(error: unknown): ApiError {
  return error instanceof ApiError
    ? error
    : new ApiError(0, "unreachable", "The server couldn't be reached.", "Is `totalfloat serve` still running?");
}

/**
 * The page's editing, outside React so it can be tested alone: React reads it
 * through `subscribe` and `state`.
 */
export class Editing {
  private current: EditState;
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly api: Api,
    private readonly storage: Storage | null = typeof localStorage === "undefined" ? null : localStorage,
  ) {
    this.current = {
      pending: null,
      values: NONE,
      marks: NONE,
      notice: null,
      banner: null,
      busy: false,
      previewing: storedPreview(storage),
      message: "",
      made: [],
      queued: [],
      dropping: new Set(),
    };
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  readonly state = (): EditState => this.current;

  private page: Shown = { rows: null, head: null, writable: false };
  private fetchShown: Reload = () => Promise.resolve(null);

  /** Tells editing what the page shows now. */
  show(page: Shown): void {
    this.page = page;
  }

  /** How editing fetches what's shown again, after a change or a refusal. */
  reloadWith(reload: Reload): void {
    this.fetchShown = reload;
  }

  /**
   * Fetches what's shown again, and keeps it, so the next edit starts from
   * the head it shows. If nothing comes back, the head a change answered
   * with stands in until the page shows it.
   */
  private async reload(head?: string | null): Promise<void> {
    const shown = await this.fetchShown();
    if (shown) this.page = shown;
    else if (head) this.page = { ...this.page, head };
  }

  /** Cells committed while a request was out, sent once it's done. */
  private queued: Typed[] = [];
  private queuedChanges: Change[] = [];
  /** Changes the request out holds that were taken back meanwhile, by key. */
  private dropsQueued = new Set<string>();
  /** Holds on edits, counted, as while the branch shown is switched. */
  private held = 0;

  /**
   * Holds every edit until the returned function is called: commits, apply
   * and undo are refused meanwhile, so nothing records on a branch the page
   * is moving away from.
   */
  hold(): () => void {
    this.held++;
    let released = false;
    return () => {
      if (released) return;
      released = true;
      this.held--;
    };
  }

  /** Whether edits are held, saying so if they are. */
  private holding(): boolean {
    if (this.held > 0) this.say("The branch is being switched; edit once it's shown.", "bad");
    return this.held > 0;
  }

  /** Ends a request with `change`, then sends anything committed meanwhile. */
  private idle(change: Partial<EditState> = {}): void {
    this.set({ ...change, busy: false, queued: [], dropping: new Set() });
    const drops = [...this.dropsQueued];
    this.dropsQueued.clear();
    if (this.queued.length > 0 || this.queuedChanges.length > 0) {
      // Those taken back go before what's queued is checked with what's pending.
      const pending = this.current.pending;
      if (drops.length > 0 && pending) {
        this.current = { ...this.current, pending: { ...pending, changes: (pending.changes ?? []).filter((c) => !drops.includes(c.key)) } };
      }
      const [queued, changes] = [this.queued, this.queuedChanges];
      this.queued = [];
      this.queuedChanges = [];
      void this.take(queued, this.current.previewing, changes);
    } else if (drops.length > 0) {
      void this.drop(...drops);
    }
  }

  private set(change: Partial<EditState>): void {
    const next = { ...this.current, ...change };
    // The message is for the edits waiting: once none wait, recorded,
    // discarded or dropped, it goes with them, so a later edit can't take it.
    if (next.pending === null && next.queued.length === 0) next.message = "";
    this.current = next;
    for (const listener of this.listeners) listener();
  }

  /** Says something in the status bar. */
  say(text: string, tone: Notice["tone"] = "info", hint?: string): void {
    this.set({ notice: { tone, text, ...(hint ? { hint } : {}) } });
  }

  setMessage(message: string): void {
    this.set({ message });
  }

  setPreviewing(previewing: boolean): void {
    this.storage?.setItem(PREVIEW_SETTING, previewing ? "on" : "off");
    this.set({ previewing });
    this.say(previewing ? "Edits are previewed before they're recorded." : "Edits are recorded as soon as they're committed.");
  }

  /**
   * Takes typed cells: checks them, then previews them, or records them
   * straight away with previews turned off. Cells typed while a preview is
   * open join it, previewed again together.
   */
  async commit(given: readonly Typed[]): Promise<void> {
    await this.take(given, this.current.previewing);
  }

  /** Takes changes to the schedule's shape: they join what's pending, previewed or recorded as typed cells are. */
  async change(changes: readonly Change[]): Promise<void> {
    await this.take([], this.current.previewing, changes);
  }

  /** Checks typed cells and changes with what's pending, then previews them, or records them unless `previewing`. */
  private async take(given: readonly Typed[], previewing: boolean, added: readonly Change[] = []): Promise<void> {
    if (this.holding()) return;
    // Each duration keeps the unit shown as it was typed.
    const typed = this.page.rows ? withUnits(this.page.rows, given) : given;
    if (this.current.busy) {
      // Shown now, and sent when the request out now is done.
      this.queued.push(...typed);
      this.queuedChanges = latest([...this.queuedChanges, ...added]);
      this.set({
        values: new Map([...this.current.values, ...valuesOf(typed)]),
        ...(added.length > 0 ? { queued: latest([...this.current.queued, ...added]) } : {}),
      });
      return;
    }
    if (!this.page.writable) {
      this.say("This schedule is read-only here.", "bad", "serve a branch of a history store to edit it, such as plant.tfdb@main");
      return;
    }
    const rows = this.page.rows;
    const head = this.page.head;
    if (!rows || !head) return;
    // Later values for a cell replace earlier ones.
    const merged = new Map<CellKey, Typed>();
    for (const cell of [...(this.current.pending?.typed ?? []), ...typed]) merged.set(cellKey(cell.id, cell.column), cell);
    const all = [...merged.values()];
    const changes = latest([...(this.current.pending?.changes ?? []), ...added]);
    const built = buildBatch(rows, all);
    if ("refused" in built) {
      const [first] = built.refused.values();
      // Kept, so the cells still show them and fixing one sends the rest too.
      this.set({
        pending: { typed: built.kept, changes, batch: { edits: [], cells: [] }, head, preview: null },
        values: valuesOf(built.kept),
        marks: built.refused,
        notice: {
          tone: "bad",
          text: `${built.refused.size === 1 ? "A cell" : `${built.refused.size} cells`} can't be sent: ${first?.message ?? ""}`,
          ...(first?.hint ? { hint: first.hint } : {}),
        },
      });
      return;
    }
    // A request names one project at most: the typed cells' and the changes' must agree.
    const projects = new Set([built.batch.project, ...changes.map((c) => c.project)].filter((p) => p !== undefined));
    // Typed cells across projects name none, so a change's project can't hold them.
    const typedAcross = built.batch.project === undefined && built.kept.length > 0;
    if (projects.size > 1 || (typedAcross && projects.size > 0)) {
      this.set({
        pending: { typed: built.kept, changes, batch: { edits: [], cells: [] }, head, preview: null },
        values: valuesOf(built.kept),
        marks: NONE,
        notice: {
          tone: "bad",
          text: "These edits are in more than one project, and one request takes one.",
          hint: "drop the others in the Changes panel, apply, then make them",
        },
      });
      return;
    }
    const [project] = projects;
    // The changes go after the typed cells' edits; a refusal marks each by its key.
    const batch: Batch = {
      ...built.batch,
      ...(project !== undefined ? { project } : {}),
      edits: [...built.batch.edits, ...changes.map((c) => c.edit)],
      cells: [...built.batch.cells, ...changes.map((c) => [c.key as CellKey])],
    };
    if (batch.edits.length === 0) {
      this.set({ pending: null, values: NONE, marks: NONE, notice: { tone: "info", text: "Nothing changed." } });
      return;
    }
    const pending: Pending = { typed: built.kept, changes, batch, head, preview: null };
    this.set({ pending, values: valuesOf(built.kept), marks: NONE, banner: null });
    if (previewing) await this.preview(pending);
    else await this.record(pending);
  }

  /** Records what's pending: the second Enter, or Apply. Without a preview yet, previews first. */
  async apply(): Promise<void> {
    const pending = this.current.pending;
    if (this.current.busy || !pending || this.holding()) return;
    if (!pending.preview) {
      // Not previewed, or refused, or typed before the branch moved: check it
      // again against the schedule as it is now.
      await this.commit([]);
      return;
    }
    await this.record(pending);
  }

  /**
   * Drops typed cells or changes, by their keys: previews the rest again, even
   * with previews off, or clears what's pending if they were the last.
   */
  async drop(...asked: (CellKey | string)[]): Promise<void> {
    // With the changes that name a node a dropped one adds or renames.
    const keys = withDependents(asked, latest([...(this.current.pending?.changes ?? []), ...this.current.queued]));
    if (this.current.busy) {
      // Taken back while a request is out: a queued change goes at once,
      // and one the request holds once it's done.
      const gone = new Set<string>(keys);
      this.queuedChanges = this.queuedChanges.filter((change) => !gone.has(change.key));
      for (const key of gone) if ((this.current.pending?.changes ?? []).some((change) => change.key === key)) this.dropsQueued.add(key);
      this.set({ queued: this.current.queued.filter((change) => !gone.has(change.key)), dropping: new Set(this.dropsQueued) });
      return;
    }
    const pending = this.current.pending;
    if (!pending || this.holding()) return;
    const gone = new Set<string>(keys);
    const rest = pending.typed.filter((cell) => !gone.has(cellKey(cell.id, cell.column)));
    const changes = (pending.changes ?? []).filter((change) => !gone.has(change.key));
    if (rest.length === pending.typed.length && changes.length === (pending.changes?.length ?? 0)) return;
    if (rest.length === 0 && changes.length === 0) {
      this.set({ pending: null, values: NONE, marks: NONE, banner: null, notice: { tone: "info", text: "Dropped the last edit." } });
      return;
    }
    // Taken off the cell and the list at once, even if the rest can't be sent yet.
    const marks = new Map(this.current.marks);
    for (const key of keys) marks.delete(key as CellKey);
    this.set({ pending: { ...pending, typed: rest, changes, preview: null }, values: valuesOf(rest), marks });
    await this.take([], true);
  }

  /** Drops what's typed and not recorded. */
  discard(): void {
    if (this.current.busy) return;
    const had = this.current.pending !== null || this.current.marks.size > 0;
    this.set({ pending: null, values: NONE, marks: NONE, banner: null, notice: had ? { tone: "info", text: "Discarded." } : null });
  }

  private async preview(pending: Pending): Promise<void> {
    this.set({ busy: true, notice: { tone: "info", text: "Previewing…" } });
    try {
      // With the quality checks, so the preview can say which newly fail.
      const { answer } = await this.api.edit({ ...this.request(pending), dry_run: true, check: true });
      this.idle({
        pending: { ...pending, preview: answer },
        notice: { tone: "preview", text: `Preview: ${impactText(answer)}.`, hint: "Enter or Apply records it; Esc discards it." },
      });
    } catch (error) {
      await this.refused(asApiError(error), pending);
    }
  }

  private async record(pending: Pending): Promise<void> {
    this.set({ busy: true, notice: { tone: "info", text: "Recording…" } });
    try {
      const { answer, head } = await this.api.edit(this.request(pending));
      const recorded = answer.recorded;
      const what = answer.applied.join("; ") || "edits";
      const made = recorded ? [...this.current.made, { id: recorded.id, short: recorded.short_id, what }] : this.current.made;
      // Keep the typed values showing until the fresh rows are.
      await this.reload(head);
      this.idle({
        pending: null,
        values: NONE,
        marks: NONE,
        message: "",
        made,
        notice: {
          tone: "done",
          text: `${recorded ? `Recorded ${recorded.short_id}` : "Recorded"}: ${impactText(answer)}.`,
        },
      });
    } catch (error) {
      await this.refused(asApiError(error), pending);
    }
  }

  private request(pending: Pending) {
    const message = this.current.message.trim();
    return {
      edits: pending.batch.edits,
      head: pending.head,
      ...(message ? { message } : {}),
      ...(pending.batch.project !== undefined ? { project: pending.batch.project } : {}),
    };
  }

  /** Shows a refusal: on its cells where it names edits, else in the status bar. */
  private async refused(error: ApiError, pending: Pending): Promise<void> {
    if (error.code === "branch-moved") {
      await this.moved(pending);
      return;
    }
    const { marks, loose } = marksOf(pending.batch, error.problems);
    const first = loose[0] ?? error.problems[0];
    // Changes are marked by key: say so of them, so a refused link isn't called a cell.
    const changed = new Set((pending.changes ?? []).map((c) => c.key));
    const cells = [...marks.keys()].filter((key) => !changed.has(key)).length;
    const what =
      cells > 0
        ? cells === 1
          ? "A cell was"
          : `${cells} cells were`
        : marks.size === 1
          ? "A change was"
          : `${marks.size} changes were`;
    const text = marks.size > 0 ? `${what} refused: ${first?.message ?? error.message}` : error.message;
    this.idle({
      pending: { ...pending, preview: null },
      marks,
      notice: { tone: "bad", text, hint: first?.hint ?? error.hint },
    });
  }

  /** Someone else moved the branch: show what's there now, and keep what was typed. */
  private async moved(pending: Pending): Promise<void> {
    const count = pending.typed.length;
    this.set({
      pending: { ...pending, preview: null },
      banner: `The branch moved on since this page loaded it, so nothing was recorded. The grid now shows it as it is; your ${count === 1 ? "typed value is" : `${count} typed values are`} kept.`,
      notice: { tone: "bad", text: "Someone else changed the schedule first.", hint: "Apply again to preview your values on top of their change." },
    });
    try {
      await this.reload();
    } finally {
      this.idle();
    }
  }

  /** Takes out the last changeset this page recorded. */
  async undo(): Promise<void> {
    if (this.current.busy || this.holding()) return;
    const last = this.current.made.at(-1);
    if (!last) {
      this.say("Nothing this page recorded is left to undo.");
      return;
    }
    const head = this.page.head;
    this.set({ busy: true, notice: { tone: "info", text: `Taking out ${last.short}…` } });
    try {
      const reverted = await this.api.revert({ at: last.id, ...(head ? { head } : {}), reason: `Undo ${last.short} from the app` });
      await this.reload(reverted.head);
      // A preview open before the undo was made at the head the undo moved past.
      const pending = this.current.pending;
      this.idle({
        made: this.current.made.slice(0, -1),
        ...(pending ? { pending: { ...pending, preview: null } } : {}),
        notice: {
          tone: "done",
          text: `Took out ${last.short} (${last.what})${reverted.answer.recorded ? ` as ${reverted.answer.recorded.short_id}` : ""}.`,
          ...(pending ? { hint: "Your typed values are kept; Enter or Apply previews them again." } : {}),
        },
      });
    } catch (error) {
      const refusal = asApiError(error);
      if (refusal.code === "branch-moved") {
        this.set({
          banner: "The branch moved on since this page loaded it, so nothing was undone. The grid now shows it as it is.",
          notice: { tone: "bad", text: `Couldn't take out ${last.short}.`, hint: "Undo again to take it out of the branch as it is now." },
        });
        try {
          await this.reload();
        } finally {
          this.idle();
        }
        return;
      }
      this.idle({ notice: { tone: "bad", text: refusal.message, hint: refusal.hint } });
    }
  }

  dismissBanner(): void {
    this.set({ banner: null });
  }
}
