import type * as Wire from "./generated/core-contract";
import type { AgentRunStatus, AutoWorkflowStage, AutoWorkflowStatus, BackgroundJobStatus, CoreErrorCode, LocalResourceState, TaskStatus, TranscriptionJobStage, TranscriptionJobStatus, WorkflowStatus } from "./generated/core-contract";
export type { AgentRunStatus, AutoWorkflowStage, AutoWorkflowStatus, BackgroundJobStatus, CoreErrorCode, KnownCoreErrorCode, LocalResourceState, TaskStatus, TranscriptionJobStage, TranscriptionJobStatus, WorkflowProfile, WorkflowStatus } from "./generated/core-contract";

export type Segment = import("./generated/core-contract").CoreSegment;

export type CodexHealth = {
  available: boolean;
  authenticated: boolean;
  version: string | null;
  authMode: string | null;
};

export type AgentRunBatch = Omit<Wire.CoreAgentRunBatch,"status"> & {status:AgentRunStatus};

export type AgentRun = Omit<Wire.CoreAgentRun,"status" | "executionKind" | "batches"> & {status:AgentRunStatus;executionKind:"codex" | "api";batches:AgentRunBatch[]};

export type WordTiming = import("./generated/core-contract").CoreWord;

export type SpeechPause = import("./generated/core-contract").CoreSpeechPause;

export type SpeechEvidence = import("./generated/core-contract").CoreSpeechEvidence;

export type SpeechInsights = import("./generated/core-contract").CoreSpeechInsights;

export type Translation = import("./generated/core-contract").CoreTranslation;

export type Glossary = import("./generated/core-contract").CoreGlossary;

export type UiLocale = "zh-CN" | "en-US";
export type TranscriptionLanguage = "auto" | "en" | "zh";

export type Task = Omit<Wire.CoreTask,"status" | "instructionLocale" | LegacyTaskFields> & Partial<Pick<Wire.CoreTask,LegacyTaskFields>> & {status:TaskStatus;instructionLocale:UiLocale;stageCode?:string|null};

export type AgentPatchItem = import("./generated/core-contract").CoreAgentPatchItem;

export type AgentPatchSet = import("./generated/core-contract").CoreAgentPatchSet;

export type Workflow = Omit<Wire.CoreWorkflow,"status" | "instructionLocale"> & {status:WorkflowStatus;instructionLocale:UiLocale};

export type Version = import("./generated/core-contract").Version;

export type Edit = Omit<Wire.CoreEdit,"createdAt" | "cutRange" | "suggestion"> & Partial<Pick<Wire.CoreEdit,"createdAt" | "cutRange" | "suggestion">>;

export type CutPreview = {
  cutId: string;
  previewStart: number;
  cutStart: number;
  cutEnd: number;
  previewEnd: number;
  skipRange: boolean;
};

export type TimelineMap = import("./generated/core-contract").CoreTimelineMap;

export type MediaArtifacts = import("./generated/core-contract").CoreMediaArtifacts;

export type ExportJob = Omit<Wire.CoreExportJob,"status" | "stageCode" | "errorCode" | "workerPid"> & Partial<Pick<Wire.CoreExportJob,"stageCode" | "errorCode" | "workerPid">> & {status:BackgroundJobStatus};

export type CanvasSettings = import("./generated/core-contract").CoreCanvasSettings;

export type SubtitleStylePreset = import("./generated/core-contract").CoreSubtitleStylePreset;
export type SubtitlePosition = import("./generated/core-contract").CoreSubtitlePosition;

export type SubtitleStyle = import("./generated/core-contract").CoreSubtitleStyle;

export type SubtitleStylePresetOption = {
  id: SubtitleStylePreset;
  label: string;
  description: string;
};

