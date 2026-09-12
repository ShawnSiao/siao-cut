import { fireEvent, render, screen, waitFor, cleanup } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { CloseRecoveryDialog } from "./CloseRecoveryDialog";
import type { EditingSession } from "./editing-session";

vi.mock("../../i18n", () => ({ getUiLocale: () => "zh-CN" }));
afterEach(cleanup);
function setup(unsaved = true, journaled = false) {
  const session = { entries: () => unsaved ? [["field", { status: "failed", journaled }]] : [], flush: vi.fn(async () => {}) };
  const cancel = vi.fn(), close = vi.fn(async () => {});
  render(<CloseRecoveryDialog session={session as unknown as EditingSession} error="Command plugin:window|destroy not allowed by ACL" onCancel={cancel} onCloseWithDrafts={close}/>);
  return { session, cancel, close };
}
it("explains unsaved drafts, folds technical errors, and focuses keep editing", async () => {
  const { cancel } = setup();
  expect(screen.getByRole("dialog", { name: "还有修改未保存" })).toBeTruthy();
  expect(screen.getByText(/部分草稿尚未确认落盘/)).toBeTruthy();
  expect(document.querySelector("details")?.open).toBe(false);
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("button", { name: "继续编辑" })));
  fireEvent.keyDown(window, { key: "Escape" }); expect(cancel).toHaveBeenCalledOnce();
});
it("does not call a saved project a save failure or offer unnecessary draft exit", () => {
  setup(false);
  expect(screen.getByRole("dialog", { name: "暂时无法退出" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "保存草稿并退出" })).toBeNull();
});
it("keeps the dialog open after failed save and prevents duplicate actions", async () => {
  const { session, close } = setup();
  let reject!: (reason: Error) => void;
  session.flush.mockImplementation(() => new Promise<void>((_, fail) => { reject = fail; }));
  const retry = screen.getByRole("button", { name: "重试保存并退出" });
  fireEvent.click(retry); fireEvent.click(retry);
  expect(session.flush).toHaveBeenCalledOnce();
  reject(new Error("disk full"));
  await screen.findByRole("alert"); expect(close).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog")).toBeTruthy();
});
it("limits native destroy permission to the main window", () => {
  const capability = JSON.parse(readFileSync("src-tauri/capabilities/default.json", "utf8"));
  expect(capability.windows).toEqual(["main"]);
  expect(capability.permissions).toContain("core:window:allow-destroy");
});
