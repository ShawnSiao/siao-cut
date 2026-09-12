import type * as Wire from "./generated/core-contract";
import type { AgentRunStatus,AutoWorkflowStage,AutoWorkflowStatus,BackgroundJobStatus,CoreErrorCode,LocalResourceState,TaskStatus,TranscriptionJobStatus,WorkflowStatus } from "./generated/core-contract";
export type { AgentRunStatus,AutoWorkflowStage,AutoWorkflowStatus,BackgroundJobStatus,CoreErrorCode,KnownCoreErrorCode,LocalResourceState,TaskStatus,TranscriptionJobStage,TranscriptionJobStatus,WorkflowProfile,WorkflowStatus } from "./generated/core-contract";

export type Segment = import("./generated/core-contract").CoreSegment;

export type CodexHealth = Wire.CoreCodexHealthWire;

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

export type CutPreview = Wire.CutPreview;

export type TimelineMap = import("./generated/core-contract").CoreTimelineMap;

export type MediaArtifacts = import("./generated/core-contract").CoreMediaArtifacts;

export type ExportJob = Omit<Wire.CoreExportJob,"status" | "stageCode" | "errorCode" | "workerPid"> & Partial<Pick<Wire.CoreExportJob,"stageCode" | "errorCode" | "workerPid">> & {status:BackgroundJobStatus};

export type CanvasSettings = import("./generated/core-contract").CoreCanvasSettings;

export type SubtitleStylePreset = import("./generated/core-contract").CoreSubtitleStylePreset;
export type SubtitlePosition = import("./generated/core-contract").CoreSubtitlePosition;

export type SubtitleStyle = import("./generated/core-contract").CoreSubtitleStyle;

export type SubtitleStylePresetOption = Wire.SubtitleStylePresetOption;

export type ModelStatus = Omit<Wire.CoreModelStatusWire, "verificationStatus"> & {
  verificationStatus: "verified" | "failed" | "not_checked" | "not_installed";
};

export type ModelDownloadJob = Omit<Wire.CoreModelDownloadJobWire, "status" | "stageCode" | "errorCode" | "workerPid"> & {
  status: BackgroundJobStatus;
  stageCode?: string | null;
  errorCode?: CoreErrorCode | null;
  workerPid?: number | null;
};

export type LocalCapabilityId = "basic_media" | "url_import" | "local_transcription" | "speaker_identity";
export type LocalTranscriptionProfile = "fast" | "standard" | "quality";

export type LocalCapabilityStatus = Omit<Wire.CoreCapabilityStatusWire, "id" | "state" | "canRollback"> & {
  id: LocalCapabilityId;
  state: LocalResourceState;
  canRollback?: boolean;
};

export type LocalResourceStatus = Omit<Wire.CoreLocalResourceStatusWire, "transcriptionProfile" | "capabilities"> & {
  transcriptionProfile: LocalTranscriptionProfile;
  capabilities: LocalCapabilityStatus[];
};

export type ResourceUpdateCheck = Omit<Wire.CoreResourceUpdateCheckWire, "capabilities"> & {
  capabilities: Array<{
    capabilityId: LocalCapabilityId;
    state: "current" | LocalResourceState;
  }>;
};

export type LocalResourcePlan = Omit<Wire.CoreResourcePlanWire, "capabilityId"> & {
  capabilityId: LocalCapabilityId;
};

export type LocalResourceJob = Omit<Wire.CoreResourceJobWire, "capabilityId" | "transcriptionProfile" | "status" | "errorCode" | "workerPid"> & {
  capabilityId: LocalCapabilityId;
  transcriptionProfile?: LocalTranscriptionProfile | null;
  status: BackgroundJobStatus;
  errorCode?: CoreErrorCode | null;
  workerPid?: number | null;
};

export type SourcePreview = Omit<Wire.CoreSourcePreviewWire, "authMode" | "browser"> & {
  browser: SourceBrowser | null;
  authMode: "anonymous" | "browser";
};

export type SourceBrowser = "chrome" | "edge" | "firefox";

export type SourceImportJob = Omit<Wire.CoreSourceImportJobWire, "status" | "stageCode" | "authMode" | "errorCode" | "workerPid"> & {
  status: BackgroundJobStatus;
  stageCode?: string | null;
  authMode: "anonymous" | "browser";
  errorCode?: CoreErrorCode | null;
  workerPid?: number | null;
};

export type AudioRisk = Omit<Wire.CoreAudioRiskWire, "kind" | "unit"> & {
  kind: "silence" | "suspected_clipping" | "loudness_low" | "loudness_high";
  unit: "seconds" | "LUFS" | "dBFS" | string;
};

export type AudioAnalysisReport = Omit<Wire.CoreAudioAnalysisReportWire, "thresholds" | "risks"> & {
  thresholds: {
    silenceNoiseDb: number;
    silenceMinSeconds: number;
    clippingPeakDbfs: number;
    quietLoudnessLufs: number;
    loudLoudnessLufs: number;
  };
  risks: AudioRisk[];
};

export type AudioAnalysisJob = Omit<Wire.CoreAudioAnalysisJobWire, "status" | "stageCode" | "errorCode" | "workerPid"> & {
  status: BackgroundJobStatus;
  stageCode?: string | null;
  errorCode?: CoreErrorCode | null;
  workerPid?: number | null;
};

export type SpeakerAssetStatus = Omit<Wire.CoreSpeakerAssetStatusWire, "verificationStatus"> & {
  verificationStatus: "verified" | "failed" | "not_checked" | "not_installed";
};

