import { saveErrorMessage } from "./save-error-message";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EditReceipt, SaveEdit } from "../../generated/core-contract";
import { sampleProject } from "../../mock";
import { EditingSession, fieldKey, type EditingTransport } from "./editing-session";

const deferred = <T,>() => { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const receipt = (edit: SaveEdit): EditReceipt => ({ mutationId: edit.mutationId, projectId: edit.draft.projectId, segmentId: edit.draft.segmentId, field: edit.draft.field, text: edit.draft.text, versionId: `v-${edit.draft.revision}-${edit.draft.segmentId}`, history: null, version: null, changedDomains: ["transcript"] });
function setup(overrides: Partial<EditingTransport> = {}) {
  const project = structuredClone(sampleProject);
  const transport: EditingTransport = { journal: vi.fn(async () => {}), list: vi.fn(async () => []), discard: vi.fn(async () => {}), save: vi.fn(async (edit) => receipt(edit)), ...overrides };
  const session = new EditingSession(transport); session.observe(project);
  const key = fieldKey(project.id, project.transcript.segments[0].id, "source");
  return { session, transport, project, key };
}
afterEach(() => vi.useRealTimers());
describe("project editing queue", () => {
  it("journals at 200 ms and commits only after 800 ms idle", async () => {
    vi.useFakeTimers(); const { session, transport, key } = setup();
    await Promise.resolve(); session.change(key, "typed");
    await vi.advanceTimersByTimeAsync(199); expect(transport.journal).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1); expect(transport.journal).toHaveBeenCalledOnce(); expect(transport.save).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(600); expect(transport.save).toHaveBeenCalledOnce(); expect(session.state(key)?.status).toBe("saved");
  });
  it("serializes different rows using the acknowledged version and preserves typing in flight", async () => {
    const pending = deferred<EditReceipt>(), requests: SaveEdit[] = [];
    const { session, project, key } = setup({ save: async (edit) => { requests.push(edit); return requests.length === 1 ? pending.promise : receipt(edit); } });
    await Promise.resolve(); session.change(key, "first"); const first = session.save(key);
    await vi.waitFor(() => expect(requests).toHaveLength(1));
    session.change(key, "newer typing");
    const secondKey = fieldKey(project.id, project.transcript.segments[1].id, "source");
    session.change(secondKey, "second row"); const second = session.save(secondKey);
    expect(requests).toHaveLength(1); pending.resolve(receipt(requests[0]));
    await Promise.all([first, second]);
    expect(requests[1].expectedVersionId).toBe(receipt(requests[0]).versionId);
    expect(session.state(key)?.draft.text).toBe("newer typing"); expect(session.state(key)?.status).toBe("saved");
    session.disposeTimers();
  });
  it("retains the exact mutation ID across an ambiguous failure, then commits newer typing separately", async () => {
    const requests: SaveEdit[] = [];
    const { session, key } = setup({ save: async (edit) => { requests.push(edit); if (requests.length === 1) throw new Error("response lost"); return receipt(edit); } });
    await Promise.resolve(); session.change(key, "first"); await expect(session.save(key)).rejects.toThrow("response lost");
    session.change(key, "second"); await session.save(key);
    expect(requests[1]).toEqual(requests[0]); expect(requests[2].mutationId).not.toBe(requests[0].mutationId);
    expect(session.state(key)?.draft.text).toBe("second"); session.disposeTimers();
  });
  it("does not overwrite a draft on external refresh and refuses to flush a conflict", async () => {
    const { session, project, key } = setup(); await Promise.resolve(); session.change(key, "my draft");
    project.transcript.segments[0].text = "external change"; session.observe(project);
    expect(session.state(key)?.draft.text).toBe("my draft"); expect(session.state(key)?.status).toBe("conflict");
    await expect(session.flush()).rejects.toThrow(); expect(session.state(key)?.journaled).toBe(true); session.disposeTimers();
  });
  it("does not autosave during IME composition", async () => {
    vi.useFakeTimers(); const { session, transport, key } = setup(); await Promise.resolve();
    session.composition(key, true); session.change(key, "拼音组合"); await vi.advanceTimersByTimeAsync(1500);
    expect(transport.save).not.toHaveBeenCalled(); expect(transport.journal).toHaveBeenCalled();
    session.composition(key, false); await vi.advanceTimersByTimeAsync(800); expect(transport.save).toHaveBeenCalledOnce();
  });
  it("never replaces newly typed text with a late recovered draft", async () => {
    const loading = deferred<Awaited<ReturnType<EditingTransport["list"]>>>();
    const { session, key } = setup({ list: () => loading.promise });
    const old = { ...session.state(key)!.draft, sessionId: "previous-session", text: "recovered", revision: 3 };
    session.change(key, "fresh typing"); loading.resolve([old]); await Promise.resolve(); await Promise.resolve();
    expect(session.state(key)?.draft.text).toBe("fresh typing");
    expect(session.entries().some(([, state]) => state.draft.text === "recovered")).toBe(true); session.disposeTimers();
  });
});


