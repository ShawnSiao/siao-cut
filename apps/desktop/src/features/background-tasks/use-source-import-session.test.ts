import { act,renderHook } from "@testing-library/react";
import { afterEach,expect,it,vi } from "vitest";
import { backgroundTaskClient } from "../../domains/background-task-client";
import type { CoreEnvelope,LocalResourceStatus,RuntimeInfo,SourcePreview } from "../../types";
import { useSourceImportSession } from "./use-source-import-session";

afterEach(() => vi.restoreAllMocks());
const preview = { originalUrl: "https://example.com/old", siteMediaId: "old" } as SourcePreview;
function setup() {
  return renderHook(() => useSourceImportSession({
    localResources: { capabilities: [{id: "url_import", state: "ready"}] } as LocalResourceStatus,
    runtime: {ytDlpConfigured: true} as RuntimeInfo,
    activeProjectIdRef: {current: "project-a"}, getJob: () => null,
    setJob: vi.fn(), setNotice: vi.fn(), prepareResources: vi.fn(),
  }));
}
it("does not accept an old inspection after the URL changes and suppresses duplicate clicks", async () => {
  let finish!: (value: CoreEnvelope) => void;
  const inspect = vi.spyOn(backgroundTaskClient, "inspectSource").mockImplementation(() => new Promise(resolve => {finish = resolve;}));
  const {result} = setup();
  act(() => result.current.setSourceUrl(preview.originalUrl));
  let pending!: Promise<void>;
  act(() => {pending = result.current.inspectSource(); void result.current.inspectSource();});
  expect(inspect).toHaveBeenCalledTimes(1);
  act(() => result.current.setSourceUrl("https://example.com/new"));
  await act(async () => {finish({source: preview} as CoreEnvelope); await pending;});
  expect(result.current.sourcePreview).toBeNull();
  expect(result.current.sourceAuthorized).toBe(false);
  expect(result.current.sourceBusy).toBeNull();
});
it("revokes the inspected target when browser authorization changes", async () => {
  vi.spyOn(backgroundTaskClient, "inspectSource").mockResolvedValue({source: preview} as CoreEnvelope);
  const {result} = setup();
  act(() => result.current.setSourceUrl(preview.originalUrl));
  await act(async () => {await result.current.inspectSource();});
  expect(result.current.sourcePreview).toEqual(preview);
  act(() => {result.current.setSourceAuthorized(true);});
  act(() => {result.current.setSourceBrowser("edge");});
  expect(result.current.sourcePreview).toBeNull();
  expect(result.current.sourceAuthorized).toBe(false);
});
