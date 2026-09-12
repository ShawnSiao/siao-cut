import { useEffect, useRef, useState, type Dispatch, type RefObject, type SetStateAction } from "react";
import { getProjectCapabilities } from "../../app-view-model";
import { agentReviewClient } from "../../domains/agent-review-client";
import { aiApprovalClient } from "../../domains/ai-approval-client";
import { tr, type UiLocale } from "../../i18n";
import type { AgentRun, CodexHealth, Project, Task } from "../../types";
import type { EditingSession } from "../editing/editing-session";
import type { AiExecutionSelection } from "./types";
import { useAgentReviewPolling } from "./use-agent-review-polling";
export const isValidAgentIdentity = (value: string) => /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(value);
type Inputs = {
project: Project | null; mediaUrl: string | null; subtitleLanguage: string; uiLocale: UiLocale; editing: EditingSession;
  setProject: Dispatch<SetStateAction<Project | null>>; updateProjectSummary: (project: Project) => void; setConfirmStaleTranslation: (value: boolean) => void;
  setNotice: (value: string | null) => void; setError: Dispatch<SetStateAction<string | null>>; activeProjectIdRef: RefObject<string | null>; invalidateProjectLoads: (id: string) => void;
  onReview: () => void; refreshProject: (id: string) => Promise<Project>; withBusy: (label: string, action: () => Promise<void>) => Promise<void>
};
export function useAiReviewSession({ project, mediaUrl, subtitleLanguage, uiLocale, editing, setProject, updateProjectSummary, setConfirmStaleTranslation, setNotice, setError, activeProjectIdRef, invalidateProjectLoads, onReview, refreshProject, withBusy }: Inputs) {
  const [agentWorkflowKind, setAgentWorkflowKind] = useState<"polish" | "proofread" | "edit" | "translate" | "punctuate" | "speaker_names">("polish");
  const [codexHealth, setCodexHealth] = useState<CodexHealth | null>(null);
  const [agentRun, setAgentRun] = useState<AgentRun | null>(null);
  const [showAgentHandoff, setShowAgentHandoff] = useState(false);
  const [showAiExecutionConfirm, setShowAiExecutionConfirm] = useState(false);
  const [aiApprovalTaskId, setAiApprovalTaskId] = useState<string | null>(null);
  const [agentHandoffTaskId, setAgentHandoffTaskId] = useState<string | null>(null);
  const [agentIdentity, setAgentIdentity] = useState("external-agent");
  const [agentHandoffReady, setAgentHandoffReady] = useState(false);
  const [agentHandoffCopied, setAgentHandoffCopied] = useState(false);
  const [taskActions, setTaskActions] = useState<Record<string, "retry" | "cancel">>({});
  const [glossaryDraft, setGlossaryDraft] = useState("");
  const agentButtonRef = useRef<HTMLButtonElement>(null);
  const agentHandoffReturnFocusRef = useRef<HTMLElement>(null);
  const taskActionIdsRef = useRef(new Set<string>());
  const reviewEpoch = useRef(0);
  const capabilities = getProjectCapabilities(project, { mediaUrl, translationTarget: subtitleLanguage, agentWorkflowKind });
  useEffect(() => {
    const entries = project?.glossary.entries.filter((entry) => entry.language === subtitleLanguage) ?? [];
    setGlossaryDraft(entries.map((entry) => `${entry.source}=${entry.target}`).join("\n"));
  }, [project?.id, project?.glossary.version, subtitleLanguage]);
  const openAgentHandoff = (trigger: HTMLElement | null) => {
    agentHandoffReturnFocusRef.current = trigger;
    setAgentHandoffTaskId(null);
    setAgentHandoffReady(false);
    setAgentHandoffCopied(false);
    setShowAgentHandoff(true);
  };
  const openExistingAgentHandoff = (taskId: string, trigger: HTMLElement) => {
    agentHandoffReturnFocusRef.current = trigger;
    const claimedBy = project?.tasks.find((task) => task.id === taskId)?.lease?.worker;
    if (claimedBy)
      setAgentIdentity(claimedBy);
    setAgentHandoffTaskId(taskId);
    setAgentHandoffReady(true);
    setAgentHandoffCopied(false);
    setShowAgentHandoff(true);
  };
  const assertAgentWorkflowReady = () => {
    if (!capabilities.hasBoundMedia)
      throw new Error(tr("app.capability.mediaRequired"));
    if (!capabilities.hasTranscript)
      throw new Error(tr("app.capability.transcriptRequired"));
    if (agentWorkflowKind === "translate" && !capabilities.hasTranslationTarget)
      throw new Error(tr("app.capability.translationTargetRequired"));
  };
  const saveGlossary = () => project && withBusy(tr("app.creator.glossary.saving"), async () => {
    const entries = glossaryDraft
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const separator = line.indexOf("=");
        if (separator <= 0 || separator === line.length - 1)
          throw new Error(tr("app.creator.glossary.invalid"));
        return { source: line.slice(0, separator).trim(), target: line.slice(separator + 1).trim() };
      });
    const envelope = await editing.mutate(project.id, { kind: "replace_glossary", language: subtitleLanguage, expectedGlossaryVersion: project.glossary.version, entries: entries.map((entry) => [entry.source, entry.target]) });
    if (!envelope.project)
      throw new Error(tr("app.canvas.projectMissing"));
    setProject(envelope.project);
    updateProjectSummary(envelope.project!);
    setConfirmStaleTranslation(false);
    setNotice(tr("app.creator.glossary.saved", { version: envelope.project.glossary.version }));
  });
  const createAgentTask = () => project && withBusy(tr("app.s0222"), async () => {
    assertAgentWorkflowReady();
    const envelope = await editing.mutate(project.id, { kind: "create_workflow", workflowKind: agentWorkflowKind, locale: uiLocale, language: agentWorkflowKind === "translate" ? subtitleLanguage : null });
    await refreshProject(project.id);
    setAgentHandoffTaskId(envelope.taskId ?? null);
    setNotice({
      polish: tr("app.workflow.created.polish"),
      proofread: tr("app.workflow.created.proofread"),
      edit: tr("app.workflow.created.edit"),
      translate: tr("app.workflow.created.translate", { language: subtitleLanguage.toUpperCase() }),
      punctuate: tr("app.workflow.created.punctuate"),
      speaker_names: tr("app.workflow.created.speakerNames"),
    }[agentWorkflowKind]);
  });
  const startAiAssistance = async (target: AiExecutionSelection, approvalId?: string) => {
    if (!project) return;
    assertAgentWorkflowReady();
    if (target.kind === "copy_prompt") {
      const workflow = aiApprovalTaskId ? { taskId: aiApprovalTaskId } : await editing.mutate(project.id, { kind: "create_workflow", workflowKind: agentWorkflowKind, locale: uiLocale, language: agentWorkflowKind === "translate" ? subtitleLanguage : null });
      if (!workflow.taskId) throw new Error(tr("app.creator.agent.taskMissing"));
      await refreshProject(project.id);
      agentHandoffReturnFocusRef.current = agentButtonRef.current;
      setAgentHandoffTaskId(workflow.taskId); setAgentHandoffReady(true); setAgentHandoffCopied(false);
      setShowAgentHandoff(true); setShowAiExecutionConfirm(false);
      setNotice(tr("app.creator.agent.manualFallback"));
      return;
    }
    if (!approvalId) throw new Error("请先核对实际发送预检。");
    const envelope = await aiApprovalClient.execute(approvalId);
    if (!envelope.agentRun) throw new Error(tr("app.creator.agent.runMissing"));
    setAgentRun(envelope.agentRun); setShowAiExecutionConfirm(false);
    onReview(); setNotice(["queued", "running"].includes(envelope.agentRun.status) ? tr("app.creator.agent.started") : `已找到本次运行：${envelope.agentRun.errorMessage ?? envelope.agentRun.status}`);
    await refreshProject(project.id);
  };
  const cancelCodexAgent = () => agentRun && withBusy(tr("app.creator.agent.cancelling"), async () => {
    const envelope = await agentReviewClient.cancelAgent(agentRun.id);
    if (envelope.agentRun)
      setAgentRun(envelope.agentRun);
    if (project)
      await refreshProject(project.id);
    setNotice(tr("app.creator.agent.cancelled"));
  });
  const resumeCodexAgent = () => agentRun && withBusy(tr("app.creator.agent.resuming"), async () => {
    const envelope = await agentReviewClient.resumeAgent(agentRun.id);
    if (!envelope.agentRun)
      throw new Error(tr("app.creator.agent.runMissing"));
    setAgentRun(envelope.agentRun);
    onReview();
    setNotice(tr("app.creator.agent.resumed"));
  });
  const handoffTask = agentHandoffTaskId ? project?.tasks.find((task) => task.id === agentHandoffTaskId) ?? null : null;
  const lockedHandoffIdentity = handoffTask?.lease?.worker && ["claimed", "running"].includes(handoffTask.status)
    ? handoffTask.lease.worker
    : null;
  const handoffIdentity = lockedHandoffIdentity ?? agentIdentity.trim();
  const handoffPayloadFile = handoffTask ? `siaocut-${handoffTask.id}-claim.json` : "";
  const handoffIdentityLocked = Boolean(lockedHandoffIdentity);
  const handoffLeaseArgument = handoffTask?.lease?.id && ["claimed", "running"].includes(handoffTask.status)
    ? ` --lease-id ${handoffTask.lease.id}`
    : "";
  const handoffText = handoffTask && isValidAgentIdentity(handoffIdentity) ? [
    tr("app.agent.handoff.prompt.title", { taskId: handoffTask.id }),
    tr("app.agent.handoff.prompt.context", { worker: handoffIdentity }),
    tr("app.agent.handoff.prompt.claim", { taskId: handoffTask.id, worker: handoffIdentity, payloadFile: handoffPayloadFile, leaseArgument: handoffLeaseArgument }),
    tr("app.agent.handoff.prompt.verify", { taskId: handoffTask.id }),
    tr("app.agent.handoff.prompt.heartbeat", { taskId: handoffTask.id, worker: handoffIdentity }),
    tr("app.agent.handoff.prompt.process"),
    tr("app.agent.handoff.prompt.submit", { taskId: handoffTask.id, worker: handoffIdentity }),
    tr("app.agent.handoff.prompt.review", { taskId: handoffTask.id }),
  ].join("\n\n") : "";
  const aiConfirmationSegments = project?.transcript.segments ?? [];
  const aiConfirmationCharacters = aiConfirmationSegments.reduce((total, segment) => total + Array.from(segment.text).length, 0);
  const aiConfirmationLabel = {
    polish: tr("app.workflow.polish"),
    proofread: tr("app.workflow.proofread"),
    edit: tr("app.workflow.edit"),
    translate: tr("app.workflow.translate"),
    punctuate: tr("app.workflow.punctuate"),
    speaker_names: tr("app.workflow.speakerNames"),
  }[agentWorkflowKind];
  const aiConfirmationContext = agentWorkflowKind === "translate"
    ? `翻译术语表 ${glossaryDraft.split(/\r?\n/).filter((line) => line.trim()).length} 条`
    : agentWorkflowKind === "speaker_names" ? "说话人文本证据（不含音频）" : null;
  const copyAgentHandoff = async () => {
    if (!handoffText) return;
    try {
      await navigator.clipboard.writeText(handoffText);
      setAgentHandoffCopied(true);
    }
    catch {
      setError(tr("app.agent.handoff.copyFailed"));
    }
  };
  const applyTaskSnapshot = (projectId: string, task: Task) => {
    const update = (current: Project) => current.id === projectId
      ? { ...current, tasks: current.tasks.some((item) => item.id === task.id) ? current.tasks.map((item) => item.id === task.id ? task : item) : [...current.tasks, task] }
      : current;
    setProject((current) => current && current.id === projectId ? update(current) : current);
  };
  const updateTask = async (taskId: string, action: "retry" | "cancel") => {
    if (!project || taskActionIdsRef.current.has(taskId))
      return;
    const projectId = project.id;
    reviewEpoch.current++;
    taskActionIdsRef.current.add(taskId);
    setTaskActions((current) => ({ ...current, [taskId]: action }));
    setError(null);
    invalidateProjectLoads(projectId);
    try {
      const envelope = await agentReviewClient.updateTask(taskId, action);
      const acceptedStatuses = action === "retry" ? ["queued", "claimed", "running"] : ["cancelled"];
      if (!envelope.task || envelope.task.id !== taskId || !acceptedStatuses.includes(envelope.task.status))
        throw new Error(tr("app.agent.task.actionInvalid"));
      applyTaskSnapshot(projectId, envelope.task);
      if (activeProjectIdRef.current === projectId) {
        if (action === "retry") {
          setNotice(envelope.task.status === "queued"
            ? tr("app.agent.task.requeued", { attempt: (envelope.task.attemptCount ?? 0) + 1 })
            : tr("app.agent.task.reclaimed", {
              attempt: Math.max(1, envelope.task.attemptCount ?? 1),
              worker: envelope.task.lease?.worker ?? tr("app.agent.task.unknownWorker"),
            }));
        }
        else {
          setNotice(tr("app.s0227"));
        }
      }
      // Core returned the complete task. Poll the review projection; do not reload the transcript.
    }
    catch (cause) {
      if (activeProjectIdRef.current === projectId)
        setError(cause instanceof Error ? cause.message : String(cause));
    }
    finally {
      taskActionIdsRef.current.delete(taskId);
      setTaskActions((current) => {
        if (!(taskId in current))
          return current;
        const next = { ...current };
        delete next[taskId];
        return next;
      });
    }
  };
  const reviewPatch = (patchItemId: string, action: "apply" | "keep") => project && withBusy(action === "apply" ? tr("app.s0228") : tr("app.s0229"), async () => {
    await editing.mutate(project.id, { kind: "review_patch", patchItemId, action });
    await refreshProject(project.id);
    setNotice(action === "apply" ? tr("app.s0230") : tr("app.s0231"));
  });
  const reviewAll = (taskId: string, action: "apply" | "keep") => project && withBusy(action === "apply" ? tr("app.s0232") : tr("app.s0233"), async () => {
    await editing.mutate(project.id, { kind: "review_all", taskId, action });
    await refreshProject(project.id);
    setNotice(action === "apply" ? tr("app.s0234") : tr("app.s0235"));
  });
  useAgentReviewPolling({ project, agentRun, setAgentRun, activeProjectIdRef, refreshProject, onReview, setNotice, setError, taskActionIdsRef, reviewEpoch, invalidateProjectLoads, setProject });
  return { agentWorkflowKind, setAgentWorkflowKind, codexHealth, setCodexHealth, agentRun, setAgentRun, showAgentHandoff, setShowAgentHandoff, showAiExecutionConfirm, setShowAiExecutionConfirm, aiApprovalTaskId, setAiApprovalTaskId, agentHandoffTaskId, setAgentHandoffTaskId, agentIdentity, setAgentIdentity, agentHandoffReady, setAgentHandoffReady, agentHandoffCopied, setAgentHandoffCopied, taskActions, setTaskActions, glossaryDraft, setGlossaryDraft, agentButtonRef, agentHandoffReturnFocusRef, taskActionIdsRef, openAgentHandoff, openExistingAgentHandoff, saveGlossary, createAgentTask, startAiAssistance, cancelCodexAgent, resumeCodexAgent, handoffTask, lockedHandoffIdentity, handoffIdentity, handoffIdentityLocked, handoffText, aiConfirmationSegments, aiConfirmationCharacters, aiConfirmationLabel, aiConfirmationContext, copyAgentHandoff, updateTask, reviewPatch, reviewAll };
}
