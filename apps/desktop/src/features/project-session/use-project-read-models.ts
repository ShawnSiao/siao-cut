import { useEffect,useRef,useState,type Dispatch,type SetStateAction } from "react";
import { projectSessionClient } from "../../domains/project-session-client";
import type { Project } from "../../types";

/** Expensive reports and history are loaded only for the panel that consumes them. */
export function useProjectReadModels(project: Project | null, tab: string, setProject: Dispatch<SetStateAction<Project | null>>, onError: (message: string) => void) {
  const report = useReadModel(project, tab === "history" ? "history" : ["review", "quality", "analysis"].includes(tab) ? "insights" : null, setProject, onError);
  const review = useReadModel(project, tab === "review" ? "review" : null, setProject, onError);
  return { ready: report.ready && review.ready, reviewReady: review.ready, loading: report.loading || review.loading, error: report.error ?? review.error, retry: () => { report.retry(); review.retry(); } };
}
function useReadModel(project: Project | null, domain: "history" | "insights" | "review" | null, setProject: Dispatch<SetStateAction<Project | null>>, onError: (message: string) => void) {
  const errorRef = useRef(onError); errorRef.current = onError;
  const id = project?.id, version = project?.history.currentVersionId;
  const loaded = Boolean(project && domain && (!project.readModels || (Object.hasOwn(project.readModels, domain) && project.readModels[domain] === version)));
  const [failure, setFailure] = useState<{key: string; message: string} | null>(null);
  const [attempt, setAttempt] = useState(0);
  const key = `${id}:${version}:${domain}`;
  useEffect(() => {
    if (!id || !domain || loaded) return;
    let stale = false;
    setFailure(null);
    void projectSessionClient[domain](id).then((response) => {
      if (stale) return;
      const received = domain === "history" ? response.history?.currentVersionId : response.versionId;
      if ((received ?? null) !== (version ?? null)) throw new Error("项目版本已变化，请重试 / Project changed; retry loading");
      setProject((current) => {
        if (!current || current.id !== id || current.history.currentVersionId !== version) return current;
        const fields = domain === "review" ? { tasks: response.tasks ?? current.tasks, patchSets: response.patchSets ?? current.patchSets, workflows: response.projectWorkflows ?? current.workflows } : domain === "history" ? { versions: response.versions ?? current.versions, history: response.history ?? current.history }
          : { subtitleQuality: response.subtitleQuality ?? current.subtitleQuality, speechInsights: response.speechInsights ?? current.speechInsights };
        return { ...current, ...fields, readModels: { ...current.readModels, [domain]: version } };
      });
    }).catch((error) => {
      if (!stale) { const message = String(error); setFailure({key, message}); errorRef.current(message); }
    });
    return () => { stale = true; };
  }, [domain, id, version, loaded, setProject, attempt]);
  const error = failure?.key === key ? failure.message : null;
  return { ready: !domain || Boolean(project && (!project.readModels || Object.hasOwn(project.readModels, domain))), loading: Boolean(domain && !loaded && !error), error, retry: () => setAttempt(value => value + 1) };
}
