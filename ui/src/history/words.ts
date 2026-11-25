// The history panel's words: who made a change and when, what kind it is,
// and what a comparison of two points found, in a scheduler's terms.

import type { Baselines, Branches, ChangesetDocument, HistoryDiff, HistoryLog } from "../api";
import type { Notice } from "../edit/editing";
import { readable } from "../explain/narrate";
import { moveText } from "../dates";

export type Entry = HistoryLog["changesets"][number];
export type Comparison = HistoryDiff["diff"]["projects"][number];
export type Branch = Branches["branches"][number];
export type Baseline = Baselines["baselines"][number];

/**
 * When a changeset was made, in this computer's time zone, as
 * "Mon 2 Nov 2026, 09:00". It's when someone acted, not a schedule date, so
 * it isn't civil time; `zone` is for tests.
 */
export function madeAt(created: string, zone?: string): string {
  const at = new Date(created);
  if (Number.isNaN(at.getTime())) return created;
  const parts = new Intl.DateTimeFormat("en-GB", {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
    ...(zone ? { timeZone: zone } : {}),
  }).formatToParts(at);
  const part = (type: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === type)?.value ?? "";
  return `${part("weekday")} ${part("day")} ${part("month")} ${part("year")}, ${part("hour")}:${part("minute")}`;
}

/** The marks an entry carries: what kind of change, and whether it's a proposal. */
export function tagsOf(entry: Entry): string[] {
  const tags: string[] = [];
  if (entry.kind !== "edit") tags.push(entry.kind);
  if (entry.state === "proposed") tags.push("proposal");
  if (entry.state === "rejected") tags.push("rejected");
  return tags;
}

/** A field's name as words: `original_duration` is "Original duration". */
export function fieldWords(field: string): string {
  const words = field.replace(/_/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Elapsed hours as words: "3 hours", "2 calendar days". */
function elapsed(hours: number): string {
  const size = Math.abs(hours);
  if (size < 24) {
    const h = Math.round(size * 100) / 100;
    return `${h} ${h === 1 ? "hour" : "hours"}`;
  }
  const d = Math.round((size / 24) * 10) / 10;
  return `${d} calendar ${d === 1 ? "day" : "days"}`;
}

/** How a project's finish moved between two points, in a sentence. */
export function finishWords(project: Comparison["project"]): string {
  const { from_finish: from, to_finish: to, finish_moved_hours: moved } = project ?? {};
  if (!from && !to) return "Finish not known at either point.";
  if (!from) return `Finish ${readable(to)}; not known before.`;
  if (!to) return `Finish not known now; it was ${readable(from)}.`;
  if (moved === null || moved === undefined || moved === 0) return `Finish unchanged, ${readable(to)}.`;
  const [was, now] = moveText(from, to);
  return `Finish ${elapsed(moved)} ${moved > 0 ? "later" : "earlier"}: ${was} to ${now}.`;
}

/** What a comparison counted, as a short list: "1 changed, 2 added, 1 link added". */
export function countWords(counts: Comparison["counts"] = {}): string {
  const parts: string[] = [];
  const say = (n: number | undefined, one: string, many = one) => {
    if (n) parts.push(`${n} ${n === 1 ? one : many}`);
  };
  say(counts.changed, "activity changed", "activities changed");
  say(counts.added, "added");
  say(counts.deleted, "deleted");
  say(counts.links_added, "link added", "links added");
  say(counts.links_deleted, "link deleted", "links deleted");
  say(counts.links_changed, "link changed", "links changed");
  say(counts.became_critical, "became critical");
  say(counts.left_critical, "left the critical path");
  return parts.length > 0 ? parts.join(", ") : "No activity or link changes.";
}

/** A point to compare: a branch, a baseline or a changeset, by the name the store knows it by. */
export interface Point {
  /** What the store resolves: a branch or baseline name, or a changeset ID. */
  ref: string;
  label: string;
}

/** Every point there is to compare: branches, then baselines, then the log's entries. */
export function pointsOf(branches: readonly Branch[], baselines: readonly Baseline[], entries: readonly Entry[]): Point[] {
  return [
    ...branches.map((b) => ({ ref: b.name, label: `Branch ${b.name}` })),
    ...baselines.map((b) => ({ ref: b.name, label: `Baseline ${b.name}` })),
    ...entries.map((e) => ({ ref: e.id, label: `${e.short_id} · ${e.message || e.summary}` })),
  ];
}

/** How a panel tells the person something, in the status bar. */
export type Say = (text: string, tone?: Notice["tone"], hint?: string) => void;

/** What a changeset did, as its report says it: the edits applied and the finish it gave. */
export function didWords(shown: ChangesetDocument): { applied: string[]; finishes: string[] } {
  const report = shown.report as { applied?: unknown; projects?: unknown } | null | undefined;
  const applied = Array.isArray(report?.applied) ? report.applied.filter((line): line is string => typeof line === "string") : [];
  const finishes: string[] = [];
  if (Array.isArray(report?.projects)) {
    for (const project of report.projects as { code?: string; previous_finish?: string | null; scheduled_finish?: string | null }[]) {
      const { previous_finish: before, scheduled_finish: after } = project;
      if (!after) continue;
      const which = report.projects.length > 1 && project.code ? `${project.code}: ` : "";
      finishes.push(
        before && before !== after
          ? `${which}finish moved from ${moveText(before, after).join(" to ")}`
          : `${which}finish ${readable(after)}${before ? ", unchanged" : ""}`,
      );
    }
  }
  return { applied, finishes };
}
