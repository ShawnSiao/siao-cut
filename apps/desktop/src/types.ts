export type Segment = {
  id: string;
  start: number;
  end: number;
  text: string;
  confidence: number | null;
};

export type WordTiming = {
  id: string;
  segmentId: string;
  start: number;
  end: number;
  text: string;
  confidence: number | null;
};

export type SpeechPause = {
  start: number;
  end: number;
  duration: number;
  previousWordId: string;
  nextWordId: string;
  severity: "pause" | "long_pause";
};

export type SpeechEvidence = {
  kind: "filler" | "low_confidence";
  wordId: string;
  segmentId: string;
  start: number;
  end: number;
  text: string;
  confidence: number | null;
};

export type SpeechInsights = {
  status: "ready" | "insufficient_evidence";
  analyzerVersion: string;
  thresholds: {
    pauseSeconds: number;
    longPauseSeconds: number;
    lowConfidence: number;
  };
  spanDurationSeconds: number;
  spokenDurationSeconds: number;
  tokenCount: number;
  tokensPerMinute: number;
  pauseCount: number;
  longPauseCount: number;
  totalPauseDurationSeconds: number;
  fillerCount: number;
  lowConfidenceCount: number;
  pauses: SpeechPause[];
  evidence: SpeechEvidence[];
};

export type Translation = {
  status: "current" | "stale" | string;
  updatedAt: string;
  segments: Array<{ segmentId: string; text: string }>;
};

export type Task = {
  id: string;
  kind: string;
  language: string | null;
  status: string;
  progress: number;
  errorMessage: string | null;
  workflowId?: string | null;
};

export type AgentPatchItem = {
  id: string;
  segmentId: string | null;
  target: string;
  beforeText: string;
  afterText: string;
  currentText: string;
  reason: string;
  confidence: number | null;
  status: string;
};

export type AgentPatchSet = {
  id: string;
  taskId: string;
  kind: string;
  language: string | null;
  status: string;
  baseVersionId: string;
  createdAt: string;
  items: AgentPatchItem[];
};

export type Workflow = {
  id: string;
  kind: string;
  language: string | null;
  status: string;
  taskId: string;
  createdAt: string;
  updatedAt: string;
};

export type Version = { id: string; reason: string; createdAt: string };

export type Edit = {
  id: string;
  kind: string;
  status: string;
  segmentId: string;
  start: number;
  end: number;
  reason: string;
  cutRange?: {
    fromWordId: string;
    toWordId: string;
    selectedStart: number;
    selectedEnd: number;
    paddingMs: number;
    transcriptHash: string;
    stale: boolean;
  } | null;
  suggestion?: {
    suggestionType: "standalone_filler" | "adjacent_repetition" | "speech_restart" | string;
    confidence: number;
    detectorVersion: string;
  } | null;
};

export type CutPreview = {
  cutId: string;
  previewStart: number;
  cutStart: number;
  cutEnd: number;
  previewEnd: number;
  skipRange: boolean;
};

export type TimelineMap = {
  sourceDuration: number;
  outputDuration: number;
  keptRanges: Array<{ sourceStart: number; sourceEnd: number; outputStart: number; outputEnd: number }>;
  cuts: Array<{ editIds: string[]; sourceStart: number; sourceEnd: number; outputAt: number }>;
};

export type MediaArtifacts = {
  status: string;
  proxyPath: string | null;
  waveformPath: string | null;
  thumbnails: string[];
  sourceSha256: string;
  updatedAt: string;
  errorMessage: string | null;
};

export type ExportJob = {
  id: string;
  projectId: string;
  outputPath: string;
  status: string;
  progress: number;
  burnSubtitles: boolean;
  language: string | null;
  bilingual: boolean;
  subtitleMode: "source" | "translated" | "bilingual";
  canvasSettings: CanvasSettings;
  subtitleStyle: SubtitleStyle;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  manifestPath: string | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
};

export type CanvasSettings = {
  aspectRatio: "source" | "9:16";
  framing: "contain-blur" | "cover-center";
};

export type SubtitleStylePreset = "compact" | "standard" | "emphasis";
export type SubtitlePosition = "bottom" | "center";

export type SubtitleStyle = {
  preset: SubtitleStylePreset;
  position: SubtitlePosition;
  fontFamily: string;
  bold: boolean;
  fontSize: number;
  secondaryFontSize: number;
  primaryColor: string;
  secondaryColor: string;
  outlineColor: string;
  outlineWidth: number;
  shadowDepth: number;
  safeMarginPercent: number;
};

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
};

