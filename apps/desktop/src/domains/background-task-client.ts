import { desktopQuery } from "./desktop-query-client";
import { runCore, runCoreStructured } from "../core";
import type { UiLocale } from "../i18n";
import type { SourceBrowser, TranscriptionLanguage, WorkflowProfile } from "../types";
import type { AiExecutionSelection } from "../features/ai-assistance/types";

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

type StartTranscriptionOptions = {
  projectId: string;
  language: TranscriptionLanguage;
  prompt?: string;
  hotwords: string[];
};

export const backgroundTaskClient = {
  listModels: (verify = false) => desktopQuery({action:"models",verify}),
  listModelJobs: () => desktopQuery({action:"model_jobs"}),
  getModelJob: (jobId: string) => desktopQuery({action:"model_job",jobId}),
  installModel: (modelId: string) => runCore(["model", "install", modelId]),
  cancelModel: (jobId: string) => runCore(["model", "cancel", jobId]),
  removeModel: (modelId: string) => runCore(["model", "remove", modelId]),

  listSourceJobs: () => desktopQuery({action:"source_jobs"}),
  inspectSource: (url: string, browser?: SourceBrowser) => runCore(["source", "inspect", url, ...(browser ? ["--browser", browser] : [])]),
  startSourceImport: (url: string, confirmedMediaId: string, browser?: SourceBrowser) => runCore(["source", "start", url, "--confirm-media-id", confirmedMediaId, ...(browser ? ["--browser", browser] : [])]),
  getSourceJob: (jobId: string) => desktopQuery({action:"source_job",jobId}),
  cancelSourceImport: (jobId: string) => runCore(["source", "cancel", jobId]),
  resumeSourceImport: (jobId: string) => runCore(["source", "resume", jobId]),

  listAutoWorkflows: () => desktopQuery({action:"auto_workflows"}),
  startAutoWorkflow: (options: StartAutoWorkflowOptions) => {
    const inputArgs = options.input.kind === "local"
      ? ["--media", options.input.mediaPath, "--title", options.input.title]
      : ["--url", options.input.url, "--confirm-media-id", options.input.confirmedMediaId];
    return runCore([
      "auto", "start", ...inputArgs,
      "--model", options.modelPath,
      "--language", options.language,
      "--locale", options.locale,
      "--output", options.output,
      "--subtitle-mode", options.subtitleMode,
      "--profile", options.profile,
      ...(options.translationLanguage ? ["--translate", options.translationLanguage] : []),
      ...(options.aiExecution && options.aiExecution.kind !== "copy_prompt" ? [
        "--ai-execution", options.aiExecution.kind,
        ...(options.aiExecution.kind === "api" ? [
          "--ai-service-config-id", options.aiExecution.serviceConfigId,
          "--ai-service-revision", String(options.aiExecution.serviceRevision),
          "--ai-network-revision", String(options.aiExecution.networkRevision),
          "--ai-model-id", options.aiExecution.modelId,
        ] : []),
        "--confirm-ai-text-send",
      ] : []),
      ...(options.burnSubtitles ? ["--burn-subtitles"] : []),
    ]);
  },
  getAutoWorkflow: (workflowId: string) => desktopQuery({action:"auto_workflow",workflowId}),
  cancelAutoWorkflow: (workflowId: string) => runCore(["auto", "cancel", workflowId]),
  continueAutoWorkflow: (workflowId: string) => runCore(["auto", "continue", workflowId]),

  latestAudioAnalysis: (projectId: string) => desktopQuery({action:"audio_latest",projectId}),
  getAudioAnalysis: (jobId: string) => desktopQuery({action:"audio_job",jobId}),
  startAudioAnalysis: (projectId: string) => runCore(["speech", "audio-start", projectId]),
  cancelAudioAnalysis: (jobId: string) => runCore(["speech", "audio-cancel", jobId]),
  resumeAudioAnalysis: (jobId: string) => runCore(["speech", "audio-resume", jobId]),

  getSpeakerPackage: () => desktopQuery({action:"speaker_package",verify:true}),
  listSpeakerJobs: () => desktopQuery({action:"speaker_jobs"}),
  getSpeakerJob: (jobId: string) => desktopQuery({action:"speaker_job",jobId}),
  installSpeakerPackage: () => runCore(["speaker", "install"]),
  startSpeakerAnalysis: (projectId: string) => runCore(["speaker", "analyze", projectId]),
  cancelSpeakerJob: (jobId: string) => runCore(["speaker", "cancel", jobId]),
  resumeSpeakerJob: (jobId: string) => runCore(["speaker", "resume", jobId]),

  startWhisper: (projectId: string, modelPath: string, language: string, expectedVersionId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "start", projectId, modelPath, language, expectedVersionId, mutationId } }),
  previewTranscription: (jobId: string, offset = 0) => runCoreStructured({ kind: "transcription_job", request: { action: "preview", jobId, offset } }),
  listTranscriptions: () => runCoreStructured({ kind: "transcription_job", request: { action: "list", projectId: null } }),
  getTranscriptionHealth: () => desktopQuery({action:"transcription_health"}),
  latestTranscription: (projectId: string) => desktopQuery({action:"latest_transcription",projectId}),
  listTranscriptionReviews: (projectId: string) => desktopQuery({action:"transcription_reviews",projectId}),
  getTranscriptionJob: (jobId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "get", jobId } }),
  startTranscription: (options: StartTranscriptionOptions) => runCoreStructured({
    kind: "transcription_start",
    projectId: options.projectId,
    language: options.language,
    prompt: options.prompt,
    hotwords: options.hotwords,
  }),
  configureTranscription: (endpoint: string, modelId: string) => runCore(["transcription", "configure", "--endpoint", endpoint, "--model", modelId]),
  cancelTranscription: (jobId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "cancel", jobId } }),
  resumeTranscription: (jobId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "retry", jobId, mutationId } }),
  applyTranscription: (jobId: string, expectedVersionId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "apply", jobId, expectedVersionId, mutationId } }),
  discardTranscription: (jobId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "discard", jobId, mutationId } }),
  resolveTranscriptionReview: (itemId: string, action: "resolved" | "ignored") => runCore(["transcription", "resolve", itemId, "--action", action]),
};
