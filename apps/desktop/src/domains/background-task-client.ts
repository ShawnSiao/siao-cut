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
  listModels: (verify = false) => runCore(["model", "list", ...(verify ? ["--verify"] : [])]),
  listModelJobs: () => runCore(["model", "jobs"]),
  getModelJob: (jobId: string) => runCore(["model", "status", jobId]),
  installModel: (modelId: string) => runCore(["model", "install", modelId]),
  cancelModel: (jobId: string) => runCore(["model", "cancel", jobId]),
  removeModel: (modelId: string) => runCore(["model", "remove", modelId]),

  listSourceJobs: () => runCore(["source", "jobs"]),
  inspectSource: (url: string, browser?: SourceBrowser) => runCore(["source", "inspect", url, ...(browser ? ["--browser", browser] : [])]),
  startSourceImport: (url: string, confirmedMediaId: string, browser?: SourceBrowser) => runCore(["source", "start", url, "--confirm-media-id", confirmedMediaId, ...(browser ? ["--browser", browser] : [])]),
  getSourceJob: (jobId: string) => runCore(["source", "status", jobId]),
  cancelSourceImport: (jobId: string) => runCore(["source", "cancel", jobId]),
  resumeSourceImport: (jobId: string) => runCore(["source", "resume", jobId]),

  listAutoWorkflows: () => runCore(["auto", "list"]),
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
  getAutoWorkflow: (workflowId: string) => runCore(["auto", "status", workflowId]),
  cancelAutoWorkflow: (workflowId: string) => runCore(["auto", "cancel", workflowId]),
  continueAutoWorkflow: (workflowId: string) => runCore(["auto", "continue", workflowId]),

  latestAudioAnalysis: (projectId: string) => runCore(["speech", "audio-latest", projectId]),
  getAudioAnalysis: (jobId: string) => runCore(["speech", "audio-status", jobId]),
  startAudioAnalysis: (projectId: string) => runCore(["speech", "audio-start", projectId]),
  cancelAudioAnalysis: (jobId: string) => runCore(["speech", "audio-cancel", jobId]),
  resumeAudioAnalysis: (jobId: string) => runCore(["speech", "audio-resume", jobId]),

  getSpeakerPackage: () => runCore(["speaker", "package", "--verify"]),
  listSpeakerJobs: () => runCore(["speaker", "jobs"]),
  getSpeakerJob: (jobId: string) => runCore(["speaker", "job-status", jobId]),
  installSpeakerPackage: () => runCore(["speaker", "install"]),
  startSpeakerAnalysis: (projectId: string) => runCore(["speaker", "analyze", projectId]),
  cancelSpeakerJob: (jobId: string) => runCore(["speaker", "cancel", jobId]),
  resumeSpeakerJob: (jobId: string) => runCore(["speaker", "resume", jobId]),

  startWhisper: (projectId: string, modelPath: string, language: string, expectedVersionId: string, mutationId: string) => runCoreStructured({ kind: "transcription_job", request: { action: "start", projectId, modelPath, language, expectedVersionId, mutationId } }),
  previewTranscription: (jobId: string, offset = 0) => runCoreStructured({ kind: "transcription_job", request: { action: "preview", jobId, offset } }),
  listTranscriptions: () => runCoreStructured({ kind: "transcription_job", request: { action: "list", projectId: null } }),
  getTranscriptionHealth: () => runCore(["transcription", "health"]),
  latestTranscription: (projectId: string) => runCore(["transcription", "latest", projectId]),
  listTranscriptionReviews: (projectId: string) => runCore(["transcription", "review", projectId]),
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