export type ModelDownloadJob = {
  id: string;
  modelId: string;
  status: string;
  progress: number;
  bytesDownloaded: number;
  totalBytes: number;
  targetPath: string;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
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
  requiresConfirmation: boolean;
};

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
  status: string;
  progress: number;
  bytesDownloaded: number;
  totalBytes: number | null;
  outputDirectory: string;
  outputPath: string | null;
  outputSha256: string | null;
  toolVersion: string;
  toolSha256: string;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
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
  status: string;
  progress: number;
  report: AudioAnalysisReport | null;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
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
  generatedAt: string | null;
  speakers: SpeakerIdentity[];
  turns: SpeakerTurn[];
  associations: SegmentSpeaker[];
};

export type SpeakerJob = {
  id: string;
  kind: "install" | "analyze";
  projectId: string | null;
  status: string;
  stage: string;
  progress: number;
  bytesDownloaded: number;
  totalBytes: number;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
  attemptCount: number;
};

export type AutoWorkflow = {
  id: string;
  inputKind: "local" | "url";
  inputValue: string;
  title: string | null;
  confirmedMediaId: string | null;
  projectId: string | null;
  sourceImportId: string | null;
  modelPath: string;
  transcribeLanguage: string | null;
  translationLanguage: string | null;
  outputPath: string;
  burnSubtitles: boolean;
  subtitleMode: "source" | "translated" | "bilingual";
  status: string;
  currentStage: string;
  progress: number;
  transcriptVersionId: string | null;
  agentTaskId: string | null;
  exportJobId: string | null;
  audit: Record<string, unknown> | null;
  cancelRequestedAt: string | null;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  workerPid?: number | null;
  attemptCount: number;
};

export type AutoWorkflowEvent = {
  id: number;
  workflowId: string;
  stage: string;
  status: string;
  progress: number;
  message: string;
  createdAt: string;
};

export type Project = {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  canvasSettings: CanvasSettings;
  subtitleStyle: SubtitleStyle;
  media: {
    sourcePath: string;
    extension: string;
    durationSeconds: number | null;
  };
  mediaArtifacts: MediaArtifacts | null;
  timeline: TimelineMap;
  transcript: { sourceLanguage: string; segments: Segment[]; words: WordTiming[] };
  subtitleQuality: SubtitleQualityReport;
  speechInsights: SpeechInsights;
  translations: Record<string, Translation>;
  edits: Edit[];
  tasks: Task[];
  versions: Version[];
  history: { canUndo: boolean; canRedo: boolean; currentVersionId: string | null };
  patchSets: AgentPatchSet[];
  workflows: Workflow[];
};

export type SubtitleIssueKind = "empty_text" | "invalid_timing" | "out_of_bounds" | "overlap" | "duration_too_long" | "line_too_long" | "reading_speed_high" | "gap_too_short";

export type SubtitleQualityIssue = {
  id: string;
  kind: SubtitleIssueKind;
  severity: "warning" | "error";
  segmentId: string;
  relatedSegmentId: string | null;
  start: number;
  end: number;
  message: string;
  measuredValue: number | null;
  threshold: number | null;
};

export type SubtitleQualityReport = {
  status: "good" | "warning" | "error";
  statusLabel: string;
  issueCount: number;
  errorCount: number;
  warningCount: number;
  thresholds: {
    maxDurationSeconds: number;
    maxLineCharacters: number;
    maxCharactersPerSecond: number;
    minGapSeconds: number;
  };
  issues: SubtitleQualityIssue[];
};

export type SubtitleImportPreview = {
  format: "srt" | "vtt" | "ass";
  sourcePath: string;
  sha256: string;
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

export type CoreEnvelope = {
  apiVersion: string;
  status: "ok" | "error";
  error?: { code: string; message: string };
  message?: string;
  project?: Project;
  projects?: Project[];
  job?: ExportJob;
  jobs?: ExportJob[];
  models?: ModelStatus[];
  model?: ModelStatus;
  modelJob?: ModelDownloadJob;
  modelJobs?: ModelDownloadJob[];
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
  subtitleQuality?: SubtitleQualityReport;
  subtitleStyle?: SubtitleStyle;
  subtitleStylePresets?: SubtitleStylePresetOption[];
  subtitleImportPreview?: SubtitleImportPreview;
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
