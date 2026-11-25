// Asks before activities are deleted, naming what goes with them: their links
// and assignments, as a dry run of the delete counted them. Cancel has focus,
// so Enter alone doesn't delete.

import { useEffect, useRef } from "react";

/** What a delete takes away, as its dry run counted it. */
export interface Deleting {
  codes: string[];
  links: number;
  assignments: number;
  /** The project they're in (`proj_id`), when the file has more than one. */
  project?: number;
}

const many = (n: number, one: string, more: string) => `${n.toLocaleString()} ${n === 1 ? one : more}`;

/** The question, in words: "Delete 2 activities, with 3 links and 1 assignment?" */
export function deleteWords({ codes, links, assignments }: Deleting): string {
  return `Delete ${many(codes.length, "activity", "activities")}, with ${many(links, "link", "links")} and ${many(assignments, "assignment", "assignments")}?`;
}

export function DeleteDialog({ deleting, onDelete, onClose }: { deleting: Deleting; onDelete: () => void; onClose: () => void }) {
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => cancel.current?.focus(), []);
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div
        className="delete-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="delete-title"
        aria-describedby="delete-what"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onClose();
          }
        }}
      >
        <h2 id="delete-title">{deleteWords(deleting)}</h2>
        <p id="delete-what" className="muted">
          {deleting.codes.slice(0, 8).join(", ")}
          {deleting.codes.length > 8 ? ` and ${deleting.codes.length - 8} more` : ""}. The delete waits with the other edits until
          you apply it, and Undo puts them back once it&apos;s recorded.
        </p>
        <div className="delete-actions">
          <button type="button" ref={cancel} onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="danger" onClick={onDelete}>
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}
