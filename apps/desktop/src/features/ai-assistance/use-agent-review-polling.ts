import type { Dispatch,RefObject,SetStateAction } from "react";
import { agentReviewClient } from "../../domains/agent-review-client";
import { projectSessionClient } from "../../domains/project-session-client";
import { useBackgroundTaskRegistry } from "../../hooks/use-background-task-registry";
import { tr } from "../../i18n";
import type { AgentRun,Project } from "../../types";
import { refreshReview } from "../project-session/refresh-review";
interface Inputs {
  project: Project | null;
  agentRun: AgentRun | null;
  setAgentRun: Dispatch<SetStateAction<AgentRun | null>>;
  activeProjectIdRef: RefObject<string | null>;
  refreshProject: (id: string) => Promise<Project>;
  onReview: () => void;
  setNotice: (message: string | null) => void;
  setError: Dispatch<SetStateAction<string | null>>;
  taskActionIdsRef: RefObject<Set<string>>;
  reviewEpoch: RefObject<number>;
  invalidateProjectLoads: (id: string) => void;
  setProject: Dispatch<SetStateAction<Project | null>>;
}

/** Fetch review state independently of transcript edits and reject reads superseded by task actions. */
export function useAgentReviewPolling({ project, agentRun, setAgentRun, activeProjectIdRef, refreshProject, onReview, setNotice, setError, taskActionIdsRef, reviewEpoch, invalidateProjectLoads, setProject }: Inputs) {
  useBackgroundTaskRegistry([
    agentRun && ["queued", "running", "submitting"].includes(agentRun.status) ? {
      key: `codex-agent:${agentRun.id}`,
      intervalMs: 1200,
      poll: () => agentReviewClient.getAgentRun(agentRun.id).then(async (envelope) => {
        if (!envelope.agentRun)
          return;
        const next = envelope.agentRun;
        const isActiveProject = activeProjectIdRef.current === next.projectId;
        if (isActiveProject)
          setAgentRun(next);
        if (next.status === "completed") {
          await refreshProject(next.projectId);
          await refreshReview(next.projectId, setProject);
          if (activeProjectIdRef.current === next.projectId) {
            onReview();
            setNotice(tr("app.creator.agent.completed"));
          }
        }
        if (isActiveProject && ["failed", "interrupted"].includes(next.status))
          setError(next.errorMessage ?? tr("app.creator.agent.failed"));
        if (isActiveProject && next.status === "cancelled")
          setNotice(tr("app.creator.agent.cancelled"));
      }).catch((cause) => {
        if (activeProjectIdRef.current === agentRun.projectId)
          setError(cause instanceof Error ? cause.message : String(cause));
      }),
    } : null,
    project?.tasks.some((task) => ["queued", "claimed", "running", "failed", "interrupted"].includes(task.status)) ? {
      key: `agent-project:${project.id}`,
      intervalMs: project.tasks.some((task) => ["queued", "claimed", "running"].includes(task.status)) ? 2500 : 5000,
      poll: () => {
        if (taskActionIdsRef.current.size) return Promise.resolve();
        const epoch = reviewEpoch.current;
        return projectSessionClient.review(project.id).then(async (envelope) => {
          if (epoch !== reviewEpoch.current || taskActionIdsRef.current.size) return;
          if (activeProjectIdRef.current !== project.id) return;
          if (envelope.versionId !== project.history.currentVersionId) { await refreshProject(project.id); return; }
          invalidateProjectLoads(project.id);
          setProject((current) => {
            if (!current || current.id !== project.id || current.history.currentVersionId !== envelope.versionId) return current;
            if (JSON.stringify([current.tasks, current.patchSets, current.workflows]) === JSON.stringify([envelope.tasks, envelope.patchSets, envelope.projectWorkflows])) return current;
            return { ...current, tasks: envelope.tasks ?? current.tasks, patchSets: envelope.patchSets ?? current.patchSets, workflows: envelope.projectWorkflows ?? current.workflows };
          });
        }).catch(() => undefined);
      },
    } : null,
  ]);
}
