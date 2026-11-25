// What a refused merge's clashes are, and which changes on each side set
// them. The server says each clash in words; the values come from the
// comparison of the two heads where it has them, and the changes from each
// side's history since the branches parted, merges included.

import type { ChangesetDocument, HistoryDiff, Problem } from "../api";

/** A clash a refused merge names, read from its problem. */
export interface Clash {
  /** The activity, when the problem names one, and its project's `proj_id`. */
  code: string | null;
  project: number | null;
  /** The field both sides set, when it's a field. */
  field: string | null;
  /** This branch's value, and the other's, when they're known. */
  ours: string | null;
  theirs: string | null;
  /** The problem's own words. */
  message: string;
}

const FIELD = /^(\S+) ([a-z_]+): this branch has /;
const SEPARATOR = ", and the other has ";
const NAMED = /^(\S+) /;

/**
 * A clash from a refused merge's problem: the activity, field and both
 * values. The server's words can't always be split back into the two values
 * (a value may itself hold ", and the other has "), so they're taken from
 * `compared`, the comparison of the two heads, where it lists the field, and
 * from the words only where they split one way.
 */
export function clashOf(
  problem: Problem,
  compared?: HistoryDiff | null,
  codeOf: (project: number) => string | undefined = () => undefined,
): Clash {
  const id = /^project (\d+)$/.exec(problem.source)?.[1];
  const project = id === undefined ? null : Number(id);
  const field = FIELD.exec(problem.message);
  if (field) {
    const [whole, code = "", name = ""] = field;
    const fromDiff = valuesIn(compared, project === null ? undefined : codeOf(project), code, name);
    const rest = problem.message.slice(whole.length);
    const once = rest.indexOf(SEPARATOR) >= 0 && rest.indexOf(SEPARATOR) === rest.lastIndexOf(SEPARATOR);
    const [ours, theirs] = fromDiff ?? (once ? [rest.slice(0, rest.indexOf(SEPARATOR)), rest.slice(rest.indexOf(SEPARATOR) + SEPARATOR.length)] : [null, null]);
    return { code, project, field: name, ours, theirs, message: problem.message };
  }
  // "A3 deleted: …", "link A1 FS A2: …" and the like: the first word, when it's an activity.
  const named = NAMED.exec(problem.message)?.[1] ?? null;
  return { code: named && named !== "link" ? named : null, project, field: null, ours: null, theirs: null, message: problem.message };
}

/**
 * A field's two values in a comparison from this branch to the other, if it
 * lists them, in the project of that code; with several projects and none
 * named, none is guessed.
 */
function valuesIn(
  compared: HistoryDiff | null | undefined,
  projectCode: string | undefined,
  code: string,
  field: string,
): [string, string] | null {
  const projects = compared?.diff.projects ?? [];
  const where =
    projectCode !== undefined ? projects.filter((p) => p.project?.code === projectCode) : projects.length === 1 ? projects : [];
  for (const project of where) {
    const change = (project.changed ?? []).find((c) => c.code === code)?.changes.find((c) => c.field === field);
    if (change) return [change.from, change.to];
  }
  return null;
}

/** What `history show` gives that the walk needs. */
export type Shown = Pick<ChangesetDocument, "id" | "short_id" | "parent" | "second_parent" | "message" | "summary" | "commands">;

/**
 * The changes on each side since the branches parted, merges included: each
 * side's ancestors that the other doesn't have. `known` are changes already
 * known to be the other side's (its first-parent log), where a walk stops.
 * At most `bound` changes are read on each side.
 */
export async function sinceParting(
  show: (id: string) => Promise<Shown>,
  ours: { head: string; known: ReadonlySet<string> },
  theirs: { head: string; known: ReadonlySet<string> },
  bound = 400,
): Promise<{ ours: Shown[]; theirs: Shown[] }> {
  // Ours: from this head, not into what the other branch's line has.
  const mine = await walk(show, ours.head, theirs.known, bound);
  // Theirs: from the other head, not into anything this side has, including
  // the changes of theirs this side reached (through a merge), which are shared.
  const stop = new Set([...ours.known, ...mine.seen.keys(), ...mine.stopped]);
  const other = await walk(show, theirs.head, stop, bound);
  // Changes of ours the other side reached are shared, and so is all behind them.
  const shared = new Set<string>();
  const queue = [...other.stopped].filter((id) => mine.seen.has(id));
  while (queue.length > 0) {
    const id = queue.pop() ?? "";
    if (shared.has(id)) continue;
    shared.add(id);
    const doc = mine.seen.get(id);
    for (const parent of [doc?.parent, doc?.second_parent]) if (parent && mine.seen.has(parent)) queue.push(parent);
  }
  return {
    ours: [...mine.seen.values()].filter((doc) => !shared.has(doc.id)),
    theirs: [...other.seen.values()],
  };
}

/** Reads changes back from `head` through every parent, stopping at `stop`. */
async function walk(
  show: (id: string) => Promise<Shown>,
  head: string,
  stop: ReadonlySet<string>,
  bound: number,
): Promise<{ seen: Map<string, Shown>; stopped: Set<string> }> {
  const seen = new Map<string, Shown>();
  const stopped = new Set<string>();
  const queue = [head];
  while (queue.length > 0 && seen.size < bound) {
    const id = queue.shift() ?? "";
    if (seen.has(id)) continue;
    if (stop.has(id)) {
      stopped.add(id);
      continue;
    }
    const doc = await show(id);
    seen.set(id, doc);
    for (const parent of [doc.parent, doc.second_parent]) if (parent) queue.push(parent);
  }
  return { seen, stopped };
}

/** The fields each kind of edit can set, as comparisons name them; none listed means any. */
function fieldsOf(edit: Record<string, unknown>): string[] | null {
  switch (edit.op) {
    case "set_duration":
      // Original only when given; remaining when given, or when it follows a
      // new original on work not started. Original never follows remaining.
      return [...(edit.original != null ? ["original_duration"] : []), "remaining_duration"];
    case "set_name":
      return ["name"];
    case "set_calendar":
      return ["calendar"];
    case "set_constraint":
    case "clear_constraint":
      return ["primary_constraint", "secondary_constraint"];
    case "set_wbs":
      return ["wbs"];
    case "set_type":
      return ["type"];
    case "set_code":
    case "clear_code":
      return ["codes"];
    case "set_progress":
    case "clear_progress":
      return ["actual_start", "actual_finish", "physical_percent_complete", "status", "remaining_duration"];
    default:
      return null;
  }
}

/** The changes among `docs` whose commands set the clash's field on its activity, in its project. */
export function setting(docs: readonly Shown[], clash: Clash): Shown[] {
  if (!clash.code) return [];
  return docs.filter((doc) => {
    const commands = doc.commands as {
      edits?: Record<string, unknown>[];
      resolved?: { kind?: string; name?: string; project?: number }[];
      options?: { project?: number };
    };
    if (clash.project !== null) {
      // An edit request made for one project touched only it; otherwise the
      // projects its names were found in when it was recorded.
      const scope = commands.options?.project;
      if (scope !== undefined && scope !== clash.project) return false;
      const found = (commands.resolved ?? []).filter((r) => r.kind === "activity" && r.name === clash.code).map((r) => r.project);
      if (scope === undefined && found.length > 0 && !found.includes(clash.project)) return false;
    }
    return (commands.edits ?? []).some((edit) => {
      if (edit.activity !== clash.code) return false;
      const fields = fieldsOf(edit);
      return clash.field === null || fields === null || fields.includes(clash.field);
    });
  });
}