it("reports a stored draft for a busy project save and reuses the mutation on retry", async () => {
  const busy = Object.assign(new Error("database is busy"), {code: "database_busy"});
  const save = vi.fn().mockRejectedValueOnce(busy).mockImplementation(async edit => receipt(edit));
  const {session,key}=setup({save}); await Promise.resolve();session.change(key,"retained");
  await expect(session.save(key)).rejects.toBe(busy);
  expect(session.state(key)?.journaled).toBe(true);
  expect(saveErrorMessage(session.state(key)!,true)).toContain("本地草稿已落盘");
  expect(saveErrorMessage(session.state(key)!,false)).toContain("local draft is stored");
  await session.save(key);
  expect(save.mock.calls[1][0]).toEqual(save.mock.calls[0][0]);
  expect(session.state(key)?.status).toBe("saved");session.disposeTimers();
});
it("does not claim persistence when the journal itself is busy", async () => {
  const busy = Object.assign(new Error("busy"), {code: "database_busy"});
  const journal=vi.fn().mockRejectedValueOnce(busy).mockResolvedValue(undefined);
  const {session,key,transport}=setup({journal});await Promise.resolve();session.change(key,"only in memory");
  await expect(session.save(key)).rejects.toBe(busy);
  expect(transport.save).not.toHaveBeenCalled();expect(session.state(key)?.journaled).toBe(false);
  expect(saveErrorMessage(session.state(key)!,true)).toContain("尚未确认落盘");
  expect(saveErrorMessage(session.state(key)!,false)).toContain("not confirmed on disk");
  await session.save(key);expect(session.state(key)?.status).toBe("saved");session.disposeTimers();
});
it("does not let an older failed journal mark a newer draft as failed", async () => {
  const first=deferred<void>();const journal=vi.fn().mockReturnValueOnce(first.promise).mockResolvedValue(undefined);
  const {session,key}=setup({journal});await Promise.resolve();session.change(key,"old");
  const old=session.persist(key);const rejected=expect(old).rejects.toThrow("busy");
  await vi.waitFor(()=>expect(journal).toHaveBeenCalledOnce());session.change(key,"new");
  first.reject(Object.assign(new Error("busy"),{code:"database_busy"}));await rejected;
  expect(session.state(key)?.status).toBe("dirty");expect(session.state(key)?.draft.text).toBe("new");
  await session.persist(key);expect(session.state(key)?.journaled).toBe(true);session.disposeTimers();
});

it("replaces a busy message with a later content conflict", async () => {
  const {session,key,project}=setup({save:async()=>{throw Object.assign(new Error("busy"),{code:"database_busy"});}});
  await Promise.resolve();session.change(key,"local");await expect(session.save(key)).rejects.toThrow("busy");
  project.transcript.segments[0].text="external";session.observe(project);
  expect(session.state(key)?.status).toBe("conflict");
  expect(saveErrorMessage(session.state(key)!,true)).toContain("当前内容已变化");session.disposeTimers();
});
