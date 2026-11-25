// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Api } from "../api";
import { LayoutDialog, type LayoutMode } from "./LayoutDialog";

afterEach(cleanup);

async function settle() {
  await act(async () => {
    for (let i = 0; i < 10; i++) await Promise.resolve();
  });
}

async function dialog(mode: LayoutMode, current: string | null = null) {
  const asked: { method: string; url: string }[] = [];
  const fetcher: typeof fetch = (input, init) => {
    asked.push({ method: init?.method ?? "GET", url: String(input) });
    const body = String(input) === "/api/1/layouts" ? { schema: "totalfloat.layouts/1", layouts: [{ name: "Look ahead" }, { name: "Print" }] } : {};
    return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
  };
  const onSave = vi.fn<(name: string) => Promise<void>>(() => Promise.resolve());
  const onOpen = vi.fn<(name: string) => Promise<void>>(() => Promise.resolve());
  const onDeleted = vi.fn();
  const onClose = vi.fn();
  render(
    <LayoutDialog api={new Api(null, fetcher)} mode={mode} current={current} onSave={onSave} onOpen={onOpen} onDeleted={onDeleted} onClose={onClose} />,
  );
  await settle();
  return { asked, onSave, onOpen, onDeleted, onClose };
}

describe("the layouts dialog", () => {
  it("saves under a name, offering the saved ones, and says when it replaces one", async () => {
    const { onSave } = await dialog("save", "Look ahead");
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("Look ahead");
    expect(screen.getByText("This replaces the layout of that name.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Print" }));
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Mine" } });
    expect(screen.queryByText("This replaces the layout of that name.")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await settle();
    expect(onSave).toHaveBeenCalledWith("Mine");
  });

  it("opens one", async () => {
    const { onOpen } = await dialog("open", "Print");
    expect(screen.getByRole("button", { name: /Print/ }).textContent).toContain("(in use)");
    fireEvent.click(screen.getByRole("button", { name: /Look ahead/ }));
    await settle();
    expect(onOpen).toHaveBeenCalledWith("Look ahead");
  });

  it("deletes one only when asked twice", async () => {
    const { asked, onDeleted } = await dialog("delete");
    fireEvent.click(screen.getByRole("button", { name: "Delete layout Print" }));
    expect(asked.some((a) => a.method === "DELETE")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await settle();
    expect(asked.find((a) => a.method === "DELETE")?.url).toBe("/api/1/layouts/Print");
    expect(onDeleted).toHaveBeenCalledWith("Print");
    expect(screen.queryByRole("button", { name: "Delete layout Print" })).toBeNull();
  });

  it("says why a save was refused, and stays open", async () => {
    const onSave = vi.fn(() => Promise.reject(new Error("no")));
    render(
      <LayoutDialog
        api={new Api(null, () => Promise.resolve(new Response(JSON.stringify({ layouts: [] }))))}
        mode="save"
        current={null}
        onSave={onSave}
        onOpen={() => Promise.resolve()}
        onDeleted={() => {}}
        onClose={() => {}}
      />,
    );
    await settle();
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "x" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await settle();
    expect(screen.getByRole("alert").textContent).toBe("The layout couldn't be saved.");
  });

  it("warns of replacing a layout whose name differs only in case", async () => {
    await dialog("save");
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "print" } });
    expect(screen.getByText("This replaces the layout of that name.")).toBeTruthy();
  });

  it("keeps focus inside while it's open", async () => {
    const outside = document.createElement("button");
    document.body.append(outside);
    await dialog("open");
    act(() => outside.focus());
    expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);
    outside.remove();
  });

  it("gives focus back to what opened it", async () => {
    const launcher = document.createElement("button");
    document.body.append(launcher);
    launcher.focus();
    const fetcher: typeof fetch = () => Promise.resolve(new Response(JSON.stringify({ layouts: [] })));
    const { unmount } = render(
      <LayoutDialog
        api={new Api(null, fetcher)}
        mode="open"
        current={null}
        onSave={() => Promise.resolve()}
        onOpen={() => Promise.resolve()}
        onDeleted={() => {}}
        onClose={() => {}}
      />,
    );
    await settle();
    expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);
    unmount();
    expect(document.activeElement).toBe(launcher);
    launcher.remove();
  });
});
