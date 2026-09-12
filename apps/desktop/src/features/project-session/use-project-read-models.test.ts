import { act,renderHook,waitFor } from "@testing-library/react";
import { afterEach,expect,it,vi } from "vitest";
import { projectSessionClient } from "../../domains/project-session-client";
import { sampleProject } from "../../mock";
import { useProjectReadModels } from "./use-project-read-models";

afterEach(() => vi.restoreAllMocks());

it("loads history again when returning to a project whose workspace projection omits it", async () => {
  const first = { ...structuredClone(sampleProject), versions: [], readModels: {} };
  const second = { ...first, id: "second" };
  const query = vi.spyOn(projectSessionClient, "history").mockResolvedValue({
    apiVersion: "0.1", status: "ok", history: first.history, versions: sampleProject.versions,
  });
  const setProject = vi.fn();
  const { rerender } = renderHook(({ project }) => useProjectReadModels(project, "history", setProject, vi.fn()), { initialProps: { project: first } });
  await waitFor(() => expect(setProject).toHaveBeenCalledTimes(1));
  rerender({ project: second });
  await waitFor(() => expect(setProject).toHaveBeenCalledTimes(2));
  rerender({ project: first });
  await waitFor(() => expect(setProject).toHaveBeenCalledTimes(3));
  expect(query).toHaveBeenNthCalledWith(3, first.id);
});

it("does not query reports on export and rejects a late report from the previous project", async () => {
  let deliver!: (value: Awaited<ReturnType<typeof projectSessionClient.insights>>) => void;
  const query = vi.spyOn(projectSessionClient, "insights").mockImplementation(() => new Promise(resolve => { deliver = resolve; }));
  const setProject = vi.fn();
  const { result, rerender } = renderHook(({ tab, project }) => useProjectReadModels(project, tab, setProject, vi.fn()), {initialProps: {tab: "export", project: {...structuredClone(sampleProject), readModels: {}}}});
  expect(query).not.toHaveBeenCalled();
  rerender({tab: "quality", project: {...structuredClone(sampleProject), readModels: {}}});
  expect(result.current.loading).toBe(true);
  expect(result.current.ready).toBe(false);
  rerender({tab: "export", project: {...structuredClone(sampleProject), id: "other", readModels: {}}});
  await act(async () => deliver({apiVersion: "0.1", status: "ok", versionId: sampleProject.history.currentVersionId ?? undefined, subtitleQuality: sampleProject.subtitleQuality}));
  expect(setProject).not.toHaveBeenCalled();
});

it("offers retry after a report fails without claiming the report is ready", async () => {
  const query = vi.spyOn(projectSessionClient, "insights").mockRejectedValueOnce(new Error("read failed")).mockResolvedValue({apiVersion:"0.1",status:"ok",versionId:sampleProject.history.currentVersionId!,subtitleQuality:sampleProject.subtitleQuality});
  const setProject = vi.fn();
  const {result} = renderHook(() => useProjectReadModels({...sampleProject, readModels:{}}, "quality", setProject, vi.fn()));
  await waitFor(() => expect(result.current.error).toContain("read failed"));
  expect(result.current.ready).toBe(false);
  act(() => result.current.retry());
  await waitFor(() => expect(setProject).toHaveBeenCalledTimes(1));
  expect(query).toHaveBeenCalledTimes(2);
});
