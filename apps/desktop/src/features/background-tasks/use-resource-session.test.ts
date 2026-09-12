import { act,renderHook } from "@testing-library/react";
import { afterEach,expect,it,vi } from "vitest";
import { localResourceClient } from "../../domains/local-resource-client";
import type { CoreEnvelope,LocalResourcePlan } from "../../types";
import { useResourceSession } from "./use-resource-session";
afterEach(() => vi.restoreAllMocks());
it("does not let a late preparation plan replace the newly selected profile", async () => {
  let finishOld!: (value: CoreEnvelope) => void;
  const next = {capabilityId: "local_transcription", capabilityName: "转写", transcriptionProfile: "fast", downloadBytes: 10, unknownSize: false} satisfies LocalResourcePlan;
  vi.spyOn(localResourceClient, "plan")
    .mockImplementationOnce(() => new Promise(resolve => {finishOld = resolve;}))
    .mockResolvedValueOnce({resourcePlan: next} as CoreEnvelope);
  const {result} = renderHook(() => useResourceSession({getJob: () => null, setJob: vi.fn(), setRuntime: vi.fn(), setShowRuntime: vi.fn(), setShowSourceImport: vi.fn(), setNotice: vi.fn()}));
  let old!: Promise<void>;
  act(() => {old = result.current.openResourcePreparation("local_transcription", "manage");});
  await act(async () => {await result.current.changeResourceProfile("fast");});
  await act(async () => {finishOld({resourcePlan: {...next, transcriptionProfile: "standard"}} as CoreEnvelope); await old;});
  expect(result.current.resourceProfile).toBe("fast");
  expect(result.current.resourcePlan).toEqual(next);
  expect(result.current.resourceBusy).toBe(false);
});
