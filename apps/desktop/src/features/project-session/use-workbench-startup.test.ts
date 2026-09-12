import {act, renderHook, waitFor} from "@testing-library/react";
import {afterEach, expect, it, vi} from "vitest";
import {projectSessionClient} from "../../domains/project-session-client";
import {resetMockProjectForTest} from "../../core.mock";
import {useWorkbenchStartup} from "./use-workbench-startup";
type Inputs = Parameters<typeof useWorkbenchStartup>[0];
function inputs(): Inputs {
  return {
    setBusy: vi.fn(),
    setError: vi.fn(),
    setUpdatePolicy: vi.fn(),
    setAutoWorkflows: vi.fn(),
    setAutoWorkflow: vi.fn(),
    setTrackedAutoWorkflowIds: vi.fn(),
    setModels: vi.fn(),
    setModelJob: vi.fn(),
    setSpeakerPackage: vi.fn(),
    setSpeakerJobs: vi.fn(),
    setSpeakerJob: vi.fn(),
    setTranscriptionHealth: vi.fn(),
    setTranscriptionConfig: vi.fn(),
    setCodexHealth: vi.fn(),
    setLocalResources: vi.fn(),
    setResourceProfile: vi.fn(),
    setResourceCapability: vi.fn(),
    setResourceSetupReason: vi.fn(),
    setResourcePlan: vi.fn(),
    setShowResourceSetup: vi.fn(),
    setResourceJob: vi.fn(),
    setSourceJob: vi.fn(),
    setRuntime: vi.fn(),
    setModelPath: vi.fn(),
    setModelPathAvailable: vi.fn(),
    replaceProjectPage: vi.fn(),
    flush: vi.fn(async () => {}),
    restoreProject: vi.fn(async () => {}),
  };
}
afterEach(() => {vi.restoreAllMocks();resetMockProjectForTest();localStorage.removeItem("siaocut.modelPath");});
it("serializes refresh and releases busy only once after project restoration finishes", async () => {
  const ports=inputs();
  let finish!:()=>void;
  ports.restoreProject=vi.fn(()=>new Promise<void>(resolve=>{finish=resolve;}));
  const list=vi.spyOn(projectSessionClient,"listProjects").mockResolvedValue({items:[],total:0,nextOffset:null});
  const {result}=renderHook(()=>useWorkbenchStartup(ports));
  await waitFor(()=>expect(ports.restoreProject).toHaveBeenCalledOnce());
  const first=result.current(),second=result.current();
  expect(first).toBe(second);
  expect(list).toHaveBeenCalledOnce();
  expect(ports.setBusy).not.toHaveBeenCalledWith(null);
  await act(async()=>{finish();await first;});
  expect(vi.mocked(ports.setBusy).mock.calls.filter(([value])=>value===null)).toHaveLength(1);
});
it("discards startup results after the workbench unmounts",async()=>{
  const ports=inputs();
  let deliver!:(page:Awaited<ReturnType<typeof projectSessionClient.listProjects>>)=>void;
  vi.spyOn(projectSessionClient,"listProjects").mockImplementation(()=>new Promise(resolve=>{deliver=resolve;}));
  const {result,unmount}=renderHook(()=>useWorkbenchStartup(ports));
  await waitFor(()=>expect(deliver).toBeDefined());
  const pending=result.current();unmount();
  await act(async()=>{deliver({items:[],total:0,nextOffset:null});await pending;});
  expect(ports.setModels).not.toHaveBeenCalled();
  expect(ports.restoreProject).not.toHaveBeenCalled();
});
