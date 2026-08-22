import { tr } from "../i18n";
import type { AgentRun, AudioAnalysisJob, AutoWorkflow, ExportJob, SourceImportJob, TranscriptionJob } from "../types";

export type WorkbenchActivityKind = "local" | "source" | "transcription" | "agent" | "audio" | "export" | "auto";
export type WorkbenchActivityState = "action_required" | "failed" | "running" | "ready";

export type WorkbenchActivity = {
  id: string;
  kind: WorkbenchActivityKind;
  state: WorkbenchActivityState;
  status: string;
  stage: string | null;
  detail: string | null;
  progress: number | null;
  updatedAt: string;
  errorMessage: string | null;
  errorCode?: string | null;
};

export type WorkbenchActivityInputs = {
  busyMessage?: string | null;
  sourceJob?: SourceImportJob | null;
  transcriptionJob?: TranscriptionJob | null;
  agentRun?: AgentRun | null;
  audioAnalysisJob?: AudioAnalysisJob | null;
  exportJob?: ExportJob | null;
  autoWorkflows?: AutoWorkflow[];
  autoWorkflowErrors?: Record<string, string>;
};

const ACTIVE = new Set(["queued", "claimed", "running", "submitting", "finalizing"]);
const FAILED = new Set(["failed", "interrupted", "cancelled", "canceled"]);
const PRIORITY: Record<WorkbenchActivityState, number> = {
  action_required: 4,
  failed: 3,
  running: 2,
  ready: 1,
};

function stateFor(status: string, actionRequired: string[] = []): WorkbenchActivityState | null {
  if (actionRequired.includes(status))
    return "action_required";
  if (FAILED.has(status))
    return "failed";
  if (ACTIVE.has(status))
    return "running";
  return null;
}

function jobActivity(
  kind: WorkbenchActivityKind,
  job: { id: string; status: string; updatedAt: string; progress?: number; stageCode?: string | null; errorMessage?: string | null; errorCode?: string | null },
  detail: string | null,
  actionRequired: string[] = [],
  stage?: string | null,
  ready: string[] = [],
): WorkbenchActivity | null {
  const state = ready.includes(job.status) ? "ready" : stateFor(job.status, actionRequired);
  if (!state)
    return null;
  return {
    id: `${kind}:${job.id}`,
    kind,
    state,
    status: job.status,
    stage: stage ?? job.stageCode ?? null,
    detail,
    progress: typeof job.progress === "number" ? job.progress : null,
    updatedAt: job.updatedAt,
    errorMessage: job.errorMessage ?? null,
    errorCode: job.errorCode,
  };
}

export function deriveWorkbenchActivities(inputs: WorkbenchActivityInputs): WorkbenchActivity[] {
  const workflows = inputs.autoWorkflows ?? [];
  const childSourceIds = new Set(workflows.map((workflow) => workflow.sourceImportId).filter(Boolean));
  const childAgentTaskIds = new Set(workflows.map((workflow) => workflow.agentTaskId).filter(Boolean));
  const childExportIds = new Set(workflows.map((workflow) => workflow.exportJobId).filter(Boolean));
  const activities: Array<WorkbenchActivity | null> = [];

  if (inputs.busyMessage) {
    activities.push({
      id: "local:busy",
      kind: "local",
      state: "running",
      status: "running",
      stage: null,
      detail: inputs.busyMessage,
      progress: null,
      updatedAt: "9999-12-31T23:59:59.999Z",
      errorMessage: null,
    });
  }
  if (inputs.sourceJob && !childSourceIds.has(inputs.sourceJob.id))
    activities.push(jobActivity("source", inputs.sourceJob, inputs.sourceJob.title));
  if (inputs.transcriptionJob) {
    const candidate = inputs.transcriptionJob.candidate;
    const detail = candidate
      ? tr("app.moss.candidate.summary", { segments: candidate.segmentCount, speakers: candidate.speakerCount, warnings: candidate.warningCount })
      : inputs.transcriptionJob.modelId;
    activities.push(jobActivity("transcription", inputs.transcriptionJob, detail, ["awaiting_apply"], inputs.transcriptionJob.stage));
  }
  if (inputs.agentRun && !childAgentTaskIds.has(inputs.agentRun.taskId))
    activities.push(jobActivity("agent", inputs.agentRun, inputs.agentRun.modelId ?? inputs.agentRun.provider, [], null));
  if (inputs.audioAnalysisJob)
    activities.push(jobActivity("audio", inputs.audioAnalysisJob, null));
  if (inputs.exportJob && !childExportIds.has(inputs.exportJob.id))
    activities.push(jobActivity("export", inputs.exportJob, inputs.exportJob.outputPath));
  workflows.forEach((workflow) => {
    const activity = jobActivity(
      "auto",
      workflow,
      workflow.title ?? workflow.outputPath,
      ["needs_agent", "needs_review"],
      workflow.currentStage,
      ["completed"],
    );
    if (activity && inputs.autoWorkflowErrors?.[workflow.id])
      activity.errorMessage = inputs.autoWorkflowErrors[workflow.id];
    activities.push(activity);
  });

  return activities
    .filter((activity): activity is WorkbenchActivity => Boolean(activity))
    .sort((left, right) => PRIORITY[right.state] - PRIORITY[left.state]
      || right.updatedAt.localeCompare(left.updatedAt)
      || left.id.localeCompare(right.id));
}

export function transcriptionStageLabel(stage: string | null) {
  if (!stage)
    return tr("app.activity.stageUnknown");
  return ({
    queued: tr("app.moss.stage.queued"),
    preparing_audio: tr("app.moss.stage.preparing_audio"),
    requesting_model: tr("app.moss.stage.requesting_model"),
    validating_result: tr("app.moss.stage.validating_result"),
    awaiting_apply: tr("app.moss.stage.awaitingApply"),
    completed: tr("app.moss.stage.completed"),
    failed: tr("app.moss.stage.failed"),
    interrupted: tr("app.moss.stage.interrupted"),
    cancelled: tr("app.moss.stage.cancelled"),
    discarded: tr("app.moss.stage.discarded"),
  }[stage] ?? stage);
}