export type ModelStatus = {
  id: string;
  name: string;
  fileName: string;
  description: string;
  source: string;
  url: string;
  size: number;
  sha256: string;
  license: string;
  recommended: boolean;
  path: string;
  installed: boolean;
  bytesOnDisk: number;
  verified: boolean | null;
  verificationStatus: "verified" | "failed" | "not_checked" | "not_installed";
};

export type ModelDownloadJob = {
  id: string;
  modelId: string;
  status: BackgroundJobStatus;
  stageCode?: string | null;
  progress: number;
  bytesDownloaded: number;
  totalBytes: number;
  targetPath: string;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  errorCode?: CoreErrorCode | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
};

export type LocalCapabilityId = "basic_media" | "url_import" | "local_transcription" | "speaker_identity";
export type LocalTranscriptionProfile = "fast" | "standard" | "quality";

export type LocalCapabilityStatus = {
  id: LocalCapabilityId;
  name: string;
  state: LocalResourceState;
  enabled: boolean;
  canRollback?: boolean;
};

export type LocalResourceStatus = {
  configured: boolean;
  root: string | null;
  rootAvailable: boolean;
  writable: boolean;
  availableBytes: number | null;
  transcriptionProfile: LocalTranscriptionProfile;
  capabilities: LocalCapabilityStatus[];
  needsSetup: boolean;
};

export type ResourceUpdateCheck = {
  checkedAt: string;
  capabilities: Array<{
    capabilityId: LocalCapabilityId;
    state: "current" | LocalResourceState;
  }>;
};

export type LocalResourcePlan = {
  capabilityId: LocalCapabilityId;
  capabilityName: string;
  transcriptionProfile: LocalTranscriptionProfile | null;
  downloadBytes: number;
  unknownSize: boolean;
};

export type LocalResourceJob = {
  id: string;
  capabilityId: LocalCapabilityId;
  transcriptionProfile?: LocalTranscriptionProfile | null;
  status: BackgroundJobStatus;
  stage: string;
  progress: number;
  bytesDownloaded: number;
  totalBytes: number;
  targetRoot: string;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  errorCode?: CoreErrorCode | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
  attemptCount: number;
};

export type SourcePreview = {
  originalUrl: string;
  webpageUrl: string;
  siteMediaId: string;
  extractor: string;
  title: string;
  durationSeconds: number;
  fileSizeBytes: number | null;
  fileSizeKnown: boolean;
  thumbnailUrl: string | null;
  toolVersion: string;
  toolSha256: string;
  authMode: "anonymous" | "browser";
  browser: SourceBrowser | null;
  requiresConfirmation: boolean;
};

export type SourceBrowser = "chrome" | "edge" | "firefox";

export type SourceImportJob = {
  id: string;
  projectId: string | null;
  originalUrl: string;
  webpageUrl: string;
  siteMediaId: string;
  extractor: string;
  title: string;
  durationSeconds: number;
  fileSizeBytes: number | null;
  status: BackgroundJobStatus;
  stageCode?: string | null;
  progress: number;
  bytesDownloaded: number;
  totalBytes: number | null;
  outputDirectory: string;
  outputPath: string | null;
  outputSha256: string | null;
  toolVersion: string;
  toolSha256: string;
  authMode: "anonymous" | "browser";
  browser: SourceBrowser | null;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  errorCode?: CoreErrorCode | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
  attemptCount: number;
};

export type AudioRisk = {
  kind: "silence" | "suspected_clipping" | "loudness_low" | "loudness_high";
  start: number;
  end: number;
  measuredValue: number;
  threshold: number;
  unit: "seconds" | "LUFS" | "dBFS" | string;
  toolVersion: string;
};

export type AudioAnalysisReport = {
  analyzerVersion: string;
  toolVersion: string;
  durationSeconds: number;
  integratedLoudnessLufs: number | null;
  truePeakDbfs: number | null;
  silenceDurationSeconds: number;
  thresholds: {
    silenceNoiseDb: number;
    silenceMinSeconds: number;
    clippingPeakDbfs: number;
    quietLoudnessLufs: number;
    loudLoudnessLufs: number;
  };
  risks: AudioRisk[];
};

