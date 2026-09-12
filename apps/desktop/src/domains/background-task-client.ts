import { runCoreStructured } from "../core";
import type { AiExecutionSelection } from "../features/ai-assistance/types";
import type { UiLocale } from "../i18n";
import type { SourceBrowser,TranscriptionLanguage,WorkflowProfile } from "../types";
import { desktopControl } from "./desktop-control-client";
import { desktopQuery } from "./desktop-query-client";

type AutoWorkflowInput =
  | { kind: "local"; mediaPath: string; title: string }
  | { kind: "url"; url: string; confirmedMediaId: string };

type StartAutoWorkflowOptions = {
  input: AutoWorkflowInput;
  modelPath: string;
  language: TranscriptionLanguage;
  locale: UiLocale;
  output: string;
  subtitleMode: "source" | "translated" | "bilingual";
  profile: WorkflowProfile;
  translationLanguage?: string;
  burnSubtitles: boolean;
  aiExecution?: AiExecutionSelection;
};

export type StartTranscriptionOptions = {
  mutationId: string;
  expectedVersionId: string;
  projectId: string;
  language: TranscriptionLanguage;
  prompt?: string;
  hotwords: string[];
};

export const backgroundTaskClient = {
  listModels: (verify = false) => desktopQuery({action:"models",verify}),
  listModelJobs: () => desktopQuery({action:"model_jobs"}),
  getModelJob: (jobId: string) => desktopQuery({action:"model_job",jobId}),
  installModel: (modelId: string) => desktopControl({ action: "model_install", modelId: modelId }),
  cancelModel: (jobId: string) => desktopControl({ action: "model_cancel", jobId: jobId }),
  removeModel: (modelId: string) => desktopControl({ action: "model_remove", modelId: modelId }),

  listSourceJobs: () => desktopQuery({action:"source_jobs"}),
  inspectSource: (url: string, browser?: SourceBrowser) => desktopControl({ action: "source_inspect", url: url, browser: browser ?? null }),
  startSourceImport: (url: string, confirmedMediaId: string, browser?: SourceBrowser) => desktopControl({ action: "source_start", url: url, confirmMediaId: confirmedMediaId, startDelayMs: null, browser: browser ?? null }),
  getSourceJob: (jobId: string) => desktopQuery({action:"source_job",jobId}),
  cancelSourceImport: (jobId: string) => desktopControl({ action: "source_cancel", jobId: jobId }),
  resumeSourceImport: (jobId: string) => desktopControl({ action: "source_resume", jobId: jobId }),

  listAutoWorkflows: () => desktopQuery({action:"auto_workflows"}),
  startAutoWorkflow: (options: StartAutoWorkflowOptions) => desktopControl({action: "auto_start", options: {
    profile: options.profile,
    media: options.input.kind === "local" ? options.input.mediaPath : null,
    title: options.input.kind === "local" ? options.input.title : null,
    url: options.input.kind === "url" ? options.input.url : null,
    confirmMediaId: options.input.kind === "url" ? options.input.confirmedMediaId : null,
    model: options.modelPath, language: options.language, locale: options.locale,
    translate: options.translationLanguage ?? null,
    aiExecution: options.aiExecution?.kind === "api" ? "api" : options.aiExecution?.kind === "codex" ? "codex" : "manual",
    aiServiceConfigId: options.aiExecution?.kind === "api" ? options.aiExecution.serviceConfigId : null,
    aiServiceRevision: options.aiExecution?.kind === "api" ? options.aiExecution.serviceRevision : null,
    aiNetworkRevision: options.aiExecution?.kind === "api" ? options.aiExecution.networkRevision : null,
    aiModelId: options.aiExecution?.kind === "api" ? options.aiExecution.modelId : null,
    confirmAiTextSend: Boolean(options.aiExecution && options.aiExecution.kind !== "copy_prompt"),
    output: options.output, burnSubtitles: options.burnSubtitles, subtitleMode: options.subtitleMode, startDelayMs: null,
  }}),
  getAutoWorkflow: (workflowId: string) => desktopQuery({action:"auto_workflow",workflowId}),
  cancelAutoWorkflow: (workflowId: string) => desktopControl({action:"auto_cancel",workflowId}),
  continueAutoWorkflow: (workflowId: string) => desktopControl({action:"auto_continue",workflowId}),

  latestAudioAnalysis: (projectId: string) => desktopQuery({action:"audio_latest",projectId}),
  getAudioAnalysis: (jobId: string) => desktopQuery({action:"audio_job",jobId}),
  startAudioAnalysis: (projectId: string) => desktopControl({ action: "audio_start", projectId: projectId, startDelayMs: null }),
  cancelAudioAnalysis: (jobId: string) => desktopControl({ action: "audio_cancel", jobId: jobId }),
  resumeAudioAnalysis: (jobId: string) => desktopControl({ action: "audio_resume", jobId: jobId, startDelayMs: null }),

  getSpeakerPackage: () => desktopQuery({action:"speaker_package",verify:true}),
  listSpeakerJobs: () => desktopQuery({action:"speaker_jobs"}),
  getSpeakerJob: (jobId: string) => desktopQuery({action:"speaker_job",jobId}),
  installSpeakerPackage: () => desktopControl({ action: "speaker_install" }),
  startSpeakerAnalysis: (projectId: string) => desktopControl({ action: "speaker_analyze", projectId: projectId }),
  cancelSpeakerJob: (jobId: string) => desktopControl({ action: "speaker_cancel", jobId: jobId }),
  resumeSpeakerJob: (jobId: string) => desktopControl({ action: "speaker_resume", jobId: jobId }),

  startWhisper: (projectId: string, modelPath: string, language: string, expectedVersionId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "start", projectId, modelPath, language, expectedVersionId, mutationId } }),
  previewTranscription: (jobId: string, offset = 0) => runCoreStructured({ kind: "transcription_job", request: { action: "preview", jobId, offset } }),
  listTranscriptions: () => runCoreStructured({ kind: "transcription_job", request: { action: "list", projectId: null } }),
  getTranscriptionHealth: () => desktopQuery({action:"transcription_health"}),
  latestTranscription: (projectId: string) => desktopQuery({action:"latest_transcription",projectId}),
  listTranscriptionReviews: (projectId: string) => desktopQuery({action:"transcription_reviews",projectId}),
  getTranscriptionJob: (jobId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "get", jobId } }),
  startTranscription: (options: StartTranscriptionOptions) => runCoreStructured({kind: "transcription_job", request: {action: "start_multispeaker", ...options, prompt: options.prompt ?? null}}),
  configureTranscription: (endpoint: string, modelId: string) => desktopControl({ action: "transcription_configure", endpoint: endpoint, model: modelId }),
  cancelTranscription: (jobId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "cancel", jobId } }),
  resumeTranscription: (jobId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "retry", jobId, mutationId } }),
  applyTranscription: (jobId: string, expectedVersionId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "apply", jobId, expectedVersionId, mutationId } }),
  discardTranscription: (jobId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "discard", jobId, mutationId } }),
};
