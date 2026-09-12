import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { projectSessionClient } from "../../domains/project-session-client";
import { sampleProject } from "../../mock";
import { useProjectLifecycle } from "./use-project-lifecycle";
import { useProjectSession } from "./use-project-session";

describe("project navigation ownership", () => {
  afterEach(() => vi.restoreAllMocks());
  it("keeps newer navigation when an earlier open fails late", async () => {
    let rejectOld!: (error: Error) => void;
    const next = { ...structuredClone(sampleProject), id: "new-project" };
    vi.spyOn(projectSessionClient, "loadProject").mockImplementation((id) => id === "slow-project"
      ? new Promise((_, reject) => { rejectOld = reject; }) : Promise.resolve(next));
    const ready = vi.fn(async () => { });
    const { result } = renderHook(() => {
      const session = useProjectSession();
      const lifecycle = useProjectLifecycle(session, {
        reset: () => session.setProject(null),
        prepareMedia: async () => ({ mediaUrl: null, waveformUrl: null }),
        mediaReady: () => { }, projectReady: ready,
      });
      return { ...session, ...lifecycle };
    });
    act(() => { result.current.activeProjectIdRef.current = sampleProject.id; result.current.setProject(sampleProject); });
    let old!: Promise<unknown>;
    act(() => { old = result.current.activateProject("slow-project").catch((error) => error); });
    await act(async () => { await result.current.activateProject(next.id); });
    await act(async () => { rejectOld(new Error("old request failed")); await old; });
    expect(result.current.activeProjectIdRef.current).toBe(next.id);
    expect(result.current.project?.id).toBe(next.id);
    expect(ready).toHaveBeenCalledTimes(1);
  });
});
