import { runCore } from "../core";
import type { UiLocale } from "../i18n";
import type { TranscriptionLanguage } from "../types";

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
  translationLanguage?: string;
  burnSubtitles: boolean;
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
  inspectSource: (url: string) => runCore(["source", "inspect", url]),
  startSourceImport: (url: string, confirmedMediaId: string) => runCore(["source", "start", url, "--confirm-media-id", confirmedMediaId]),
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
      ...(options.translationLanguage ? ["--translate", options.translationLanguage] : []),
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

  getTranscriptionHealth: () => runCore(["transcription", "health"]),
  latestTranscription: (projectId: string) => runCore(["transcription", "latest", projectId]),
  listTranscriptionReviews: (projectId: string) => runCore(["transcription", "review", projectId]),
  getTranscriptionJob: (jobId: string) => runCore(["transcription", "status", jobId]),
  startTranscription: (options: StartTranscriptionOptions) => runCore([
    "transcription", "start", options.projectId,
    "--language", options.language,
    ...(options.prompt ? ["--prompt", options.prompt] : []),
    ...options.hotwords.flatMap((hotword) => ["--hotword", hotword]),
  ]),
  configureTranscription: (endpoint: string, modelId: string) => runCore(["transcription", "configure", "--endpoint", endpoint, "--model", modelId]),
  cancelTranscription: (jobId: string) => runCore(["transcription", "cancel", jobId]),
  resumeTranscription: (jobId: string) => runCore(["transcription", "resume", jobId]),
  applyTranscription: (jobId: string, expectedVersionId: string) => runCore(["transcription", "apply", jobId, "--expected-version", expectedVersionId, "--confirm-replace"]),
  discardTranscription: (jobId: string) => runCore(["transcription", "discard", jobId]),
  resolveTranscriptionReview: (itemId: string, action: "resolved" | "ignored") => runCore(["transcription", "resolve", itemId, "--action", action]),
};
