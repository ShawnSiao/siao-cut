import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity, Bot, Check, ChevronDown, ChevronRight, ChevronUp, CircleAlert, Clock3, Cpu, Database, Download, FileVideo2,
  FileText, Film, FolderOpen, FolderPlus, HardDrive, History, Link2, LoaderCircle, Play, RefreshCw, RotateCcw, Search,
  Scissors, Settings2, ShieldCheck, Sparkles, Trash2, Undo2, Redo2, Headphones, ListChecks, MoreHorizontal, MoveHorizontal, Users, X,
} from "lucide-react";
import { authorizeArtifact, authorizeMedia, checkForUpdate, installUpdate, listProjects, loadProject, openLogDirectory, pickMedia, pickModel, pickSubtitleFile, pickTranscriptPath, pickVideoPath, runCore, runtimeInfo, selectAsrBackend, updaterPolicy } from "./core";
import type { AudioAnalysisJob, AudioRisk, AutoWorkflow, CanvasSettings, CutPreview, ExportJob, ModelDownloadJob, ModelStatus, Project, RuntimeInfo, Segment, SourceImportJob, SourcePreview, SpeakerIdentity, SpeakerJob, SpeakerPackageStatus, SpeakerTrack, SpeechEvidence, SpeechInsights, SpeechPause, SubtitleImportPreview, SubtitleQualityIssue, UpdateMetadata, UpdatePolicy } from "./types";
import { Button, Dialog, IconButton, StatusBadge } from "./components/ui";

type HumanState = "正在处理" | "需要 Agent 继续" | "需要你确认" | "本地处理完成";
type SegmentSelectionMode = "replace" | "toggle" | "range";
type StructureEditMode = "split" | "merge" | "timing" | "offset";
const structureEditLabels: Record<StructureEditMode, string> = { split: "拆分字幕", merge: "合并字幕", timing: "调整字幕时间", offset: "批量偏移字幕" };

export type ExportPreferencesV1 = {
  version: 1;
  subtitleMode: "source" | "translated" | "bilingual";
  subtitleLanguage: string;
  transcriptFormat: "srt" | "vtt" | "ass" | "markdown";
};

export const DEFAULT_EXPORT_PREFERENCES: ExportPreferencesV1 = {
  version: 1,
  subtitleMode: "source",
  subtitleLanguage: "en",
  transcriptFormat: "srt",
};

export const parseExportPreferences = (raw: string | null): ExportPreferencesV1 => {
  if (!raw) return DEFAULT_EXPORT_PREFERENCES;
  try {
    const candidate = JSON.parse(raw) as Partial<ExportPreferencesV1>;
    const subtitleModes = ["source", "translated", "bilingual"];
    const transcriptFormats = ["srt", "vtt", "ass", "markdown"];
    if (candidate.version !== 1 || !subtitleModes.includes(candidate.subtitleMode ?? "") || !transcriptFormats.includes(candidate.transcriptFormat ?? "")) return DEFAULT_EXPORT_PREFERENCES;
    return {
      version: 1,
      subtitleMode: candidate.subtitleMode as ExportPreferencesV1["subtitleMode"],
      subtitleLanguage: typeof candidate.subtitleLanguage === "string" ? candidate.subtitleLanguage : "en",
      transcriptFormat: candidate.transcriptFormat as ExportPreferencesV1["transcriptFormat"],
    };
  } catch {
    return DEFAULT_EXPORT_PREFERENCES;
  }
};

