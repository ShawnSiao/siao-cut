import { act,renderHook } from "@testing-library/react";
import { afterEach,expect,it,vi } from "vitest";
import { backgroundTaskClient } from "../../domains/background-task-client";
import type { CoreEnvelope,RuntimeInfo,SourcePreview } from "../../types";
import { useAutoWorkflowSession } from "./use-auto-workflow-session";
afterEach(() => vi.restoreAllMocks());
it("ignores an old URL inspection and prevents duplicate workflow setup requests", async () => {
  let finish!: (value: CoreEnvelope) => void;
  const inspect = vi.spyOn(backgroundTaskClient, "inspectSource").mockImplementation(() => new Promise(resolve => {finish = resolve;}));
  const {result} = renderHook(() => useAutoWorkflowSession({runtime: {ytDlpConfigured: true} as RuntimeInfo, modelPath: null, modelPathAvailable: false, setModelPathAvailable: vi.fn(), transcriptionLanguage: "auto", uiLocale: "zh-CN", activeProjectIdRef: {current: null}, getWorkflow: () => null, setWorkflow: vi.fn(), setWorkflows: vi.fn(), openProject: vi.fn(), setNotice: vi.fn()}));
  act(() => result.current.setAutoUrl("https://example.com/old"));
  let pending!: Promise<void>;
  act(() => {pending = result.current.inspectAutoSource(); void result.current.inspectAutoSource();});
  expect(inspect).toHaveBeenCalledTimes(1);
  act(() => result.current.setAutoUrl("https://example.com/new"));
  await act(async () => {finish({source: {originalUrl: "https://example.com/old"} as SourcePreview} as CoreEnvelope); await pending;});
  expect(result.current.autoSourcePreview).toBeNull();
  expect(result.current.autoAuthorized).toBe(false);
});
