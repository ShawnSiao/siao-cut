import { runCore } from "../core";
import type { UiLocale } from "../i18n";

type AgentWorkflowKind = "polish" | "proofread" | "edit" | "translate" | "punctuate" | "speaker_names";

export const agentReviewClient = {
  getCodexHealth: () => runCore(["agent", "health"]),
  startAgent: (taskId: string, timeoutSeconds = 900) => runCore(["agent", "start", taskId, "--timeout-seconds", String(timeoutSeconds)]),
  getAgentRun: (runId: string) => runCore(["agent", "status", runId]),
  listAgentRuns: (projectId?: string) => runCore(["agent", "list", ...(projectId ? [projectId] : [])]),
  cancelAgent: (runId: string) => runCore(["agent", "cancel", runId]),
  resumeAgent: (runId: string) => runCore(["agent", "resume", runId]),
  createWorkflow: (projectId: string, kind: AgentWorkflowKind, locale: UiLocale, language?: string) => runCore([
    "workflow", "create", projectId,
    "--kind", kind,
    "--locale", locale,
    ...(kind === "translate" && language ? ["--lang", language] : []),
  ]),
  updateTask: (taskId: string, action: "retry" | "cancel") => runCore(["task", action, taskId]),
  reviewPatch: (patchItemId: string, action: "apply" | "keep") => runCore(["task", "review", patchItemId, "--action", action]),
  reviewAll: (taskId: string, action: "apply" | "keep") => runCore(["task", "review-all", taskId, "--action", action]),
};
