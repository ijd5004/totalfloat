// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DeleteDialog, deleteWords } from "./DeleteDialog";

afterEach(cleanup);

describe("the delete confirmation", () => {
  it("counts the activities, links and assignments that go", () => {
    expect(deleteWords({ codes: ["A1", "A2"], links: 3, assignments: 1 })).toBe("Delete 2 activities, with 3 links and 1 assignment?");
    expect(deleteWords({ codes: ["A1"], links: 1, assignments: 0 })).toBe("Delete 1 activity, with 1 link and 0 assignments?");
  });

  it("names them, starts on Cancel, and deletes only on Delete", () => {
    const onDelete = vi.fn();
    const onClose = vi.fn();
    const codes = Array.from({ length: 10 }, (_, k) => `A${k + 1}`);
    render(<DeleteDialog deleting={{ codes, links: 4, assignments: 2 }} onDelete={onDelete} onClose={onClose} />);
    const dialog = screen.getByRole("alertdialog", { name: "Delete 10 activities, with 4 links and 2 assignments?" });
    expect(dialog.textContent).toContain("A1, A2, A3, A4, A5, A6, A7, A8 and 2 more.");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onDelete).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onDelete).toHaveBeenCalledTimes(1);
  });
});
