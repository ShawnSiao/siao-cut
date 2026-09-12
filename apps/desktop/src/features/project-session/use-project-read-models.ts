import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";
import { projectSessionClient } from "../../domains/project-session-client";
import type { Project } from "../../types";

/** History and computed diagnostics are refreshed only when their panel is being read. */
export function useProjectReadModels(project: Project | null, tab: string, setProject: Dispatch<SetStateAction<Project | null>>, onError: (message: string) => void) {
  const errorRef = useRef(onError); errorRef.current = onError;
  const domain = tab === "history" ? "history" : tab === "quality" || tab === "analysis" ? "insights" : null;
  useEffect(() => {
    if (!project || !domain) return;
    const id = project.id, version = project.history.currentVersionId;
    let stale = false;
    void projectSessionClient[domain](id).then((response) => {
      if (stale) return;
      const received = domain === "history" ? response.history?.currentVersionId : response.versionId;
      if (received !== version) return;
      setProject((current) => {
        if (!current || current.id !== id || current.history.currentVersionId !== version) return current;
        return domain === "history" ? { ...current, versions: response.versions ?? current.versions, history: response.history ?? current.history }
          : { ...current, subtitleQuality: response.subtitleQuality ?? current.subtitleQuality, speechInsights: response.speechInsights ?? current.speechInsights };
      });
    }).catch((error) => { if (!stale) errorRef.current(String(error)); });
    return () => { stale = true; };
  }, [domain, project?.id, project?.history.currentVersionId, setProject]);
}