const formatTime = (seconds: number) => {
  const minutes = Math.floor(seconds / 60);
  const rest = Math.floor(seconds % 60);
  return `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
};

const formatBytes = (bytes: number | null) => {
  if (bytes == null) return "未知";
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  return `${Math.max(0.1, bytes / 1024 ** 2).toFixed(1)} MB`;
};

export const taskLabel = (project: Project | null): HumanState => {
  if (!project) return "本地处理完成";
  if (project.patchSets.some((set) => set.items.some((item) => ["pending", "conflict"].includes(item.status)))) return "需要你确认";
  if (project.edits.some((edit) => ["suggested", "proposed"].includes(edit.status))) return "需要你确认";
  if (project.tasks.some((task) => ["queued", "failed", "interrupted"].includes(task.status))) return "需要 Agent 继续";
  if (project.tasks.some((task) => ["claimed", "running"].includes(task.status))) return "正在处理";
  return "本地处理完成";
};

export const cutSuggestionLabel = (type: string | undefined) => ({
  standalone_filler: "口头语",
  adjacent_repetition: "相邻重复",
  speech_restart: "说话重启",
}[type ?? ""] ?? "手动词范围");

export const sourceStatusLabel = (status: string) => ({
  queued: "等待下载",
  running: "正在下载",
  finalizing: "正在校验媒体",
  cancelled: "已取消",
  interrupted: "已中断",
  failed: "导入失败",
  completed: "导入完成",
}[status] ?? status);

export const autoStageLabel = (stage: string) => ({
  import: "导入素材",
  transcribe: "本地转录",
  suggestions: "生成粗剪建议",
  translate: "等待 Agent 翻译",
  review: "等待人工确认",
  audit: "导出前审计",
  export: "导出成片",
  complete: "流程完成",
}[stage] ?? stage);

export const autoStatusLabel = (status: string) => ({
  queued: "等待启动",
  running: "正在处理",
  needs_agent: "需要 Agent 继续",
  needs_review: "需要你确认",
  interrupted: "流程已中断",
  failed: "流程失败",
  cancelled: "已取消",
  completed: "已完成",
}[status] ?? status);

export const audioRiskLabel = (kind: AudioRisk["kind"]) => ({
  silence: "静音区间",
  suspected_clipping: "疑似削波",
  loudness_low: "综合响度偏低",
  loudness_high: "综合响度偏高",
}[kind]);

export const audioUnitLabel = (unit: string) => unit === "seconds" ? "秒" : unit;

export const shouldCheckForUpdates = (lastCheck: string | null, now: number, enabled: boolean) => {
  if (!enabled) return false;
  if (!lastCheck) return true;
  const checkedAt = Date.parse(lastCheck);
  return !Number.isFinite(checkedAt) || now - checkedAt >= 24 * 60 * 60 * 1000;
};

export const startSerialPolling = (poll: () => Promise<unknown>, intervalMs: number) => {
  let cancelled = false;
  let timer: number | null = null;
  const schedule = () => {
    if (cancelled) return;
    timer = window.setTimeout(() => {
      void poll().catch(() => undefined).finally(schedule);
    }, intervalMs);
  };
  schedule();
  return () => {
    cancelled = true;
    if (timer != null) window.clearTimeout(timer);
  };
};

export const clearTransientCoreError = (message: string | null) =>
  message && /core_service_(?:unavailable|no_response)/.test(message) ? null : message;

function App() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [project, setProject] = useState<Project | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedSegmentIds, setSelectedSegmentIds] = useState<string[]>([]);
  const [selectionAnchorId, setSelectionAnchorId] = useState<string | null>(null);
  const [mediaUrl, setMediaUrl] = useState<string | null>(null);
  const [waveformUrl, setWaveformUrl] = useState<string | null>(null);
  const [activeExport, setActiveExport] = useState<ExportJob | null>(null);
  const [audioAnalysisJob, setAudioAnalysisJob] = useState<AudioAnalysisJob | null>(null);
  const [speakerPackage, setSpeakerPackage] = useState<SpeakerPackageStatus | null>(null);
  const [speakerTrack, setSpeakerTrack] = useState<SpeakerTrack | null>(null);
  const [speakerJob, setSpeakerJob] = useState<SpeakerJob | null>(null);
  const [busy, setBusy] = useState<string | null>("正在读取本地项目");
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
  const [updatePolicy, setUpdatePolicy] = useState<UpdatePolicy | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<UpdateMetadata | null>(null);
  const [updateBusy, setUpdateBusy] = useState<string | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [modelJob, setModelJob] = useState<ModelDownloadJob | null>(null);
  const [sourcePreview, setSourcePreview] = useState<SourcePreview | null>(null);
  const [sourceJob, setSourceJob] = useState<SourceImportJob | null>(null);
  const [sourceUrl, setSourceUrl] = useState("");
  const [sourceAuthorized, setSourceAuthorized] = useState(false);
  const [sourceBusy, setSourceBusy] = useState<string | null>(null);
  const [sourceError, setSourceError] = useState<string | null>(null);
  const [showSourceImport, setShowSourceImport] = useState(false);
  const [autoWorkflow, setAutoWorkflow] = useState<AutoWorkflow | null>(null);
  const [showAutoWorkflow, setShowAutoWorkflow] = useState(false);
  const [autoInputKind, setAutoInputKind] = useState<"local" | "url">("local");
  const [autoMediaPath, setAutoMediaPath] = useState("");
  const [autoUrl, setAutoUrl] = useState("");
  const [autoSourcePreview, setAutoSourcePreview] = useState<SourcePreview | null>(null);
  const [autoAuthorized, setAutoAuthorized] = useState(false);
  const [autoTranslate, setAutoTranslate] = useState(false);
  const [autoTranslationLanguage, setAutoTranslationLanguage] = useState("en");
  const [autoBurnSubtitles, setAutoBurnSubtitles] = useState(true);
  const [autoSubtitleMode, setAutoSubtitleMode] = useState<"source" | "translated" | "bilingual">("source");
  const [autoBusy, setAutoBusy] = useState<string | null>(null);
  const [autoError, setAutoError] = useState<string | null>(null);
  const [modelPath, setModelPath] = useState<string | null>(() => localStorage.getItem("siaocut.modelPath"));
  const [showRuntime, setShowRuntime] = useState(false);
  const [showExportPanel, setShowExportPanel] = useState(false);
  const [showSubtitleSafeArea, setShowSubtitleSafeArea] = useState(true);
  const [showMoreMenu, setShowMoreMenu] = useState(false);
  const [search, setSearch] = useState("");
  const [replacement, setReplacement] = useState("");
  const [qualityFilter, setQualityFilter] = useState<"all" | "warning" | "error">("all");
  const [showSubtitleImport, setShowSubtitleImport] = useState(false);
  const [subtitleImportPath, setSubtitleImportPath] = useState("");
  const [subtitleImportPreview, setSubtitleImportPreview] = useState<SubtitleImportPreview | null>(null);
  const [subtitleImportBusy, setSubtitleImportBusy] = useState<string | null>(null);
  const [subtitleImportError, setSubtitleImportError] = useState<string | null>(null);
  const [subtitleReplaceConfirmed, setSubtitleReplaceConfirmed] = useState(false);
  const [structureEditMode, setStructureEditMode] = useState<StructureEditMode | null>(null);
  const [structureStart, setStructureStart] = useState("");
  const [structureEnd, setStructureEnd] = useState("");
  const [structureTextOffset, setStructureTextOffset] = useState("");
  const [structureDelta, setStructureDelta] = useState("0.100");
  const [structureBusy, setStructureBusy] = useState(false);
  const [structureError, setStructureError] = useState<string | null>(null);
  const [exportFormat, setExportFormat] = useState<"srt" | "vtt" | "ass" | "markdown">(() => parseExportPreferences(localStorage.getItem("siaocut.exportPreferences.v1")).transcriptFormat);
  const [subtitleMode, setSubtitleMode] = useState<"source" | "translated" | "bilingual">(() => parseExportPreferences(localStorage.getItem("siaocut.exportPreferences.v1")).subtitleMode);
  const [subtitleLanguage, setSubtitleLanguage] = useState(() => parseExportPreferences(localStorage.getItem("siaocut.exportPreferences.v1")).subtitleLanguage);
  const [wordRange, setWordRange] = useState<{ segmentId: string; start: number; end: number } | null>(null);
  const [cutPadding, setCutPadding] = useState<30 | 100 | 200>(100);
  const [cutPreview, setCutPreview] = useState<CutPreview | null>(null);
  const [deleteCandidate, setDeleteCandidate] = useState<Project | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const runtimeButtonRef = useRef<HTMLButtonElement>(null);
  const sourceButtonRef = useRef<HTMLButtonElement>(null);
  const autoButtonRef = useRef<HTMLButtonElement>(null);
  const exportButtonRef = useRef<HTMLButtonElement>(null);
  const exportPanelRef = useRef<HTMLElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const replacementInputRef = useRef<HTMLInputElement>(null);
  const subtitleImportButtonRef = useRef<HTMLButtonElement>(null);

  const refreshLatestExport = useCallback(async (projectId: string) => {
    const envelope = await runCore(["video", "list", projectId]);
    setActiveExport(envelope.jobs?.[0] ?? null);
  }, []);

  const refreshLatestAudioAnalysis = useCallback(async (projectId: string) => {
    const envelope = await runCore(["speech", "audio-latest", projectId]);
    setAudioAnalysisJob(envelope.audioAnalysisJob ?? null);
  }, []);

  const refreshSpeakerTrack = useCallback(async (projectId: string) => {
    const envelope = await runCore(["speaker", "track", projectId]);
    setSpeakerTrack(envelope.speakerTrack ?? null);
  }, []);

  const refreshProject = useCallback(async (projectId: string, refreshMedia = false) => {
    const next = await loadProject(projectId);
    const [nextMediaUrl, nextWaveformUrl] = refreshMedia
      ? await Promise.all([
        authorizeArtifact(next.id, "preview").then((preview) => preview ?? authorizeMedia(next.id)),
        authorizeArtifact(next.id, "waveform"),
      ])
      : [null, null];
    if (refreshMedia) {
      videoRef.current?.pause();
      setMediaUrl(nextMediaUrl);
      setWaveformUrl(nextWaveformUrl);
      setActiveExport(null);
      setWordRange(null);
      setCutPreview(null);
    }
    setProject(next);
    setProjects((current) => current.map((item) => item.id === next.id ? next : item));
    setSelectedId((current) => next.transcript.segments.some((segment) => segment.id === current) ? current : next.transcript.segments[0]?.id ?? null);
    await Promise.all([refreshLatestExport(next.id), refreshLatestAudioAnalysis(next.id), refreshSpeakerTrack(next.id)]);
  }, [refreshLatestAudioAnalysis, refreshLatestExport, refreshSpeakerTrack]);

  const initialize = useCallback(async () => {
    setBusy("正在检查本地运行环境");
    setError(null);
    const [projectsResult, runtimeResult, modelsResult, modelJobsResult, sourceJobsResult, autoWorkflowsResult, updatePolicyResult, speakerPackageResult, speakerJobsResult] = await Promise.allSettled([
      listProjects(),
      runtimeInfo(),
      runCore(["model", "list"]),
      runCore(["model", "jobs"]),
      runCore(["source", "jobs"]),
      runCore(["auto", "list"]),
      updaterPolicy(),
      runCore(["speaker", "package", "--verify"]),
      runCore(["speaker", "jobs"]),
    ]);
    const errors: string[] = [];
    let activeAutoWorkflow: AutoWorkflow | null = null;
    if (updatePolicyResult.status === "fulfilled") setUpdatePolicy(updatePolicyResult.value);
    if (autoWorkflowsResult.status === "fulfilled") {
      const workflows = autoWorkflowsResult.value.workflows ?? [];
      activeAutoWorkflow = workflows.find((item) => ["queued", "running", "needs_agent", "needs_review", "failed", "interrupted"].includes(item.status)) ?? null;
      setAutoWorkflow(activeAutoWorkflow);
    } else {
      errors.push(`自动工作流读取失败：${autoWorkflowsResult.reason instanceof Error ? autoWorkflowsResult.reason.message : String(autoWorkflowsResult.reason)}`);
    }
    let managedModelPath: string | null = null;
    if (modelsResult.status === "fulfilled") {
      const available = modelsResult.value.models ?? [];
      setModels(available);
      managedModelPath = available.find((item) => item.installed && item.recommended)?.path
        ?? available.find((item) => item.installed)?.path
        ?? null;
    } else {
      errors.push(`模型目录读取失败：${modelsResult.reason instanceof Error ? modelsResult.reason.message : String(modelsResult.reason)}`);
    }
    if (modelJobsResult.status === "fulfilled") {
      setModelJob(modelJobsResult.value.modelJobs?.find((item) => ["queued", "running"].includes(item.status)) ?? null);
    }
    if (speakerPackageResult.status === "fulfilled") {
      setSpeakerPackage(speakerPackageResult.value.speakerPackage ?? null);
    } else {
      errors.push(`说话人模型读取失败：${speakerPackageResult.reason instanceof Error ? speakerPackageResult.reason.message : String(speakerPackageResult.reason)}`);
    }
    if (speakerJobsResult.status === "fulfilled") {
      const jobs = speakerJobsResult.value.speakerJobs ?? [];
      setSpeakerJob(jobs.find((item) => ["queued", "running"].includes(item.status)) ?? jobs[0] ?? null);
    }
    if (sourceJobsResult.status === "fulfilled") {
      const jobs = (sourceJobsResult.value.sourceJobs ?? []).filter((item) => item.id !== activeAutoWorkflow?.sourceImportId);
      setSourceJob(jobs.find((item) => ["queued", "running", "finalizing"].includes(item.status)) ?? jobs[0] ?? null);
    } else {
      errors.push(`URL 导入任务读取失败：${sourceJobsResult.reason instanceof Error ? sourceJobsResult.reason.message : String(sourceJobsResult.reason)}`);
    }
    if (runtimeResult.status === "fulfilled") {
      setRuntime(runtimeResult.value);
      setModelPath((current) => {
        const next = current ?? managedModelPath ?? (runtimeResult.value.defaultModelAvailable ? runtimeResult.value.defaultModelPath : null);
        if (next) localStorage.setItem("siaocut.modelPath", next);
        return next;
      });
    } else {
      errors.push(`运行环境检查失败：${runtimeResult.reason instanceof Error ? runtimeResult.reason.message : String(runtimeResult.reason)}`);
    }
    if (projectsResult.status === "fulfilled") {
      setProjects(projectsResult.value);
      const first = projectsResult.value[0] ?? null;
      setProject(first);
      const firstSegmentId = first?.transcript.segments[0]?.id ?? null;
      setSelectedId(firstSegmentId);
      setSelectedSegmentIds(firstSegmentId ? [firstSegmentId] : []);
      setSelectionAnchorId(firstSegmentId);
      if (first) {
        try {
          setMediaUrl(await authorizeArtifact(first.id, "preview") ?? await authorizeMedia(first.id));
          setWaveformUrl(await authorizeArtifact(first.id, "waveform"));
          await Promise.all([refreshLatestExport(first.id), refreshLatestAudioAnalysis(first.id), refreshSpeakerTrack(first.id)]);
        }
        catch (cause) { errors.push(`媒体暂时无法预览：${cause instanceof Error ? cause.message : String(cause)}`); }
      }
    } else {
      errors.push(`项目读取失败：${projectsResult.reason instanceof Error ? projectsResult.reason.message : String(projectsResult.reason)}`);
    }
    setError(errors.length ? errors.join(" ") : null);
    setBusy(null);
  }, [refreshLatestAudioAnalysis, refreshLatestExport, refreshSpeakerTrack]);

  useEffect(() => {
    void initialize();
  }, [initialize]);

  const checkUpdates = useCallback(async (automatic = false) => {
    if (!updatePolicy?.enabled) return;
    setUpdateBusy("正在检查更新");
    setUpdateError(null);
    try {
      const candidate = await checkForUpdate();
      localStorage.setItem("siaocut.updateLastCheckedAt", new Date().toISOString());
      setAvailableUpdate(candidate);
      if (!automatic && !candidate) setNotice("当前已是最新版本。");
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      if (!automatic) setUpdateError(message);
    } finally {
      setUpdateBusy(null);
    }
  }, [updatePolicy?.enabled]);

  useEffect(() => {
    if (!updatePolicy || !shouldCheckForUpdates(localStorage.getItem("siaocut.updateLastCheckedAt"), Date.now(), updatePolicy.enabled)) return;
    void checkUpdates(true);
  }, [checkUpdates, updatePolicy]);

  const confirmUpdateInstall = async () => {
    if (!availableUpdate) return;
    setUpdateBusy("正在下载并验证更新");
    setUpdateError(null);
    try {
      await installUpdate((event) => {
        if (event.event === "Verifying") setUpdateBusy("正在核对更新签名");
      });
    } catch (cause) {
      setUpdateError(cause instanceof Error ? cause.message : String(cause));
      setUpdateBusy(null);
    }
  };

  useEffect(() => {
    if (!project?.tasks.some((task) => ["queued", "claimed", "running", "interrupted"].includes(task.status))) return;
    return startSerialPolling(() =>
      loadProject(project.id).then((next) => {
        setError(clearTransientCoreError);
        setProject(next);
        setProjects((current) => current.map((item) => item.id === next.id ? next : item));
      }).catch(() => undefined),
    2500);
  }, [project?.id, project?.tasks]);

  useEffect(() => {
    if (!activeExport || !["queued", "running"].includes(activeExport.status)) return;
    return startSerialPolling(() =>
      runCore(["video", "status", activeExport.id]).then((envelope) => {
        setError(clearTransientCoreError);
        if (!envelope.job) return;
        setActiveExport(envelope.job);
        if (envelope.job.status === "completed") setNotice(`视频已导出到 ${envelope.job.outputPath}`);
        if (envelope.job.status === "failed") setError(envelope.job.errorMessage ?? "视频导出失败。");
        if (envelope.job.status === "cancelled") setNotice("视频导出已取消，原片未修改。");
      }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
    1000);
  }, [activeExport?.id, activeExport?.status]);

  useEffect(() => {
    if (!audioAnalysisJob || !["queued", "running"].includes(audioAnalysisJob.status)) return;
    return startSerialPolling(() =>
      runCore(["speech", "audio-status", audioAnalysisJob.id]).then((envelope) => {
        setError(clearTransientCoreError);
        if (!envelope.audioAnalysisJob) return;
        setAudioAnalysisJob(envelope.audioAnalysisJob);
        if (envelope.audioAnalysisJob.status === "completed") setNotice("本地音频质量分析完成；风险仅供定位确认。 ");
        if (["failed", "interrupted"].includes(envelope.audioAnalysisJob.status)) setError(envelope.audioAnalysisJob.errorMessage ?? "本地音频质量分析中断，可以显式继续。");
        if (envelope.audioAnalysisJob.status === "cancelled") setNotice("本地音频质量分析已取消，不影响其他工作流。 ");
      }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
    700);
  }, [audioAnalysisJob?.id, audioAnalysisJob?.status]);

  useEffect(() => {
    if (!modelJob || !["queued", "running"].includes(modelJob.status)) return;
    return startSerialPolling(() =>
      runCore(["model", "status", modelJob.id]).then(async (envelope) => {
        setError(clearTransientCoreError);
        if (!envelope.modelJob) return;
        setModelJob(envelope.modelJob);
        if (envelope.modelJob.status === "completed") {
          const catalog = await runCore(["model", "list"]);
          const available = catalog.models ?? [];
          setModels(available);
          const installed = available.find((item) => item.id === envelope.modelJob?.modelId);
          if (installed) {
            localStorage.setItem("siaocut.modelPath", installed.path);
            setModelPath(installed.path);
          }
          setNotice("模型已下载并通过 SHA-256 校验，可以开始本地转录。");
        }
        if (envelope.modelJob.status === "failed") setError(envelope.modelJob.errorMessage ?? "模型下载失败。");
        if (envelope.modelJob.status === "cancelled") setNotice("模型下载已暂停；已下载部分可在下次继续使用。");
      }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
    800);
  }, [modelJob?.id, modelJob?.status]);

  useEffect(() => {
    if (!speakerJob || !["queued", "running"].includes(speakerJob.status)) return;
    return startSerialPolling(() =>
      runCore(["speaker", "job-status", speakerJob.id]).then(async (envelope) => {
        setError(clearTransientCoreError);
        if (!envelope.speakerJob) return;
        const next = envelope.speakerJob;
        setSpeakerJob(next);
        if (next.status === "completed" && next.kind === "install") {
          const status = await runCore(["speaker", "package", "--verify"]);
          setSpeakerPackage(status.speakerPackage ?? null);
          setNotice("说话人模型包已安装并逐文件通过 SHA-256 校验。 ");
        }
        if (next.status === "completed" && next.kind === "analyze" && next.projectId) {
          await Promise.all([refreshProject(next.projectId), refreshSpeakerTrack(next.projectId)]);
          setNotice("本地说话人轨已生成；字幕和剪辑没有被修改。 ");
        }
        if (["failed", "interrupted"].includes(next.status)) setError(next.errorMessage ?? "说话人任务未完成，可以显式继续。 ");
        if (next.status === "cancelled") setNotice("说话人任务已取消，不影响字幕、剪辑或原片。 ");
      }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
    800);
  }, [refreshProject, refreshSpeakerTrack, speakerJob?.id, speakerJob?.status]);

  useEffect(() => {
    if (!sourceJob || !["queued", "running", "finalizing"].includes(sourceJob.status)) return;
    return startSerialPolling(() =>
      runCore(["source", "status", sourceJob.id]).then(async (envelope) => {
        setError(clearTransientCoreError);
        if (!envelope.sourceJob) return;
        const nextJob = envelope.sourceJob;
        setSourceJob(nextJob);
        if (nextJob.status === "failed") {
          const message = nextJob.errorMessage ?? "URL 导入失败。";
          setSourceError(message);
          setError(message);
        }
        if (nextJob.status === "interrupted") {
          const message = nextJob.errorMessage ?? "上次 URL 导入被中断，可以显式继续。";
          setSourceError(message);
          setError(message);
        }
        if (nextJob.status === "cancelled") setNotice("URL 导入已取消；没有创建半成品项目，已下载部分仍保留在本机。");
        if (nextJob.status === "completed" && nextJob.projectId) {
          const imported = await loadProject(nextJob.projectId);
          setProjects((current) => [imported, ...current.filter((item) => item.id !== imported.id)]);
          setProject(imported);
          setSelectedId(imported.transcript.segments[0]?.id ?? null);
          setMediaUrl(await authorizeArtifact(imported.id, "preview") ?? await authorizeMedia(imported.id));
          setWaveformUrl(await authorizeArtifact(imported.id, "waveform"));
          await Promise.all([refreshLatestExport(imported.id), refreshLatestAudioAnalysis(imported.id)]);
          setShowSourceImport(false);
          setNotice("URL 视频已校验并创建本地项目；原 URL、站点媒体 ID、工具版本和文件哈希已保存。");
        }
      }).catch((cause) => {
        const message = cause instanceof Error ? cause.message : String(cause);
        setSourceError(message);
        setError(message);
      }),
    600);
  }, [refreshLatestAudioAnalysis, refreshLatestExport, sourceJob?.id, sourceJob?.status]);

  useEffect(() => {
    if (!autoWorkflow || !["queued", "running", "needs_agent", "needs_review"].includes(autoWorkflow.status)) return;
    return startSerialPolling(() =>
      runCore(["auto", "status", autoWorkflow.id]).then(async (envelope) => {
        setError(clearTransientCoreError);
        setAutoError(clearTransientCoreError);
        if (!envelope.workflow) return;
        const next = envelope.workflow;
        setAutoWorkflow({ ...next });
        if (next.projectId && (project?.id !== next.projectId || ["needs_review", "completed"].includes(next.status))) {
          await refreshProject(next.projectId, next.status === "completed");
        }
        if (next.status === "completed") setNotice(`一键工作流已完成，视频已导出到 ${next.outputPath}`);
        if (next.status === "failed") setError(next.errorMessage ?? "自动工作流失败，可以显式继续。");
        if (next.status === "interrupted") setError(next.errorMessage ?? "自动工作流被中断，可以显式继续。");
      }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
    800);
  }, [autoWorkflow?.id, autoWorkflow?.status, project?.id, refreshProject]);

  const selected = project?.transcript.segments.find((segment) => segment.id === selectedId) ?? null;
  const selectedWords = project?.transcript.words.filter((word) => word.segmentId === selectedId) ?? [];
  const activeWordRange = wordRange?.segmentId === selectedId ? wordRange : null;
  const filteredSegments = useMemo(() => {
    const issueSegmentIds = qualityFilter === "all" ? null : new Set(project?.subtitleQuality.issues.filter((issue) => issue.severity === qualityFilter).map((issue) => issue.segmentId));
    return project?.transcript.segments.filter((segment) => segment.text.toLowerCase().includes(search.toLowerCase()) && (!issueSegmentIds || issueSegmentIds.has(segment.id))) ?? [];
  }, [project, qualityFilter, search]);
  const visibleQualityIssues = project?.subtitleQuality.issues.filter((issue) => qualityFilter === "all" || issue.severity === qualityFilter) ?? [];
  const selectedSegments = useMemo(() => project?.transcript.segments.filter((segment) => selectedSegmentIds.includes(segment.id)) ?? [], [project, selectedSegmentIds]);
  const allVisibleSegmentsSelected = filteredSegments.length > 0 && filteredSegments.every((segment) => selectedSegmentIds.includes(segment.id));
  const selectedScopeLabel = selectedSegments.length
    ? `${selectedSegments.length} 段 · ${formatTime(selectedSegments[0].start)} — ${formatTime(selectedSegments.at(-1)!.end)}`
    : "尚未选择字幕";
  const firstSelectedIndex = project?.transcript.segments.findIndex((segment) => segment.id === selectedSegments[0]?.id) ?? -1;
  const secondSelectedIndex = project?.transcript.segments.findIndex((segment) => segment.id === selectedSegments[1]?.id) ?? -1;
  const mergeCandidatesAdjacent = selectedSegments.length === 2 && firstSelectedIndex >= 0 && secondSelectedIndex === firstSelectedIndex + 1;

  useEffect(() => {
    const segmentIds = new Set(project?.transcript.segments.map((segment) => segment.id) ?? []);
    setSelectedSegmentIds((current) => {
      const valid = current.filter((id) => segmentIds.has(id));
      if (selectedId && valid.includes(selectedId)) return valid;
      return selectedId && segmentIds.has(selectedId) ? [selectedId] : valid;
    });
    setSelectionAnchorId((current) => current && segmentIds.has(current) ? current : selectedId && segmentIds.has(selectedId) ? selectedId : null);
  }, [project, selectedId]);

  const translationLanguages = project ? Object.keys(project.translations) : [];
  const pendingTranslationLanguages = project?.tasks
    .filter((task) => task.kind === "translate" && task.language && !["done", "completed", "cancelled", "canceled"].includes(task.status))
    .map((task) => task.language!) ?? [];
  const translationLanguageOptions = Array.from(new Set([...translationLanguages, ...pendingTranslationLanguages]));
  const selectedSubtitleLanguage = translationLanguageOptions.includes(subtitleLanguage) ? subtitleLanguage : translationLanguageOptions[0] ?? "";
  const selectedTranslation = selectedSubtitleLanguage ? project?.translations[selectedSubtitleLanguage] : undefined;
  const translation = selectedTranslation ? [selectedSubtitleLanguage, selectedTranslation] as const : undefined;
  const selectedTranslationPending = Boolean(selectedSubtitleLanguage && !selectedTranslation);
  const selectedTranslationText = selectedTranslation?.segments.find((segment) => segment.segmentId === selected?.id)?.text ?? "";
  const captionPrimaryText = subtitleMode === "translated" ? selectedTranslationText : selected?.text ?? "";
  const captionSecondaryText = subtitleMode === "bilingual" ? selectedTranslationText : "";
  const captionPreviewStyle = project ? {
    color: project.subtitleStyle.primaryColor,
    fontFamily: `"${project.subtitleStyle.fontFamily}", "Microsoft YaHei UI", sans-serif`,
    fontSize: `${Math.max(14, Math.round(project.subtitleStyle.fontSize * 0.36))}px`,
    fontWeight: project.subtitleStyle.bold ? 700 : 400,
    bottom: project.subtitleStyle.position === "bottom" ? `${project.subtitleStyle.safeMarginPercent}%` : undefined,
    textShadow: `0 ${project.subtitleStyle.shadowDepth}px ${Math.max(1, project.subtitleStyle.shadowDepth * 2)}px ${project.subtitleStyle.outlineColor}, 0 0 ${project.subtitleStyle.outlineWidth * 2}px ${project.subtitleStyle.outlineColor}`,
  } : undefined;
  const currentDeleteCandidate = deleteCandidate ? projects.find((item) => item.id === deleteCandidate.id) ?? deleteCandidate : null;
  const deleteActiveTaskCount = currentDeleteCandidate?.tasks.filter((task) => ["queued", "claimed", "running"].includes(task.status)).length ?? 0;
  const deleteBlockMessage = deleteActiveTaskCount > 0
    ? `该项目有 ${deleteActiveTaskCount} 项正在运行或等待 Agent 处理的任务。请先取消任务，再删除项目。`
    : currentDeleteCandidate && activeExport?.projectId === currentDeleteCandidate.id && ["queued", "running"].includes(activeExport.status)
      ? "该项目正在导出视频。请先取消导出，再删除项目。"
      : currentDeleteCandidate && audioAnalysisJob?.projectId === currentDeleteCandidate.id && ["queued", "running"].includes(audioAnalysisJob.status)
        ? "该项目正在分析音频质量。请先取消分析，再删除项目。"
      : currentDeleteCandidate && speakerJob?.projectId === currentDeleteCandidate.id && ["queued", "running"].includes(speakerJob.status)
        ? "该项目正在分析说话人。请先取消分析，再删除项目。"
      : currentDeleteCandidate && autoWorkflow?.projectId === currentDeleteCandidate.id && ["queued", "running", "needs_agent", "needs_review", "failed", "interrupted"].includes(autoWorkflow.status)
        ? "该项目仍有关联的一键工作流。请先取消流程，再删除项目。"
        : null;
  const humanState = busy ? "正在处理" : taskLabel(project);
  const humanStateTone = humanState === "需要你确认" ? "warning" : humanState === "需要 Agent 继续" ? "agent" : humanState === "正在处理" ? "info" : "success";
  const orderedPatchSets = project?.patchSets
    .map((set) => ({ ...set, items: set.items.filter((item) => ["pending", "conflict"].includes(item.status)).sort((left, right) => Number(right.status === "conflict") - Number(left.status === "conflict")) }))
    .filter((set) => set.items.length)
    .sort((left, right) => Number(right.items.some((item) => item.status === "conflict")) - Number(left.items.some((item) => item.status === "conflict"))) ?? [];
  const pendingEdits = project?.edits.filter((edit) => ["suggested", "proposed"].includes(edit.status)) ?? [];
  const failedTasks = project?.tasks.filter((task) => ["failed", "interrupted"].includes(task.status)) ?? [];
  const processingTasks = project?.tasks.filter((task) => ["queued", "claimed", "running"].includes(task.status)) ?? [];
  const recentTasks = project?.tasks.filter((task) => ["completed", "cancelled", "canceled"].includes(task.status)).slice(-5).reverse() ?? [];
  const audioRisks = audioAnalysisJob?.status === "completed" ? audioAnalysisJob.report?.risks ?? [] : [];
  const projectSpeakerJob = speakerJob?.projectId === project?.id ? speakerJob : null;
  const speakerById = new Map(speakerTrack?.speakers.map((speaker) => [speaker.id, speaker]) ?? []);
  const associationBySegment = new Map(speakerTrack?.associations.map((association) => [association.segmentId, association]) ?? []);
  const actionableReviewCount = orderedPatchSets.reduce((count, set) => count + set.items.length, 0) + pendingEdits.length + failedTasks.length + audioRisks.length + Number(Boolean(projectSpeakerJob && ["failed", "interrupted"].includes(projectSpeakerJob.status)));

  useEffect(() => {
    localStorage.setItem("siaocut.exportPreferences.v1", JSON.stringify({
      version: 1,
      subtitleMode,
      subtitleLanguage,
      transcriptFormat: exportFormat,
    } satisfies ExportPreferencesV1));
  }, [exportFormat, subtitleLanguage, subtitleMode]);

  useEffect(() => {
    if (!project || subtitleMode === "source") return;
    if (!translationLanguageOptions.includes(subtitleLanguage)) setSubtitleMode("source");
  }, [project, subtitleLanguage, subtitleMode, translationLanguageOptions]);

  useEffect(() => {
    if (!showExportPanel) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    window.requestAnimationFrame(() => exportPanelRef.current?.querySelector<HTMLElement>("button, select")?.focus());
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setShowExportPanel(false);
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("keydown", closeOnEscape);
      previous?.focus();
    };
  }, [showExportPanel]);

  const selectSegment = (segment: Segment) => {
    setSelectedId(segment.id);
    setSelectedSegmentIds([segment.id]);
    setSelectionAnchorId(segment.id);
    setWordRange((current) => current?.segmentId === segment.id ? current : null);
    if (videoRef.current) videoRef.current.currentTime = segment.start;
  };

  const selectSegmentInWorkbench = (segment: Segment, mode: SegmentSelectionMode) => {
    if (!project || mode === "replace") {
      selectSegment(segment);
      return;
    }
    if (mode === "range") {
      const anchorIndex = project.transcript.segments.findIndex((item) => item.id === (selectionAnchorId ?? selectedId));
      const targetIndex = project.transcript.segments.findIndex((item) => item.id === segment.id);
      if (anchorIndex < 0 || targetIndex < 0) {
        selectSegment(segment);
        return;
      }
      const [start, end] = anchorIndex <= targetIndex ? [anchorIndex, targetIndex] : [targetIndex, anchorIndex];
      setSelectedSegmentIds(project.transcript.segments.slice(start, end + 1).map((item) => item.id));
      setSelectedId(segment.id);
    } else {
      const alreadySelected = selectedSegmentIds.includes(segment.id);
      if (alreadySelected && selectedSegmentIds.length > 1) {
        const next = selectedSegmentIds.filter((id) => id !== segment.id);
        setSelectedSegmentIds(next);
        setSelectedId(next.at(-1) ?? null);
      } else if (!alreadySelected) {
        setSelectedSegmentIds([...selectedSegmentIds, segment.id]);
        setSelectedId(segment.id);
      }
      setSelectionAnchorId(segment.id);
    }
    setWordRange(null);
    if (videoRef.current) videoRef.current.currentTime = segment.start;
  };

  const moveSegmentSelection = (direction: -1 | 1) => {
    if (!project?.transcript.segments.length) return;
    const currentIndex = project.transcript.segments.findIndex((segment) => segment.id === selectedId);
    const nextIndex = Math.min(project.transcript.segments.length - 1, Math.max(0, (currentIndex < 0 ? 0 : currentIndex) + direction));
    selectSegment(project.transcript.segments[nextIndex]);
  };

  const locateSpeechEvidence = (evidence: SpeechEvidence) => {
    const segment = project?.transcript.segments.find((candidate) => candidate.id === evidence.segmentId);
    if (segment) selectSegment(segment);
  };

  const locateSpeechPause = (pause: SpeechPause) => {
    const word = project?.transcript.words.find((candidate) => candidate.id === pause.nextWordId);
    const segment = word && project?.transcript.segments.find((candidate) => candidate.id === word.segmentId);
    if (segment) selectSegment(segment);
  };

  const locateAudioRisk = (risk: AudioRisk) => {
    if (videoRef.current) videoRef.current.currentTime = risk.start;
  };

  const selectWordForCut = (index: number) => {
    if (!selectedId) return;
    setWordRange((current) => {
      if (!current || current.segmentId !== selectedId || current.start !== current.end) {
        return { segmentId: selectedId, start: index, end: index };
      }
      return { segmentId: selectedId, start: Math.min(current.start, index), end: Math.max(current.end, index) };
    });
  };

  const withBusy = async (label: string, action: () => Promise<void>) => {
    setBusy(label);
    setError(null);
    try { await action(); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); } finally { setBusy(null); }
  };

  const importMedia = () => withBusy("正在校验并创建本地项目", async () => {
    const path = await pickMedia();
    if (!path) return;
    const envelope = await runCore(["import", path]);
    if (!envelope.project) throw new Error("导入完成，但 Core 未返回项目");
    videoRef.current?.pause();
    setProjects((current) => [envelope.project!, ...current.filter((item) => item.id !== envelope.project!.id)]);
    setProject(envelope.project);
    setSelectedId(null);
    setSelectedSegmentIds([]);
    setSelectionAnchorId(null);
    setMediaUrl(await authorizeMedia(envelope.project.id));
    setWaveformUrl(null);
    setActiveExport(null);
    setAudioAnalysisJob(null);
    setSpeakerTrack(null);
    setWordRange(null);
    setCutPreview(null);
    setNotice("项目已创建；原片未被复制或修改。");
  });

  const switchProject = (projectId: string) => {
    if (project?.id === projectId) return;
    void withBusy("正在切换项目", async () => {
      await refreshProject(projectId, true);
    });
  };

  const openDeleteDialog = (candidate: Project) => {
    setDeleteError(null);
    setDeleteCandidate(candidate);
  };

  const closeDeleteDialog = () => {
    if (deleteBusy) return;
    setDeleteCandidate(null);
    setDeleteError(null);
  };

  const deleteProject = async () => {
    if (!currentDeleteCandidate || deleteBlockMessage) return;
    const deleting = currentDeleteCandidate;
    setDeleteBusy(true);
    setDeleteError(null);
    try {
      await runCore(["project", "delete", deleting.id]);
      const remaining = projects.filter((item) => item.id !== deleting.id);
      setProjects(remaining);
      setDeleteCandidate(null);
      if (project?.id === deleting.id) {
        videoRef.current?.pause();
        setProject(null);
        setSelectedId(null);
        setMediaUrl(null);
        setWaveformUrl(null);
        setActiveExport(null);
        setAudioAnalysisJob(null);
        setSpeakerTrack(null);
        setWordRange(null);
        setCutPreview(null);
        if (remaining[0]) await refreshProject(remaining[0].id, true);
      }
      setNotice(`项目「${deleting.title}」已删除；原始媒体文件未被修改。`);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setDeleteError(message.replace(/^project_busy:\s*/, ""));
    } finally {
      setDeleteBusy(false);
    }
  };

  const withSourceBusy = async (label: string, action: () => Promise<void>) => {
    setSourceBusy(label);
    setSourceError(null);
    try { await action(); } catch (cause) { setSourceError(cause instanceof Error ? cause.message : String(cause)); } finally { setSourceBusy(null); }
  };

  const inspectSource = () => withSourceBusy("正在读取公开单视频信息", async () => {
    const url = sourceUrl.trim();
    if (!runtime?.ytDlpConfigured) throw new Error("当前安装未检测到固定版本的 yt-dlp，请先检查运行环境。");
    if (!url) throw new Error("请输入公开 HTTPS 单视频 URL。");
    const envelope = await runCore(["source", "inspect", url]);
    if (!envelope.source) throw new Error("Core 未返回 URL 视频信息。");
    setSourcePreview(envelope.source);
    setSourceJob(null);
    setSourceAuthorized(false);
  });

  const startSourceImport = () => sourcePreview && withSourceBusy("正在重新核对并创建后台下载任务", async () => {
    if (!sourceAuthorized) throw new Error("请先确认有权下载并处理此视频。");
    const envelope = await runCore(["source", "start", sourcePreview.originalUrl, "--confirm-media-id", sourcePreview.siteMediaId]);
    if (!envelope.sourceJob) throw new Error("Core 未返回 URL 导入任务。");
    setSourceJob(envelope.sourceJob);
    setNotice("URL 导入已进入后台；完成媒体校验前不会创建项目。");
  });

  const cancelSourceImport = () => sourceJob && withSourceBusy("正在请求取消 URL 导入", async () => {
    const envelope = await runCore(["source", "cancel", sourceJob.id]);
    if (!envelope.sourceJob) throw new Error("Core 未返回取消后的 URL 导入任务。");
    setSourceJob(envelope.sourceJob);
  });

  const resumeSourceImport = () => sourceJob && withSourceBusy("正在显式继续 URL 导入", async () => {
    const envelope = await runCore(["source", "resume", sourceJob.id]);
    if (!envelope.sourceJob) throw new Error("Core 未返回继续后的 URL 导入任务。");
    setSourceJob(envelope.sourceJob);
    setNotice(`URL 导入已显式继续；这是第 ${envelope.sourceJob.attemptCount} 次尝试。`);
  });

  const resetSourceImport = () => {
    if (sourceJob && ["queued", "running", "finalizing"].includes(sourceJob.status)) return;
    setSourcePreview(null);
    setSourceJob(null);
    setSourceUrl("");
    setSourceAuthorized(false);
    setSourceError(null);
  };

  const withAutoBusy = async (label: string, action: () => Promise<void>) => {
    setAutoBusy(label);
    setAutoError(null);
    try { await action(); } catch (cause) { setAutoError(cause instanceof Error ? cause.message : String(cause)); } finally { setAutoBusy(null); }
  };

  const chooseAutoMedia = () => withAutoBusy("正在选择本地素材", async () => {
    const path = await pickMedia();
    if (path) setAutoMediaPath(path);
  });

  const inspectAutoSource = () => withAutoBusy("正在读取公开单视频信息", async () => {
    if (!runtime?.ytDlpConfigured) throw new Error("当前安装未检测到固定版本的 yt-dlp，请先检查运行环境。");
    if (!autoUrl.trim()) throw new Error("请输入公开 HTTPS 单视频 URL。");
    const envelope = await runCore(["source", "inspect", autoUrl.trim()]);
    if (!envelope.source) throw new Error("Core 未返回 URL 视频信息。");
    setAutoSourcePreview(envelope.source);
    setAutoAuthorized(false);
  });

  const startAutoWorkflow = () => withAutoBusy("正在创建一键工作流", async () => {
    if (!modelPath) throw new Error("尚未选择转录模型，请先在「运行环境」中选择本机模型。");
    if (autoTranslate && !autoTranslationLanguage.trim()) throw new Error("请输入 Agent 翻译任务的目标语言。");
    if (autoInputKind === "local" && !autoMediaPath) throw new Error("请先选择本地音视频。");
    if (autoInputKind === "url" && (!autoSourcePreview || !autoAuthorized)) throw new Error("请先读取 URL 信息并确认有权处理该视频。");
    const output = await pickVideoPath(autoSourcePreview?.title ?? "SiaoCut-一键成片");
    if (!output) return;
    const inputArgs = autoInputKind === "local"
      ? ["--media", autoMediaPath, "--title", "一键成片项目"]
      : ["--url", autoSourcePreview!.originalUrl, "--confirm-media-id", autoSourcePreview!.siteMediaId];
    const envelope = await runCore([
      "auto", "start", ...inputArgs,
      "--model", modelPath,
      "--language", "auto",
      "--output", output,
      "--subtitle-mode", autoTranslate ? autoSubtitleMode : "source",
      ...(autoTranslate ? ["--translate", autoTranslationLanguage] : []),
      ...(autoBurnSubtitles ? ["--burn-subtitles"] : []),
    ]);
    if (!envelope.workflow) throw new Error("Core 未返回自动工作流。");
    setAutoWorkflow({ ...envelope.workflow });
    setShowAutoWorkflow(false);
    setNotice("一键工作流已启动；粗剪与 Agent 结果仍会停下来等待人工确认。");
  });

  const cancelAutoWorkflow = () => autoWorkflow && withAutoBusy("正在取消自动工作流", async () => {
    const envelope = await runCore(["auto", "cancel", autoWorkflow.id]);
    if (!envelope.workflow) throw new Error("Core 未返回取消后的自动工作流。");
    setAutoWorkflow(null);
    setNotice("自动工作流已取消；已完成的本地项目和中间证据仍然保留。");
  });

  const continueAutoWorkflow = () => autoWorkflow && withAutoBusy("正在显式继续自动工作流", async () => {
    const envelope = await runCore(["auto", "continue", autoWorkflow.id]);
    if (!envelope.workflow) throw new Error("Core 未返回继续后的自动工作流。");
    setAutoWorkflow({ ...envelope.workflow });
    setNotice(`自动工作流已显式继续；这是第 ${envelope.workflow.attemptCount} 次尝试。`);
  });

  const openAutoProject = () => autoWorkflow?.projectId && withAutoBusy("正在打开待审项目", async () => {
    await refreshProject(autoWorkflow.projectId!, true);
  });

  const changeAsrBackend = (backend: "cpu" | "vulkan") => withBusy("正在切换本地转录后端", async () => {
    const next = await selectAsrBackend(backend);
    setRuntime(next);
    setNotice(backend === "vulkan" ? "已启用 Vulkan；转录会优先使用检测到的兼容显卡。" : "已切换到 CPU；无需独立显卡。 ");
  });

  const openDiagnostics = () => withBusy("正在打开日志目录", async () => {
    await openLogDirectory();
    setNotice("已打开本机诊断日志目录。");
  });

  const relinkMedia = () => project && withBusy("正在校验重新选择的原片", async () => {
    const path = await pickMedia();
    if (!path) return;
    await runCore(["project", "relink", project.id, path]);
    await refreshProject(project.id, true);
    setNotice("已重新定位原片；内容哈希与项目记录一致。");
  });

  const transcribe = () => project && withBusy("正在本地转录，关闭窗口后 Core 仍会继续", async () => {
    if (!runtime?.ffmpegConfigured) throw new Error("未检测到 FFmpeg。请先在「运行环境」中完成配置。");
    if (!runtime?.asrConfigured) throw new Error("未检测到 whisper.cpp。请先在「运行环境」中完成配置。");
    if (!modelPath) throw new Error("尚未选择转录模型。请先在「运行环境」中选择本机模型。");
    const result = await runCore(["transcribe", project.id, "--model", modelPath, "--language", "auto"]);
    await refreshProject(project.id);
    setNotice(Number(result.segments ?? 0) === 0 ? "未检测到清晰人声；没有生成字幕。可以更换素材或模型后重试。" : "本地转录完成，可以从文字开始校对。");
  });

  const startAudioAnalysis = () => project && withBusy("正在启动本地音频质量分析", async () => {
    if (!runtime?.ffmpegConfigured) throw new Error("未检测到 FFmpeg。请先在「运行环境」中完成配置。");
    const envelope = await runCore(["speech", "audio-start", project.id]);
    if (!envelope.audioAnalysisJob) throw new Error("Core 未返回音频分析任务。");
    setAudioAnalysisJob(envelope.audioAnalysisJob);
    setNotice("本地音频质量分析已开始；媒体不会上传，也不阻断编辑和导出。");
  });

  const cancelAudioAnalysis = () => audioAnalysisJob && withBusy("正在取消本地音频质量分析", async () => {
    const envelope = await runCore(["speech", "audio-cancel", audioAnalysisJob.id]);
    if (envelope.audioAnalysisJob) setAudioAnalysisJob(envelope.audioAnalysisJob);
  });

  const resumeAudioAnalysis = () => audioAnalysisJob && withBusy("正在继续本地音频质量分析", async () => {
    const envelope = await runCore(["speech", "audio-resume", audioAnalysisJob.id]);
    if (!envelope.audioAnalysisJob) throw new Error("Core 未返回继续后的音频分析任务。");
    setAudioAnalysisJob(envelope.audioAnalysisJob);
    setNotice(`已显式继续本地音频质量分析；这是第 ${envelope.audioAnalysisJob.attemptCount} 次尝试。`);
  });

  const installSpeakerPackage = () => withBusy("正在创建说话人模型安装任务", async () => {
    const envelope = await runCore(["speaker", "install"]);
    if (!envelope.speakerJob) throw new Error("Core 未返回说话人模型安装任务。 ");
    setSpeakerJob(envelope.speakerJob);
    if (envelope.speakerJob.status === "completed") {
      const status = await runCore(["speaker", "package", "--verify"]);
      setSpeakerPackage(status.speakerPackage ?? null);
      setNotice("说话人模型包已安装并通过 SHA-256 校验。 ");
    } else {
      setNotice("说话人模型包已开始下载；只访问界面显示的固定来源。 ");
    }
  });

  const startSpeakerAnalysis = () => project && withBusy("正在启动本地说话人分析", async () => {
    if (!speakerPackage?.installed || speakerPackage.verified !== true) throw new Error("请先在「运行环境」中显式安装并校验说话人模型包。 ");
    const envelope = await runCore(["speaker", "analyze", project.id]);
    if (!envelope.speakerJob) throw new Error("Core 未返回说话人分析任务。 ");
    setSpeakerJob(envelope.speakerJob);
    if (envelope.speakerJob.status === "completed") {
      await Promise.all([refreshProject(project.id), refreshSpeakerTrack(project.id)]);
      setNotice("本地说话人轨已生成；字幕和剪辑没有被修改。 ");
    } else {
      setNotice("本地说话人分析已开始；结果只会进入可审阅说话人轨。 ");
    }
  });

  const cancelSpeakerJob = () => speakerJob && withBusy("正在取消说话人任务", async () => {
    const envelope = await runCore(["speaker", "cancel", speakerJob.id]);
    if (envelope.speakerJob) setSpeakerJob(envelope.speakerJob);
  });

  const resumeSpeakerJob = () => speakerJob && withBusy("正在继续说话人任务", async () => {
    const envelope = await runCore(["speaker", "resume", speakerJob.id]);
    if (!envelope.speakerJob) throw new Error("Core 未返回继续后的说话人任务。 ");
    setSpeakerJob(envelope.speakerJob);
    setNotice(`说话人任务已显式继续；这是第 ${envelope.speakerJob.attemptCount} 次尝试。`);
  });

  const renameSpeaker = (speakerId: string, name: string) => project && withBusy("正在保存说话人名称", async () => {
    const envelope = await runCore(["speaker", "rename", project.id, speakerId, "--name", name]);
    if (!envelope.speakerTrack) throw new Error("Core 未返回更新后的说话人轨。 ");
    setSpeakerTrack(envelope.speakerTrack);
    await refreshProject(project.id);
    setNotice("说话人名称已更新，可撤销或从版本历史恢复。 ");
  });

  const mergeSpeaker = (fromId: string, intoId: string) => project && withBusy("正在合并说话人", async () => {
    const envelope = await runCore(["speaker", "merge", project.id, "--from", fromId, "--into", intoId]);
    if (!envelope.speakerTrack) throw new Error("Core 未返回合并后的说话人轨。 ");
    setSpeakerTrack(envelope.speakerTrack);
    await refreshProject(project.id);
    setNotice("说话人已合并，可撤销或从版本历史恢复。 ");
  });

  const assignSpeaker = (segmentId: string, speakerId: string) => project && withBusy("正在重新分配说话人", async () => {
    const envelope = await runCore(["speaker", "assign", project.id, segmentId, speakerId]);
    if (!envelope.speakerTrack) throw new Error("Core 未返回更新后的说话人轨。 ");
    setSpeakerTrack(envelope.speakerTrack);
    await refreshProject(project.id);
    setNotice("当前字幕段的说话人已更新，可撤销或从版本历史恢复。 ");
  });

  const editSegment = (segment: Segment, text: string) => project && text.trim() !== segment.text && withBusy("正在保存文稿", async () => {
    await runCore(["transcript", "edit", project.id, segment.id, "--text", text.trim()]);
    await refreshProject(project.id);
    setNotice("原文已更新；对应译文需要更新。");
  });

  const replaceAll = () => project && search && withBusy("正在批量替换", async () => {
    const result = await runCore(["transcript", "replace", project.id, "--find", search, "--replace", replacement]);
    await refreshProject(project.id);
    setNotice(Number(result.changedSegments ?? 0) === 0 ? "没有找到匹配文字。" : `已替换 ${result.changedSegments} 个字幕段；可以从版本历史恢复。`);
  });

  const openStructureEdit = (mode: StructureEditMode) => {
    const target = selectedSegments[0];
    if (!project || !target) return;
    setStructureError(null);
    if (mode === "split") {
      const characterCount = Array.from(target.text).length;
      let splitAt = target.start + (target.end - target.start) / 2;
      const crossingWord = project.transcript.words.find((word) => word.segmentId === target.id && word.start < splitAt && word.end > splitAt);
      if (crossingWord) {
        if (crossingWord.end < target.end) splitAt = crossingWord.end;
        else if (crossingWord.start > target.start) splitAt = crossingWord.start;
      }
      setStructureTextOffset(String(Math.max(1, Math.floor(characterCount / 2))));
      setStructureStart(splitAt.toFixed(3));
    } else if (mode === "timing") {
      setStructureStart(target.start.toFixed(3));
      setStructureEnd(target.end.toFixed(3));
    } else if (mode === "offset") {
      setStructureDelta("0.100");
    }
    setStructureEditMode(mode);
  };

  const applyStructureEdit = async () => {
    if (!project || !structureEditMode || !selectedSegments.length) return;
    setStructureBusy(true);
    setStructureError(null);
    try {
      let args: string[];
      if (structureEditMode === "split") {
        const textOffset = Number(structureTextOffset);
        const at = Number(structureStart);
        if (!Number.isInteger(textOffset) || textOffset <= 0 || !Number.isFinite(at)) throw new Error("拆分位置必须是有效的文字序号和时间。");
        args = ["transcript", "split", project.id, selectedSegments[0].id, "--text-offset", String(textOffset), "--at", String(at)];
      } else if (structureEditMode === "merge") {
        if (!mergeCandidatesAdjacent) throw new Error("只能合并两个时间相邻的字幕段。");
        args = ["transcript", "merge", project.id, selectedSegments[0].id, selectedSegments[1].id];
      } else if (structureEditMode === "timing") {
        const start = Number(structureStart);
        const end = Number(structureEnd);
        if (!Number.isFinite(start) || !Number.isFinite(end) || start < 0 || end <= start) throw new Error("结束时间必须晚于非负的开始时间。");
        args = ["transcript", "timing", project.id, selectedSegments[0].id, "--start", String(start), "--end", String(end)];
      } else {
        const delta = Number(structureDelta);
        if (!Number.isFinite(delta) || delta === 0) throw new Error("批量偏移必须是非零秒数。");
        args = ["transcript", "offset", project.id, ...selectedSegments.flatMap((segment) => ["--segment", segment.id]), "--delta", String(delta)];
      }
      const envelope = await runCore(args);
      if (!envelope.structureEdit?.project) throw new Error("Core 未返回字幕结构操作结果。");
      const result = envelope.structureEdit;
      const nextProject = result.project;
      setProject(nextProject);
      setProjects((current) => current.map((item) => item.id === nextProject.id ? nextProject : item));
      const nextSelection = result.affectedSegmentIds.filter((id) => nextProject.transcript.segments.some((segment) => segment.id === id));
      setSelectedSegmentIds(nextSelection);
      setSelectedId(nextSelection[0] ?? nextProject.transcript.segments[0]?.id ?? null);
      setSelectionAnchorId(nextSelection[0] ?? null);
      setWordRange(null);
      setCutPreview(null);
      await refreshSpeakerTrack(project.id);
      setStructureEditMode(null);
      const messages: Record<StructureEditMode, string> = {
        split: "字幕已拆分，受影响的译文与剪辑证据已失效；可用 Ctrl+Z 撤销。",
        merge: "相邻字幕已合并，受影响的译文与剪辑证据已失效；可用 Ctrl+Z 撤销。",
        timing: "字幕时间已更新，不再可信的证据已失效；可用 Ctrl+Z 撤销。",
        offset: `已将 ${selectedSegments.length} 段字幕批量偏移 ${Number(structureDelta) > 0 ? "+" : ""}${Number(structureDelta).toFixed(3)} 秒；可用 Ctrl+Z 撤销。`,
      };
      setNotice(messages[structureEditMode]);
    } catch (cause) {
      setStructureError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setStructureBusy(false);
    }
  };

  const openSubtitleImport = () => {
    setSubtitleImportPath("");
    setSubtitleImportPreview(null);
    setSubtitleImportError(null);
    setSubtitleReplaceConfirmed(false);
    setShowSubtitleImport(true);
  };

  const inspectSubtitleFile = async () => {
    if (!project) return;
    setSubtitleImportBusy("正在预检字幕文件");
    setSubtitleImportError(null);
    try {
      const path = await pickSubtitleFile();
      if (!path) return;
      setSubtitleImportPath(path);
      setSubtitleReplaceConfirmed(false);
      const envelope = await runCore(["transcript", "inspect-file", project.id, path]);
      if (!envelope.subtitleImportPreview) throw new Error("Core 未返回字幕预检结果。");
      setSubtitleImportPreview(envelope.subtitleImportPreview);
    } catch (cause) {
      setSubtitleImportPreview(null);
      setSubtitleImportError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSubtitleImportBusy(null);
    }
  };

  const confirmSubtitleImport = async () => {
    if (!project || !subtitleImportPreview || !subtitleReplaceConfirmed) return;
    setSubtitleImportBusy("正在替换项目字幕");
    setSubtitleImportError(null);
    try {
      const envelope = await runCore([
        "transcript", "import-file", project.id, subtitleImportPath,
        "--confirm-replace", "--expected-sha256", subtitleImportPreview.sha256,
      ]);
      if (!envelope.project) throw new Error("Core 未返回替换后的项目。");
      setProject(envelope.project);
      setProjects((current) => current.map((item) => item.id === envelope.project?.id ? envelope.project : item) as Project[]);
      setSelectedId(envelope.project.transcript.segments[0]?.id ?? null);
      setWordRange(null);
      setCutPreview(null);
      await refreshSpeakerTrack(project.id);
      setShowSubtitleImport(false);
      setQualityFilter("all");
      setNotice(`已导入 ${envelope.project.transcript.segments.length} 段字幕并创建可撤销版本；原片和既有导出文件未修改。`);
    } catch (cause) {
      setSubtitleImportError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSubtitleImportBusy(null);
    }
  };

  const locateSubtitleIssue = (issue: SubtitleQualityIssue) => {
    const segment = project?.transcript.segments.find((candidate) => candidate.id === issue.segmentId);
    if (segment) selectSegment(segment);
  };

  const exportTranscript = () => project && withBusy("正在导出字幕或文稿", async () => {
    const output = await pickTranscriptPath(project.title, exportFormat);
    if (!output) return;
    await runCore(["transcript", "export", project.id, "--format", exportFormat, "--output", output, ...subtitleArgs()]);
    setNotice(`${exportFormat === "markdown" ? "文稿" : "字幕"}已导出到 ${output}`);
  });

  const subtitleArgs = () => {
    if (subtitleMode === "source") return ["--subtitle-mode", "source"];
    if (!selectedSubtitleLanguage) throw new Error("项目中没有可用译文，无法导出译文字幕。");
    if (selectedTranslationPending) throw new Error(`${selectedSubtitleLanguage.toUpperCase()} 译文仍在等待 Agent，完成后才能导出译文字幕。`);
    return ["--subtitle-mode", subtitleMode, "--lang", selectedSubtitleLanguage];
  };

  const changeCanvas = (settings: CanvasSettings) => project && withBusy("正在更新画布设置", async () => {
    await runCore(["canvas", "set", project.id, "--aspect-ratio", settings.aspectRatio, "--framing", settings.framing]);
    await refreshProject(project.id);
    setMediaUrl(await authorizeMedia(project.id));
    setNotice(settings.aspectRatio === "9:16" ? "画布已改为 9:16；请重新生成预览以查看最终构图。" : "已恢复原始画布比例；请重新生成预览。 ");
  });

  const changeSubtitleStyle = (preset: Project["subtitleStyle"]["preset"], position: Project["subtitleStyle"]["position"]) => project && withBusy("正在更新字幕样式", async () => {
    const envelope = await runCore(["transcript", "set-style", project.id, "--preset", preset, "--position", position]);
    if (!envelope.project) throw new Error("Core 未返回更新后的字幕样式。");
    setProject(envelope.project);
    setProjects((current) => current.map((item) => item.id === envelope.project!.id ? envelope.project! : item));
    setNotice("字幕样式已更新；正文和时间未修改，可撤销。");
  });

  const preparePreview = () => project && withBusy("正在生成一次性预览资源", async () => {
    await runCore(["media", "prepare", project.id]);
    await refreshProject(project.id, true);
    setNotice("代理视频、波形和关键帧已生成；后续审阅不会重复生成完整视频。");
  });

  const exportVideo = () => project && withBusy("正在创建视频导出任务", async () => {
    const output = await pickVideoPath(project.title);
    if (!output) return;
    const envelope = await runCore(["video", "export", project.id, "--output", output, "--burn-subtitles", ...subtitleArgs()]);
    if (!envelope.job) throw new Error("Core 未返回视频导出任务。");
    setActiveExport(envelope.job);
    setNotice(envelope.job.status === "completed" ? `视频已导出到 ${envelope.job.outputPath}` : "视频导出已开始，关闭窗口后仍会继续。");
  });

  const cancelExport = () => activeExport && withBusy("正在取消视频导出", async () => {
    const envelope = await runCore(["video", "cancel", activeExport.id]);
    if (envelope.job) setActiveExport(envelope.job);
    setNotice("已请求取消视频导出。");
  });

  const retryExport = () => activeExport && withBusy("正在重新开始视频导出", async () => {
    const envelope = await runCore(["video", "retry", activeExport.id]);
    if (!envelope.job) throw new Error("Core 未返回视频导出任务。");
    setActiveExport(envelope.job);
    setNotice("视频导出已重新开始，将从安全边界重新生成成片。");
  });

  const updateCut = (editId: string, action: "apply" | "restore") => project && withBusy(action === "apply" ? "正在应用软剪辑" : "正在恢复此处", async () => {
    await runCore(["cut", action, project.id, editId]);
    await refreshProject(project.id);
    setNotice(action === "apply" ? "已应用软剪辑；预览时间线已更新，原片未修改。" : "已恢复此处；预览时间线已更新。");
  });

  const detectSuggestions = () => project && withBusy("正在检测口头语和重复表达", async () => {
    const envelope = await runCore(["cut", "detect", project.id]);
    const count = envelope.suggestions?.length ?? 0;
    await refreshProject(project.id);
    setNotice(count ? `发现 ${count} 条粗剪建议；试听并确认后才会应用。` : "没有发现达到置信度门槛的新建议。");
  });

  const startCutPreview = async (editId: string) => {
    if (!project) return;
    const envelope = await runCore(["cut", "preview", project.id, editId]);
    if (!envelope.preview) throw new Error("Core 未返回切点试听窗口。");
    setCutPreview(envelope.preview);
    const video = videoRef.current;
    if (!video) {
      setNotice("切点已创建；生成媒体预览后可试听切点前后 1 秒。");
      return;
    }
    video.currentTime = envelope.preview.previewStart;
    await video.play();
    setNotice("正在试听切点前后 1 秒；中间剪切范围会自动跳过。");
  };

  const previewCut = (editId: string) => withBusy("正在准备切点试听", async () => {
    await startCutPreview(editId);
  });

  const createWordCut = () => project && selected && activeWordRange && withBusy("正在创建词范围剪辑", async () => {
    const from = selectedWords[activeWordRange.start];
    const to = selectedWords[activeWordRange.end];
    if (!from || !to) throw new Error("所选词范围已经变化，请重新选择。");
    const envelope = await runCore([
      "cut", "create", project.id,
      "--segment", selected.id,
      "--from-word", from.id,
      "--to-word", to.id,
      "--padding-ms", String(cutPadding),
    ]);
    if (!envelope.cut) throw new Error("Core 未返回词范围剪辑。");
    await refreshProject(project.id);
    setWordRange(null);
    await startCutPreview(envelope.cut.id);
  });

  const handleVideoTimeUpdate = () => {
    const video = videoRef.current;
    if (!video || !project) return;
    if (cutPreview) {
      if (video.currentTime >= cutPreview.cutStart && video.currentTime < cutPreview.cutEnd - 0.01) {
        video.currentTime = cutPreview.cutEnd;
        return;
      }
      if (video.currentTime >= cutPreview.previewEnd) {
        video.pause();
        setCutPreview(null);
        return;
      }
    }
    const cut = project.timeline.cuts.find((candidate) => video.currentTime >= candidate.sourceStart && video.currentTime < candidate.sourceEnd - 0.01);
    if (cut) video.currentTime = cut.sourceEnd;
  };

  const restoreVersion = (versionId: string) => project && withBusy("正在恢复可逆版本", async () => {
    await runCore(["project", "restore", project.id, versionId]);
    await refreshProject(project.id);
    setNotice("已恢复所选版本，后续版本仍保留在历史中。");
  });

  const navigateHistory = (action: "undo" | "redo") => project && withBusy(action === "undo" ? "正在撤销" : "正在重做", async () => {
    const envelope = await runCore(["project", action, project.id]);
    if (!envelope.project) throw new Error("Core 未返回历史操作后的项目。");
    setProject(envelope.project);
    setProjects((current) => current.map((item) => item.id === envelope.project?.id ? envelope.project : item) as Project[]);
    setWordRange(null);
    setCutPreview(null);
    setNotice(action === "undo" ? "已撤销上一步项目修改。" : "已重做项目修改。");
  });

  useEffect(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      const target = event.target;
      const modifier = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      const dialogOpen = showRuntime || showSourceImport || showAutoWorkflow || showSubtitleImport || Boolean(structureEditMode) || Boolean(currentDeleteCandidate);
      const editingTarget = target instanceof HTMLElement && (target.isContentEditable || target.matches("input, textarea, select"));
      if (event.key === "Escape" && showMoreMenu) {
        event.preventDefault();
        setShowMoreMenu(false);
        return;
      }
      if (!dialogOpen && modifier && key === "f") {
        event.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
        return;
      }
      if (!dialogOpen && modifier && key === "h") {
        event.preventDefault();
        replacementInputRef.current?.focus();
        replacementInputRef.current?.select();
        return;
      }
      if (!dialogOpen && modifier && event.shiftKey && key === "e") {
        event.preventDefault();
        if (project) setShowExportPanel(true);
        return;
      }
      if (!dialogOpen && !busy && !editingTarget && modifier && event.shiftKey && ["s", "m", "t", "o"].includes(key)) {
        event.preventDefault();
        const mode = ({ s: "split", m: "merge", t: "timing", o: "offset" } as const)[key as "s" | "m" | "t" | "o"];
        if (mode === "split" && selectedSegments.length === 1 && Array.from(selectedSegments[0].text).length > 1) openStructureEdit(mode);
        if (mode === "merge" && mergeCandidatesAdjacent) openStructureEdit(mode);
        if (mode === "timing" && selectedSegments.length === 1) openStructureEdit(mode);
        if (mode === "offset" && selectedSegments.length > 0) openStructureEdit(mode);
        return;
      }
      if (!dialogOpen && !busy && !editingTarget && event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
        event.preventDefault();
        moveSegmentSelection(event.key === "ArrowUp" ? -1 : 1);
        return;
      }
      if (target instanceof HTMLElement && (editingTarget || target.matches("button"))) return;
      if (dialogOpen || busy) return;
      if (modifier && event.key.toLowerCase() === "z") {
        event.preventDefault();
        void navigateHistory(event.shiftKey ? "redo" : "undo");
        return;
      }
      if (modifier && event.key.toLowerCase() === "y") {
        event.preventDefault();
        void navigateHistory("redo");
        return;
      }
      const video = videoRef.current;
      if (!video || modifier || event.altKey) return;
      if (event.code === "Space") {
        event.preventDefault();
        if (video.paused) void video.play(); else video.pause();
      } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        const change = event.key === "ArrowLeft" ? -1 : 1;
        video.currentTime = Math.max(0, Math.min(video.duration || project?.timeline.sourceDuration || 0, video.currentTime + change));
      }
    };
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  }, [busy, currentDeleteCandidate, mergeCandidatesAdjacent, project, selectedSegmentIds, showAutoWorkflow, showMoreMenu, showRuntime, showSourceImport, showSubtitleImport, structureEditMode]);

  const chooseModel = () => withBusy("正在选择本机模型", async () => {
    const path = await pickModel();
    if (!path) return;
    localStorage.setItem("siaocut.modelPath", path);
    setModelPath(path);
    setNotice("已选择本机模型；模型文件不会上传。");
  });

  const installModel = (modelId: string) => withBusy("正在创建模型下载任务", async () => {
    const envelope = await runCore(["model", "install", modelId]);
    if (!envelope.modelJob) throw new Error("Core 未返回模型下载任务。");
    setModelJob(envelope.modelJob);
    if (envelope.modelJob.status === "completed") {
      const catalog = await runCore(["model", "list"]);
      const available = catalog.models ?? [];
      setModels(available);
      const installed = available.find((item) => item.id === modelId);
      if (installed) {
        localStorage.setItem("siaocut.modelPath", installed.path);
        setModelPath(installed.path);
      }
      setNotice("模型已下载并通过 SHA-256 校验，可以开始本地转录。");
      return;
    }
    setNotice("模型下载已开始；来源、体积和许可证显示在当前页面。");
  });

  const cancelModel = () => modelJob && withBusy("正在暂停模型下载", async () => {
    const envelope = await runCore(["model", "cancel", modelJob.id]);
    if (envelope.modelJob) setModelJob(envelope.modelJob);
  });

  const removeModel = (modelId: string) => withBusy("正在移除本机模型", async () => {
    await runCore(["model", "remove", modelId]);
    const catalog = await runCore(["model", "list"]);
    const available = catalog.models ?? [];
    setModels(available);
    const selected = models.find((item) => item.id === modelId)?.path;
    if (selected && selected === modelPath) {
      localStorage.removeItem("siaocut.modelPath");
      setModelPath(null);
    }
    setNotice("模型已移除；项目、字幕和原始媒体未受影响。");
  });

  const createAgentTask = () => project && withBusy("正在创建文字任务", async () => {
    await runCore(["workflow", "create", project.id, "--kind", "polish"]);
    await refreshProject(project.id);
    setNotice("润色工作流已创建，需要 Agent 继续。媒体文件不会交给 Agent。");
  });

  const updateTask = (taskId: string, action: "retry" | "cancel") => project && withBusy(action === "retry" ? "正在重新排队" : "正在取消任务", async () => {
    await runCore(["task", action, taskId]);
    await refreshProject(project.id);
    setNotice(action === "retry" ? "任务已重新排队，需要 Agent 继续。" : "取消请求已记录。");
  });

  const reviewPatch = (patchItemId: string, action: "apply" | "keep") => project && withBusy(action === "apply" ? "正在应用建议" : "正在保留原文", async () => {
    await runCore(["task", "review", patchItemId, "--action", action]);
    await refreshProject(project.id);
    setNotice(action === "apply" ? "已应用此条建议，并创建可恢复版本。" : "已保留当前文本。");
  });

  const reviewAll = (taskId: string, action: "apply" | "keep") => project && withBusy(action === "apply" ? "正在应用全部建议" : "正在保留全部原文", async () => {
    await runCore(["task", "review-all", taskId, "--action", action]);
    await refreshProject(project.id);
    setNotice(action === "apply" ? "已应用全部待审建议。" : "已保留全部当前文本。");
  });

  return (
    <main className="app-shell">
      <aside className="rail">
        <div className="brand"><span className="brand-mark">S</span><span>SiaoCut</span></div>
        <div className="new-project-actions">
          <button ref={autoButtonRef} className="new-project auto" onClick={() => setShowAutoWorkflow(true)}><Sparkles size={16} />一键成片</button>
          <button className="new-project" onClick={importMedia}><FolderPlus size={16} />新建项目</button>
          <button ref={sourceButtonRef} className="new-project url" onClick={() => setShowSourceImport(true)}><Link2 size={16} />从 URL 导入</button>
        </div>
        <div className="rail-heading">项目</div>
        <nav aria-label="本地项目">
          {projects.map((item) => (
            <div className={`project-entry ${project?.id === item.id ? "active" : ""}`} key={item.id}>
              <button className="project-link" onClick={() => switchProject(item.id)}>
                <span className="project-dot" /><span><strong>{item.title}</strong><small>{item.transcript.segments.length} 段字幕</small></span><ChevronRight size={14} />
              </button>
              <button className="project-delete" aria-label={`删除项目 ${item.title}`} title="删除项目" onClick={() => openDeleteDialog(item)}><Trash2 size={14} /></button>
            </div>
          ))}
          {!projects.length && !busy && <p className="empty-rail">还没有本地项目。</p>}
        </nav>
        <button ref={runtimeButtonRef} className="runtime-link" onClick={() => setShowRuntime(true)}><Settings2 size={15} /><span>运行环境</span></button>
        <div className="privacy"><ShieldCheck size={15} /><span>媒体和文稿保存在本机</span></div>
      </aside>

      <section className="workbench">
        <header className="topbar">
          <div className="topbar-heading"><p className="eyebrow">项目 · 单源口播</p><h1>{project?.title ?? "新建第一个项目"}</h1></div>
          <div className="command-bar" aria-label="项目命令">
            <StatusBadge tone={humanStateTone}>{humanState}</StatusBadge>
            <div className="command-history" aria-label="编辑历史">
              <IconButton label="撤销" shortcut="Ctrl+Z" disabled={!project?.history.canUndo || Boolean(busy)} onClick={() => navigateHistory("undo")}><Undo2 size={15} /></IconButton>
              <IconButton label="重做" shortcut="Ctrl+Shift+Z" disabled={!project?.history.canRedo || Boolean(busy)} onClick={() => navigateHistory("redo")}><Redo2 size={15} /></IconButton>
            </div>
            <Button disabled={!project || Boolean(busy)} onClick={transcribe}><RefreshCw size={15} />转录</Button>
            <Button className="rough-cut-command" disabled={!project?.transcript.words.length || Boolean(busy)} onClick={detectSuggestions}><Scissors size={15} />粗剪建议</Button>
            <Button variant="agent" disabled={!project || Boolean(busy)} onClick={createAgentTask}><Bot size={15} />交给 Agent</Button>
            <div className="command-more">
              <IconButton label="更多命令" onClick={() => setShowMoreMenu((current) => !current)}><MoreHorizontal size={17} /></IconButton>
              {showMoreMenu && <div className="command-menu" role="menu">
                <button role="menuitem" disabled={!project || Boolean(busy)} onClick={() => { setShowMoreMenu(false); void preparePreview(); }}><Film size={14} />生成预览</button>
                <button role="menuitem" disabled={!project || Boolean(busy)} onClick={() => { setShowMoreMenu(false); void relinkMedia(); }}><Link2 size={14} />重新定位原片</button>
              </div>}
            </div>
            <div className="export-split">
              <Button ref={exportButtonRef} variant="primary" className="export-main" disabled={!project || Boolean(busy) || selectedTranslationPending || Boolean(activeExport && ["queued", "running"].includes(activeExport.status))} onClick={exportVideo}><Download size={15} />导出视频</Button>
              <button className="export-settings" aria-label="打开导出设置" title="打开导出设置 · Ctrl+Shift+E" disabled={!project} onClick={() => setShowExportPanel(true)}><ChevronDown size={15} /></button>
            </div>
          </div>
        </header>

        {(notice || error) && <div className={`notice ${error ? "error" : ""}`} role="status">{error && <CircleAlert size={15} />}{error ?? notice}{error && <button className="notice-action" onClick={() => void initialize()}>重新检查</button>}<button aria-label="关闭提示" title="关闭提示" onClick={() => { setNotice(null); setError(null); }}>×</button></div>}
        {busy && <div className="progress-strip"><LoaderCircle size={14} className="spin" />{busy}</div>}
        {activeExport && ["queued", "running"].includes(activeExport.status) && <div className="export-progress" role="status"><Film size={15} /><span>正在导出视频 · {Math.round(activeExport.progress * 100)}%</span><progress value={activeExport.progress} max={1} /><button onClick={cancelExport}>取消导出</button></div>}
        {activeExport && ["failed", "interrupted"].includes(activeExport.status) && <div className="export-progress interrupted" role="status"><CircleAlert size={15} /><span>{activeExport.status === "interrupted" ? "上次导出被中断" : "视频导出失败"}</span><small>{activeExport.errorMessage ?? "可以重新开始，不会覆盖原片。"}</small><button onClick={retryExport}>重新开始</button></div>}
        {sourceJob && !showSourceImport && ["queued", "running", "finalizing"].includes(sourceJob.status) && <div className="source-progress" role="status"><Link2 size={15} /><span><strong>{sourceStatusLabel(sourceJob.status)} · {Math.round(sourceJob.progress * 100)}%</strong><small>{sourceJob.title}</small></span><progress value={sourceJob.progress} max={1} /><button onClick={() => setShowSourceImport(true)}>查看任务</button></div>}
        {autoWorkflow && <section className={`auto-progress ${autoWorkflow.status}`} aria-label="一键工作流状态">
          <Sparkles size={17} />
          <span className="auto-progress-copy"><strong>{autoStatusLabel(autoWorkflow.status)} · {autoStageLabel(autoWorkflow.currentStage)}</strong><small>{!autoWorkflow.projectId && ["needs_agent", "needs_review"].includes(autoWorkflow.status) ? "关联项目已删除；请取消此流程后重新创建。" : autoWorkflow.status === "needs_agent" ? "翻译任务已交给 Agent；结果不会自动写入文稿。" : autoWorkflow.status === "needs_review" ? "请在「下一步」中处理全部建议，然后显式继续。" : autoWorkflow.errorMessage ?? autoWorkflow.outputPath}</small></span>
          <progress value={autoWorkflow.progress} max={1} aria-label="一键工作流进度" />
          <span className="auto-progress-percent">{Math.round(autoWorkflow.progress * 100)}%</span>
          <div className="auto-progress-actions">
            {autoWorkflow.projectId && ["needs_agent", "needs_review"].includes(autoWorkflow.status) && <button onClick={() => void openAutoProject()}>打开待审项目</button>}
            {["queued", "running", "needs_agent", "needs_review", "failed", "interrupted"].includes(autoWorkflow.status) && <button disabled={Boolean(autoBusy)} onClick={() => void cancelAutoWorkflow()}>取消流程</button>}
            {autoWorkflow.status === "needs_review" && <button className="primary" disabled={Boolean(autoBusy)} onClick={() => void continueAutoWorkflow()}>确认完成并继续</button>}
            {["failed", "interrupted"].includes(autoWorkflow.status) && <button className="primary" disabled={Boolean(autoBusy)} onClick={() => void continueAutoWorkflow()}>显式继续</button>}
            {["completed", "cancelled"].includes(autoWorkflow.status) && <button onClick={() => setShowAutoWorkflow(true)}>新建一键流程</button>}
          </div>
          {autoError && <p className="auto-progress-error" role="alert">{autoError}</p>}
        </section>}

        {!project ? (
          <section className="welcome-card">
            <div className="welcome-icon"><FileVideo2 size={30} /></div>
            <p className="eyebrow">本地优先</p><h2>从一段口播开始。</h2>
            <p>导入本地音视频后，SiaoCut 会建立可恢复项目。Agent 只能领取你明确创建的文字任务。</p>
            <RuntimeChecklist runtime={runtime} modelPath={modelPath} onChooseModel={chooseModel} compact />
            <div className="welcome-actions"><button className="button primary" onClick={() => setShowAutoWorkflow(true)}><Sparkles size={16} />一键成片</button><button className="button quiet" onClick={importMedia}><FolderPlus size={16} />选择音视频</button><button className="button quiet" onClick={() => setShowSourceImport(true)}><Link2 size={16} />导入公开 URL</button></div>
          </section>
        ) : (
          <>
            <section className="stage-grid">
              <article className="video-panel">
                <div className="video-frame">
                  {mediaUrl ? <video key={project.id} ref={videoRef} src={mediaUrl} controls preload="metadata" onTimeUpdate={handleVideoTimeUpdate} /> : <div className="video-placeholder"><Play size={30} /><span>音频项目或媒体预览尚未授权</span></div>}
                  {showSubtitleSafeArea && <div className="subtitle-safe-area" aria-label="字幕安全区" style={{ inset: `${project.subtitleStyle.safeMarginPercent}% 6%` }} />}
                  {selected && captionPrimaryText && <div className={`caption-overlay ${project.subtitleStyle.position}`} data-preset={project.subtitleStyle.preset} data-position={project.subtitleStyle.position} data-outline-width={project.subtitleStyle.outlineWidth} style={captionPreviewStyle}>
                    <span className="caption-primary">{captionPrimaryText}</span>
                    {captionSecondaryText && <span className="caption-secondary" style={{ color: project.subtitleStyle.secondaryColor, fontSize: `${Math.max(12, Math.round(project.subtitleStyle.secondaryFontSize * 0.36))}px` }}>{captionSecondaryText}</span>}
                  </div>}
                </div>
                <div className="transport-summary"><Clock3 size={14} /><span>{selected ? `${formatTime(selected.start)} — ${formatTime(selected.end)}` : "选择一段字幕以定位媒体"}</span><button className="relink-media" onClick={relinkMedia}>重新定位原片</button><span className="shortcut-hint">Space 播放 · ←/→ 定位</span><span className="spacer" /><span>成片 {formatTime(project.timeline.outputDuration)} · 原片 {formatTime(project.timeline.sourceDuration)}</span></div>
                {audioRisks.length > 0 && <div className="audio-risk-strip" role="status"><CircleAlert size={14} /><strong>{audioRisks.length} 项音频风险</strong><span>{audioRiskLabel(audioRisks[0].kind)} · {formatTime(audioRisks[0].start)}</span><button onClick={() => locateAudioRisk(audioRisks[0])}>定位</button></div>}
              </article>

              <aside className="review-panel">
                <div className="section-title"><div><p className="eyebrow">人工审阅队列</p><h2>下一步</h2></div>{actionableReviewCount > 0 && <span className="state-count" aria-label={`${actionableReviewCount} 项需要处理`}>{actionableReviewCount}</span>}</div>
                <div className="review-panel-scroll" role="region" aria-label="工作流任务列表" tabIndex={0}>
                  {orderedPatchSets.map((set) => <section className="patch-set" key={set.id}>
                    <header><span>{set.kind}{set.language ? ` · ${set.language.toUpperCase()}` : ""}</span>{set.items.length > 1 && <div><button onClick={() => reviewAll(set.taskId, "keep")}>全部保留</button><button onClick={() => reviewAll(set.taskId, "apply")}>全部应用</button></div>}</header>
                    {set.items.map((item) => <PatchReviewCard key={item.id} item={item} onReview={(action) => reviewPatch(item.id, action)} onSelect={() => { const segment = project.transcript.segments.find((candidate) => candidate.id === item.segmentId); if (segment) selectSegment(segment); }} />)}
                  </section>)}
                  {pendingEdits.map((edit) => <article className="review-item" key={edit.id}><span className="review-tag">需要人工确认 · {cutSuggestionLabel(edit.suggestion?.suggestionType)}</span><strong>{edit.reason}</strong><p>{formatTime(edit.start)} — {formatTime(edit.end)}{edit.suggestion ? ` · 置信度 ${Math.round(edit.suggestion.confidence * 100)}%` : ""} · 尚未删除</p><div className="cut-actions"><button onClick={() => selectSegment(project.transcript.segments.find((segment) => segment.id === edit.segmentId)!)}>定位原文</button>{edit.kind === "word_cut" && <button onClick={() => previewCut(edit.id)}><Headphones size={11} />试听切点</button>}<button onClick={() => updateCut(edit.id, "apply")}>应用软剪辑</button></div></article>)}
                  {audioRisks.map((risk, index) => <article className="review-item audio-risk-item" key={`${risk.kind}-${risk.start}-${index}`}><span className="review-tag warning"><CircleAlert size={12} />音频质量 · 等待确认</span><strong>{audioRiskLabel(risk.kind)}</strong><p>{formatTime(risk.start)} — {formatTime(risk.end)} · 实测 {risk.measuredValue} {audioUnitLabel(risk.unit)} · 阈值 {risk.threshold} {audioUnitLabel(risk.unit)}</p><button onClick={() => locateAudioRisk(risk)}>定位原片</button></article>)}
                  {audioAnalysisJob && ["failed", "interrupted"].includes(audioAnalysisJob.status) && <article className="review-item task-item failure"><span className="review-tag failure"><CircleAlert size={12} />音频分析{audioAnalysisJob.status === "interrupted" ? "已中断" : "失败"}</span><strong>本地音频质量分析</strong><p>{audioAnalysisJob.errorMessage ?? "可以显式继续，不影响其他工作流。"}</p><button onClick={resumeAudioAnalysis}><RefreshCw size={11} />显式继续</button></article>}
                  {audioAnalysisJob && ["queued", "running"].includes(audioAnalysisJob.status) && <article className="review-item task-item processing"><span className="review-tag info"><Activity size={12} />本机正在分析</span><strong>音频质量</strong><p>{Math.round(audioAnalysisJob.progress * 100)}% · 媒体不会上传</p><button disabled={Boolean(audioAnalysisJob.cancelRequestedAt)} onClick={cancelAudioAnalysis}>{audioAnalysisJob.cancelRequestedAt ? "正在取消" : "取消分析"}</button></article>}
                  {projectSpeakerJob && ["failed", "interrupted"].includes(projectSpeakerJob.status) && <article className="review-item task-item failure"><span className="review-tag failure"><CircleAlert size={12} />说话人分析{projectSpeakerJob.status === "interrupted" ? "已中断" : "失败"}</span><strong>本地说话人轨</strong><p>{projectSpeakerJob.errorMessage ?? "可以显式继续，不影响字幕和剪辑。"}</p><button onClick={resumeSpeakerJob}><RefreshCw size={11} />显式继续</button></article>}
                  {projectSpeakerJob && ["queued", "running"].includes(projectSpeakerJob.status) && <article className="review-item task-item processing"><span className="review-tag info"><Users size={12} />本机正在分析</span><strong>说话人轨</strong><p>{projectSpeakerJob.stage} · {Math.round(projectSpeakerJob.progress * 100)}% · 结果不会自动写入字幕</p><button onClick={cancelSpeakerJob}>取消分析</button></article>}
                  {failedTasks.map((task) => <article className="review-item task-item failure" key={task.id}><span className="review-tag failure"><CircleAlert size={12} />Agent {task.status === "interrupted" ? "已中断" : "处理失败"}</span><strong>{task.kind}</strong><p>{Math.round(task.progress * 100)}%{task.errorMessage ? ` · ${task.errorMessage}` : ""}</p><button onClick={() => updateTask(task.id, "retry")}><RefreshCw size={11} />重新排队</button></article>)}
                  {processingTasks.map((task) => <article className="review-item task-item processing" key={task.id}><span className="review-tag agent"><Bot size={12} />{task.status === "queued" ? "等待 Agent" : "Agent 正在处理"}</span><strong>{task.kind}</strong><p>{Math.round(task.progress * 100)}%</p><button onClick={() => updateTask(task.id, "cancel")}>取消任务</button></article>)}
                  {actionableReviewCount === 0 && processingTasks.length === 0 && !audioAnalysisJob?.status.match(/queued|running|failed|interrupted/) && !projectSpeakerJob?.status.match(/queued|running|failed|interrupted/) && <div className="all-clear"><Check size={20} /><span>没有待处理事项</span></div>}
                  {recentTasks.length > 0 && <details className="review-history"><summary>近期记录 · {recentTasks.length}</summary><div>{recentTasks.map((task) => <article className="review-item recent" key={task.id}><span className="review-tag success"><Check size={12} />{task.status === "completed" ? "已完成" : "已取消"}</span><strong>{task.kind}</strong><p>{Math.round(task.progress * 100)}%</p></article>)}</div></details>}
                  <div className="review-principle"><Sparkles size={16} /><span>Agent 结果会先显示差异，不会直接改写文稿。</span></div>
                </div>
              </aside>
            </section>

            <section className="editor-grid">
              <article className="transcript-panel">
                <header className="panel-header"><div><p className="eyebrow">可校对文稿</p><h2>转录</h2></div><div className="find-replace"><button ref={subtitleImportButtonRef} className="subtitle-import-command" disabled={Boolean(busy)} onClick={openSubtitleImport}><FileText size={12} />导入字幕</button><button className="detect-suggestions" disabled={!project.transcript.words.length || Boolean(busy)} onClick={detectSuggestions}><Scissors size={12} />检测粗剪建议</button><label className="search"><Search size={14} /><input ref={searchInputRef} value={search} onChange={(event) => setSearch(event.target.value)} placeholder="查找文字" title="Ctrl+F" /></label><input ref={replacementInputRef} aria-label="替换为" value={replacement} onChange={(event) => setReplacement(event.target.value)} placeholder="替换为" title="Ctrl+H" /><button disabled={!search || Boolean(busy)} onClick={replaceAll}>全部替换</button></div></header>
                <div className="transcript-meta"><span>点选文字定位视频；文字可直接编辑</span><span>{project.transcript.sourceLanguage.toUpperCase()} · {project.transcript.segments.length} 段 · {project.transcript.words.length} 词</span></div>
                <section className="subtitle-workbench-toolbar" aria-label="字幕结构工具栏">
                  <div className="subtitle-selection-summary"><ListChecks size={15} /><span><strong>{selectedScopeLabel}</strong><small>Ctrl 点选 · Shift 连选</small></span></div>
                  <div className="subtitle-selection-controls">
                    <button aria-label="上一段字幕" title="上一段字幕 · Alt+↑" disabled={!selectedId || project.transcript.segments[0]?.id === selectedId || Boolean(busy)} onClick={() => moveSegmentSelection(-1)}><ChevronUp size={14} /></button>
                    <button aria-label="下一段字幕" title="下一段字幕 · Alt+↓" disabled={!selectedId || project.transcript.segments.at(-1)?.id === selectedId || Boolean(busy)} onClick={() => moveSegmentSelection(1)}><ChevronDown size={14} /></button>
                    <button className="selection-scope" disabled={!filteredSegments.length || Boolean(busy)} onClick={() => {
                      if (allVisibleSegmentsSelected && selected) {
                        setSelectedSegmentIds([selected.id]);
                        setSelectionAnchorId(selected.id);
                      } else {
                        const ids = filteredSegments.map((segment) => segment.id);
                        setSelectedSegmentIds(ids);
                        setSelectedId(filteredSegments[0]?.id ?? null);
                        setSelectionAnchorId(filteredSegments[0]?.id ?? null);
                      }
                    }}>{allVisibleSegmentsSelected ? "仅保留当前" : `选择当前结果 ${filteredSegments.length}`}</button>
                  </div>
                  <div className="subtitle-structure-actions">
                    <button disabled={selectedSegments.length !== 1 || Array.from(selectedSegments[0]?.text ?? "").length < 2 || Boolean(busy)} title="拆分字幕 · Ctrl+Shift+S" onClick={() => openStructureEdit("split")}><Scissors size={13} />拆分</button>
                    <button disabled={!mergeCandidatesAdjacent || Boolean(busy)} title="合并相邻字幕 · Ctrl+Shift+M" onClick={() => openStructureEdit("merge")}><Link2 size={13} />合并</button>
                    <button disabled={selectedSegments.length !== 1 || Boolean(busy)} title="调整时间 · Ctrl+Shift+T" onClick={() => openStructureEdit("timing")}><Clock3 size={13} />时间</button>
                    <button disabled={!selectedSegments.length || Boolean(busy)} title="批量偏移 · Ctrl+Shift+O" onClick={() => openStructureEdit("offset")}><MoveHorizontal size={13} />偏移</button>
                  </div>
                </section>
                <section className={`subtitle-quality-summary ${project.subtitleQuality.status}`} aria-label="字幕质量">
                  <div className="subtitle-quality-state">{project.subtitleQuality.status === "good" ? <Check size={15} /> : <CircleAlert size={15} />}<span><strong>{project.subtitleQuality.statusLabel}</strong><small>{project.subtitleQuality.errorCount} 项错误 · {project.subtitleQuality.warningCount} 项提醒</small></span></div>
                  <div className="subtitle-quality-filters" aria-label="字幕问题筛选"><button className={qualityFilter === "all" ? "active" : ""} onClick={() => setQualityFilter("all")}>全部</button><button className={qualityFilter === "error" ? "active" : ""} disabled={!project.subtitleQuality.errorCount} onClick={() => setQualityFilter("error")}>错误 {project.subtitleQuality.errorCount}</button><button className={qualityFilter === "warning" ? "active" : ""} disabled={!project.subtitleQuality.warningCount} onClick={() => setQualityFilter("warning")}>提醒 {project.subtitleQuality.warningCount}</button></div>
                  {visibleQualityIssues.length > 0 && <div className="subtitle-quality-issues">{visibleQualityIssues.slice(0, 4).map((issue) => <button className={issue.severity} key={issue.id} onClick={() => locateSubtitleIssue(issue)}><CircleAlert size={12} /><span><strong>{issue.message}</strong><small>{formatTime(issue.start)} · 点击定位</small></span></button>)}</div>}
                </section>
                <div className="segment-list" aria-label="字幕文稿列表">
                  {filteredSegments.map((segment) => { const association = associationBySegment.get(segment.id); return <SegmentRow key={segment.id} segment={segment} speaker={association ? speakerById.get(association.speakerId) : undefined} speakerManual={association?.source === "manual"} selected={selectedSegmentIds.includes(segment.id)} active={segment.id === selectedId} translation={translation?.[1]} onSelect={(mode) => selectSegmentInWorkbench(segment, mode)} onSave={(text) => editSegment(segment, text)} />; })}
                  {!filteredSegments.length && <p className="empty-list">{project.transcript.segments.length ? "没有匹配的文字。" : "尚无字幕。点击「转录」开始本地识别。"}</p>}
                </div>
              </article>

              <aside className="context-panel">
                <p className="eyebrow">当前段落</p><h2>{selected?.text ?? "选择一段字幕"}</h2>
                <dl><div><dt>时间</dt><dd>{selected ? `${formatTime(selected.start)} — ${formatTime(selected.end)}` : "—"}</dd></div><div><dt>置信度</dt><dd>{selected?.confidence == null ? "未提供" : `${Math.round(selected.confidence * 100)}%`}</dd></div><div><dt>译文</dt><dd className={translation?.[1].status === "stale" ? "stale" : ""}>{translation ? (translation[1].status === "stale" ? "需要更新" : `${translation[0].toUpperCase()} · 已同步`) : "尚未创建"}</dd></div></dl>
                <SpeechInsightsPanel insights={project.speechInsights} onLocateEvidence={locateSpeechEvidence} onLocatePause={locateSpeechPause} />
                <AudioQualityPanel job={audioAnalysisJob} onStart={startAudioAnalysis} onCancel={cancelAudioAnalysis} onResume={resumeAudioAnalysis} onLocate={locateAudioRisk} disabled={Boolean(busy)} />
                <SpeakerTrackPanel packageStatus={speakerPackage} track={speakerTrack} job={projectSpeakerJob} selectedSegmentId={selectedId} disabled={Boolean(busy)} onOpenRuntime={() => setShowRuntime(true)} onAnalyze={startSpeakerAnalysis} onCancel={cancelSpeakerJob} onResume={resumeSpeakerJob} onRename={renameSpeaker} onMerge={mergeSpeaker} onAssign={assignSpeaker} />
                {selectedWords.length > 0 && <section className="word-evidence" aria-label="词级时间"><div className="word-heading"><div><p className="eyebrow">词级时间</p><small>点第一个词，再点结束词</small></div>{activeWordRange && <button className="clear-range" onClick={() => setWordRange(null)}>清除</button>}</div><div className="word-tokens">{selectedWords.map((word, index) => <button className={activeWordRange && index >= activeWordRange.start && index <= activeWordRange.end ? "selected" : ""} key={word.id} onClick={() => selectWordForCut(index)} title={`${formatTime(word.start)} — ${formatTime(word.end)}${word.confidence == null ? "" : ` · ${Math.round(word.confidence * 100)}%`}`}>{word.text}</button>)}</div>{activeWordRange && <div className="word-cut-controls"><label>起点把手<input aria-label="剪切起点" type="range" min="0" max={selectedWords.length - 1} value={activeWordRange.start} onChange={(event) => setWordRange({ ...activeWordRange, start: Math.min(Number(event.target.value), activeWordRange.end) })} /><small>{selectedWords[activeWordRange.start]?.text}</small></label><label>终点把手<input aria-label="剪切终点" type="range" min="0" max={selectedWords.length - 1} value={activeWordRange.end} onChange={(event) => setWordRange({ ...activeWordRange, end: Math.max(Number(event.target.value), activeWordRange.start) })} /><small>{selectedWords[activeWordRange.end]?.text}</small></label><label className="padding-select">安全留白<select aria-label="安全留白" value={cutPadding} onChange={(event) => setCutPadding(Number(event.target.value) as 30 | 100 | 200)}><option value="30">30 ms</option><option value="100">100 ms</option><option value="200">200 ms</option></select></label><button className="create-word-cut" disabled={Boolean(busy)} onClick={createWordCut}><Scissors size={12} />创建并试听</button></div>}</section>}
                <div className="version-block"><div className="section-title"><div><p className="eyebrow">可恢复</p><h2>版本</h2></div><div className="history-controls"><button aria-label="撤销" title="Ctrl+Z" disabled={!project.history.canUndo || Boolean(busy)} onClick={() => navigateHistory("undo")}><Undo2 size={14} /></button><button aria-label="重做" title="Ctrl+Shift+Z / Ctrl+Y" disabled={!project.history.canRedo || Boolean(busy)} onClick={() => navigateHistory("redo")}><Redo2 size={14} /></button><History size={16} /></div></div>{project.versions.slice().reverse().slice(0, 4).map((version) => <button className="version-row" key={version.id} onClick={() => restoreVersion(version.id)}><span><strong>{version.reason}</strong><small>{new Date(version.createdAt).toLocaleString("zh-CN")}</small></span><RotateCcw size={14} /></button>)}</div>
              </aside>
            </section>

            <section className="timeline-panel">
              <div className="section-title"><div><p className="eyebrow">证据与节奏</p><h2>字幕时间轴</h2></div><span className="timeline-note">软剪辑建议以虚线显示，原片未改动</span></div>
              {waveformUrl && <img className="waveform" src={waveformUrl} alt="本地音频波形" />}
              <div className="timeline-track">{project.transcript.segments.map((segment) => { const edit = project.edits.find((candidate) => candidate.segmentId === segment.id && ["suggested", "proposed", "applied"].includes(candidate.status)); const association = associationBySegment.get(segment.id); const speaker = association ? speakerById.get(association.speakerId) : undefined; return <div className="timeline-segment-shell" key={segment.id} style={{ flexGrow: Math.max(1, segment.end - segment.start) }}><button className={`timeline-segment ${edit && ["suggested", "proposed"].includes(edit.status) ? "suggested" : ""} ${edit?.status === "applied" ? "applied" : ""} ${selectedSegmentIds.includes(segment.id) ? "selected" : ""} ${selectedId === segment.id ? "active" : ""}`} onClick={() => selectSegment(segment)} title={`${speaker ? `${speaker.label} · ` : ""}${segment.text}`}>{speaker && <i className={`speaker-color speaker-${speaker.colorIndex % 6}`} />}{segment.text}</button>{edit?.status === "applied" && <button className="timeline-restore" onClick={() => void updateCut(edit.id, "restore")}><Scissors size={11} />恢复此处</button>}</div>; })}</div>
            </section>
          </>
        )}
      </section>
      {showExportPanel && project && <aside ref={exportPanelRef} className="export-panel" aria-label="导出设置">
        <header className="export-panel-header"><div><p className="eyebrow">集中管理</p><h2>导出设置</h2></div><IconButton label="关闭导出设置" onClick={() => setShowExportPanel(false)}><X size={17} /></IconButton></header>
        <div className="export-panel-body">
          <section className="export-group" aria-labelledby="export-canvas-heading">
            <div><h3 id="export-canvas-heading">画面</h3><p>画布设置保存在当前项目中。</p></div>
            <label><span>画布比例</span><select aria-label="画布比例" value={project.canvasSettings.aspectRatio} onChange={(event) => void changeCanvas({ ...project.canvasSettings, aspectRatio: event.target.value as CanvasSettings["aspectRatio"] })}><option value="source">原始比例</option><option value="9:16">9:16 竖屏</option></select></label>
            <label><span>竖屏构图</span><select aria-label="竖屏构图" disabled={project.canvasSettings.aspectRatio === "source"} value={project.canvasSettings.framing} onChange={(event) => void changeCanvas({ ...project.canvasSettings, framing: event.target.value as CanvasSettings["framing"] })}><option value="contain-blur">完整画面＋模糊背景</option><option value="cover-center">居中裁切铺满</option></select></label>
          </section>
          <section className="export-group" aria-labelledby="export-subtitle-heading">
            <div><h3 id="export-subtitle-heading">字幕</h3><p>只控制 SiaoCut 生成的字幕，不会隐藏原片已烧录字幕。</p></div>
            <label><span>字幕内容</span><select aria-label="字幕模式" value={subtitleMode} onChange={(event) => setSubtitleMode(event.target.value as typeof subtitleMode)}><option value="source">原文</option><option value="translated">仅译文</option><option value="bilingual">双语</option></select></label>
            <label><span>译文语言</span><select aria-label="译文语言" disabled={!translationLanguageOptions.length} value={selectedSubtitleLanguage} onChange={(event) => setSubtitleLanguage(event.target.value)}>{translationLanguageOptions.length ? translationLanguageOptions.map((language) => <option value={language} key={language}>{language.toUpperCase()}{translationLanguages.includes(language) ? "" : "（等待 Agent）"}</option>) : <option value="">暂无译文</option>}</select></label>
            <label><span>字幕文件格式</span><select aria-label="导出格式" value={exportFormat} onChange={(event) => setExportFormat(event.target.value as typeof exportFormat)}><option value="srt">SRT</option><option value="vtt">VTT</option><option value="ass">ASS</option><option value="markdown">Markdown</option></select></label>
            {selectedTranslationPending && <p className="export-warning"><CircleAlert size={14} />{selectedSubtitleLanguage.toUpperCase()} 译文正在等待 Agent，完成前不能导出该字幕模式。</p>}
          </section>
          <section className="export-group subtitle-style-group" aria-labelledby="export-subtitle-style-heading">
            <div><h3 id="export-subtitle-style-heading">字幕样式</h3><p>样式保存在项目中，并在视频导出任务创建时固化。</p></div>
            <label><span>样式预设</span><select aria-label="字幕样式预设" disabled={Boolean(busy)} value={project.subtitleStyle.preset} onChange={(event) => void changeSubtitleStyle(event.target.value as Project["subtitleStyle"]["preset"], project.subtitleStyle.position)}><option value="compact">紧凑 · 42 px</option><option value="standard">清晰 · 52 px</option><option value="emphasis">强调 · 60 px</option></select></label>
            <label><span>字幕位置</span><select aria-label="字幕位置" disabled={Boolean(busy)} value={project.subtitleStyle.position} onChange={(event) => void changeSubtitleStyle(project.subtitleStyle.preset, event.target.value as Project["subtitleStyle"]["position"])}><option value="bottom">底部安全区</option><option value="center">画面中央</option></select></label>
            <label className="subtitle-safe-toggle"><input type="checkbox" checked={showSubtitleSafeArea} onChange={(event) => setShowSubtitleSafeArea(event.target.checked)} /><span>显示字幕安全区</span></label>
            <div className="subtitle-style-summary"><span><strong>{project.subtitleStyle.fontSize} px</strong><small>主字幕</small></span><span><strong>{project.subtitleStyle.secondaryFontSize} px</strong><small>第二语言</small></span><span><strong>{project.subtitleStyle.outlineWidth} px</strong><small>描边</small></span><span><strong>{project.subtitleStyle.safeMarginPercent}%</strong><small>安全边距</small></span></div>
          </section>
          <section className="export-safety"><ShieldCheck size={17} /><span><strong>原片不会修改</strong><small>视频导出创建新文件，软剪辑和字幕仍可恢复。</small></span></section>
        </div>
        <footer className="export-panel-actions">
          <Button disabled={Boolean(busy) || selectedTranslationPending} onClick={exportTranscript}><Download size={15} />导出字幕</Button>
          <Button variant="primary" disabled={Boolean(busy) || selectedTranslationPending || Boolean(activeExport && ["queued", "running"].includes(activeExport.status))} onClick={exportVideo}><Film size={15} />导出视频</Button>
        </footer>
      </aside>}
      {structureEditMode && project && <Dialog label={structureEditLabels[structureEditMode]} className="runtime-dialog subtitle-structure-dialog" onClose={() => { if (!structureBusy) setStructureEditMode(null); }}>
        <button autoFocus className="dialog-close" aria-label={`关闭${structureEditLabels[structureEditMode]}`} title="关闭 · Esc" disabled={structureBusy} onClick={() => setStructureEditMode(null)}><X size={18} /></button>
        <p className="eyebrow">先核对范围，再创建版本</p><h2>{structureEditLabels[structureEditMode]}</h2>
        <section className="subtitle-operation-scope" aria-label="字幕操作范围">
          <header><ListChecks size={15} /><span><strong>作用范围：{selectedScopeLabel}</strong><small>{selectedSegments.length > 4 ? `显示前 4 段，共 ${selectedSegments.length} 段` : "按时间顺序执行"}</small></span></header>
          <div>{selectedSegments.slice(0, 4).map((segment) => <span key={segment.id}><code>{formatTime(segment.start)}—{formatTime(segment.end)}</code><small>{segment.text}</small></span>)}</div>
        </section>
        {structureEditMode === "split" && selectedSegments[0] && <div className="subtitle-structure-form">
          <label><span>文字拆分位置</span><input type="number" min="1" max={Math.max(1, Array.from(selectedSegments[0].text).length - 1)} step="1" value={structureTextOffset} onChange={(event) => setStructureTextOffset(event.target.value)} /><small>按字符计数，必须位于文字内部</small></label>
          <label><span>时间拆分点</span><input type="number" min={selectedSegments[0].start} max={selectedSegments[0].end} step="0.001" value={structureStart} onChange={(event) => setStructureStart(event.target.value)} /><small>秒；不得穿过词级证据</small></label>
          <div className="subtitle-split-preview" role="region" aria-label="拆分预览"><span><small>左段</small>{Array.from(selectedSegments[0].text).slice(0, Number(structureTextOffset) || 0).join("")}</span><span><small>右段</small>{Array.from(selectedSegments[0].text).slice(Number(structureTextOffset) || 0).join("")}</span></div>
        </div>}
        {structureEditMode === "merge" && <div className="subtitle-merge-preview" aria-label="合并预览"><small>合并后正文</small><p>{selectedSegments.map((segment) => segment.text.trim()).join(" ")}</p></div>}
        {structureEditMode === "timing" && selectedSegments[0] && <div className="subtitle-structure-form timing">
          <label><span>开始时间</span><input type="number" min="0" step="0.001" value={structureStart} onChange={(event) => setStructureStart(event.target.value)} /><small>当前 {selectedSegments[0].start.toFixed(3)} 秒</small></label>
          <label><span>结束时间</span><input type="number" min="0" max={project.media.durationSeconds ?? undefined} step="0.001" value={structureEnd} onChange={(event) => setStructureEnd(event.target.value)} /><small>当前 {selectedSegments[0].end.toFixed(3)} 秒</small></label>
        </div>}
        {structureEditMode === "offset" && <div className="subtitle-structure-form offset">
          <label><span>统一偏移</span><input type="number" step="0.001" value={structureDelta} onChange={(event) => setStructureDelta(event.target.value)} /><small>正数向后，负数向前，单位为秒</small></label>
          <p><MoveHorizontal size={14} />执行后范围：{formatTime(Math.max(0, (selectedSegments[0]?.start ?? 0) + (Number(structureDelta) || 0)))} — {formatTime(Math.max(0, (selectedSegments.at(-1)?.end ?? 0) + (Number(structureDelta) || 0)))}</p>
        </div>}
        <p className="subtitle-structure-impact"><History size={14} />操作会创建可恢复版本；不修改原片或既有导出文件。受影响的词级、剪辑、说话人或译文证据将按 Core 规则失效。</p>
        {structureError && <div className="source-error" role="alert"><CircleAlert size={15} />{structureError}</div>}
        <button className="button primary full" disabled={structureBusy} onClick={() => void applyStructureEdit()}>{structureBusy ? <><LoaderCircle className="spin" size={14} />正在应用</> : `确认${structureEditMode === "offset" ? `偏移 ${selectedSegments.length} 段` : structureEditMode === "merge" ? "合并 2 段" : structureEditMode === "split" ? "拆分当前段" : "更新时间"}`}</button>
      </Dialog>}
      {showSubtitleImport && project && <Dialog label="导入字幕" className="runtime-dialog subtitle-import-dialog" onClose={() => setShowSubtitleImport(false)} returnFocusRef={subtitleImportButtonRef}>
        <button autoFocus className="dialog-close" aria-label="关闭导入字幕" title="关闭导入字幕 · Esc" onClick={() => setShowSubtitleImport(false)}><X size={18} /></button>
        <p className="eyebrow">先预检，再替换</p><h2>导入字幕文件</h2>
        <p className="dialog-copy">支持 UTF-8 编码的 SRT、VTT、ASS 和 SSA。预检只读取本机文件；确认前不会修改项目。</p>
        <div className="subtitle-import-file"><span><small>字幕文件</small><strong title={subtitleImportPath}>{subtitleImportPath ? subtitleImportPath.split(/[\\/]/).at(-1) : "尚未选择"}</strong></span><button className="button quiet" disabled={Boolean(subtitleImportBusy)} onClick={() => void inspectSubtitleFile()}><FolderOpen size={14} />{subtitleImportPath ? "重新选择" : "选择文件"}</button></div>
        {subtitleImportBusy && <div className="subtitle-import-progress" role="status"><LoaderCircle className="spin" size={14} />{subtitleImportBusy}</div>}
        {subtitleImportPreview && <section className={`subtitle-import-preview ${subtitleImportPreview.quality.status}`} aria-label="字幕导入预检">
          <header><span><small>{subtitleImportPreview.format.toUpperCase()} · SHA-256 {subtitleImportPreview.sha256.slice(0, 10)}…</small><strong>{subtitleImportPreview.segmentCount} 段字幕</strong></span>{subtitleImportPreview.quality.status === "good" ? <Check size={18} /> : <CircleAlert size={18} />}</header>
          <div className="subtitle-import-quality"><strong>{subtitleImportPreview.quality.statusLabel}</strong><span>{subtitleImportPreview.quality.errorCount} 项错误 · {subtitleImportPreview.quality.warningCount} 项提醒</span></div>
          {subtitleImportPreview.quality.issues.length > 0 && <div className="subtitle-import-issues">{subtitleImportPreview.quality.issues.slice(0, 5).map((issue) => <div className={issue.severity} key={issue.id}><CircleAlert size={12} /><span><strong>{issue.message}</strong><small>{formatTime(issue.start)} — {formatTime(issue.end)}</small></span></div>)}</div>}
          <label className="subtitle-replace-confirm"><input type="checkbox" checked={subtitleReplaceConfirmed} disabled={!subtitleImportPreview.canImport || Boolean(subtitleImportBusy)} onChange={(event) => setSubtitleReplaceConfirmed(event.target.checked)} /><span>确认用这份文件替换当前字幕。现有词级证据和软剪辑将失效，译文标记为待更新；操作可撤销。</span></label>
          <button className="button primary full" disabled={!subtitleImportPreview.canImport || !subtitleReplaceConfirmed || Boolean(subtitleImportBusy)} onClick={() => void confirmSubtitleImport()}>{subtitleImportPreview.canImport ? "确认替换字幕" : "修复错误后重新预检"}</button>
          <p className="runtime-disclosure">原片和既有导出文件不会修改。预检哈希不一致时，Core 会拒绝写入并要求重新预检。</p>
        </section>}
        {subtitleImportError && <div className="source-error" role="alert"><CircleAlert size={15} />{subtitleImportError}</div>}
      </Dialog>}
      {showAutoWorkflow && <Dialog label="一键工作流" className="runtime-dialog auto-dialog" onClose={() => setShowAutoWorkflow(false)} returnFocusRef={autoButtonRef}><button autoFocus className="dialog-close" aria-label="关闭一键工作流" title="关闭一键工作流 · Esc" onClick={() => setShowAutoWorkflow(false)}><X size={18} /></button><p className="eyebrow">可恢复的本地流水线</p><h2>从素材到可审阅成片</h2><p className="dialog-copy">一次启动导入、转录、粗剪建议、可选翻译、审计与导出。粗剪和 Agent 结果不会自动应用，流程会停下来等待确认。</p>
        <div className="auto-form">
          <label><span>素材来源</span><select aria-label="一键素材来源" value={autoInputKind} disabled={Boolean(autoBusy)} onChange={(event) => { setAutoInputKind(event.target.value as "local" | "url"); setAutoSourcePreview(null); setAutoAuthorized(false); setAutoError(null); }}><option value="local">本地音视频</option><option value="url">公开单视频 URL</option></select></label>
          {autoInputKind === "local" ? <div className="auto-file-row"><span><small>本地素材</small><strong title={autoMediaPath}>{autoMediaPath || "尚未选择"}</strong></span><button className="button quiet" disabled={Boolean(autoBusy)} onClick={() => void chooseAutoMedia()}><FolderOpen size={14} />选择文件</button></div> : <>
            <form className="source-form" onSubmit={(event) => { event.preventDefault(); void inspectAutoSource(); }}><label><span>公开视频 URL</span><input autoComplete="url" aria-label="一键公开视频 URL" placeholder="https://…" value={autoUrl} disabled={Boolean(autoBusy)} onChange={(event) => { setAutoUrl(event.target.value); setAutoSourcePreview(null); setAutoAuthorized(false); setAutoError(null); }} /></label><button className="button quiet" type="submit" disabled={Boolean(autoBusy) || !autoUrl.trim()}><Search size={14} />读取视频信息</button></form>
            {autoSourcePreview && <section className="source-preview auto-source-preview" aria-label="一键待确认视频信息"><header><span><small>{autoSourcePreview.extractor}</small><strong>{autoSourcePreview.title}</strong></span><ShieldCheck size={19} /></header><dl><div><dt>时长</dt><dd>{formatTime(autoSourcePreview.durationSeconds)}</dd></div><div><dt>站点媒体 ID</dt><dd>{autoSourcePreview.siteMediaId}</dd></div></dl><label className="source-consent"><input type="checkbox" checked={autoAuthorized} onChange={(event) => setAutoAuthorized(event.target.checked)} /><span>我确认有权下载并处理此视频，且标题、时长和媒体 ID 符合预期。</span></label></section>}
          </>}
          <div className="auto-file-row"><span><small>本地转录模型</small><strong title={modelPath ?? undefined}>{modelPath ?? "尚未选择"}</strong></span><button className="button quiet" onClick={() => { setShowAutoWorkflow(false); setShowRuntime(true); }}>管理模型</button></div>
          <div className="auto-options">
            <label className="auto-check"><input type="checkbox" checked={autoTranslate} onChange={(event) => { setAutoTranslate(event.target.checked); if (!event.target.checked) setAutoSubtitleMode("source"); }} /><span>创建 Agent 翻译任务</span></label>
            <label><span>目标语言</span><input aria-label="一键翻译语言" value={autoTranslationLanguage} disabled={!autoTranslate} onChange={(event) => setAutoTranslationLanguage(event.target.value)} /></label>
            <label><span>成片字幕</span><select aria-label="一键字幕模式" value={autoSubtitleMode} disabled={!autoTranslate} onChange={(event) => setAutoSubtitleMode(event.target.value as typeof autoSubtitleMode)}><option value="source">原文</option><option value="translated">仅译文</option><option value="bilingual">双语</option></select></label>
            <label className="auto-check"><input type="checkbox" checked={autoBurnSubtitles} onChange={(event) => setAutoBurnSubtitles(event.target.checked)} /><span>将所选字幕烧录到成片</span></label>
          </div>
          <button className="button primary full" disabled={Boolean(autoBusy) || !modelPath || (autoTranslate && !autoTranslationLanguage.trim()) || (autoInputKind === "local" ? !autoMediaPath : !autoSourcePreview || !autoAuthorized)} onClick={() => void startAutoWorkflow()}>{autoBusy ? <LoaderCircle className="spin" size={14} /> : <Sparkles size={14} />}启动一键工作流</button>
          {autoError && <div className="source-error" role="alert"><CircleAlert size={15} />{autoError}</div>}
        </div>
      </Dialog>}
      {showRuntime && <Dialog label="运行环境" className="runtime-dialog" onClose={() => setShowRuntime(false)} returnFocusRef={runtimeButtonRef}><button autoFocus className="dialog-close" aria-label="关闭运行环境" title="关闭运行环境 · Esc" onClick={() => setShowRuntime(false)}><X size={18} /></button><p className="eyebrow">本机组件</p><h2>运行环境</h2><p className="dialog-copy">转录与说话人分析只使用本机运行时。模型只有在明确选择后才会从显示的固定来源下载。</p><RuntimeChecklist runtime={runtime} modelPath={modelPath} onChooseModel={chooseModel} /><AsrBackendPicker runtime={runtime} onSelect={changeAsrBackend} /><ModelManager models={models} selectedPath={modelPath} job={modelJob} onSelect={(path) => { localStorage.setItem("siaocut.modelPath", path); setModelPath(path); }} onInstall={installModel} onCancel={cancelModel} onRemove={removeModel} /><SpeakerPackageManager packageStatus={speakerPackage} job={speakerJob?.kind === "install" ? speakerJob : null} disabled={Boolean(busy)} onInstall={installSpeakerPackage} onCancel={cancelSpeakerJob} onResume={resumeSpeakerJob} /><DiagnosticsPanel runtime={runtime} onOpen={openDiagnostics} /><UpdatePanel policy={updatePolicy} update={availableUpdate} busy={updateBusy} error={updateError} onCheck={() => void checkUpdates()} onInstall={() => void confirmUpdateInstall()} /><button className="button quiet full" onClick={() => void initialize()}><RefreshCw size={14} />重新检查运行环境</button></Dialog>}
      {currentDeleteCandidate && <Dialog label="删除项目" className="confirm-dialog" onClose={closeDeleteDialog}><div className="confirm-icon"><Trash2 size={20} /></div><p className="eyebrow">删除本地项目记录</p><h2>删除「{currentDeleteCandidate.title}」？</h2><p className="dialog-copy">字幕、编辑记录、任务和项目设置将从 SiaoCut 中删除。原始音视频文件不会删除或修改。</p>{(deleteBlockMessage || deleteError) && <div className="confirm-error" role="alert"><CircleAlert size={16} /><span>{deleteBlockMessage ?? deleteError}</span></div>}<div className="confirm-actions"><button className="button quiet" disabled={deleteBusy} onClick={closeDeleteDialog}>取消</button><button className="button danger" disabled={deleteBusy || Boolean(deleteBlockMessage)} onClick={() => void deleteProject()}>{deleteBusy ? <LoaderCircle className="spin" size={14} /> : <Trash2 size={14} />}确认删除</button></div></Dialog>}
      {showSourceImport && <Dialog label="URL 导入" className="runtime-dialog source-dialog" onClose={() => setShowSourceImport(false)} returnFocusRef={sourceButtonRef}><button autoFocus className="dialog-close" aria-label="关闭 URL 导入" title="关闭 URL 导入 · Esc" onClick={() => setShowSourceImport(false)}><X size={18} /></button><p className="eyebrow">受审计的网络导入</p><h2>导入公开单视频</h2><p className="dialog-copy">只接受无登录、无 Cookie 的公开 HTTPS 单视频。最长 2 小时、最大 4 GB；下载会访问显示的 URL，媒体完成校验后才创建本地项目。</p>
        {!sourceJob && <form className="source-form" onSubmit={(event) => { event.preventDefault(); void inspectSource(); }}><label><span>公开视频 URL</span><input autoComplete="url" aria-label="公开视频 URL" placeholder="https://…" value={sourceUrl} disabled={Boolean(sourceBusy)} onChange={(event) => { setSourceUrl(event.target.value); setSourcePreview(null); setSourceAuthorized(false); setSourceError(null); }} /></label><button className="button primary" type="submit" disabled={Boolean(sourceBusy) || !sourceUrl.trim()}>{sourceBusy && !sourcePreview ? <LoaderCircle className="spin" size={14} /> : <Search size={14} />}读取视频信息</button></form>}
        {sourcePreview && !sourceJob && <section className="source-preview" aria-label="待确认视频信息"><header><span><small>{sourcePreview.extractor}</small><strong>{sourcePreview.title}</strong></span><ShieldCheck size={19} /></header><dl><div><dt>时长</dt><dd>{formatTime(sourcePreview.durationSeconds)}</dd></div><div><dt>{sourcePreview.fileSizeKnown ? "文件大小" : "预估大小"}</dt><dd>{formatBytes(sourcePreview.fileSizeBytes)}</dd></div><div><dt>站点媒体 ID</dt><dd>{sourcePreview.siteMediaId}</dd></div><div><dt>下载工具</dt><dd>yt-dlp {sourcePreview.toolVersion}</dd></div></dl><p className="source-url" title={sourcePreview.webpageUrl}>{sourcePreview.webpageUrl}</p><label className="source-consent"><input type="checkbox" checked={sourceAuthorized} onChange={(event) => setSourceAuthorized(event.target.checked)} /><span>我确认有权下载并处理此视频，并确认上方标题、时长和站点媒体 ID 对应预期内容。</span></label><button className="button primary full" disabled={!sourceAuthorized || Boolean(sourceBusy)} onClick={() => void startSourceImport()}>{sourceBusy ? <LoaderCircle className="spin" size={14} /> : <Download size={14} />}确认信息并开始下载</button></section>}
        {sourceJob && <section className="source-job" aria-label="URL 导入任务"><header><span className={`source-state ${sourceJob.status}`}><i />{sourceStatusLabel(sourceJob.status)}</span><strong>{sourceJob.title}</strong><small>第 {sourceJob.attemptCount} 次尝试 · {sourceJob.siteMediaId}</small></header><div className="source-job-progress"><progress value={sourceJob.progress} max={1} /><span>{Math.round(sourceJob.progress * 100)}% · {formatBytes(sourceJob.bytesDownloaded)} / {formatBytes(sourceJob.totalBytes ?? sourceJob.fileSizeBytes)}</span></div><dl><div><dt>工具</dt><dd>yt-dlp {sourceJob.toolVersion}</dd></div><div><dt>项目</dt><dd>{sourceJob.projectId ?? "媒体校验通过后创建"}</dd></div></dl>{sourceJob.errorMessage && <p className="source-job-error">{sourceJob.errorMessage}</p>}<div className="source-job-actions">{["queued", "running"].includes(sourceJob.status) && <button disabled={Boolean(sourceBusy) || Boolean(sourceJob.cancelRequestedAt)} onClick={() => void cancelSourceImport()}>{sourceJob.cancelRequestedAt ? "正在取消" : "取消并保留分片"}</button>}{["cancelled", "failed", "interrupted"].includes(sourceJob.status) && <button className="primary" disabled={Boolean(sourceBusy)} onClick={() => void resumeSourceImport()}><RefreshCw size={13} />显式继续</button>}{!["queued", "running", "finalizing"].includes(sourceJob.status) && <button onClick={resetSourceImport}>导入另一个 URL</button>}</div></section>}
        {sourceError && <div className="source-error" role="alert"><CircleAlert size={15} />{sourceError}</div>}
        <p className="runtime-disclosure">固定工具不读取浏览器 Cookie、用户配置或插件，也不能自行更新。原 URL、站点媒体 ID、工具版本和本地文件哈希会随任务保存。</p>
      </Dialog>}
    </main>
  );
}

export function SpeechInsightsPanel({ insights, onLocateEvidence, onLocatePause }: { insights: SpeechInsights; onLocateEvidence: (evidence: SpeechEvidence) => void; onLocatePause: (pause: SpeechPause) => void }) {
  return <section className="speech-insights" aria-label="语音节奏">
    <header><div><p className="eyebrow">本地语音智能</p><h3><Activity size={15} />语音节奏</h3></div><small>{insights.analyzerVersion}</small></header>
    {insights.status === "insufficient_evidence" ? <p className="speech-empty">缺少词级时间。完成本地转录后显示语速、停顿和低置信度证据。</p> : <>
      <div className="speech-metrics">
        <span><strong>{insights.tokensPerMinute}</strong><small>词条/分钟</small></span>
        <span><strong>{insights.pauseCount}</strong><small>停顿</small></span>
        <span><strong>{insights.fillerCount}</strong><small>口头语</small></span>
        <span><strong>{insights.lowConfidenceCount}</strong><small>低置信度</small></span>
      </div>
      {(insights.pauses.length > 0 || insights.evidence.length > 0) && <div className="speech-findings">
        {insights.pauses.slice(0, 2).map((pause) => <button key={`${pause.previousWordId}-${pause.nextWordId}`} onClick={() => onLocatePause(pause)} aria-label={`定位${pause.severity === "long_pause" ? "长停顿" : "停顿"} ${formatTime(pause.start)} 至 ${formatTime(pause.end)}`}><Clock3 size={12} /><span><strong>{pause.severity === "long_pause" ? "长停顿" : "停顿"}</strong><small>{pause.duration.toFixed(1)} 秒 · {formatTime(pause.start)}</small></span></button>)}
        {insights.evidence.slice(0, 3).map((evidence, index) => <button key={`${evidence.kind}-${evidence.wordId}-${index}`} onClick={() => onLocateEvidence(evidence)} aria-label={`定位${evidence.kind === "filler" ? "口头语" : "低置信度"} ${evidence.text} ${formatTime(evidence.start)}`}><CircleAlert size={12} /><span><strong>{evidence.kind === "filler" ? "口头语" : "低置信度"} · {evidence.text}</strong><small>{formatTime(evidence.start)}{evidence.confidence == null ? "" : ` · ${Math.round(evidence.confidence * 100)}%`}</small></span></button>)}
      </div>}
      <p className="speech-disclosure">根据当前词级时间实时计算，只提供定位证据，不会自动剪辑。</p>
    </>}
  </section>;
}

export function AudioQualityPanel({ job, onStart, onCancel, onResume, onLocate, disabled }: { job: AudioAnalysisJob | null; onStart: () => void; onCancel: () => void; onResume: () => void; onLocate: (risk: AudioRisk) => void; disabled: boolean }) {
  const active = job && ["queued", "running"].includes(job.status);
  const resumable = job && ["cancelled", "failed", "interrupted"].includes(job.status);
  return <section className="audio-quality" aria-label="音频质量">
    <header><div><p className="eyebrow">本机 FFmpeg</p><h3><Headphones size={15} />音频质量</h3></div>{job?.report && <small>{job.report.analyzerVersion}</small>}</header>
    {!job && <><p className="speech-empty">检查综合响度、峰值、静音区间和疑似削波。</p><button className="audio-analysis-action" disabled={disabled} onClick={onStart}>开始本地分析</button></>}
    {active && <div className="audio-analysis-progress"><span><LoaderCircle className="spin" size={13} />正在本机分析 · {Math.round(job.progress * 100)}%</span><progress max={1} value={job.progress} /><button disabled={disabled || Boolean(job.cancelRequestedAt)} onClick={onCancel}>{job.cancelRequestedAt ? "正在取消" : "取消"}</button></div>}
    {resumable && <div className="audio-analysis-error"><p>{job.errorMessage ?? (job.status === "cancelled" ? "分析已取消。" : "分析未完成。")}</p><button disabled={disabled} onClick={onResume}><RefreshCw size={12} />显式继续</button></div>}
    {job?.status === "completed" && job.report && <>
      <div className="speech-metrics">
        <span><strong>{job.report.integratedLoudnessLufs ?? "—"}</strong><small>综合响度 LUFS</small></span>
        <span><strong>{job.report.truePeakDbfs ?? "—"}</strong><small>真峰值 dBFS</small></span>
        <span><strong>{job.report.risks.length}</strong><small>待确认风险</small></span>
        <span><strong>{job.report.silenceDurationSeconds.toFixed(1)}</strong><small>静音秒数</small></span>
      </div>
      {job.report.risks.length > 0 ? <div className="speech-findings">{job.report.risks.slice(0, 3).map((risk, index) => <button key={`${risk.kind}-${risk.start}-${index}`} onClick={() => onLocate(risk)}><CircleAlert size={12} /><span><strong>{audioRiskLabel(risk.kind)}</strong><small>{formatTime(risk.start)} · {risk.measuredValue} {audioUnitLabel(risk.unit)} / 阈值 {risk.threshold}</small></span></button>)}</div> : <p className="audio-quality-ok"><Check size={13} />未发现超过固定阈值的音频风险</p>}
      <p className="speech-disclosure" title={job.report.toolVersion}>固定阈值与工具版本随证据保存；结果不会自动修改媒体。</p>
      <button className="audio-analysis-action quiet" disabled={disabled} onClick={onStart}>重新分析</button>
    </>}
  </section>;
}

export function SpeakerTrackPanel({ packageStatus, track, job, selectedSegmentId, disabled, onOpenRuntime, onAnalyze, onCancel, onResume, onRename, onMerge, onAssign }: {
  packageStatus: SpeakerPackageStatus | null;
  track: SpeakerTrack | null;
  job: SpeakerJob | null;
  selectedSegmentId: string | null;
  disabled: boolean;
  onOpenRuntime: () => void;
  onAnalyze: () => void;
  onCancel: () => void;
  onResume: () => void;
  onRename: (speakerId: string, name: string) => void;
  onMerge: (fromId: string, intoId: string) => void;
  onAssign: (segmentId: string, speakerId: string) => void;
}) {
  const active = job && ["queued", "running"].includes(job.status);
  const resumable = job && ["cancelled", "failed", "interrupted"].includes(job.status);
  const association = track?.associations.find((item) => item.segmentId === selectedSegmentId);
  return <section className="speaker-track-panel" aria-label="说话人轨">
    <header><div><p className="eyebrow">本机说话人分离</p><h3><Users size={15} />说话人轨</h3></div>{track?.status === "ready" && <small>{track.speakers.length} 人 · {track.turns.length} 段</small>}</header>
    {!packageStatus?.installed || packageStatus.verified !== true ? <>
      <p className="speech-empty">可选模型尚未安装。项目、字幕编辑和导出仍可正常使用。</p>
      <button className="audio-analysis-action quiet" disabled={disabled} onClick={onOpenRuntime}>查看模型来源与安装</button>
    </> : null}
    {packageStatus?.installed && packageStatus.verified === true && !active && !resumable && (!track || track.status === "not_analyzed") && <>
      <p className="speech-empty">生成说话区间并按最大时间重叠关联字幕；结果需要人工审阅。</p>
      <button className="audio-analysis-action" disabled={disabled} onClick={onAnalyze}>开始本地分析</button>
    </>}
    {active && <div className="audio-analysis-progress"><span><LoaderCircle className="spin" size={13} />{job.stage} · {Math.round(job.progress * 100)}%</span><progress max={1} value={job.progress} /><button disabled={disabled} onClick={onCancel}>取消</button></div>}
    {resumable && <div className="audio-analysis-error"><p>{job.errorMessage ?? "说话人任务未完成。"}</p><button disabled={disabled} onClick={onResume}><RefreshCw size={12} />显式继续</button></div>}
    {track?.status === "no_speech" && <><p className="speech-empty">本次没有生成可靠说话区间，字幕未被修改。</p><button className="audio-analysis-action quiet" disabled={disabled} onClick={onAnalyze}>重新分析</button></>}
    {track?.status === "ready" && <>
      {selectedSegmentId && <label className="speaker-assignment"><span>当前字幕说话人</span><select aria-label="当前字幕说话人" value={association?.speakerId ?? ""} disabled={disabled} onChange={(event) => event.target.value && onAssign(selectedSegmentId, event.target.value)}><option value="">未关联</option>{track.speakers.map((speaker) => <option value={speaker.id} key={speaker.id}>{speaker.label}</option>)}</select>{association?.source === "manual" && <small>人工指定 · 可撤销</small>}</label>}
      <div className="speaker-identities">{track.speakers.map((speaker) => <SpeakerIdentityRow key={speaker.id} speaker={speaker} allSpeakers={track.speakers} disabled={disabled} onRename={onRename} onMerge={onMerge} />)}</div>
      <p className="speech-disclosure">{track.runtimeVersion} · 只保存说话区间和字幕关联，不改写文本或应用剪辑。</p>
      <button className="audio-analysis-action quiet" disabled={disabled} onClick={onAnalyze}>重新分析</button>
    </>}
  </section>;
}

function SpeakerIdentityRow({ speaker, allSpeakers, disabled, onRename, onMerge }: { speaker: SpeakerIdentity; allSpeakers: SpeakerIdentity[]; disabled: boolean; onRename: (speakerId: string, name: string) => void; onMerge: (fromId: string, intoId: string) => void }) {
  const [name, setName] = useState(speaker.label);
  const [mergeTarget, setMergeTarget] = useState("");
  useEffect(() => setName(speaker.label), [speaker.label]);
  const save = () => {
    const next = name.trim();
    if (next && next !== speaker.label) onRename(speaker.id, next);
    else setName(speaker.label);
  };
  return <div className="speaker-identity-row">
    <i className={`speaker-color speaker-${speaker.colorIndex % 6}`} />
    <input aria-label={`${speaker.label}名称`} value={name} maxLength={40} disabled={disabled} onChange={(event) => setName(event.target.value)} onBlur={save} onKeyDown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") { event.preventDefault(); save(); } }} title="Ctrl+Enter 保存名称" />
    {allSpeakers.length > 1 && <><select aria-label={`合并${speaker.label}到`} value={mergeTarget} disabled={disabled} onChange={(event) => setMergeTarget(event.target.value)}><option value="">合并到…</option>{allSpeakers.filter((item) => item.id !== speaker.id).map((item) => <option value={item.id} key={item.id}>{item.label}</option>)}</select><button disabled={disabled || !mergeTarget} onClick={() => mergeTarget && onMerge(speaker.id, mergeTarget)}>合并</button></>}
  </div>;
}

export function PatchReviewCard({ item, onReview, onSelect }: { item: Project["patchSets"][number]["items"][number]; onReview: (action: "apply" | "keep") => void; onSelect: () => void }) {
  const conflict = item.status === "conflict";
  return <article className={`review-item patch-review ${conflict ? "conflict" : ""}`}>
    <span className={`review-tag ${conflict ? "conflict" : ""}`}>{conflict && <CircleAlert size={12} />}{conflict ? "状态冲突 · 当前文本已变化" : "需要人工确认"}</span>
    <strong>{item.reason}</strong>
    <div className="patch-diff">
      <p><small>任务原文</small>{item.beforeText || "（空）"}</p>
      {conflict && <p className="current"><small>当前文本</small>{item.currentText || "（空）"}</p>}
      <p className="proposed"><small>Agent 建议</small>{item.target === "cut" && !item.afterText ? "删除完整片段" : item.afterText}</p>
    </div>
    <p className="patch-meta">{item.confidence == null ? "未提供置信度" : `置信度 ${Math.round(item.confidence * 100)}%`}</p>
    <div className="patch-actions"><button onClick={onSelect}>定位原文</button><span /><button onClick={() => onReview("keep")}>保留原文</button><button className="apply" onClick={() => onReview("apply")}>应用建议</button></div>
  </article>;
}

function RuntimeChecklist({ runtime, modelPath, onChooseModel, compact = false }: { runtime: RuntimeInfo | null; modelPath: string | null; onChooseModel: () => void; compact?: boolean }) {
  const items = [
    { icon: Database, label: "Core", ok: Boolean(runtime), detail: runtime ? `API ${runtime.coreApiVersion}` : "未连接" },
    { icon: HardDrive, label: "FFmpeg", ok: runtime?.ffmpegConfigured ?? false, detail: runtime?.ffmpegConfigured ? "已配置" : "未检测到" },
    { icon: Cpu, label: "whisper.cpp", ok: runtime?.asrConfigured ?? false, detail: runtime?.asrConfigured ? `${runtime.asrBackend.toUpperCase()}${runtime.asrDevice ? ` · ${runtime.asrDevice}` : ""}${runtime.vadConfigured ? " · VAD" : ""}` : "未检测到" },
    { icon: Download, label: "URL 导入", ok: runtime?.ytDlpConfigured ?? false, detail: runtime?.ytDlpConfigured ? "yt-dlp 2026.06.09" : "未检测到" },
  ];
  const modelName = modelPath ? modelPath.split(/[\\/]/).pop() : "尚未选择";
  if (compact) return <div className="runtime-checklist compact" aria-label="本机运行组件">
    <div className="runtime-components">
      {items.map(({ icon: Icon, label, ok, detail }) => <div className="runtime-row" key={label}>
        <span className="runtime-component-icon"><Icon size={16} /></span>
        <span><strong>{label}</strong><small>{detail}</small></span>
        <i className={ok ? "ok" : "missing"} aria-label={`${label}${ok ? "可用" : "不可用"}`}>{ok ? <Check size={13} /> : <CircleAlert size={13} />}</i>
      </div>)}
    </div>
    <div className="runtime-model-row">
      <span className="runtime-component-icon"><FileVideo2 size={16} /></span>
      <span><strong>转录模型</strong><small>{modelName}</small></span>
      <button onClick={onChooseModel}>{modelPath ? "更换模型" : "选择模型"}</button>
    </div>
  </div>;
  return <div className="runtime-checklist">{items.map(({ icon: Icon, label, ok, detail }) => <div className="runtime-row" key={label}><Icon size={16} /><span><strong>{label}</strong><small>{detail}</small></span><i className={ok ? "ok" : "missing"}>{ok ? <Check size={13} /> : <CircleAlert size={13} />}</i></div>)}<div className="runtime-row model"><FileVideo2 size={16} /><span><strong>转录模型</strong><small title={modelPath ?? ""}>{modelName}</small></span><button onClick={onChooseModel}>选择模型</button></div></div>;
}

function UpdatePanel({ policy, update, busy, error, onCheck, onInstall }: {
  policy: UpdatePolicy | null;
  update: UpdateMetadata | null;
  busy: string | null;
  error: string | null;
  onCheck: () => void;
  onInstall: () => void;
}) {
  return <section className="update-panel" aria-label="应用更新">
    <header><span><strong>应用更新</strong><small>当前版本 {policy?.currentVersion ?? "读取中"} · 每 24 小时检查</small></span><ShieldCheck size={16} /></header>
    {!policy?.enabled && <p>{policy?.disabledReason ?? "正在读取更新策略。"}</p>}
    {update && <div className="update-release">
      <strong>SiaoCut {update.version}</strong>
      <small>{formatBytes(update.sizeBytes)}{update.publishedAt ? ` · ${new Date(update.publishedAt).toLocaleDateString("zh-CN")}` : ""}</small>
      <p>{update.notes || "此版本未提供变更说明。"}</p>
      <button className="button primary full" disabled={Boolean(busy)} onClick={onInstall}>{busy ? <LoaderCircle className="spin" size={14} /> : <Download size={14} />}{busy ?? "确认并安装"}</button>
      <em>确认后会下载并校验 Tauri 签名、SHA-256 和 Authenticode；Windows 安装时将关闭应用，不会静默重启。</em>
    </div>}
    {error && <p className="update-error" role="alert">{error}</p>}
    <button className="button quiet full" disabled={!policy?.enabled || Boolean(busy)} onClick={onCheck}>{busy && !update ? <LoaderCircle className="spin" size={14} /> : <RefreshCw size={14} />}{busy && !update ? busy : "手动检查更新"}</button>
  </section>;
}

function AsrBackendPicker({ runtime, onSelect }: { runtime: RuntimeInfo | null; onSelect: (backend: "cpu" | "vulkan") => void }) {
  if (!runtime || !runtime.availableAsrBackends.includes("vulkan")) return null;
  return <section className="backend-picker"><span><strong>转录加速</strong><small>Vulkan 已在真机验证；不兼容时可随时返回 CPU。</small></span><div>{(["cpu", "vulkan"] as const).map((backend) => <button key={backend} className={runtime.asrBackend === backend ? "active" : ""} onClick={() => onSelect(backend)} aria-pressed={runtime.asrBackend === backend}>{backend.toUpperCase()}</button>)}</div></section>;
}

function DiagnosticsPanel({ runtime, onOpen }: { runtime: RuntimeInfo | null; onOpen: () => void }) {
  const available = runtime?.diagnosticsAvailable ?? false;
  return <section className="diagnostics-panel"><span><strong>诊断日志</strong><small title={runtime?.logDirectory ?? undefined}>{available ? "仅保存在本机；单文件 2 MiB，保留 3 份历史。" : "诊断日志不可用；错误仍会显示在当前窗口。"}</small></span><button disabled={!available} onClick={onOpen}><FolderOpen size={14} />打开日志目录</button></section>;
}

function ModelManager({ models, selectedPath, job, onSelect, onInstall, onCancel, onRemove }: { models: ModelStatus[]; selectedPath: string | null; job: ModelDownloadJob | null; onSelect: (path: string) => void; onInstall: (modelId: string) => void; onCancel: () => void; onRemove: (modelId: string) => void }) {
  const formatSize = (bytes: number) => `${Math.round(bytes / 1024 / 1024)} MB`;
  return <section className="model-manager">
    <div className="model-heading"><span><strong>按需模型</strong><small>下载前显示来源、体积与许可证</small></span><ShieldCheck size={17} /></div>
    <div className="model-options">{models.map((model) => {
      const currentJob = job?.modelId === model.id ? job : null;
      const downloading = currentJob && ["queued", "running"].includes(currentJob.status);
      const selected = model.installed && model.path === selectedPath;
      return <article className={`model-option ${selected ? "selected" : ""}`} key={model.id}>
        <header><span><strong>{model.name}{model.recommended ? " · 推荐" : ""}</strong><small>{formatSize(model.size)} · {model.license}</small></span>{selected && <i><Check size={12} />使用中</i>}</header>
        <p>{model.description}</p>
        <small className="model-source" title={model.source}>来源：ggerganov/whisper.cpp</small>
        {downloading && <div className="model-progress"><span style={{ width: `${Math.max(2, currentJob.progress * 100)}%` }} /><small>{Math.round(currentJob.progress * 100)}% · {formatSize(currentJob.bytesDownloaded)} / {formatSize(currentJob.totalBytes)}</small></div>}
        <div className="model-actions">
          {downloading ? <button onClick={onCancel}>暂停下载</button> : model.installed ? <><button className="primary" onClick={() => onSelect(model.path)}>{selected ? "正在使用" : "使用此模型"}</button><button onClick={() => onRemove(model.id)}>移除</button></> : <button className="primary" onClick={() => onInstall(model.id)}><Download size={13} />下载</button>}
        </div>
      </article>;
    })}</div>
    <p className="runtime-disclosure">下载只访问上方显示的模型来源；完成后会校验 SHA-256。媒体不会上传。</p>
  </section>;
}

export function SpeakerPackageManager({ packageStatus, job, disabled, onInstall, onCancel, onResume }: { packageStatus: SpeakerPackageStatus | null; job: SpeakerJob | null; disabled: boolean; onInstall: () => void; onCancel: () => void; onResume: () => void }) {
  const active = job && ["queued", "running"].includes(job.status);
  const resumable = job && ["cancelled", "failed", "interrupted"].includes(job.status);
  return <section className="speaker-package-manager" aria-label="说话人模型包">
    <div className="model-heading"><span><strong>说话人分离 · 可选</strong><small>{packageStatus?.runtimeVersion ?? "正在读取"} · CPU 本地运行</small></span><Users size={17} /></div>
    <p>{packageStatus?.description ?? "读取固定模型包信息中。"}</p>
    {packageStatus && <div className="speaker-package-summary"><span><strong>{formatBytes(packageStatus.downloadSize)}</strong><small>下载体积</small></span><span><strong>{packageStatus.license}</strong><small>组合许可证</small></span><span className={packageStatus.verified === true ? "verified" : "optional"}>{packageStatus.verified === true ? <Check size={13} /> : <ShieldCheck size={13} />}{packageStatus.verified === true ? "已校验" : "未安装"}</span></div>}
    {packageStatus && <details><summary>查看 {packageStatus.assets.length} 个组件、来源与 SHA-256</summary><div className="speaker-asset-list">{packageStatus.assets.map((asset) => <article key={asset.id}><span><strong>{asset.name}</strong><small>{formatBytes(asset.size)} · {asset.license}</small></span><small title={asset.source}>{asset.source.replace(/^https?:\/\//, "")}</small><code title={asset.sha256}>SHA-256 {asset.sha256.slice(0, 12)}…</code></article>)}</div></details>}
    {active && <div className="model-progress"><span style={{ width: `${Math.max(2, job.progress * 100)}%` }} /><small>{job.stage} · {Math.round(job.progress * 100)}% · {formatBytes(job.bytesDownloaded)} / {formatBytes(job.totalBytes)}</small></div>}
    {job?.errorMessage && <p className="speaker-package-error" role="alert">{job.errorMessage}</p>}
    <div className="model-actions">{active ? <button disabled={disabled} onClick={onCancel}>取消并保留分片</button> : resumable ? <button disabled={disabled} onClick={onResume}><RefreshCw size={13} />显式继续</button> : packageStatus?.installed && packageStatus.verified === true ? <span className="speaker-package-ready"><Check size={13} />可以开始本地说话人分析</span> : <button className="primary" disabled={disabled || !packageStatus} onClick={onInstall}><Download size={13} />明确安装</button>}</div>
    <p className="runtime-disclosure">不随应用默认安装；只有点击「明确安装」才访问上方固定来源。媒体不会上传。</p>
  </section>;
}

function SegmentRow({ segment, speaker, speakerManual, selected, active, translation, onSelect, onSave }: { segment: Segment; speaker?: SpeakerIdentity; speakerManual?: boolean; selected: boolean; active: boolean; translation?: Project["translations"][string]; onSelect: (mode: SegmentSelectionMode) => void; onSave: (text: string) => void }) {
  const [draft, setDraft] = useState(segment.text);
  useEffect(() => setDraft(segment.text), [segment.text]);
  const translated = translation?.segments.find((item) => item.segmentId === segment.id)?.text;
  return <article className={`segment-row ${selected ? "selected" : ""} ${active ? "active" : ""}`} aria-label={`字幕段 ${formatTime(segment.start)} 至 ${formatTime(segment.end)}`} onClick={(event) => onSelect(event.shiftKey ? "range" : event.ctrlKey || event.metaKey ? "toggle" : "replace")}>
    <input className="segment-select" type="checkbox" aria-label={`选择字幕 ${formatTime(segment.start)} 至 ${formatTime(segment.end)}`} checked={selected} onClick={(event) => { event.stopPropagation(); onSelect(event.shiftKey ? "range" : "toggle"); }} onChange={() => undefined} />
    <button className="segment-time" aria-label={`定位到 ${formatTime(segment.start)}`}>{formatTime(segment.start)}{speaker && <small><i className={`speaker-color speaker-${speaker.colorIndex % 6}`} />{speaker.label}{speakerManual ? " · 人工" : ""}</small>}</button>
    <div><textarea value={draft} onChange={(event) => setDraft(event.target.value)} onFocus={() => { if (!active) onSelect("replace"); }} onBlur={() => onSave(draft)} onKeyDown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") { event.preventDefault(); onSave(draft); } }} onClick={(event) => event.stopPropagation()} aria-label={`${formatTime(segment.start)} 字幕文本`} title="Ctrl+Enter 保存当前字幕" /><p className={translation?.status === "stale" ? "translation stale" : "translation"}>{translated ?? ""}</p></div>
    <span className={segment.confidence != null && segment.confidence < 0.8 ? "confidence low" : "confidence"}>{segment.confidence == null ? "—" : `${Math.round(segment.confidence * 100)}%`}</span>
  </article>;
}

export default App;
