import { act,renderHook } from "@testing-library/react";
import { describe,expect,it,vi } from "vitest";
import { projectSessionClient } from "../../domains/project-session-client";
import type { EditReceipt } from "../../generated/core-contract";
import { sampleProject } from "../../mock";
import { applyEditReceipt,useProjectSession } from "./use-project-session";

describe("project session projections", () => {
  it("rejects a late full-project response after navigation changed the active project", () => {
    const {result} = renderHook(() => useProjectSession());
    act(() => {result.current.activeProjectIdRef.current=sampleProject.id;result.current.setProject(sampleProject);});
    act(() => {result.current.activeProjectIdRef.current="next-project";result.current.setProject(null);});
    act(() => result.current.setProject({...sampleProject,title:"Late response"}));
    expect(result.current.project).toBeNull();
    act(() => result.current.setProject({...sampleProject,id:"next-project",title:"Current project"}));
    expect(result.current.project?.title).toBe("Current project");
  });
  it("ignores an old next page after the project list is reloaded", async () => {
    let resolvePage!: (page: Awaited<ReturnType<typeof projectSessionClient.listProjects>>) => void;
    const list = vi.spyOn(projectSessionClient, "listProjects").mockImplementation(() => new Promise((resolve) => { resolvePage = resolve; }));
    const { result } = renderHook(() => useProjectSession());
    act(() => result.current.replaceProjectPage({ items: [], nextOffset: 50, total: 100 }));
    let pending!: Promise<void>;
    act(() => { pending = result.current.loadMoreProjects(); });
    act(() => result.current.replaceProjectPage({ items: [], nextOffset: null, total: 0 }));
    await act(async () => { resolvePage({ items: [{ id: "deleted-project", title: "Old", createdAt: "", updatedAt: "", segmentCount: 0, durationSeconds: null, versionId: null }], nextOffset: 100, total: 150 }); await pending; });
    expect(result.current.projects).toEqual([]);
    expect(result.current.nextProjectOffset).toBeNull();
    expect(result.current.projectPageLoading).toBe(false);
    list.mockRestore();
  });
  it("acknowledges one row without transport calls or rebuilding unrelated rows and invalidates an older read", async () => {
    const project = structuredClone(sampleProject); project.edits = [];
    const load = vi.spyOn(projectSessionClient, "loadProject");
    const { result } = renderHook(() => useProjectSession());
    act(() => { result.current.activeProjectIdRef.current = project.id; result.current.setProject(project); });
    const oldRead = result.current.beginProjectLoad(project.id);
    const receipt: EditReceipt = { mutationId: "save-once", projectId: project.id, segmentId: project.transcript.segments[0].id, field: "source", text: "确认后的新内容", versionId: "v-new", changedDomains: ["transcript", "history"], history: { canUndo: true, canRedo: false, currentVersionId: "v-new" }, version: { id: "v-new", createdAt: "2026-09-12", reason: "编辑字幕" } };
    await act(async () => { await result.current.acknowledgeEdit(receipt); });
    expect(load).not.toHaveBeenCalled();
    expect(result.current.isCurrentProjectLoad(project.id, oldRead)).toBe(false);
    expect(result.current.project?.transcript.segments[0].text).toBe(receipt.text);
    expect(result.current.project?.transcript.segments[1]).toBe(project.transcript.segments[1]);
    expect(result.current.project?.tasks).toBe(project.tasks);
    expect(result.current.project?.versions).toBe(project.versions);
    expect(result.current.projects[0]).not.toHaveProperty("transcript");
    load.mockRestore();
  });
  it("does not apply a late receipt to a different active project", () => {
    const receipt = { projectId: "another" } as EditReceipt;
    expect(applyEditReceipt(sampleProject, receipt)).toBe(sampleProject);
  });
});
