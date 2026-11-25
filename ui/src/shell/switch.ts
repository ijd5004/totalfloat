// Switching the store's checked-out branch from the page.

import { type Api, ApiError, type Summary } from "../api";
import type { Editing } from "../edit/editing";
import type { Say } from "../history/words";

/**
 * Switches the store to branch `name`, then shows it if this page follows
 * the checked-out branch. An edit being recorded must finish first, or it
 * could record on the branch switched to; typed values not yet sent are
 * dropped, as they were typed on the other branch.
 */
export async function switchBranch(
  api: Api,
  editing: Editing,
  name: string,
  reload: () => Promise<{ summary: Summary | null } | null>,
  say: Say,
): Promise<void> {
  if (editing.state().busy) {
    throw new ApiError(0, "edit-in-progress", "An edit is being recorded.", "switch once it's done");
  }
  if (editing.state().pending || editing.state().marks.size > 0) editing.discard();
  // No edit starts until the branch switched to is shown.
  const release = editing.hold();
  let now: Summary | null | undefined;
  try {
    await api.branch({ action: "switch", name });
    now = (await reload())?.summary;
  } finally {
    release();
  }
  if (now && now.branch !== name) {
    say(
      `The store is on ${name} now, but this page serves ${now.location}, so it still shows ${now.branch ?? "that point"}.`,
      "info",
      "serve the store without @branch to follow switches",
    );
  } else {
    say(`Switched to ${name}.`, "done");
  }
}
