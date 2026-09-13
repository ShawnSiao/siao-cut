import { useCallback,useRef } from "react";
import { projectSessionClient } from "../../domains/project-session-client";
import type { Project } from "../../types";
import type { useProjectSession } from "./use-project-session";

type Session = Pick<ReturnType<typeof useProjectSession>, "projectRef" | "activeProjectIdRef" | "beginProjectLoad" | "isCurrentProjectLoad" | "setProject" | "updateProjectSummary">;
interface Events {
  reset: () => void;
  prepareMedia: (project: Project) => Promise<{ mediaUrl: string | null; waveformUrl: string | null }>;
  mediaReady: (project: Project, media: { mediaUrl: string | null; waveformUrl: string | null }) => void;
  projectReady: (project: Project, sequence: number, opening: boolean) => Promise<void>;
}

/** Owns project opening, stale-read fencing and rollback after a failed switch. */
export function useProjectLifecycle(session: Session, events: Events) {
  const latest = useRef({ session, events });
  latest.current = { session, events };
  const activation = useRef(0);
  const refreshProject = useCallback(async (id: string, refreshMedia = false) => {
    const { session } = latest.current;
    const sequence = session.beginProjectLoad(id);
    const next = await projectSessionClient.loadProject(id);
    const media = refreshMedia ? await latest.current.events.prepareMedia(next) : null;
    if (!session.isCurrentProjectLoad(id, sequence)) return next;
    session.updateProjectSummary(next);
    if (session.activeProjectIdRef.current !== id) return next;
    if (media) latest.current.events.mediaReady(next, media);
    const opening = session.projectRef.current?.id !== id;
    session.setProject(current => current?.id === id ? { ...next, subtitleQuality: current.subtitleQuality, speechInsights: current.speechInsights, versions: current.versions, tasks: current.tasks, patchSets: current.patchSets, workflows: current.workflows, readModels: { ...current.readModels, review: undefined } } : next);
    await latest.current.events.projectReady(next, sequence, opening);
    return next;
  }, []);
  const activateProject = useCallback(async (id: string) => {
    const attempt = ++activation.current;
    const { session } = latest.current;
    const previous = session.activeProjectIdRef.current;
    session.activeProjectIdRef.current = id;
    latest.current.events.reset();
    try {
      await refreshProject(id, true);
    } catch (error) {
      // A failed earlier switch must not roll back a more recent navigation.
      if (attempt === activation.current && session.activeProjectIdRef.current === id) {
        session.activeProjectIdRef.current = previous;
        if (previous) await refreshProject(previous, true).catch(() => undefined);
      }
      throw error;
    }
  }, [refreshProject]);
  return { refreshProject, activateProject };
}