export type AudioAnalysisJob = {
  id: string;
  projectId: string;
  status: BackgroundJobStatus;
  stageCode?: string | null;
  progress: number;
  report: AudioAnalysisReport | null;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  errorCode?: CoreErrorCode | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
  attemptCount: number;
};

export type SpeakerAssetStatus = {
  id: string;
  name: string;
  source: string;
  license: string;
  size: number;
  sha256: string;
  installed: boolean;
  verified: boolean | null;
  verificationStatus: "verified" | "failed" | "not_checked" | "not_installed";
};

export type SpeakerPackageStatus = {
  id: string;
  name: string;
  runtimeVersion: string;
  description: string;
  source: string;
  license: string;
  downloadSize: number;
  installedSize: number;
  installed: boolean;
  verified: boolean | null;
  verificationStatus: "verified" | "failed" | "not_checked" | "not_installed";
  assets: SpeakerAssetStatus[];
};

export type SpeakerIdentity = {
  id: string;
  sourceLabel: string;
  label: string;
  colorIndex: number;
  createdAt: string;
};

export type SpeakerTurn = {
  id: string;
  speakerId: string;
  start: number;
  end: number;
  confidence: number | null;
  source: string;
  modelVersion: string;
  createdAt: string;
};

export type SegmentSpeaker = {
  segmentId: string;
  speakerId: string;
  source: "overlap" | "manual" | string;
  confidence: number | null;
  updatedAt: string;
};

export type SpeakerTrack = {
  status: "not_analyzed" | "ready" | "no_speech" | string;
  runtimeVersion: string;
  segmentationModel: string;
  embeddingModel: string;
  providerId: string;
  modelId: string;
  sourceKind: "cascade" | "end_to_end" | string;
  generatedAt: string | null;
  speakers: SpeakerIdentity[];
  turns: SpeakerTurn[];
  associations: SegmentSpeaker[];
};

export type SpeakerJob = {
  id: string;
  kind: "install" | "analyze";
  projectId: string | null;
  status: BackgroundJobStatus;
  stage: string;
  stageCode?: string | null;
  progress: number;
  bytesDownloaded: number;
  totalBytes: number;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  errorCode?: CoreErrorCode | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
  attemptCount: number;
};

export type AutoWorkflow = Omit<Wire.CoreAutoWorkflow,"status" | "currentStage" | "inputKind" | "aiExecutionKind" | "audit" | "instructionLocale" | "errorCode" | "workerPid"> & Partial<Pick<Wire.CoreAutoWorkflow,"errorCode" | "workerPid">> & {status:AutoWorkflowStatus;currentStage:AutoWorkflowStage;inputKind:"local"|"url";aiExecutionKind:"codex"|"api"|null;audit:Record<string,unknown>|null;instructionLocale:UiLocale;stageCode?:string|null};

export type TranscriptionProviderConfig = {
  providerId: "moss_openai" | string;
  endpoint: string;
  modelId: string;
  updatedAt: string;
};

export type TranscriptionProviderHealth = {
  providerId: string;
  endpoint: string;
  modelId: string;
  state: "healthy" | "unavailable";
  detail: string;
  checkedAt: string;
};

export type TranscriptionJob = {
  id: string;
  projectId: string;
  providerId: string;
  endpoint: string;
  modelId: string;
  language: string | null;
  prompt: string | null;
  hotwords: string[];
  status: TranscriptionJobStatus;
  stage: TranscriptionJobStage | string;
  resultRunId: string | null;
  baseVersionId: string | null;
  sourceSha256: string | null;
  inputAudioSha256: string | null;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  errorCode?: CoreErrorCode | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
  attemptCount: number;
  candidate: TranscriptionCandidateSummary | null;
};

