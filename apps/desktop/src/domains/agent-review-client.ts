import { desktopControl } from "./desktop-control-client";
import { desktopQuery } from "./desktop-query-client";

/** Execution lifecycle only. Sending and applying content use approval/editing contracts. */
export const agentReviewClient = {
  getCodexHealth: () => desktopQuery({action:"agent_health"}),
  getAgentRun: (runId: string) => desktopQuery({action:"agent_run",runId}),
  listAgentRuns: (projectId?: string) => desktopQuery({action:"agent_runs",projectId:projectId??null}),
  cancelAgent: (runId: string) => desktopControl({ action: "agent_cancel", runId: runId }),
  resumeAgent: (runId: string) => desktopControl({action:"agent_resume",runId,startDelayMs:null}),
  updateTask: (taskId: string, action: "retry" | "cancel") => desktopControl({action: action === "retry" ? "task_retry" : "task_cancel", taskId}),
};
