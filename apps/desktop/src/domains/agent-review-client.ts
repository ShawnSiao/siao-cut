import { runCore } from "../core";

/** Execution lifecycle only. Sending and applying content use approval/editing contracts. */
export const agentReviewClient = {
  getCodexHealth: () => runCore(["agent", "health"]),
  getAgentRun: (runId: string) => runCore(["agent", "status", runId]),
  listAgentRuns: (projectId?: string) => runCore(["agent", "list", ...(projectId ? [projectId] : [])]),
  cancelAgent: (runId: string) => runCore(["agent", "cancel", runId]),
  resumeAgent: (runId: string) => runCore(["agent", "resume", runId]),
  updateTask: (taskId: string, action: "retry" | "cancel") => runCore(["task", action, taskId]),
};