export type TranscriptionCandidateSummary = {
  runId: string;
  segmentCount: number;
  speakerCount: number;
  durationSeconds: number | null;
  warningCount: number;
  baseVersionId: string | null;
  currentVersionId: string | null;
  canApply: boolean;
};

export type ProjectDeletionPreflight = {
  projectId: string;
  expectedVersionId: string;
  deletable: boolean;
  blockers: Array<{ kind: string; id: string; status: string }>;
};

export type TranscriptionReviewItem = {
  id: string;
  projectId: string;
  runId: string;
  segmentId: string | null;
  severity: "info" | "warning" | "error";
  kind: "missing_punctuation" | "rapid_speaker_switch" | "short_fragment" | string;
  message: string;
  status: "open" | "resolved" | "ignored";
  createdAt: string;
  resolvedAt: string | null;
};

export type AutoWorkflowEvent = Omit<Wire.CoreAutoWorkflowEvent,"stage" | "status"> & {stage:AutoWorkflowStage;status:AutoWorkflowStatus};

/** Rust wire shape; legacy preview fixtures may omit media hashes and optional task metadata. */
export type Project = Omit<import("./generated/core-contract").CoreProject,"media" | "tasks" | "workflows" | "edits"> & {
  media: Omit<import("./generated/core-contract").CoreMedia,"sha256"> & {sha256?:string};
  tasks:Task[];workflows:Workflow[];edits:Edit[];
};

export type SubtitleIssueKind = import("./generated/core-contract").CoreSubtitleIssueKind;

export type SubtitleQualityIssue = import("./generated/core-contract").CoreSubtitleQualityIssue;

export type SubtitleQualityReport = import("./generated/core-contract").CoreSubtitleQualityReport;

export type SubtitleImportPreview = {
  format: "srt" | "vtt" | "ass";
  sourcePath: string;
  sha256: string;
  expectedVersionId: string;
  segmentCount: number;
  segments: Segment[];
  quality: SubtitleQualityReport;
  canImport: boolean;
  requiresConfirmation: boolean;
};

export type SubtitleStructureEdit = {
  operation: "split" | "merge" | "timing" | "offset";
  affectedSegmentIds: string[];
  createdSegmentId: string | null;
  removedSegmentIds: string[];
  impact: {
    translationsMarkedStale: number;
    translationSegmentsRemoved: number;
    wordsReassigned: number;
    wordsRemoved: number;
    wordsShifted: number;
    editsRestored: number;
    wordCutsInvalidated: number;
    agentPatchItemsRebased: number;
    speakerAssociationsCopied: number;
    speakerAssociationsRemoved: number;
  };
  project: Project;
};

export type TimingValidation = {
  status: "verified";
  timeDomain: "original_media";
  mode: "whisper_no_vad";
  vadUsed: false;
  segmentCount: number;
  wordCount: number;
};

export type TranscriptReplacementPreflight = {
  canReplace: boolean;
  currentVersionId: string;
  blockers: {
    edits: number;
    patchItems: number;
    taskSegments: number;
  };
};

