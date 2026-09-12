import { desktopQuery } from "./desktop-query-client";
import { runCore } from "../core";

/** Execution lifecycle only. Sending and applying content use approval/editing contracts. */
export const agentReviewClient = {
  getCodexHealth: () => desktopQuery({action:"agent_health"}),
  getAgentRun: (runId: string) => desktopQuery({action:"agent_run",runId}),
  listAgentRuns: (projectId?: string) => desktopQuery({action:"agent_runs",projectId:projectId??null}),
  cancelAgent: (runId: string) => runCore(["agent", "cancel", runId]),
  resumeAgent: (runId: string) => runCore(["agent", "resume", runId]),
  updateTask: (taskId: string, action: "retry" | "cancel") => runCore(["task", action, taskId]),
};
