import { agentReviewClient } from "../../domains/agent-review-client";
import { tr } from "../../i18n";
import type { AgentRun, Project } from "../../types";
import type { RefObject } from "react";
import { approvalRecoveryMessage, requiresNewApproval } from "./approval-recovery";

type WorkflowKind = "polish" | "proofread" | "edit" | "translate" | "punctuate" | "speaker_names";
type Recovery = {
  agentRun: AgentRun; project: Project | null; activeProjectIdRef: RefObject<string | null>;
  refreshProject(id: string): Promise<Project>; setAgentWorkflowKind(kind: WorkflowKind): void;
  setAiApprovalTaskId(id: string | null): void; setShowAiExecutionConfirm(show: boolean): void;
  setError(error: string | null): void; setNotice(notice: string): void;
  setAgentRun(run: AgentRun): void; onReview(): void;
};

export async function resumeAgentWithRecovery(input: Recovery) {
  const { agentRun, project } = input;
  let envelope;
  try { envelope = await agentReviewClient.resumeAgent(agentRun.id); }
  catch (cause) {
    if (!requiresNewApproval(cause) || !project) throw cause;
    const refreshed = await input.refreshProject(project.id);
    if (input.activeProjectIdRef.current !== project.id) return;
    const task = refreshed.tasks.find(item => item.id === agentRun.taskId);
    if (task && ["polish", "proofread", "edit", "translate", "punctuate", "speaker_names"].includes(task.kind)) input.setAgentWorkflowKind(task.kind as WorkflowKind);
    input.setAiApprovalTaskId(null); input.setError(null); input.setNotice(approvalRecoveryMessage);
    input.setShowAiExecutionConfirm(true);
    return;
  }
  if (!envelope.agentRun) throw new Error(tr("app.creator.agent.runMissing"));
  input.setAgentRun(envelope.agentRun); input.onReview(); input.setNotice(tr("app.creator.agent.resumed"));
}
