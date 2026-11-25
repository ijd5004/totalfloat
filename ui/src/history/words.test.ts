import { describe, expect, it } from "vitest";

import log from "../api/golden/history-log-1.json";
import { countWords, fieldWords, finishWords, madeAt, pointsOf, tagsOf } from "./words";

type Entry = Parameters<typeof tagsOf>[0];
const [edit, imported] = log.changesets as [Entry, Entry];

describe("the history panel's words", () => {
  it("says when a change was made, in the zone asked for", () => {
    expect(madeAt("2026-11-02T09:00:00Z", "UTC")).toBe("Mon 2 Nov 2026, 09:00");
    expect(madeAt("2026-11-02T09:00:00Z", "America/Los_Angeles")).toBe("Mon 2 Nov 2026, 01:00");
    expect(madeAt("not a time", "UTC")).toBe("not a time");
  });

  it("marks imports, reverts, merges and proposals, not plain edits", () => {
    expect(tagsOf(edit)).toEqual([]);
    expect(tagsOf(imported)).toEqual(["import"]);
    expect(tagsOf({ ...edit, state: "proposed" })).toEqual(["proposal"]);
    expect(tagsOf({ ...edit, kind: "revert", state: "rejected" })).toEqual(["revert", "rejected"]);
  });

  it("names fields in words", () => {
    expect(fieldWords("original_duration")).toBe("Original duration");
  });

  it("says how the finish moved", () => {
    const base = { code: "P", from_finish: "2026-01-15 17:00", to_finish: "2026-01-16 17:00" };
    expect(finishWords({ ...base, finish_moved_hours: 24 })).toBe(
      "Finish 1 calendar day later: 15 Jan 26 to 16 Jan 26.",
    );
    expect(finishWords({ ...base, to_finish: "2026-01-15 09:00", finish_moved_hours: -8 })).toBe(
      "Finish 8 hours earlier: 15 Jan 26 17:00 to 15 Jan 26 09:00.",
    );
    expect(finishWords({ ...base, to_finish: base.from_finish, finish_moved_hours: 0 })).toBe("Finish unchanged, 15 Jan 26.");
    expect(finishWords({ code: "P", from_finish: null, to_finish: null })).toBe("Finish not known at either point.");
    expect(finishWords(undefined)).toBe("Finish not known at either point.");
  });

  it("lists only what a comparison counted", () => {
    expect(countWords({ changed: 1, added: 2, deleted: 0, links_added: 1, became_critical: 0 })).toBe(
      "1 activity changed, 2 added, 1 link added",
    );
    expect(countWords({ changed: 0 })).toBe("No activity or link changes.");
    expect(countWords(undefined)).toBe("No activity or link changes.");
  });

  it("offers branches, baselines and changes to compare, by the name the store knows", () => {
    const points = pointsOf(
      [{ name: "main", head: "h", short_id: "h", current: true, author: "Pat", created: "", last_change: "" }],
      [{ name: "submitted", at: "a", short_id: "a", author: "Pat", created: "", point: "" }],
      [edit],
    );
    expect(points).toEqual([
      { ref: "main", label: "Branch main" },
      { ref: "submitted", label: "Baseline submitted" },
      { ref: edit.id, label: "0101010101 · Crane late" },
    ]);
  });
});
