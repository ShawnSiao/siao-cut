import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { sampleProject } from "../../mock";
import { useEditingSession } from "./use-editing-session";
import { fieldKey } from "./editing-session";

const native = vi.hoisted(() => ({ close: undefined as undefined | ((event: { preventDefault(): void }) => Promise<void>), destroy: vi.fn(async () => {}), unlisten: vi.fn() }));
const client = vi.hoisted(() => ({ journal: vi.fn(async () => {}), list: vi.fn(async () => []), discard: vi.fn(async () => {}), save: vi.fn() }));
vi.mock("../../domains/editing-client", () => ({ editingClient: client }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ destroy: native.destroy, onCloseRequested: async (callback: typeof native.close) => { native.close = callback; return native.unlisten; } }) }));
afterEach(() => { cleanup(); delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; vi.clearAllMocks(); });

it("prevents native close and offers recovery when committing fails", async () => {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  client.save.mockRejectedValue(new Error("disk unavailable"));
  const project = structuredClone(sampleProject);
  const { result } = renderHook(() => useEditingSession(project, async () => {}));
  await waitFor(() => expect(native.close).toBeTypeOf("function"));
  const key = fieldKey(project.id, project.transcript.segments[0].id, "source");
  act(() => result.current.session.change(key, "unsaved edit"));
  const preventDefault = vi.fn();
  await act(async () => { await native.close!({ preventDefault }); });
  expect(preventDefault).toHaveBeenCalled(); expect(native.destroy).not.toHaveBeenCalled();
  expect(result.current.closeError).toBe("disk unavailable");
  expect(result.current.session.state(key)?.draft.text).toBe("unsaved edit");
  await act(async () => { await result.current.closeWithDrafts(); });
  expect(client.journal).toHaveBeenCalled(); expect(native.destroy).toHaveBeenCalledOnce();
});

it("does not exit with drafts if journal persistence fails", async () => {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  client.journal.mockRejectedValueOnce(new Error("journal disk full"));
  const project = structuredClone(sampleProject);
  const { result } = renderHook(() => useEditingSession(project, async () => {}));
  act(() => result.current.session.change(fieldKey(project.id, project.transcript.segments[0].id, "source"), "draft"));
  await act(async () => { await result.current.closeWithDrafts(); });
  expect(native.destroy).not.toHaveBeenCalled(); expect(result.current.closeError).toContain("journal disk full");
});