export type CoreEnvelope = {
  tasks?: Task[];
  patchSets?: AgentPatchSet[];
  projectWorkflows?: Workflow[];
  projectPage?: import("./generated/core-contract").ProjectPage;
  history?: import("./generated/core-contract").HistoryState;
  versions?: import("./generated/core-contract").Version[];

  versionId?: string | null;
  mutationId?: string;
  editReceipt?: import("./generated/core-contract").EditReceipt;
  aiSendPreview?: import("./generated/core-contract").AiSendPreview;
  drafts?: import("./generated/core-contract").Draft[];
  apiVersion: string;
  status: "ok" | "error";
  error?: { code: CoreErrorCode; message: string; technicalDetails?: string | null };
  code?: CoreErrorCode;
  message?: string;
  taskId?: string;
  task?: Task | null;
  agentRunId?: string;
  codex?: CodexHealth;
  agentRun?: AgentRun;
  agentRuns?: AgentRun[];
  project?: Project;
  projects?: Project[];
  job?: ExportJob;
  jobs?: ExportJob[];
  models?: ModelStatus[];
  model?: ModelStatus;
  modelJob?: ModelDownloadJob;
  modelJobs?: ModelDownloadJob[];
  localResources?: LocalResourceStatus;
  resourcePlan?: LocalResourcePlan;
  resourceJob?: LocalResourceJob;
  resourceJobs?: LocalResourceJob[];
  resourceUpdateCheck?: ResourceUpdateCheck;
  source?: SourcePreview;
  sourceJob?: SourceImportJob;
  sourceJobs?: SourceImportJob[];
  workflow?: AutoWorkflow;
  workflows?: AutoWorkflow[];
  events?: AutoWorkflowEvent[];
  cut?: Edit;
  preview?: CutPreview;
  suggestions?: Edit[];
  speechInsights?: SpeechInsights;
  audioAnalysisJob?: AudioAnalysisJob | null;
  speakerPackage?: SpeakerPackageStatus;
  speakerTrack?: SpeakerTrack;
  speakerJob?: SpeakerJob;
  speakerJobs?: SpeakerJob[];
  config?: TranscriptionProviderConfig;
  providerHealth?: TranscriptionProviderHealth;
  transcriptionJob?: TranscriptionJob | null;
  transcriptionJobs?: TranscriptionJob[];
  candidatePreview?: { jobId: string; versionId: string; overwrittenSegments: number; total: number; offset: number; segments: { start: number; end: number; text: string }[] };
  deletionPreflight?: ProjectDeletionPreflight;
  reviewItem?: TranscriptionReviewItem;
  reviewItems?: TranscriptionReviewItem[];
  subtitleQuality?: SubtitleQualityReport;
  subtitleStyle?: SubtitleStyle;
  subtitleStylePresets?: SubtitleStylePresetOption[];
  subtitleImportPreview?: SubtitleImportPreview;
  transcriptReplacementPreflight?: TranscriptReplacementPreflight;
  timingValidation?: TimingValidation;
  structureEdit?: SubtitleStructureEdit;
  subtitleImport?: {
    format: "srt" | "vtt" | "ass";
    sha256: string;
    insertedSegments: number;
    quality: SubtitleQualityReport;
    project: Project;
  };
  [key: string]: unknown;
};

export type RuntimeInfo = {
  corePath: string;
  coreApiVersion: string;
  ffmpegConfigured: boolean;
  asrConfigured: boolean;
  vadConfigured: boolean;
  vadTimelineVerified: boolean;
  vadStatus: "verified" | "safe_fallback" | "not_configured";
  vadReasonCode: string | null;
  ytDlpConfigured: boolean;
  asrBackend: string;
  asrDevice: string | null;
  availableAsrBackends: string[];
  ffmpegPath: string | null;
  whisperPath: string | null;
  ytDlpPath: string | null;
  runtimeManifestPath: string | null;
  defaultModelPath: string;
  defaultModelAvailable: boolean;
  logDirectory: string | null;
  diagnosticsAvailable: boolean;
};

export type UpdatePolicy = {
  currentVersion: string;
  enabled: boolean;
  automaticCheckIntervalHours: number;
  disabledReason: string | null;
};

export type UpdateMetadata = {
  version: string;
  currentVersion: string;
  notes: string | null;
  publishedAt: string | null;
  sizeBytes: number;
};

export type UpdateDownloadEvent = {
  event: "Started" | "Progress" | "Finished" | "Verifying";
  data?: { contentLength?: number; chunkLength?: number };
};

type LegacyTaskFields="createdAt" | "lease" | "lastActivity" | "errorCode" | "attemptCount" | "completedAt" | "cancelRequestedAt" | "baseVersionId" | "workflowId";
