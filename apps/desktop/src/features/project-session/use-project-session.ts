import { useCallback,useRef,useState,type SetStateAction } from "react";
import { projectSessionClient } from "../../domains/project-session-client";
import type { EditReceipt,ProjectPage,ProjectSummary } from "../../generated/core-contract";
import type { Project } from "../../types";

export function projectSummary(project: Project): ProjectSummary {
  return {
    id: project.id, title: project.title, createdAt: project.createdAt, updatedAt: project.updatedAt,
    segmentCount: project.transcript.segments.length, durationSeconds: project.media.durationSeconds,
    versionId: project.history.currentVersionId
  };
}

/** Apply only acknowledged text; unrelated rows and read models retain their identity. */
export function applyEditReceipt(project: Project, receipt: EditReceipt): Project {
  if (project.id !== receipt.projectId) return project;
  const source = receipt.field === "source";
  const transcript = source ? {
    ...project.transcript, segments: project.transcript.segments.map((segment) =>
      segment.id === receipt.segmentId ? { ...segment, text: receipt.text } : segment)
  } : project.transcript;
  const translations = Object.fromEntries(Object.entries(project.translations).map(([language, translation]) => {
    if (!source && receipt.field !== `translation:${language}`) return [language, translation];
    const segments = translation.segments.map((segment) => segment.segmentId !== receipt.segmentId ? segment
      : source ? { ...segment, status: "stale" } : { ...segment, text: receipt.text, status: "current" });
    return [language, { ...translation, segments, status: segments.every((segment) => segment.status === "current") ? "current" : "stale" }];
  }));
  return {
    ...project, transcript, translations,
    history: receipt.history ?? { ...project.history, currentVersionId: receipt.versionId, canUndo: true, canRedo: false },
    updatedAt: receipt.version?.createdAt ?? project.updatedAt,
  };
}

/** The only writable Project owner. Other sessions consume it or send acknowledged changes. */
export function useProjectSession() {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [project, setProjectState] = useState<Project | null>(null);
  const projectRef = useRef<Project | null>(null);
  const activeProjectIdRef = useRef<string | null>(null);
  const sequences = useRef(new Map<string, number>());
  const [nextProjectOffset, setNextProjectOffset] = useState<number | null>(null);
  const [projectPageLoading, setProjectPageLoading] = useState(false);
  const pageInFlight = useRef(false);
  const pageGeneration = useRef(0);
  const replaceProjectPage = useCallback((page: ProjectPage) => {
    pageGeneration.current += 1;
    setProjects(page.items);
    setNextProjectOffset(page.nextOffset);
  }, []);
  const setProject = useCallback((action: SetStateAction<Project | null>) => {
    const next = typeof action === "function" ? action(projectRef.current) : action;
    if (next && activeProjectIdRef.current && next.id !== activeProjectIdRef.current) return;
    projectRef.current = next; setProjectState(next);
  }, []);
  const beginProjectLoad = useCallback((id: string) => {
    const sequence = (sequences.current.get(id) ?? 0) + 1;
    sequences.current.set(id, sequence); return sequence;
  }, []);
  const isCurrentProjectLoad = useCallback((id: string, sequence: number) => sequences.current.get(id) === sequence, []);
  const invalidateProjectLoads = useCallback((id: string) => { beginProjectLoad(id); }, [beginProjectLoad]);
  const updateProjectSummary = useCallback((next: Project) => {
    const summary = projectSummary(next);
    setProjects((items) => items.some((item) => item.id === next.id) ? items.map((item) => item.id === next.id ? summary : item) : [summary, ...items]);
  }, []);
  const loadMoreProjects = async () => {
    if (nextProjectOffset === null || pageInFlight.current) return;
    const generation = pageGeneration.current;
    pageInFlight.current = true; setProjectPageLoading(true);
    try {
      const page = await projectSessionClient.listProjects(nextProjectOffset);
      if (generation !== pageGeneration.current) return;
      setProjects((items) => [...items, ...page.items.filter((item) => !items.some((current) => current.id === item.id))]);
      setNextProjectOffset(page.nextOffset);
    } finally { pageInFlight.current = false; setProjectPageLoading(false); }
  };
  const acknowledgeEdit = useCallback(async (receipt: EditReceipt) => {
    // Invalidate any load started before this acknowledgment, including background refreshes.
    invalidateProjectLoads(receipt.projectId);
    const current = projectRef.current;
    if (!current || current.id !== receipt.projectId) return;
    if (current.edits.some((edit) => edit.segmentId === receipt.segmentId && edit.kind === "word_cut")) {
      // Source edits also invalidate timed word cuts. Refresh that affected project projection.
      const sequence = beginProjectLoad(current.id);
      const next = await projectSessionClient.loadProject(current.id);
      if (activeProjectIdRef.current === current.id && isCurrentProjectLoad(current.id, sequence)) { setProject(next); updateProjectSummary(next); return next; }
      return;
    }
    const next = applyEditReceipt(current, receipt);
    setProject(next); updateProjectSummary(next); return next;
  }, [beginProjectLoad, invalidateProjectLoads, isCurrentProjectLoad, setProject, updateProjectSummary]);
  return {
projects, setProjects, project, setProject, projectRef, activeProjectIdRef, beginProjectLoad, isCurrentProjectLoad,
    invalidateProjectLoads, updateProjectSummary, acknowledgeEdit, nextProjectOffset, replaceProjectPage, projectPageLoading, loadMoreProjects
};
}
