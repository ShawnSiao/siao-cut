import { sampleProject } from "./mock";
import type { AgentRun, AudioAnalysisJob, AutoWorkflow, CoreEnvelope, ExportJob, ModelDownloadJob, ModelStatus, Project, RuntimeInfo, SourceImportJob, SourcePreview, SpeakerJob, SpeakerPackageStatus, SpeakerTrack, SubtitleImportPreview, SubtitleStructureEdit, TranscriptionJob, TranscriptionProviderConfig, TranscriptionProviderHealth, TranscriptionReviewItem, UpdateDownloadEvent, UpdateMetadata, UpdatePolicy } from "./types";

const isTauri = () => "__TAURI_INTERNALS__" in window;
const mockSubtitleStylePresets = [
  { id: "compact", label: "紧凑", description: "42 px，适合信息密度较高的双语字幕" },
  { id: "standard", label: "清晰", description: "52 px，默认口播字幕，兼顾可读性和画面占用" },
  { id: "emphasis", label: "强调", description: "60 px，加粗描边，适合短句和重点表达" },
] satisfies Array<{ id: Project["subtitleStyle"]["preset"]; label: string; description: string }>;

const resolveMockSubtitleStyle = (preset: Project["subtitleStyle"]["preset"], position: Project["subtitleStyle"]["position"]): Project["subtitleStyle"] => {
  const sizes = preset === "compact"
    ? { fontSize: 42, secondaryFontSize: 32, outlineWidth: 2, shadowDepth: 1, safeMarginPercent: 6 }
    : preset === "emphasis"
      ? { fontSize: 60, secondaryFontSize: 46, outlineWidth: 4, shadowDepth: 2, safeMarginPercent: 10 }
      : { fontSize: 52, secondaryFontSize: 40, outlineWidth: 3, shadowDepth: 1, safeMarginPercent: 8 };
  return {
    preset,
    position,
    fontFamily: "Microsoft YaHei UI",
    bold: true,
    primaryColor: "#F2F4F5",
    secondaryColor: "#B5BEC6",
    outlineColor: "#080A0D",
    ...sizes,
  };
};
let mockProject = structuredClone(sampleProject);
let mockProjects: Project[] = [];
let mockUndoStack: Project[] = [];
let mockRedoStack: Project[] = [];
let mockStructureCounter = 0;
const mockJobs = new Map<string, ExportJob>();
const mockModelJobs = new Map<string, ModelDownloadJob>();
const mockSourceJobs = new Map<string, SourceImportJob>();
const mockSourcePolls = new Map<string, number>();
const mockAutoWorkflows = new Map<string, AutoWorkflow>();
const mockAutoPolls = new Map<string, number>();
const mockAudioJobs = new Map<string, AudioAnalysisJob>();
const mockSpeakerJobs = new Map<string, SpeakerJob>();
const mockSpeakerTracks = new Map<string, SpeakerTrack>();
const mockTranscriptionJobs = new Map<string, TranscriptionJob>();
const mockAgentRuns = new Map<string, AgentRun>();
const mockAgentPolls = new Map<string, number>();
let mockTranscriptionConfig: TranscriptionProviderConfig = { providerId: "moss_openai", endpoint: "http://127.0.0.1:8000", modelId: "OpenMOSS-Team/MOSS-Transcribe-Diarize", updatedAt: new Date().toISOString() };
let mockTranscriptionHealth: TranscriptionProviderHealth = { ...mockTranscriptionConfig, state: "healthy", detail: "本机 MOSS 服务可用。", checkedAt: new Date().toISOString() };
let mockTranscriptionReviews: TranscriptionReviewItem[] = [];

function syncMockProject(next: Project): Project {
  mockProject = structuredClone(next);
  mockProjects = mockProjects.map((item) => item.id === mockProject.id ? mockProject : item);
  return structuredClone(mockProject);
}
function resetMockHistory(project: Project) {
  const baseline = structuredClone(project);
  baseline.history = { canUndo: false, canRedo: false, currentVersionId: baseline.versions.at(-2)?.id ?? null };
  mockUndoStack = [baseline];
  mockRedoStack = [];
  mockProject.history = { ...mockProject.history, canUndo: true, canRedo: false };
}

function recordMockSnapshot() {
  mockUndoStack.push(structuredClone(mockProject));
  mockRedoStack = [];
}

function markTextDependentsStale(project: Project, segmentIds: string[]) {
  Object.values(project.translations).forEach((translation) => { translation.status = "stale"; });
  project.edits.forEach((edit) => {
    if (edit.segmentId && segmentIds.includes(edit.segmentId) && edit.cutRange) edit.cutRange.stale = true;
  });
}
const speakerAssets = [
  { id: "runtime", name: "sherpa-onnx Windows x64 CPU", source: "https://github.com/k2-fsa/sherpa-onnx", license: "Apache-2.0", size: 323584, sha256: "86d696832204b7859aef601a0f996371abca6f955d71e1242f308027872a0e9c" },
  { id: "onnxruntime", name: "ONNX Runtime", source: "https://github.com/microsoft/onnxruntime", license: "MIT", size: 15394304, sha256: "8b695444d1a35ed0c8338b8c14438b3be5e0a3b222b88b1e7b4ce8753f135b50" },
  { id: "onnxruntime-providers", name: "ONNX Runtime provider bridge", source: "https://github.com/microsoft/onnxruntime", license: "MIT", size: 10752, sha256: "ebc55b0f28e8a79cbf78e810a7f510ba70e75a2dfbcfcc6aca31ab2b8710a59a" },
  { id: "segmentation", name: "pyannote segmentation 3.0 int8", source: "https://huggingface.co/pyannote/segmentation-3.0", license: "MIT", size: 1540506, sha256: "d582f4b4c6b48205de7e0643c57df0df5615a3c176189be3fc461e9d18827b5d" },
  { id: "embedding", name: "3D-Speaker ERes2Net Base 16 kHz", source: "https://github.com/modelscope/3D-Speaker", license: "Apache-2.0", size: 39593761, sha256: "1a331345f04805badbb495c775a6ddffcdd1a732567d5ec8b3d5749e3c7a5e4b" },
];
let mockSpeakerPackage: SpeakerPackageStatus = {
  id: "sherpa-onnx-speaker-zh-en-v1",
  name: "本地说话人分离（中英）",
  runtimeVersion: "sherpa-onnx 1.13.2",
  description: "CPU 本地运行；分析结果只进入待审阅说话人轨，不改写字幕或剪辑。",
  source: "https://github.com/k2-fsa/sherpa-onnx",
  license: "Apache-2.0 / MIT",
  downloadSize: 64389270,
  installedSize: 56862907,
  installed: false,
  verified: null,
  verificationStatus: "not_installed",
  assets: speakerAssets.map((asset) => ({ ...asset, installed: false, verified: null, verificationStatus: "not_installed" as const })),
};

const emptySpeakerTrack = (): SpeakerTrack => ({
  status: "not_analyzed",
  runtimeVersion: "sherpa-onnx 1.13.2",
  segmentationModel: "pyannote segmentation 3.0 int8",
  embeddingModel: "3D-Speaker ERes2Net Base 16 kHz",
  providerId: "legacy_diarization",
  modelId: "",
  sourceKind: "cascade",
  generatedAt: null,
  speakers: [],
  turns: [],
  associations: [],
});

const analyzedSpeakerTrack = (): SpeakerTrack => {
  const createdAt = new Date().toISOString();
  return {
    ...emptySpeakerTrack(),
    status: "ready",
    generatedAt: createdAt,
    speakers: [
      { id: "voice-a", sourceLabel: "speaker_00", label: "说话人 1", colorIndex: 0, createdAt },
      { id: "voice-b", sourceLabel: "speaker_01", label: "说话人 2", colorIndex: 1, createdAt },
    ],
    turns: [
      { id: "turn-a", speakerId: "voice-a", start: 12.4, end: 18.6, confidence: null, source: "sherpa-onnx", modelVersion: "sherpa-onnx 1.13.2", createdAt },
      { id: "turn-b", speakerId: "voice-b", start: 18.6, end: 27.2, confidence: null, source: "sherpa-onnx", modelVersion: "sherpa-onnx 1.13.2", createdAt },
    ],
    associations: [
      { segmentId: "s1", speakerId: "voice-a", source: "overlap", confidence: 1, updatedAt: createdAt },
      { segmentId: "s2", speakerId: "voice-a", source: "overlap", confidence: 1, updatedAt: createdAt },
      { segmentId: "s3", speakerId: "voice-b", source: "overlap", confidence: 1, updatedAt: createdAt },
      { segmentId: "s4", speakerId: "voice-b", source: "overlap", confidence: 1, updatedAt: createdAt },
    ],
  };
};
const mockSourcePreview: SourcePreview = {
  originalUrl: "https://www.youtube.com/watch?v=HOfdboHvshg",
  webpageUrl: "https://www.youtube.com/watch?v=HOfdboHvshg",
  siteMediaId: "HOfdboHvshg",
  extractor: "youtube",
  title: "Sintel Trailer, Durian Open Movie Project",
  durationSeconds: 52,
  fileSizeBytes: 12075092,
  fileSizeKnown: false,
  thumbnailUrl: null,
  toolVersion: "2026.06.09",
  toolSha256: "3a48cb955d55c8821b60ccbdbbc6f61bc958f2f3d3b7ad5eaf3d83a543293a27",
  requiresConfirmation: true,
};
const mockSubtitlePreview: SubtitleImportPreview = {
  format: "srt",
  sourcePath: "demo.srt",
  sha256: "a".repeat(64),
  segmentCount: 2,
  segments: [
    { id: "preview-1", start: 0, end: 2, text: "导入后的第一条字幕", confidence: null },
    { id: "preview-2", start: 1.9, end: 4, text: "导入后的第二条字幕", confidence: null },
  ],
  quality: {
    status: "warning",
    statusLabel: "1 项质量提醒",
    issueCount: 1,
    errorCount: 0,
    warningCount: 1,
    thresholds: { maxDurationSeconds: 8, maxLineCharacters: 42, maxCharactersPerSecond: 20, minGapSeconds: 0.12, maxLines: 2 },
    issues: [{ id: "quality-overlap-preview-2", kind: "overlap", severity: "warning", segmentId: "preview-2", relatedSegmentId: "preview-1", start: 1.9, end: 4, message: "与上一条字幕时间重叠", measuredValue: 0.1, threshold: 0 }],
  },
  canImport: true,
  requiresConfirmation: true,
};