export type SpeakerPackageStatus = Omit<Wire.CoreSpeakerPackageStatusWire, "verificationStatus" | "assets"> & {
  verificationStatus: "verified" | "failed" | "not_checked" | "not_installed";
  assets: SpeakerAssetStatus[];
};

export type SpeakerIdentity = Wire.CoreSpeakerIdentityWire;

export type SpeakerTurn = Wire.CoreSpeakerTurnWire;

export type SegmentSpeaker = Omit<Wire.CoreSegmentSpeakerWire, "source"> & {
  source: "overlap" | "manual" | string;
};

export type SpeakerTrack = Omit<Wire.CoreSpeakerTrackWire, "status" | "sourceKind" | "speakers" | "turns" | "associations"> & {
  status: "not_analyzed" | "ready" | "no_speech" | string;
  sourceKind: "cascade" | "end_to_end" | string;
  speakers: SpeakerIdentity[];
  turns: SpeakerTurn[];
  associations: SegmentSpeaker[];
};

export type SpeakerJob = Omit<Wire.CoreSpeakerJobWire, "kind" | "status" | "stageCode" | "errorCode" | "workerPid"> & {
  kind: "install" | "analyze";
  status: BackgroundJobStatus;
  stageCode?: string | null;
  errorCode?: CoreErrorCode | null;
  workerPid?: number | null;
};

export type AutoWorkflow = Omit<Wire.CoreAutoWorkflow,"status" | "currentStage" | "inputKind" | "aiExecutionKind" | "audit" | "instructionLocale" | "errorCode" | "workerPid"> & Partial<Pick<Wire.CoreAutoWorkflow,"errorCode" | "workerPid">> & {status:AutoWorkflowStatus;currentStage:AutoWorkflowStage;inputKind:"local"|"url";aiExecutionKind:"codex"|"api"|null;audit:Record<string,unknown>|null;instructionLocale:UiLocale;stageCode?:string|null};

export type TranscriptionProviderConfig = Omit<Wire.CoreProviderConfigWire, "providerId"> & {
  providerId: "moss_openai" | string;
};

export type TranscriptionProviderHealth = Omit<Wire.CoreProviderHealthWire, "state"> & {
  state: "healthy" | "unavailable";
};

export type TranscriptionJob = Omit<Wire.CoreTranscriptionJobWire, "hotwords" | "status" | "errorCode" | "workerPid"> & {
  hotwords: string[];
  status: TranscriptionJobStatus;
  errorCode?: CoreErrorCode | null;
  workerPid?: number | null;
};

export type TranscriptionCandidateSummary = Wire.CoreTranscriptionCandidateSummaryWire;

export type ProjectDeletionPreflight = Omit<Wire.CoreProjectDeletionPreflightWire, "blockers"> & {
  blockers: Array<{ kind: string; id: string; status: string }>;
};

export type TranscriptionReviewItem = Omit<Wire.CoreReviewItemWire, "severity" | "kind" | "status"> & {
  severity: "info" | "warning" | "error";
  kind: "missing_punctuation" | "rapid_speaker_switch" | "short_fragment" | string;
  status: "open" | "resolved" | "ignored";
};

export type AutoWorkflowEvent = Omit<Wire.CoreAutoWorkflowEvent,"stage" | "status"> & {stage:AutoWorkflowStage;status:AutoWorkflowStatus};

/** Rust wire shape; legacy preview fixtures may omit media hashes and optional task metadata. */
export type Project = Omit<import("./generated/core-contract").CoreProject,"media" | "tasks" | "workflows" | "edits"> & {
  media: Omit<import("./generated/core-contract").CoreMedia,"sha256"> & {sha256?:string};
  tasks:Task[];workflows:Workflow[];edits:Edit[];
  /** UI read-model provenance; never sent back as project content. */
  readModels?: Partial<Record<"history" | "insights" | "review", string | null>>;
};

export type SubtitleIssueKind = import("./generated/core-contract").CoreSubtitleIssueKind;

export type SubtitleQualityIssue = import("./generated/core-contract").CoreSubtitleQualityIssue;

export type SubtitleQualityReport = import("./generated/core-contract").CoreSubtitleQualityReport;

export type SubtitleImportPreview = Omit<Wire.CoreSubtitleImportPreviewWire, "format" | "segments" | "quality"> & {
  format: "srt" | "vtt" | "ass";
  segments: Segment[];
  quality: SubtitleQualityReport;
};

export type SubtitleStructureEdit = Omit<Wire.CoreStructureEditResultWire, "operation" | "affectedSegmentIds" | "removedSegmentIds" | "impact" | "project"> & {
  operation: "split" | "merge" | "timing" | "offset";
  affectedSegmentIds: string[];
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

export type TimingValidation = Wire.CoreTimingValidationWire;

export type TranscriptReplacementPreflight = Omit<Wire.CoreTranscriptReplacementPreflightWire, "blockers"> & {
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

export type RuntimeInfo = Omit<Wire.DesktopRuntimeInfo,"vadStatus"> & {vadStatus:"verified" | "safe_fallback" | "not_configured"};

export type UpdatePolicy = Wire.UpdatePolicy;

export type UpdateMetadata = Wire.UpdateMetadata;

export type UpdateDownloadEvent = Wire.DownloadEvent;

type LegacyTaskFields="createdAt" | "lease" | "lastActivity" | "errorCode" | "attemptCount" | "completedAt" | "cancelRequestedAt" | "baseVersionId" | "workflowId";