const mockStructureImpact = (): SubtitleStructureEdit["impact"] => ({
  translationsMarkedStale: 0,
  translationSegmentsRemoved: 0,
  wordsReassigned: 0,
  wordsRemoved: 0,
  wordsShifted: 0,
  editsRestored: 0,
  wordCutsInvalidated: 0,
  agentPatchItemsRebased: 0,
  speakerAssociationsCopied: 0,
  speakerAssociationsRemoved: 0,
});

function refreshMockSubtitleQuality() {
  const thresholds = { maxDurationSeconds: 8, maxLineCharacters: 42, maxCharactersPerSecond: 20, minGapSeconds: 0.12, maxLines: 2 };
  const issues: Project["subtitleQuality"]["issues"] = [];
  const segments = [...mockProject.transcript.segments].sort((left, right) => left.start - right.start || left.end - right.end);
  const add = (segment: Project["transcript"]["segments"][number], kind: Project["subtitleQuality"]["issues"][number]["kind"], severity: "warning" | "error", message: string, measuredValue: number | null, threshold: number | null, relatedSegmentId: string | null = null) => {
    issues.push({ id: `quality-${kind}-${segment.id}`, kind, severity, segmentId: segment.id, relatedSegmentId, start: segment.start, end: segment.end, message, measuredValue, threshold });
  };
  for (const segment of segments) {
    const validTiming = Number.isFinite(segment.start) && Number.isFinite(segment.end) && segment.start >= 0 && segment.end > segment.start;
    if (!validTiming) add(segment, "invalid_timing", "error", "时间范围无效", null, null);
    if (!segment.text.trim()) add(segment, "empty_text", "error", "字幕文本为空", null, null);
    if (validTiming && mockProject.media.durationSeconds != null && segment.end > mockProject.media.durationSeconds) add(segment, "out_of_bounds", "error", "字幕结束时间超过原片时长", segment.end, mockProject.media.durationSeconds);
    if (!validTiming) continue;
    const duration = segment.end - segment.start;
    if (duration > thresholds.maxDurationSeconds) add(segment, "duration_too_long", "warning", "单条字幕持续时间过长", duration, thresholds.maxDurationSeconds);
    const maxLine = Math.max(0, ...segment.text.split(/\r?\n/).map((line) => Array.from(line).filter((character) => !/\s/.test(character)).length));
    if (maxLine > thresholds.maxLineCharacters) add(segment, "line_too_long", "warning", "单行字幕字符过多", maxLine, thresholds.maxLineCharacters);
    const lineCount = segment.text.split(/\r?\n/).length;
    if (mockProject.transcript.sourceLanguage.toLowerCase().startsWith("en") && lineCount > thresholds.maxLines) add(segment, "too_many_lines", "warning", "英文字幕超过两行", lineCount, thresholds.maxLines);
    const visibleCharacters = Array.from(segment.text).filter((character) => !/\s/.test(character)).length;
    const readingSpeed = visibleCharacters / duration;
    if (readingSpeed > thresholds.maxCharactersPerSecond) add(segment, "reading_speed_high", "warning", "字幕阅读速度过快", readingSpeed, thresholds.maxCharactersPerSecond);
  }
  for (let index = 1; index < segments.length; index += 1) {
    const previous = segments[index - 1];
    const current = segments[index];
    const gap = current.start - previous.end;
    if (gap < 0) add(current, "overlap", "warning", "与上一条字幕时间重叠", -gap, 0, previous.id);
    else if (gap < thresholds.minGapSeconds) add(current, "gap_too_short", "warning", "与上一条字幕间隔过短", gap, thresholds.minGapSeconds, previous.id);
  }
  const errorCount = issues.filter((issue) => issue.severity === "error").length;
  const warningCount = issues.length - errorCount;
  mockProject.subtitleQuality = {
    status: errorCount ? "error" : warningCount ? "warning" : "good",
    statusLabel: errorCount ? `${errorCount} 项错误需要处理` : warningCount ? `${warningCount} 项质量提醒` : "未发现字幕问题",
    issueCount: issues.length,
    errorCount,
    warningCount,
    thresholds,
    issues,
  };
}
function finishMockStructureEdit(operation: SubtitleStructureEdit["operation"], affectedSegmentIds: string[], createdSegmentId: string | null, removedSegmentIds: string[], impact = mockStructureImpact()): CoreEnvelope {
  refreshMockSubtitleQuality();
  const versionId = `v${mockProject.versions.length + 1}`;
  const reasons = { split: "拆分字幕段", merge: "合并相邻字幕段", timing: "调整字幕时间", offset: "批量偏移字幕" };
  mockProject.versions.push({ id: versionId, reason: reasons[operation], createdAt: new Date().toISOString() });
  mockProject.history = { canUndo: true, canRedo: false, currentVersionId: versionId };
  const committed = syncMockProject(mockProject);
  const structureEdit: SubtitleStructureEdit = { operation, affectedSegmentIds, createdSegmentId, removedSegmentIds, impact, project: committed };
  return { apiVersion: "0.1", status: "ok", structureEdit };
}
const mockModels: ModelStatus[] = [
  { id: "tiny", name: "省空间", fileName: "ggml-tiny.bin", description: "约 74 MB，适合快速试用与低配置电脑。", source: "https://huggingface.co/ggerganov/whisper.cpp", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin", size: 77691713, sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21", license: "MIT", recommended: false, path: "C:\\Models\\ggml-tiny.bin", installed: true, bytesOnDisk: 77691713, verified: true, verificationStatus: "verified" },
  { id: "base", name: "平衡", fileName: "ggml-base.bin", description: "约 141 MB，默认推荐，兼顾速度与中英识别质量。", source: "https://huggingface.co/ggerganov/whisper.cpp", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin", size: 147951465, sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe", license: "MIT", recommended: true, path: "C:\\Models\\ggml-base.bin", installed: false, bytesOnDisk: 0, verified: null, verificationStatus: "not_installed" },
  { id: "small", name: "高质量", fileName: "ggml-small.bin", description: "约 465 MB，识别质量更高，CPU 转录耗时更长。", source: "https://huggingface.co/ggerganov/whisper.cpp", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin", size: 487601967, sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b", license: "MIT", recommended: false, path: "C:\\Models\\ggml-small.bin", installed: false, bytesOnDisk: 0, verified: null, verificationStatus: "not_installed" },
];

function updateMockTimeline() {
  const applied = mockProject.edits.filter((edit) => edit.status === "applied").sort((a, b) => a.start - b.start);
  let source = 0;
  let output = 0;
  const keptRanges: Project["timeline"]["keptRanges"] = [];
  const cuts: Project["timeline"]["cuts"] = [];
  for (const edit of applied) {
    if (edit.start > source) {
      keptRanges.push({ sourceStart: source, sourceEnd: edit.start, outputStart: output, outputEnd: output + edit.start - source });
      output += edit.start - source;
    }
    cuts.push({ editIds: [edit.id], sourceStart: edit.start, sourceEnd: edit.end, outputAt: output });
    source = edit.end;
  }
  if (source < 278) keptRanges.push({ sourceStart: source, sourceEnd: 278, outputStart: output, outputEnd: output + 278 - source });
  mockProject.timeline = { sourceDuration: 278, outputDuration: output + 278 - source, keptRanges, cuts };
}

function ensureOk(envelope: CoreEnvelope): CoreEnvelope {
  if (envelope.status === "error") {
    throw new Error(envelope.error?.message ?? envelope.message ?? "Core 请求失败");
  }
  return envelope;
}

export async function mockRun(args: string[]): Promise<CoreEnvelope> {
  const [command, subcommand] = args;
  const valueAfter = (flag: string) => args.includes(flag) ? args[args.indexOf(flag) + 1] : null;
  if (command === "import") {
    const imported = structuredClone(sampleProject);
    const sourcePath = args[1] ?? "demo.mp4";
    imported.id = `p-imported-${Date.now()}`;
    imported.title = sourcePath.split(/[\\/]/).at(-1)?.replace(/\.[^.]+$/, "") || "新项目";
    imported.media.sourcePath = sourcePath;
    imported.updatedAt = new Date().toISOString();
    mockProject = imported;
    mockProjects = [imported, ...mockProjects.filter((item) => item.id !== imported.id)];
    resetMockHistory(imported);
    return { apiVersion: "0.1", status: "ok", project: structuredClone(imported), message: "本地媒体已导入。" };
  }
  if (command === "project" && subcommand === "list") {
    mockProject = structuredClone(sampleProject);
    const secondary = structuredClone(sampleProject);
    secondary.id = "p_secondary";
    secondary.title = "第二个本地项目";
    secondary.updatedAt = "2026-07-16T09:00:00Z";
    secondary.transcript = { sourceLanguage: "en", segments: [{ id: "s-secondary", start: 1, end: 2, text: "Second project subtitle", confidence: 0.98 }], words: [] };
    secondary.subtitleQuality = { status: "good", statusLabel: "未发现字幕问题", issueCount: 0, errorCount: 0, warningCount: 0, thresholds: { maxDurationSeconds: 8, maxLineCharacters: 42, maxCharactersPerSecond: 20, minGapSeconds: 0.12, maxLines: 2 }, issues: [] };
    secondary.speechInsights = { status: "insufficient_evidence", analyzerVersion: "rhythm-v1", thresholds: { pauseSeconds: 0.8, longPauseSeconds: 1.5, lowConfidence: 0.75 }, spanDurationSeconds: 0, spokenDurationSeconds: 0, tokenCount: 0, tokensPerMinute: 0, pauseCount: 0, longPauseCount: 0, totalPauseDurationSeconds: 0, fillerCount: 0, lowConfidenceCount: 0, pauses: [], evidence: [] };
    secondary.translations = {};
    secondary.edits = [];
    secondary.tasks = [];
    secondary.patchSets = [];
    secondary.workflows = [];
    secondary.versions = [];
    secondary.history = { canUndo: false, canRedo: false, currentVersionId: null };
    mockProjects = [mockProject, secondary];
    resetMockHistory(mockProject);
    mockJobs.clear();
    mockSourceJobs.clear();
    mockSourcePolls.clear();
    mockAutoWorkflows.clear();
    mockAutoPolls.clear();
    mockAudioJobs.clear();
    mockSpeakerJobs.clear();
    mockSpeakerTracks.clear();
    mockTranscriptionJobs.clear();
    mockAgentRuns.clear();
    mockAgentPolls.clear();
    mockTranscriptionReviews = [];
    mockStructureCounter = 0;
    mockSpeakerPackage = { ...mockSpeakerPackage, installed: false, verified: null, verificationStatus: "not_installed", assets: speakerAssets.map((asset) => ({ ...asset, installed: false, verified: null, verificationStatus: "not_installed" as const })) };
    return { apiVersion: "0.1", status: "ok", projects: mockProjects };
  }
  if (command === "project" && subcommand === "show") {
    const nextProject = mockProjects.find((project) => project.id === args[2]) ?? mockProject;
    if (nextProject.id !== mockProject.id) {
      mockProject = structuredClone(nextProject);
      resetMockHistory(mockProject);
    }
    return { apiVersion: "0.1", status: "ok", project: structuredClone(mockProject) };
  }
  if (command === "project" && subcommand === "delete-preflight") {
    const candidate = mockProjects.find((project) => project.id === args[2]);
    const blockers: Array<{ kind: string; id: string; status: string }> = candidate?.tasks.filter((task) => ["queued", "claimed", "running"].includes(task.status)).map((task) => ({ kind: "agent_task", id: task.id, status: task.status })) ?? [];
    for (const job of mockTranscriptionJobs.values()) {
      if (job.projectId === args[2] && ["queued", "running", "finalizing", "awaiting_apply"].includes(job.status))
        blockers.push({ kind: "transcription", id: job.id, status: job.status });
    }
    return { apiVersion: "0.1", status: "ok", deletionPreflight: { projectId: args[2], deletable: blockers.length === 0, blockers } };
  }
  if (command === "project" && subcommand === "delete") {
    mockProjects = mockProjects.filter((project) => project.id !== args[2]);
    mockProject = mockProjects[0] ?? structuredClone(sampleProject);
    return { apiVersion: "0.1", status: "ok", projectId: args[2], message: "项目已删除；原始媒体文件未被修改。" };
  }
  if (command === "project" && subcommand === "restore") {
    mockProject = structuredClone(sampleProject);
    return { apiVersion: "0.1", status: "ok", project: mockProject, message: "已恢复版本。" };
  }
  if (command === "project" && ["undo", "redo"].includes(subcommand)) {
    if (subcommand === "undo") {
      const previous = mockUndoStack.pop();
      if (!previous) throw new Error("没有可撤销的项目修改");
      mockRedoStack.push(structuredClone(mockProject));
      const restored = structuredClone(previous);
      restored.history = { ...restored.history, canUndo: mockUndoStack.length > 0, canRedo: true };
      return { apiVersion: "0.1", status: "ok", project: syncMockProject(restored) };
    }
    const next = mockRedoStack.pop();
    if (!next) throw new Error("没有可重做的项目修改");
    mockUndoStack.push(structuredClone(mockProject));
    const restored = structuredClone(next);
    restored.history = { ...restored.history, canUndo: true, canRedo: mockRedoStack.length > 0 };
    return { apiVersion: "0.1", status: "ok", project: syncMockProject(restored) };
  }
  if (command === "project" && subcommand === "relink") {
    const relinked = structuredClone(mockProject);
    relinked.media.sourcePath = args[3] ?? "demo.mp4";
    relinked.updatedAt = new Date().toISOString();
    mockProject = relinked;
    mockProjects = mockProjects.map((item) => item.id === relinked.id ? relinked : item);
    return { apiVersion: "0.1", status: "ok", project: structuredClone(relinked), message: "已重新定位原片，并校验内容与项目记录一致。" };
  }
  if (command === "canvas" && subcommand === "set") {
    mockProject.canvasSettings = {
      aspectRatio: args[args.indexOf("--aspect-ratio") + 1] as Project["canvasSettings"]["aspectRatio"],
      framing: args[args.indexOf("--framing") + 1] as Project["canvasSettings"]["framing"],
    };
    if (mockProject.mediaArtifacts) mockProject.mediaArtifacts.status = "stale";
    return { apiVersion: "0.1", status: "ok", project: mockProject, canvasSettings: mockProject.canvasSettings };
  }
  if (command === "transcript" && subcommand === "style") {
    return { apiVersion: "0.1", status: "ok", subtitleStyle: mockProject.subtitleStyle, subtitleStylePresets: mockSubtitleStylePresets };
  }
  if (command === "transcript" && subcommand === "set-style") {
    const preset = args[args.indexOf("--preset") + 1] as Project["subtitleStyle"]["preset"];
    const position = args[args.indexOf("--position") + 1] as Project["subtitleStyle"]["position"];
    mockProject.subtitleStyle = resolveMockSubtitleStyle(preset, position);
    const versionId = `v${mockProject.versions.length + 1}`;
    mockProject.versions.push({ id: versionId, reason: "更新字幕样式", createdAt: new Date().toISOString() });
    mockProject.history = { canUndo: true, canRedo: false, currentVersionId: versionId };
    mockProjects = mockProjects.map((item) => item.id === mockProject.id ? mockProject : item);
    return { apiVersion: "0.1", status: "ok", project: mockProject, subtitleStyle: mockProject.subtitleStyle, subtitleStylePresets: mockSubtitleStylePresets, message: "字幕样式已更新；正文和时间未修改，可通过项目历史撤销。" };
  }
  if (command === "transcript" && subcommand === "edit") {
    const segmentId = args[3];
    const text = args[args.indexOf("--text") + 1];
    const next = structuredClone(mockProject);
    const current = next.transcript.segments.find((segment) => segment.id === segmentId);
    if (!current) throw new Error("字幕段不存在");
    if (current.text === text) return { apiVersion: "0.1", status: "ok", project: structuredClone(mockProject), message: "字幕内容未变化。" };
    recordMockSnapshot();
    next.transcript.segments = next.transcript.segments.map((segment) => segment.id === segmentId ? { ...segment, text } : segment);
    markTextDependentsStale(next, [segmentId]);
    next.history = { ...next.history, canUndo: true, canRedo: false };
    return { apiVersion: "0.1", status: "ok", project: syncMockProject(next), message: "原文已更新；已有译文已标记为待更新。" };
  }
  if (command === "transcript" && subcommand === "replace") {
    const find = args[args.indexOf("--find") + 1];
    const replacement = args[args.indexOf("--replace") + 1];
    let changedSegments = 0;
    const next = structuredClone(mockProject);
    const changedSegmentIds: string[] = [];
    next.transcript.segments = next.transcript.segments.map((segment) => {
      if (!segment.text.includes(find)) return segment;
      changedSegments += 1;
      changedSegmentIds.push(segment.id);
      return { ...segment, text: segment.text.replaceAll(find, replacement) };
    });
    if (!changedSegments) return { apiVersion: "0.1", status: "ok", project: structuredClone(mockProject), changedSegments };
    recordMockSnapshot();
    markTextDependentsStale(next, changedSegmentIds);
    next.history = { ...next.history, canUndo: true, canRedo: false };
    return { apiVersion: "0.1", status: "ok", project: syncMockProject(next), changedSegments };
  }
  if (command === "transcript" && subcommand === "split") {
    const segmentId = args[3];
    const segmentIndex = mockProject.transcript.segments.findIndex((segment) => segment.id === segmentId);
    const current = mockProject.transcript.segments[segmentIndex];
    if (!current) throw new Error("字幕段不存在");
    const textOffset = Number(args[args.indexOf("--text-offset") + 1]);
    const at = Number(args[args.indexOf("--at") + 1]);
    const characters = Array.from(current.text);
    if (textOffset <= 0 || textOffset >= characters.length || at <= current.start || at >= current.end) throw new Error("拆分位置必须位于字幕段内部");
    const leftText = characters.slice(0, textOffset).join("").trim();
    const rightText = characters.slice(textOffset).join("").trim();
    if (!/[\p{L}\p{N}\p{S}]/u.test(leftText) || !/[\p{L}\p{N}\p{S}]/u.test(rightText)) throw new Error("拆分后的两段字幕都必须包含文字或数字");
    recordMockSnapshot();
    mockStructureCounter += 1;
    const createdId = `s-structure-${mockStructureCounter}`;
    const left = { ...current, end: at, text: leftText };
    const right = { ...current, id: createdId, start: at, text: rightText };
    mockProject.transcript.segments.splice(segmentIndex, 1, left, right);
    let reassigned = 0;
    mockProject.transcript.words.forEach((word) => { if (word.segmentId === segmentId && word.start >= at) { word.segmentId = createdId; reassigned += 1; } });
    Object.values(mockProject.translations).forEach((translation) => {
      const before = translation.segments.length;
      translation.segments = translation.segments.filter((item) => item.segmentId !== segmentId);
      if (before !== translation.segments.length) translation.status = "stale";
    });
    const impact = mockStructureImpact();
    impact.wordsReassigned = reassigned;
    return finishMockStructureEdit("split", [segmentId, createdId], createdId, [], impact);
  }
  if (command === "transcript" && subcommand === "merge") {
    const requested = [args[3], args[4]];
    const indexes = requested.map((id) => mockProject.transcript.segments.findIndex((segment) => segment.id === id)).sort((left, right) => left - right);
    if (indexes.some((index) => index < 0) || indexes[1] - indexes[0] !== 1) throw new Error("只允许合并相邻字幕段");
    const left = mockProject.transcript.segments[indexes[0]];
    const right = mockProject.transcript.segments[indexes[1]];
    const separatorIndex = args.indexOf("--separator");
    const separator = separatorIndex >= 0 ? args[separatorIndex + 1] : " ";
    recordMockSnapshot();
    left.end = right.end;
    left.text = `${left.text.trimEnd()}${separator}${right.text.trimStart()}`;
    mockProject.transcript.segments.splice(indexes[1], 1);
    let reassigned = 0;
    mockProject.transcript.words.forEach((word) => { if (word.segmentId === right.id) { word.segmentId = left.id; reassigned += 1; } });
    Object.values(mockProject.translations).forEach((translation) => {
      const before = translation.segments.length;
      translation.segments = translation.segments.filter((item) => ![left.id, right.id].includes(item.segmentId));
      if (before !== translation.segments.length) translation.status = "stale";
    });
    const impact = mockStructureImpact();
    impact.wordsReassigned = reassigned;
    return finishMockStructureEdit("merge", [left.id], null, [right.id], impact);
  }
  if (command === "transcript" && subcommand === "timing") {
    const segment = mockProject.transcript.segments.find((item) => item.id === args[3]);
    if (!segment) throw new Error("字幕段不存在");
    const start = Number(args[args.indexOf("--start") + 1]);
    const end = Number(args[args.indexOf("--end") + 1]);
    if (!Number.isFinite(start) || !Number.isFinite(end) || start < 0 || end <= start) throw new Error("字幕时间范围无效");
    if (Math.abs(start - segment.start) < 0.0005 && Math.abs(end - segment.end) < 0.0005) throw new Error("字幕时间没有变化");
    recordMockSnapshot();
    segment.start = start;
    segment.end = end;
    return finishMockStructureEdit("timing", [segment.id], null, []);
  }
  if (command === "transcript" && subcommand === "offset") {
    const segmentIds = args.flatMap((value, index) => value === "--segment" ? [args[index + 1]] : []);
    const delta = Number(args[args.indexOf("--delta") + 1]);
    if (!Number.isFinite(delta) || delta === 0 || !segmentIds.length) throw new Error("批量偏移参数无效");
    for (const segment of mockProject.transcript.segments) {
      if (segmentIds.includes(segment.id) && segment.start + delta < 0) throw new Error("偏移后字幕时间无效");
    }
    recordMockSnapshot();
    for (const segment of mockProject.transcript.segments) {
      if (!segmentIds.includes(segment.id)) continue;
      segment.start += delta;
      segment.end += delta;
    }
    let shifted = 0;
    mockProject.transcript.words.forEach((word) => { if (segmentIds.includes(word.segmentId)) { word.start += delta; word.end += delta; shifted += 1; } });
    const impact = mockStructureImpact();
    impact.wordsShifted = shifted;
    return finishMockStructureEdit("offset", segmentIds, null, [], impact);
  }
  if (command === "transcript" && subcommand === "inspect-file") {
    return { apiVersion: "0.1", status: "ok", subtitleImportPreview: structuredClone(mockSubtitlePreview), message: "字幕文件已预检；尚未写入项目。" };
  }
  if (command === "transcript" && subcommand === "import-file") {
    if (!args.includes("--confirm-replace")) throw new Error("替换项目字幕需要显式确认");
    const nextProject = structuredClone(mockProject);
    nextProject.transcript = {
      sourceLanguage: nextProject.transcript.sourceLanguage,
      segments: mockSubtitlePreview.segments.map((segment, index) => ({ ...segment, id: `imported-${index + 1}` })),
      words: [],
    };
    nextProject.subtitleQuality = {
      ...structuredClone(mockSubtitlePreview.quality),
      issues: mockSubtitlePreview.quality.issues.map((issue) => ({ ...issue, segmentId: "imported-2", relatedSegmentId: "imported-1" })),
    };
    Object.values(nextProject.translations).forEach((translation) => { translation.status = "stale"; translation.segments = []; });
    nextProject.edits = [];
    nextProject.patchSets.forEach((set) => set.items.forEach((item) => { item.segmentId = null; }));
    nextProject.versions.push({ id: `v${nextProject.versions.length + 1}`, reason: "导入 Srt 字幕", createdAt: new Date().toISOString() });
    nextProject.history = { canUndo: true, canRedo: false, currentVersionId: nextProject.versions.at(-1)?.id ?? null };
    mockProject = nextProject;
    mockProjects = mockProjects.map((item) => item.id === mockProject.id ? mockProject : item);
    return { apiVersion: "0.1", status: "ok", project: mockProject, subtitleImport: { format: "srt", sha256: mockSubtitlePreview.sha256, insertedSegments: 2, quality: mockProject.subtitleQuality, project: mockProject }, message: "字幕已替换并创建可撤销版本；原片和既有导出文件未修改。" };
  }
  if (command === "transcript" && subcommand === "quality") return { apiVersion: "0.1", status: "ok", subtitleQuality: mockProject.subtitleQuality };
  if (command === "transcript" && subcommand === "export") {
    return { apiVersion: "0.1", status: "ok", projectId: mockProject.id, outputPath: valueAfter("--output"), format: valueAfter("--format"), message: "字幕已导出。" };
  }
  if (command === "transcribe") {
    const language = valueAfter("--language");
    if (language === "en" || language === "zh") mockProject.transcript.sourceLanguage = language;
    return { apiVersion: "0.1", status: "ok", project: mockProject, message: "已完成本地转录。" };
  }
  if (command === "speech" && subcommand === "analyze") return { apiVersion: "0.1", status: "ok", projectId: mockProject.id, speechInsights: mockProject.speechInsights, message: "已根据本机词级时间生成语音节奏分析。" };
  if (command === "speech" && subcommand === "audio-start") {
    const now = new Date().toISOString();
    const job: AudioAnalysisJob = {
      id: `audio-${Date.now()}`,
      projectId: args[2],
      status: "completed",
      progress: 1,
      report: {
        analyzerVersion: "ffmpeg-audio-v1",
        toolVersion: "ffmpeg version 7.1",
        durationSeconds: 278,
        integratedLoudnessLufs: -25.4,
        truePeakDbfs: -0.1,
        silenceDurationSeconds: 1.3,
        thresholds: { silenceNoiseDb: -40, silenceMinSeconds: 0.8, clippingPeakDbfs: -0.1, quietLoudnessLufs: -24, loudLoudnessLufs: -14 },
        risks: [
          { kind: "silence", start: 18.7, end: 20, measuredValue: 1.3, threshold: 0.8, unit: "seconds", toolVersion: "ffmpeg version 7.1" },
          { kind: "suspected_clipping", start: 0, end: 278, measuredValue: -0.1, threshold: -0.1, unit: "dBFS", toolVersion: "ffmpeg version 7.1" },
          { kind: "loudness_low", start: 0, end: 278, measuredValue: -25.4, threshold: -24, unit: "LUFS", toolVersion: "ffmpeg version 7.1" },
        ],
      },
      cancelRequestedAt: null,
      errorMessage: null,
      createdAt: now,
      updatedAt: now,
      completedAt: now,
      attemptCount: 1,
    };
    mockAudioJobs.set(job.id, job);
    return { apiVersion: "0.1", status: "ok", audioAnalysisJob: job };
  }
  if (command === "speech" && subcommand === "audio-latest") {
    const job = Array.from(mockAudioJobs.values()).reverse().find((candidate) => candidate.projectId === args[2]) ?? null;
    return { apiVersion: "0.1", status: "ok", audioAnalysisJob: job };
  }
  if (command === "speech" && subcommand === "audio-status") return { apiVersion: "0.1", status: "ok", audioAnalysisJob: mockAudioJobs.get(args[2]) ?? null };
  if (command === "speech" && subcommand === "audio-cancel") {
    const job = mockAudioJobs.get(args[2]);
    if (job) { job.status = "cancelled"; job.cancelRequestedAt = new Date().toISOString(); }
    return { apiVersion: "0.1", status: "ok", audioAnalysisJob: job ?? null };
  }
  if (command === "speech" && subcommand === "audio-resume") {
    const job = mockAudioJobs.get(args[2]);
    if (job) { job.status = "completed"; job.progress = 1; job.cancelRequestedAt = null; job.attemptCount += 1; }
    return { apiVersion: "0.1", status: "ok", audioAnalysisJob: job ?? null };
  }
  if (command === "media" && subcommand === "prepare") {
    mockProject.mediaArtifacts = { status: "ready", proxyPath: "proxy.mp4", waveformPath: "waveform.png", thumbnails: [], sourceSha256: "demo", updatedAt: new Date().toISOString(), errorMessage: null };
    return { apiVersion: "0.1", status: "ok", project: mockProject, message: "预览资源已生成；原片未修改。" };
  }
  if (command === "cut" && ["apply", "restore"].includes(subcommand)) {
    const edit = mockProject.edits.find((candidate) => candidate.id === args[3]);
    if (edit) edit.status = subcommand === "apply" ? "applied" : "restored";
    updateMockTimeline();
    return { apiVersion: "0.1", status: "ok", project: mockProject };
  }
  if (command === "cut" && subcommand === "detect") {
    const existing = mockProject.edits.find((edit) => edit.id === "e-detected");
    if (existing) return { apiVersion: "0.1", status: "ok", project: mockProject, suggestions: [] };
    const cut: Project["edits"][number] = {
      id: "e-detected",
      kind: "word_cut",
      status: "proposed",
      segmentId: "s4",
      start: 24.4,
      end: 25.0,
      reason: "说话重启：你可以",
      cutRange: { fromWordId: "w3", toWordId: "w3", selectedStart: 24.4, selectedEnd: 24.9, paddingMs: 100, transcriptHash: "demo", stale: false },
      suggestion: { suggestionType: "speech_restart", confidence: 0.96, detectorVersion: "heuristic-v1" },
    };
    mockProject.edits.push(cut);
    return { apiVersion: "0.1", status: "ok", project: mockProject, suggestions: [cut] };
  }
  if (command === "cut" && subcommand === "create") {
    const segmentId = args[args.indexOf("--segment") + 1];
    const fromWordId = args[args.indexOf("--from-word") + 1];
    const toWordId = args[args.indexOf("--to-word") + 1];
    const paddingMs = Number(args[args.indexOf("--padding-ms") + 1]);
    const words = mockProject.transcript.words.filter((word) => word.segmentId === segmentId);
    const from = words.find((word) => word.id === fromWordId)!;
    const to = words.find((word) => word.id === toWordId)!;
    const cut: Project["edits"][number] = {
      id: `e${mockProject.edits.length + 1}`,
      kind: "word_cut",
      status: "proposed",
      segmentId,
      start: Math.max(0, from.start - paddingMs / 1000),
      end: to.end + paddingMs / 1000,
      reason: `词范围：${words.slice(words.indexOf(from), words.indexOf(to) + 1).map((word) => word.text).join("")}`,
      cutRange: { fromWordId, toWordId, selectedStart: from.start, selectedEnd: to.end, paddingMs, transcriptHash: "demo", stale: false },
    };
    mockProject.edits.push(cut);
    return { apiVersion: "0.1", status: "ok", project: mockProject, cut };
  }
  if (command === "cut" && subcommand === "preview") {
    const cut = mockProject.edits.find((candidate) => candidate.id === args[3])!;
    return { apiVersion: "0.1", status: "ok", preview: { cutId: cut.id, previewStart: Math.max(0, cut.start - 1), cutStart: cut.start, cutEnd: cut.end, previewEnd: Math.min(mockProject.timeline.sourceDuration, cut.end + 1), skipRange: true } };
  }
  if (command === "video" && subcommand === "export") {
    const id = `x${mockJobs.size + 1}`;
    const now = new Date().toISOString();
    const subtitleModeIndex = args.indexOf("--subtitle-mode");
    const subtitleMode = (subtitleModeIndex >= 0 ? args[subtitleModeIndex + 1] : "source") as ExportJob["subtitleMode"];
    const language = args.includes("--lang") ? args[args.indexOf("--lang") + 1] : null;
    const job: ExportJob = { id, projectId: mockProject.id, outputPath: args[args.indexOf("--output") + 1], status: "completed", progress: 1, burnSubtitles: args.includes("--burn-subtitles"), language, bilingual: subtitleMode === "bilingual", subtitleMode, canvasSettings: structuredClone(mockProject.canvasSettings), subtitleStyle: structuredClone(mockProject.subtitleStyle), cancelRequestedAt: null, errorMessage: null, manifestPath: "demo.siaocut.json", createdAt: now, updatedAt: now, completedAt: now };
    mockJobs.set(id, job);
    return { apiVersion: "0.1", status: "ok", job, jobId: id };
  }
  if (command === "video" && subcommand === "status") return { apiVersion: "0.1", status: "ok", job: mockJobs.get(args[2]) };
  if (command === "video" && subcommand === "list") return { apiVersion: "0.1", status: "ok", jobs: Array.from(mockJobs.values()).reverse() };
  if (command === "video" && subcommand === "retry") {
    const job = mockJobs.get(args[2]);
    if (job) { job.status = "completed"; job.progress = 1; job.errorMessage = null; }
    return { apiVersion: "0.1", status: "ok", job };
  }
  if (command === "model" && subcommand === "list") return { apiVersion: "0.1", status: "ok", models: structuredClone(mockModels) };
  if (command === "model" && subcommand === "jobs") return { apiVersion: "0.1", status: "ok", modelJobs: Array.from(mockModelJobs.values()).reverse() };
  if (command === "model" && subcommand === "install") {
    const model = mockModels.find((item) => item.id === args[2])!;
    model.installed = true;
    model.bytesOnDisk = model.size;
    model.verified = true;
    model.verificationStatus = "verified";
    const now = new Date().toISOString();
    const job: ModelDownloadJob = { id: `m${mockModelJobs.size + 1}`, modelId: model.id, status: "completed", progress: 1, bytesDownloaded: model.size, totalBytes: model.size, targetPath: model.path, cancelRequestedAt: null, errorMessage: null, createdAt: now, updatedAt: now, completedAt: now };
    mockModelJobs.set(job.id, job);
    return { apiVersion: "0.1", status: "ok", modelJob: job };
  }
  if (command === "model" && subcommand === "status") return { apiVersion: "0.1", status: "ok", modelJob: mockModelJobs.get(args[2]) };
  if (command === "model" && subcommand === "remove") {
    const model = mockModels.find((item) => item.id === args[2]);
    if (model) { model.installed = false; model.bytesOnDisk = 0; model.verified = null; model.verificationStatus = "not_installed"; }
    return { apiVersion: "0.1", status: "ok" };
  }
  if (command === "speaker" && subcommand === "package") return { apiVersion: "0.1", status: "ok", speakerPackage: structuredClone(mockSpeakerPackage) };
  if (command === "speaker" && subcommand === "jobs") return { apiVersion: "0.1", status: "ok", speakerJobs: Array.from(mockSpeakerJobs.values()).reverse() };
  if (command === "speaker" && subcommand === "install") {
    mockSpeakerPackage = { ...mockSpeakerPackage, installed: true, verified: true, verificationStatus: "verified", assets: mockSpeakerPackage.assets.map((asset) => ({ ...asset, installed: true, verified: true, verificationStatus: "verified" })) };
    const timestamp = new Date().toISOString();
    const job: SpeakerJob = { id: `speaker-install-${Date.now()}`, kind: "install", projectId: null, status: "completed", stage: "完成", progress: 1, bytesDownloaded: mockSpeakerPackage.downloadSize, totalBytes: mockSpeakerPackage.downloadSize, cancelRequestedAt: null, errorMessage: null, createdAt: timestamp, updatedAt: timestamp, completedAt: timestamp, attemptCount: 1 };
    mockSpeakerJobs.set(job.id, job);
    return { apiVersion: "0.1", status: "ok", speakerJob: job };
  }
  if (command === "speaker" && subcommand === "track") return { apiVersion: "0.1", status: "ok", speakerTrack: structuredClone(mockSpeakerTracks.get(args[2]) ?? emptySpeakerTrack()) };
  if (command === "speaker" && subcommand === "analyze") {
    const timestamp = new Date().toISOString();
    const track = analyzedSpeakerTrack();
    mockSpeakerTracks.set(args[2], track);
    const job: SpeakerJob = { id: `speaker-analyze-${Date.now()}`, kind: "analyze", projectId: args[2], status: "completed", stage: "完成", progress: 1, bytesDownloaded: 0, totalBytes: 0, cancelRequestedAt: null, errorMessage: null, createdAt: timestamp, updatedAt: timestamp, completedAt: timestamp, attemptCount: 1 };
    mockSpeakerJobs.set(job.id, job);
    return { apiVersion: "0.1", status: "ok", speakerJob: job };
  }
  if (command === "speaker" && subcommand === "job-status") return { apiVersion: "0.1", status: "ok", speakerJob: mockSpeakerJobs.get(args[2]) };
  if (command === "speaker" && subcommand === "cancel") {
    const job = mockSpeakerJobs.get(args[2]);
    if (job) { job.status = "cancelled"; job.cancelRequestedAt = new Date().toISOString(); }
    return { apiVersion: "0.1", status: "ok", speakerJob: job };
  }
  if (command === "speaker" && subcommand === "resume") {
    const job = mockSpeakerJobs.get(args[2]);
    if (job) { job.status = "completed"; job.progress = 1; job.stage = "完成"; job.cancelRequestedAt = null; job.attemptCount += 1; }
    return { apiVersion: "0.1", status: "ok", speakerJob: job };
  }
  if (command === "speaker" && subcommand === "rename") {
    const track = mockSpeakerTracks.get(args[2]) ?? analyzedSpeakerTrack();
    const speaker = track.speakers.find((item) => item.id === args[3]);
    if (speaker) speaker.label = args[args.indexOf("--name") + 1];
    mockSpeakerTracks.set(args[2], track);
    return { apiVersion: "0.1", status: "ok", speakerTrack: structuredClone(track) };
  }
  if (command === "speaker" && subcommand === "merge") {
    const track = mockSpeakerTracks.get(args[2]) ?? analyzedSpeakerTrack();
    const from = args[args.indexOf("--from") + 1];
    const into = args[args.indexOf("--into") + 1];
    track.turns.forEach((turn) => { if (turn.speakerId === from) turn.speakerId = into; });
    track.associations.forEach((association) => { if (association.speakerId === from) { association.speakerId = into; association.source = "manual"; association.confidence = null; } });
    track.speakers = track.speakers.filter((speaker) => speaker.id !== from);
    mockSpeakerTracks.set(args[2], track);
    return { apiVersion: "0.1", status: "ok", speakerTrack: structuredClone(track) };
  }
  if (command === "speaker" && subcommand === "assign") {
    const track = mockSpeakerTracks.get(args[2]) ?? analyzedSpeakerTrack();
    const association = track.associations.find((item) => item.segmentId === args[3]);
    if (association) { association.speakerId = args[4]; association.source = "manual"; association.confidence = null; }
    mockSpeakerTracks.set(args[2], track);
    return { apiVersion: "0.1", status: "ok", speakerTrack: structuredClone(track) };
  }
  if (command === "transcription" && subcommand === "providers") return { apiVersion: "0.1", status: "ok", providers: [{ id: "whisper_cpp", role: "quick", isDefault: true, wordTimings: true, integratedDiarization: false }, { id: "moss_openai", role: "multispeaker_longform", isDefault: false, wordTimings: false, integratedDiarization: true, config: structuredClone(mockTranscriptionConfig) }] };
  if (command === "transcription" && subcommand === "configure") {
    mockTranscriptionConfig = { providerId: "moss_openai", endpoint: valueAfter("--endpoint") ?? mockTranscriptionConfig.endpoint, modelId: valueAfter("--model") ?? mockTranscriptionConfig.modelId, updatedAt: new Date().toISOString() };
    mockTranscriptionHealth = { ...mockTranscriptionConfig, state: "healthy", detail: "本机 MOSS 服务可用。", checkedAt: new Date().toISOString() };
    return { apiVersion: "0.1", status: "ok", config: structuredClone(mockTranscriptionConfig) };
  }
  if (command === "transcription" && subcommand === "health") return { apiVersion: "0.1", status: "ok", providerHealth: structuredClone(mockTranscriptionHealth) };
  if (command === "transcription" && subcommand === "latest") return { apiVersion: "0.1", status: "ok", transcriptionJob: Array.from(mockTranscriptionJobs.values()).filter((job) => job.projectId === args[2]).at(-1) ?? null };
  if (command === "transcription" && subcommand === "jobs") return { apiVersion: "0.1", status: "ok", transcriptionJobs: Array.from(mockTranscriptionJobs.values()).reverse() };
  if (command === "transcription" && subcommand === "start") {
    const timestamp = new Date().toISOString();
    const awaitingApply = valueAfter("--prompt") === "simulate-conflict";
    const currentVersionId = mockProject.history.currentVersionId ?? mockProject.versions.at(-1)?.id ?? "mock-current-version";
    const job: TranscriptionJob = { id: `transcription-${Date.now()}`, projectId: args[2], providerId: "moss_openai", endpoint: mockTranscriptionConfig.endpoint, modelId: mockTranscriptionConfig.modelId, language: valueAfter("--language"), prompt: valueAfter("--prompt"), hotwords: [], status: awaitingApply ? "awaiting_apply" : "completed", stage: awaitingApply ? "awaiting_apply" : "completed", resultRunId: "trun-demo", baseVersionId: awaitingApply ? "mock-base-version" : currentVersionId, sourceSha256: "mock-source", inputAudioSha256: "mock-audio", candidate: awaitingApply ? { runId: "trun-demo", segmentCount: 18, speakerCount: 3, durationSeconds: 142.4, warningCount: 2, baseVersionId: "mock-base-version", currentVersionId, canApply: true } : null, cancelRequestedAt: null, errorMessage: null, createdAt: timestamp, updatedAt: timestamp, completedAt: awaitingApply ? null : timestamp, attemptCount: 1 };
    mockTranscriptionJobs.set(job.id, job);
    const track = analyzedSpeakerTrack();
    track.providerId = "moss_openai"; track.modelId = mockTranscriptionConfig.modelId; track.sourceKind = "end_to_end"; track.runtimeVersion = "openai-compatible-loopback-v1";
    if (!awaitingApply) {
      mockSpeakerTracks.set(args[2], track);
      mockTranscriptionReviews = [{ id: "review-demo", projectId: args[2], runId: "trun-demo", segmentId: "s3", severity: "warning", kind: "rapid_speaker_switch", message: "这里发生快速说话人切换，请人工确认人物归属。", status: "open", createdAt: timestamp, resolvedAt: null }];
    }
    return { apiVersion: "0.1", status: "ok", transcriptionJob: structuredClone(job) };
  }
  if (command === "transcription" && subcommand === "status") return { apiVersion: "0.1", status: "ok", transcriptionJob: structuredClone(mockTranscriptionJobs.get(args[2]) ?? null) };
  if (command === "transcription" && subcommand === "cancel") { const job = mockTranscriptionJobs.get(args[2]); if (job) job.status = "cancelled"; return { apiVersion: "0.1", status: "ok", transcriptionJob: structuredClone(job) }; }
  if (command === "transcription" && subcommand === "resume") { const job = mockTranscriptionJobs.get(args[2]); if (job) { job.status = "completed"; job.stage = "completed"; job.attemptCount += 1; } return { apiVersion: "0.1", status: "ok", transcriptionJob: structuredClone(job) }; }
  if (command === "transcription" && subcommand === "apply") {
    const job = mockTranscriptionJobs.get(args[2]);
    if (job?.candidate && args.includes("--confirm-replace") && valueAfter("--expected-version") === job.candidate.currentVersionId) {
      recordMockSnapshot();
      mockProject.transcript.segments = [{ id: "moss-candidate-1", start: 0, end: 8.4, text: "这是经过明确确认后应用的多人转写候选结果。", confidence: 0.96 }];
      mockProject.history = { ...mockProject.history, canUndo: true, currentVersionId: `mock-applied-${Date.now()}` };
      syncMockProject(mockProject);
      const track = analyzedSpeakerTrack();
      track.providerId = "moss_openai"; track.modelId = mockTranscriptionConfig.modelId; track.sourceKind = "end_to_end"; track.runtimeVersion = "openai-compatible-loopback-v1";
      mockSpeakerTracks.set(job.projectId, track);
      mockTranscriptionReviews = [{ id: "review-demo", projectId: job.projectId, runId: "trun-demo", segmentId: "moss-candidate-1", severity: "warning", kind: "rapid_speaker_switch", message: "这里发生快速说话人切换，请人工确认人物归属。", status: "open", createdAt: new Date().toISOString(), resolvedAt: null }];
      job.status = "completed"; job.stage = "completed"; job.completedAt = new Date().toISOString(); job.candidate = null;
    }
    return { apiVersion: "0.1", status: "ok", transcriptionJob: structuredClone(job) };
  }
  if (command === "transcription" && subcommand === "discard") {
    const job = mockTranscriptionJobs.get(args[2]);
    if (job) { job.status = "discarded"; job.stage = "discarded"; job.candidate = null; job.completedAt = new Date().toISOString(); }
    return { apiVersion: "0.1", status: "ok", transcriptionJob: structuredClone(job) };
  }
  if (command === "transcription" && subcommand === "review") return { apiVersion: "0.1", status: "ok", reviewItems: structuredClone(mockTranscriptionReviews.filter((item) => args.includes("--all") || item.status === "open")) };
  if (command === "transcription" && subcommand === "resolve") { const item = mockTranscriptionReviews.find((candidate) => candidate.id === args[2]); if (item) { item.status = valueAfter("--action") as TranscriptionReviewItem["status"]; item.resolvedAt = new Date().toISOString(); } return { apiVersion: "0.1", status: "ok", reviewItem: structuredClone(item) }; }
  if (command === "transcription" && subcommand === "export") return { apiVersion: "0.1", status: "ok", projectId: args[2], output: valueAfter("--output"), format: valueAfter("--format"), audit: { ready: true, openErrorCount: 0, openWarningCount: mockTranscriptionReviews.filter((item) => item.status === "open" && item.severity === "warning").length, warningsConfirmed: args.includes("--confirm-warnings") } };
  if (command === "source" && subcommand === "inspect") {
    return { apiVersion: "0.1", status: "ok", source: { ...mockSourcePreview, originalUrl: args[2], webpageUrl: args[2] }, message: "已读取公开单视频信息；确认前不会下载或创建项目。" };
  }
  if (command === "source" && subcommand === "jobs") {
    return { apiVersion: "0.1", status: "ok", sourceJobs: Array.from(mockSourceJobs.values()).reverse() };
  }
  if (command === "source" && subcommand === "start") {
    const confirmedId = args[args.indexOf("--confirm-media-id") + 1];
    if (confirmedId !== mockSourcePreview.siteMediaId) {
      return { apiVersion: "0.1", status: "error", error: { code: "source_confirmation_mismatch", message: "站点媒体 ID 已变化，请重新确认。" } };
    }
    const now = new Date().toISOString();
    const job: SourceImportJob = {
      id: `src-${mockSourceJobs.size + 1}`,
      projectId: null,
      originalUrl: args[2],
      webpageUrl: args[2],
      siteMediaId: confirmedId,
      extractor: "youtube",
      title: mockSourcePreview.title,
      durationSeconds: mockSourcePreview.durationSeconds,
      fileSizeBytes: mockSourcePreview.fileSizeBytes,
      status: "running",
      progress: 0.18,
      bytesDownloaded: 2173516,
      totalBytes: mockSourcePreview.fileSizeBytes,
      outputDirectory: "C:\\SiaoCut\\imports\\src-1",
      outputPath: null,
      outputSha256: null,
      toolVersion: mockSourcePreview.toolVersion,
      toolSha256: mockSourcePreview.toolSha256,
      cancelRequestedAt: null,
      errorMessage: null,
      createdAt: now,
      updatedAt: now,
      completedAt: null,
      workerPid: 1234,
      attemptCount: 1,
    };
    mockSourceJobs.set(job.id, job);
    mockSourcePolls.set(job.id, 0);
    return { apiVersion: "0.1", status: "ok", sourceJob: job, sourceJobId: job.id };
  }
  if (command === "source" && subcommand === "status") {
    const job = mockSourceJobs.get(args[2]);
    if (job && ["queued", "running", "finalizing"].includes(job.status)) {
      const polls = (mockSourcePolls.get(job.id) ?? 0) + 1;
      mockSourcePolls.set(job.id, polls);
      if (polls >= 3) {
        mockProject = { ...structuredClone(sampleProject), id: "p-url", title: job.title };
        job.status = "completed";
        job.progress = 1;
        job.bytesDownloaded = 12051566;
        job.totalBytes = 12051566;
        job.projectId = mockProject.id;
        job.outputPath = "C:\\SiaoCut\\imports\\src-1\\source.mp4";
        job.outputSha256 = "8733510bcd314de0149d2e6ea14376f8b654b1d52ceca53beecd46a6dc373031";
        job.completedAt = new Date().toISOString();
      } else {
        job.progress = polls === 1 ? 0.46 : 0.82;
        job.bytesDownloaded = Math.round((job.totalBytes ?? 12075092) * job.progress);
      }
      job.updatedAt = new Date().toISOString();
    }
    return { apiVersion: "0.1", status: "ok", sourceJob: job };
  }
  if (command === "source" && subcommand === "cancel") {
    const job = mockSourceJobs.get(args[2]);
    if (job) {
      job.status = "cancelled";
      job.cancelRequestedAt = new Date().toISOString();
      job.completedAt = job.cancelRequestedAt;
      job.workerPid = null;
    }
    return { apiVersion: "0.1", status: "ok", sourceJob: job };
  }
  if (command === "source" && subcommand === "resume") {
    const job = mockSourceJobs.get(args[2]);
    if (job) {
      job.status = "running";
      job.cancelRequestedAt = null;
      job.completedAt = null;
      job.errorMessage = null;
      job.attemptCount += 1;
      job.workerPid = 5678;
      mockSourcePolls.set(job.id, 0);
    }
    return { apiVersion: "0.1", status: "ok", sourceJob: job };
  }
  if (command === "auto" && subcommand === "list") {
    return { apiVersion: "0.1", status: "ok", workflows: Array.from(mockAutoWorkflows.values()).reverse() };
  }
  if (command === "auto" && subcommand === "start") {
    const media = valueAfter("--media");
    const url = valueAfter("--url");
    const now = new Date().toISOString();
    const workflow: AutoWorkflow = {
      id: `auto-${mockAutoWorkflows.size + 1}`,
      inputKind: url ? "url" : "local",
      inputValue: url ?? media ?? "demo.mp4",
      title: valueAfter("--title"),
      confirmedMediaId: valueAfter("--confirm-media-id"),
      projectId: null,
      sourceImportId: null,
      modelPath: valueAfter("--model") ?? "C:\\Models\\ggml-tiny.bin",
      transcribeLanguage: valueAfter("--language"),
      translationLanguage: valueAfter("--translate"),
      outputPath: valueAfter("--output") ?? "SiaoCut-auto.mp4",
      burnSubtitles: args.includes("--burn-subtitles"),
      subtitleMode: (valueAfter("--subtitle-mode") ?? "source") as AutoWorkflow["subtitleMode"],
      status: "running",
      currentStage: "import",
      progress: 0.08,
      transcriptVersionId: null,
      agentTaskId: null,
      exportJobId: null,
      audit: null,
      cancelRequestedAt: null,
      errorMessage: null,
      createdAt: now,
      updatedAt: now,
      completedAt: null,
      workerPid: 4321,
      attemptCount: 1,
      instructionLocale: (valueAfter("--locale") ?? "zh-CN") as AutoWorkflow["instructionLocale"],
    };
    mockAutoWorkflows.set(workflow.id, workflow);
    mockAutoPolls.set(workflow.id, 0);
    return { apiVersion: "0.1", status: "ok", workflow, workflowId: workflow.id, message: "自动工作流已启动；内容判断阶段仍会暂停等待确认。" };
  }
  if (command === "auto" && subcommand === "status") {
    const workflow = mockAutoWorkflows.get(args[2]);
    if (workflow?.status === "running" && workflow.currentStage === "export") {
      workflow.status = "completed";
      workflow.currentStage = "complete";
      workflow.progress = 1;
      workflow.completedAt = new Date().toISOString();
      workflow.updatedAt = workflow.completedAt;
      workflow.workerPid = null;
    } else if (workflow && ["queued", "running", "needs_agent"].includes(workflow.status)) {
      const polls = (mockAutoPolls.get(workflow.id) ?? 0) + 1;
      mockAutoPolls.set(workflow.id, polls);
      if (workflow.status === "needs_agent") {
        workflow.status = "needs_review";
        workflow.currentStage = "review";
        workflow.progress = 0.68;
      } else if (polls === 1) {
        workflow.currentStage = "transcribe";
        workflow.progress = 0.32;
        workflow.projectId = "p-auto";
        mockProject = { ...structuredClone(sampleProject), id: workflow.projectId, title: workflow.title ?? (workflow.inputKind === "url" ? mockSourcePreview.title : "一键成片项目"), tasks: [], patchSets: [], edits: structuredClone(sampleProject.edits.slice(0, 1)) };
      } else if (polls === 2) {
        workflow.currentStage = "suggestions";
        workflow.progress = 0.52;
        workflow.transcriptVersionId = "v-auto-transcript";
      } else if (workflow.translationLanguage) {
        workflow.status = "needs_agent";
        workflow.currentStage = "translate";
        workflow.progress = 0.58;
        workflow.agentTaskId = "t-auto-translate";
      } else {
        workflow.status = "needs_review";
        workflow.currentStage = "review";
        workflow.progress = 0.68;
      }
      workflow.updatedAt = new Date().toISOString();
    }
    return { apiVersion: "0.1", status: "ok", workflow };
  }
  if (command === "auto" && subcommand === "cancel") {
    const workflow = mockAutoWorkflows.get(args[2]);
    if (workflow) {
      workflow.status = "cancelled";
      workflow.cancelRequestedAt = new Date().toISOString();
      workflow.completedAt = workflow.cancelRequestedAt;
      workflow.updatedAt = workflow.cancelRequestedAt;
      workflow.workerPid = null;
    }
    return { apiVersion: "0.1", status: "ok", workflow };
  }
  if (command === "auto" && subcommand === "continue") {
    const workflow = mockAutoWorkflows.get(args[2]);
    const reviewPending = mockProject.edits.some((edit) => ["suggested", "proposed"].includes(edit.status))
      || mockProject.patchSets.some((set) => set.items.some((item) => ["pending", "conflict"].includes(item.status)));
    if (workflow?.status === "needs_review" && reviewPending) {
      return { apiVersion: "0.1", status: "error", error: { code: "auto_workflow_review_pending", message: "仍有 Agent 修改或粗剪建议等待人工处理" } };
    }
    if (workflow) {
      workflow.status = "running";
      workflow.currentStage = "export";
      workflow.progress = 0.86;
      workflow.errorMessage = null;
      workflow.attemptCount += 1;
      workflow.updatedAt = new Date().toISOString();
      workflow.workerPid = 8765;
    }
    return { apiVersion: "0.1", status: "ok", workflow };
  }
  if (command === "task" && subcommand === "create") {
    mockProject.tasks.push({ id: `t${mockProject.tasks.length + 1}`, kind: args[args.indexOf("--kind") + 1], language: valueAfter("--lang"), status: "queued", progress: 0, errorMessage: null, instructionLocale: (valueAfter("--locale") ?? "zh-CN") as "zh-CN" | "en-US" });
    return { apiVersion: "0.1", status: "ok", project: mockProject, message: "任务已创建，等待 Agent 领取。" };
  }
  if (command === "workflow" && subcommand === "create") {
    const workflowId = `wf${mockProject.workflows.length + 1}`;
    const taskId = `t${mockProject.tasks.length + 1}`;
    const instructionLocale = (valueAfter("--locale") ?? "zh-CN") as "zh-CN" | "en-US";
    const language = valueAfter("--lang");
    mockProject.tasks.push({ id: taskId, kind: args[args.indexOf("--kind") + 1], language, status: "queued", progress: 0, errorMessage: null, workflowId, instructionLocale });
    mockProject.workflows.push({ id: workflowId, kind: args[args.indexOf("--kind") + 1], language, status: "waiting_agent", taskId, createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), instructionLocale });
    return { apiVersion: "0.1", status: "ok", project: mockProject, workflowId, taskId, message: "工作流已创建，需要 Agent 继续。" };
  }
  if (command === "agent" && subcommand === "health") {
    return { apiVersion: "0.1", status: "ok", codex: { available: true, authenticated: true, version: "codex-cli 0.144.5", authMode: "chatgpt" }, message: "Codex CLI 已就绪。" };
  }
  if (command === "agent" && subcommand === "start") {
    const task = mockProject.tasks.find((candidate) => candidate.id === args[2]);
    if (!task) return { apiVersion: "0.1", status: "error", error: { code: "invalid_request", message: "Agent 任务不存在。" } };
    const now = new Date().toISOString();
    const run: AgentRun = {
      id: `agent-run-${mockAgentRuns.size + 1}`,
      taskId: task.id,
      projectId: mockProject.id,
      provider: "codex-cli",
      status: "running",
      baseVersionId: mockProject.history.currentVersionId ?? mockProject.versions.at(-1)?.id ?? "v1",
      progress: 0.08,
      currentBatch: 1,
      batchCount: 1,
      timeoutSeconds: Number(valueAfter("--timeout-seconds") ?? 900),
      cliVersion: "codex-cli 0.144.5",
      authMode: "chatgpt",
      codexThreadId: null,
      cancelRequestedAt: null,
      errorCode: null,
      errorMessage: null,
      createdAt: now,
      updatedAt: now,
      startedAt: now,
      completedAt: null,
      workerPid: 2468,
      attemptCount: 1,
      batches: [{ id: `agent-batch-${mockAgentRuns.size + 1}`, ordinal: 0, status: "running", segmentIds: mockProject.transcript.segments.map((segment) => segment.id), codexThreadId: null, errorCode: null, errorMessage: null, startedAt: now, completedAt: null, attemptCount: 1 }],
    };
    task.status = "running";
    task.progress = run.progress;
    mockAgentRuns.set(run.id, run);
    mockAgentPolls.set(run.id, 0);
    syncMockProject(mockProject);
    return { apiVersion: "0.1", status: "ok", taskId: task.id, agentRunId: run.id, agentRun: structuredClone(run) };
  }
  if (command === "agent" && subcommand === "status") {
    const run = mockAgentRuns.get(args[2]);
    if (run && ["queued", "running", "submitting"].includes(run.status)) {
      const polls = (mockAgentPolls.get(run.id) ?? 0) + 1;
      mockAgentPolls.set(run.id, polls);
      const task = mockProject.tasks.find((candidate) => candidate.id === run.taskId);
      if (polls >= 2) {
        const now = new Date().toISOString();
        run.status = "completed";
        run.progress = 1;
        run.currentBatch = run.batchCount;
        run.completedAt = now;
        run.updatedAt = now;
        run.workerPid = null;
        run.codexThreadId = "mock-codex-thread";
        run.batches = run.batches.map((batch) => ({ ...batch, status: "completed", codexThreadId: "mock-codex-thread", completedAt: now }));
        if (task) {
          task.status = "review";
          task.progress = 1;
        }
        if (!mockProject.patchSets.some((set) => set.taskId === run.taskId)) {
          const segment = mockProject.transcript.segments[0];
          mockProject.patchSets.push({
            id: `patch-${run.id}`,
            taskId: run.taskId,
            kind: task?.kind ?? "polish",
            language: task?.language ?? null,
            status: "pending_review",
            baseVersionId: run.baseVersionId,
            createdAt: now,
            items: [{ id: `patch-item-${run.id}`, segmentId: segment.id, target: "transcript", beforeText: segment.text, afterText: `${segment.text}（已润色）`, currentText: segment.text, reason: "本机 Codex 提供的待审建议", confidence: 0.9, status: "pending" }],
          });
        }
        syncMockProject(mockProject);
      } else {
        run.progress = 0.62;
        run.updatedAt = new Date().toISOString();
        if (task) task.progress = run.progress;
      }
    }
    return { apiVersion: "0.1", status: "ok", agentRunId: run?.id, agentRun: structuredClone(run) };
  }
  if (command === "agent" && subcommand === "list") {
    const projectId = args[2];
    const runs = Array.from(mockAgentRuns.values()).filter((run) => !projectId || run.projectId === projectId).reverse();
    return { apiVersion: "0.1", status: "ok", agentRuns: structuredClone(runs) };
  }
  if (command === "agent" && subcommand === "cancel") {
    const run = mockAgentRuns.get(args[2]);
    if (run) {
      run.status = "cancelled";
      run.cancelRequestedAt = new Date().toISOString();
      run.completedAt = run.cancelRequestedAt;
      run.workerPid = null;
      run.batches = run.batches.map((batch) => ["queued", "running"].includes(batch.status) ? { ...batch, status: "cancelled", completedAt: run.completedAt } : batch);
      const task = mockProject.tasks.find((candidate) => candidate.id === run.taskId);
      if (task) task.status = "cancelled";
      syncMockProject(mockProject);
    }
    return { apiVersion: "0.1", status: "ok", agentRunId: run?.id, agentRun: structuredClone(run) };
  }
  if (command === "agent" && subcommand === "resume") {
    const run = mockAgentRuns.get(args[2]);
    if (run) {
      run.status = "running";
      run.progress = 0.05;
      run.cancelRequestedAt = null;
      run.completedAt = null;
      run.workerPid = 9753;
      run.attemptCount += 1;
      run.updatedAt = new Date().toISOString();
      run.batches = run.batches.map((batch) => ({ ...batch, status: "running", completedAt: null, attemptCount: batch.attemptCount + 1 }));
      mockAgentPolls.set(run.id, 0);
      const task = mockProject.tasks.find((candidate) => candidate.id === run.taskId);
      if (task) task.status = "running";
      syncMockProject(mockProject);
    }
    return { apiVersion: "0.1", status: "ok", agentRunId: run?.id, agentRun: structuredClone(run) };
  }
  if (command === "task" && subcommand === "review") {
    const item = mockProject.patchSets.flatMap((set) => set.items).find((candidate) => candidate.id === args[2]);
    const action = args[args.indexOf("--action") + 1];
    if (item) {
      item.status = action === "apply" ? "applied" : "kept";
      if (action === "apply" && item.segmentId) {
        const segment = mockProject.transcript.segments.find((candidate) => candidate.id === item.segmentId);
        if (segment) segment.text = item.afterText;
        Object.values(mockProject.translations).forEach((translation) => { translation.status = "stale"; });
      }
      const set = mockProject.patchSets.find((candidate) => candidate.items.includes(item));
      if (set) set.status = item.status;
      const task = mockProject.tasks.find((candidate) => candidate.id === set?.taskId);
      if (task) task.status = "done";
    }
    return { apiVersion: "0.1", status: "ok", project: mockProject };
  }
  if (command === "task" && subcommand === "review-all") {
    const set = mockProject.patchSets.find((candidate) => candidate.taskId === args[2]);
    const status = args[args.indexOf("--action") + 1] === "apply" ? "applied" : "kept";
    set?.items.filter((item) => ["pending", "conflict"].includes(item.status)).forEach((item) => { item.status = status; });
    if (set) set.status = status;
    return { apiVersion: "0.1", status: "ok", project: mockProject };
  }
  if (command === "task" && ["retry", "cancel"].includes(subcommand)) {
    const task = mockProject.tasks.find((item) => item.id === args[2]);
    if (task) task.status = subcommand === "retry" ? "queued" : "cancelled";
    return { apiVersion: "0.1", status: "ok", project: mockProject };
  }
  return {
    apiVersion: "0.1",
    status: "error",
    error: {
      code: "unsupported_command",
      message: `浏览器预览未实现命令：${args.join(" ")}`,
      technicalDetails: "Mock Core only returns success for explicitly implemented commands.",
    },
  };
}
