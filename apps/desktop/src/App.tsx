import { changeUiLocale, getUiLocale, tr, type UiLocale } from "./i18n";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type SyntheticEvent } from "react";
import { Activity, Bot, Check, ChevronDown, ChevronRight, ChevronUp, CircleAlert, Clock3, Cpu, Database, Download, FileVideo2, FileText, Film, FolderOpen, FolderPlus, HardDrive, History, Link2, LoaderCircle, Play, RefreshCw, RotateCcw, Search, Scissors, Settings2, ShieldCheck, Sparkles, Trash2, Undo2, Redo2, Headphones, ListChecks, MoreHorizontal, MoveHorizontal, Users, X, } from "lucide-react";
import { authorizeArtifact, authorizeMedia, checkForUpdate, installUpdate, listProjects, loadProject, openLogDirectory, pickMedia, pickModel, pickSubtitleFile, pickTranscriptPath, pickVideoPath, runCore, runtimeInfo, selectAsrBackend, updaterPolicy } from "./core";
import type { AudioAnalysisJob, AudioRisk, AutoWorkflow, CanvasSettings, CutPreview, ExportJob, ModelDownloadJob, ModelStatus, Project, RuntimeInfo, Segment, SourceImportJob, SourcePreview, SpeakerIdentity, SpeakerJob, SpeakerPackageStatus, SpeakerTrack, SpeechEvidence, SpeechInsights, SpeechPause, SubtitleImportPreview, SubtitleQualityIssue, TranscriptionLanguage, UpdateMetadata, UpdatePolicy } from "./types";
import { Button, Dialog, IconButton, StatusBadge } from "./components/ui";
import { JobFailureDetails } from "./components/job-failure";
import { AsrBackendPicker, AudioQualityPanel, DiagnosticsPanel, ModelManager, PatchReviewCard, RuntimeChecklist, SegmentRow, SpeakerPackageManager, SpeakerTrackPanel, SpeechInsightsPanel, UpdatePanel } from "./components/workbench-panels";
export { AudioQualityPanel, PatchReviewCard, SpeakerPackageManager, SpeakerTrackPanel, SpeechInsightsPanel } from "./components/workbench-panels";
import { audioRiskLabel, audioUnitLabel, autoStageLabel, autoStatusLabel, clearTransientCoreError, cutSuggestionLabel, DEFAULT_EXPORT_PREFERENCES, editReasonLabel, formatBytes, formatTime, getProjectCapabilities, hasMeaningfulSubtitleText, isHttpsSourceUrl, modelDescription, modelName, parseExportPreferences, parseTranscriptionLanguage, patchReasonLabel, segmentCountLabel, shouldCheckForUpdates, sourceStatusLabel, startSerialPolling, structureEditLabel, subtitleCountLabel, subtitleIssueLabel, subtitleQualityStatusLabel, taskLabel, TRANSCRIPTION_LANGUAGE_STORAGE_KEY, versionReasonLabel, wordCountLabel, type ExportPreferencesV1, type SegmentSelectionMode, type StructureEditMode } from "./app-view-model";
export * from "./app-view-model";

export async function resolveCanvasMedia(
    projectId: string,
    authorizePreview: typeof authorizeArtifact = authorizeArtifact,
    authorizeSource: typeof authorizeMedia = authorizeMedia,
) {
    let warning: string | null = null;
    try {
        const preview = await authorizePreview(projectId, "preview");
        if (preview)
            return { mediaUrl: preview, warning: null };
    }
    catch (cause) {
        warning = cause instanceof Error ? cause.message : String(cause);
    }
    try {
        return { mediaUrl: await authorizeSource(projectId), warning };
    }
    catch (cause) {
        const sourceWarning = cause instanceof Error ? cause.message : String(cause);
        return { mediaUrl: null, warning: warning ? `${warning}; ${sourceWarning}` : sourceWarning };
    }
}

export function resolvePlaybackDuration(mediaDuration: number, fallbackDuration: number | null | undefined) {
    return Number.isFinite(mediaDuration) && mediaDuration > 0 ? mediaDuration : fallbackDuration ?? 0;
}

function App() {
    const [uiLocale, setUiLocale] = useState<UiLocale>(() => getUiLocale());
    const selectUiLocale = (locale: UiLocale) => {
        changeUiLocale(locale);
        setUiLocale(locale);
        setNotice(null);
        setError(null);
    };
    const selectTranscriptionLanguage = (language: TranscriptionLanguage) => {
        localStorage.setItem(TRANSCRIPTION_LANGUAGE_STORAGE_KEY, language);
        setTranscriptionLanguage(language);
    };
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
    const [busy, setBusy] = useState<string | null>(tr("app.s0038"));
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
    const [transcriptionLanguage, setTranscriptionLanguage] = useState<TranscriptionLanguage>(() => parseTranscriptionLanguage(localStorage.getItem(TRANSCRIPTION_LANGUAGE_STORAGE_KEY)));
    const [agentWorkflowKind, setAgentWorkflowKind] = useState<"polish" | "proofread" | "edit" | "translate">("polish");
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
    const [emptyReplacementConfirmed, setEmptyReplacementConfirmed] = useState(false);
    const [inspectorTab, setInspectorTab] = useState<"segment" | "analysis" | "history">("segment");
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
    const [wordRange, setWordRange] = useState<{
        segmentId: string;
        start: number;
        end: number;
    } | null>(null);
    const [cutPadding, setCutPadding] = useState<30 | 100 | 200>(100);
    const [cutPreview, setCutPreview] = useState<CutPreview | null>(null);
    const [playback, setPlayback] = useState({ playing: false, currentTime: 0, duration: 0 });
    const [deleteCandidate, setDeleteCandidate] = useState<Project | null>(null);
    const [deleteBusy, setDeleteBusy] = useState(false);
    const [deleteError, setDeleteError] = useState<string | null>(null);
    const videoRef = useRef<HTMLVideoElement>(null);
    const runtimeButtonRef = useRef<HTMLButtonElement>(null);
    const sourceButtonRef = useRef<HTMLButtonElement>(null);
    const autoButtonRef = useRef<HTMLButtonElement>(null);
    const exportButtonRef = useRef<HTMLButtonElement>(null);
    const exportPanelRef = useRef<HTMLElement>(null);
    const commandMoreRef = useRef<HTMLDivElement>(null);
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
            setPlayback({ playing: false, currentTime: 0, duration: next.media.durationSeconds ?? 0 });
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
        setBusy(tr("app.s0039"));
        setError(null);
        const [projectsResult, runtimeResult, modelsResult, modelJobsResult, sourceJobsResult, autoWorkflowsResult, updatePolicyResult, speakerPackageResult, speakerJobsResult] = await Promise.allSettled([
            listProjects(),
            runtimeInfo(),
            runCore(["model", "list", "--verify"]),
            runCore(["model", "jobs"]),
            runCore(["source", "jobs"]),
            runCore(["auto", "list"]),
            updaterPolicy(),
            runCore(["speaker", "package", "--verify"]),
            runCore(["speaker", "jobs"]),
        ]);
        const errors: string[] = [];
        let activeAutoWorkflow: AutoWorkflow | null = null;
        if (updatePolicyResult.status === "fulfilled")
            setUpdatePolicy(updatePolicyResult.value);
        if (autoWorkflowsResult.status === "fulfilled") {
            const workflows = autoWorkflowsResult.value.workflows ?? [];
            activeAutoWorkflow = workflows.find((item) => ["queued", "running", "needs_agent", "needs_review", "failed", "interrupted"].includes(item.status)) ?? null;
            setAutoWorkflow(activeAutoWorkflow);
        }
        else {
            errors.push(tr("app.s0040", { "0": autoWorkflowsResult.reason instanceof Error ? autoWorkflowsResult.reason.message : String(autoWorkflowsResult.reason) }));
        }
        let managedModelPath: string | null = null;
        if (modelsResult.status === "fulfilled") {
            const available = modelsResult.value.models ?? [];
            setModels(available);
            managedModelPath = available.find((item) => item.installed && item.recommended)?.path
                ?? available.find((item) => item.installed)?.path
                ?? null;
        }
        else {
            errors.push(tr("app.s0041", { "0": modelsResult.reason instanceof Error ? modelsResult.reason.message : String(modelsResult.reason) }));
        }
        if (modelJobsResult.status === "fulfilled") {
            setModelJob(modelJobsResult.value.modelJobs?.find((item) => ["queued", "running"].includes(item.status)) ?? null);
        }
        if (speakerPackageResult.status === "fulfilled") {
            setSpeakerPackage(speakerPackageResult.value.speakerPackage ?? null);
        }
        else {
            errors.push(tr("app.s0042", { "0": speakerPackageResult.reason instanceof Error ? speakerPackageResult.reason.message : String(speakerPackageResult.reason) }));
        }
        if (speakerJobsResult.status === "fulfilled") {
            const jobs = speakerJobsResult.value.speakerJobs ?? [];
            setSpeakerJob(jobs.find((item) => ["queued", "running"].includes(item.status)) ?? jobs[0] ?? null);
        }
        if (sourceJobsResult.status === "fulfilled") {
            const jobs = (sourceJobsResult.value.sourceJobs ?? []).filter((item) => item.id !== activeAutoWorkflow?.sourceImportId);
            setSourceJob(jobs.find((item) => ["queued", "running", "finalizing"].includes(item.status)) ?? jobs[0] ?? null);
        }
        else {
            errors.push(tr("app.s0043", { "0": sourceJobsResult.reason instanceof Error ? sourceJobsResult.reason.message : String(sourceJobsResult.reason) }));
        }
        if (runtimeResult.status === "fulfilled") {
            setRuntime(runtimeResult.value);
            setModelPath((current) => {
                const next = current ?? managedModelPath ?? (runtimeResult.value.defaultModelAvailable ? runtimeResult.value.defaultModelPath : null);
                if (next)
                    localStorage.setItem("siaocut.modelPath", next);
                return next;
            });
        }
        else {
            errors.push(tr("app.s0044", { "0": runtimeResult.reason instanceof Error ? runtimeResult.reason.message : String(runtimeResult.reason) }));
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
                catch (cause) {
                    errors.push(tr("app.s0045", { "0": cause instanceof Error ? cause.message : String(cause) }));
                }
            }
        }
        else {
            errors.push(tr("app.s0046", { "0": projectsResult.reason instanceof Error ? projectsResult.reason.message : String(projectsResult.reason) }));
        }
        setError(errors.length ? errors.join(" ") : null);
        setBusy(null);
    }, [refreshLatestAudioAnalysis, refreshLatestExport, refreshSpeakerTrack]);
    useEffect(() => {
        void initialize();
    }, [initialize]);
    const checkUpdates = useCallback(async (automatic = false) => {
        if (!updatePolicy?.enabled)
            return;
        setUpdateBusy(tr("app.s0047"));
        setUpdateError(null);
        try {
            const candidate = await checkForUpdate();
            localStorage.setItem("siaocut.updateLastCheckedAt", new Date().toISOString());
            setAvailableUpdate(candidate);
            if (!automatic && !candidate)
                setNotice(tr("app.s0048"));
        }
        catch (cause) {
            const message = cause instanceof Error ? cause.message : String(cause);
            if (!automatic)
                setUpdateError(message);
        }
        finally {
            setUpdateBusy(null);
        }
    }, [updatePolicy?.enabled]);
    useEffect(() => {
        if (!updatePolicy || !shouldCheckForUpdates(localStorage.getItem("siaocut.updateLastCheckedAt"), Date.now(), updatePolicy.enabled))
            return;
        void checkUpdates(true);
    }, [checkUpdates, updatePolicy]);
    const confirmUpdateInstall = async () => {
        if (!availableUpdate)
            return;
        setUpdateBusy(tr("app.s0049"));
        setUpdateError(null);
        try {
            await installUpdate((event) => {
                if (event.event === "Verifying")
                    setUpdateBusy(tr("app.s0050"));
            });
        }
        catch (cause) {
            setUpdateError(cause instanceof Error ? cause.message : String(cause));
            setUpdateBusy(null);
        }
    };
    useEffect(() => {
        if (!project?.tasks.some((task) => ["queued", "claimed", "running", "interrupted"].includes(task.status)))
            return;
        return startSerialPolling(() => loadProject(project.id).then((next) => {
            setError(clearTransientCoreError);
            setProject(next);
            setProjects((current) => current.map((item) => item.id === next.id ? next : item));
        }).catch(() => undefined), 2500);
    }, [project?.id, project?.tasks]);
    useEffect(() => {
        if (!activeExport || !["queued", "running"].includes(activeExport.status))
            return;
        return startSerialPolling(() => runCore(["video", "status", activeExport.id]).then((envelope) => {
            setError(clearTransientCoreError);
            if (!envelope.job)
                return;
            setActiveExport(envelope.job);
            if (envelope.job.status === "completed")
                setNotice(tr("app.s0051", { "0": envelope.job.outputPath }));
            if (envelope.job.status === "failed")
                setError(envelope.job.errorMessage ?? tr("app.s0052"));
            if (envelope.job.status === "cancelled")
                setNotice(tr("app.s0053"));
        }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))), 1000);
    }, [activeExport?.id, activeExport?.status]);
    useEffect(() => {
        if (!audioAnalysisJob || !["queued", "running"].includes(audioAnalysisJob.status))
            return;
        return startSerialPolling(() => runCore(["speech", "audio-status", audioAnalysisJob.id]).then((envelope) => {
            setError(clearTransientCoreError);
            if (!envelope.audioAnalysisJob)
                return;
            setAudioAnalysisJob(envelope.audioAnalysisJob);
            if (envelope.audioAnalysisJob.status === "completed")
                setNotice(tr("app.s0054"));
            if (["failed", "interrupted"].includes(envelope.audioAnalysisJob.status))
                setError(envelope.audioAnalysisJob.errorMessage ?? tr("app.s0055"));
            if (envelope.audioAnalysisJob.status === "cancelled")
                setNotice(tr("app.s0056"));
        }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))), 700);
    }, [audioAnalysisJob?.id, audioAnalysisJob?.status]);
    useEffect(() => {
        if (!modelJob || !["queued", "running"].includes(modelJob.status))
            return;
        return startSerialPolling(() => runCore(["model", "status", modelJob.id]).then(async (envelope) => {
            setError(clearTransientCoreError);
            if (!envelope.modelJob)
                return;
            setModelJob(envelope.modelJob);
            if (envelope.modelJob.status === "completed") {
                const catalog = await runCore(["model", "list", "--verify"]);
                const available = catalog.models ?? [];
                setModels(available);
                const installed = available.find((item) => item.id === envelope.modelJob?.modelId);
                if (installed) {
                    localStorage.setItem("siaocut.modelPath", installed.path);
                    setModelPath(installed.path);
                }
                setNotice(tr("app.s0057"));
            }
            if (envelope.modelJob.status === "failed")
                setError(envelope.modelJob.errorMessage ?? tr("app.s0058"));
            if (envelope.modelJob.status === "cancelled")
                setNotice(tr("app.s0059"));
        }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))), 800);
    }, [modelJob?.id, modelJob?.status]);
    useEffect(() => {
        if (!speakerJob || !["queued", "running"].includes(speakerJob.status))
            return;
        return startSerialPolling(() => runCore(["speaker", "job-status", speakerJob.id]).then(async (envelope) => {
            setError(clearTransientCoreError);
            if (!envelope.speakerJob)
                return;
            const next = envelope.speakerJob;
            setSpeakerJob(next);
            if (next.status === "completed" && next.kind === "install") {
                const status = await runCore(["speaker", "package", "--verify"]);
                setSpeakerPackage(status.speakerPackage ?? null);
                setNotice(tr("app.s0060"));
            }
            if (next.status === "completed" && next.kind === "analyze" && next.projectId) {
                await Promise.all([refreshProject(next.projectId), refreshSpeakerTrack(next.projectId)]);
                setNotice(tr("app.s0061"));
            }
            if (["failed", "interrupted"].includes(next.status))
                setError(next.errorMessage ?? tr("app.s0062"));
            if (next.status === "cancelled")
                setNotice(tr("app.s0063"));
        }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))), 800);
    }, [refreshProject, refreshSpeakerTrack, speakerJob?.id, speakerJob?.status]);
    useEffect(() => {
        if (!sourceJob || !["queued", "running", "finalizing"].includes(sourceJob.status))
            return;
        return startSerialPolling(() => runCore(["source", "status", sourceJob.id]).then(async (envelope) => {
            setError(clearTransientCoreError);
            if (!envelope.sourceJob)
                return;
            const nextJob = envelope.sourceJob;
            setSourceJob(nextJob);
            if (nextJob.status === "failed") {
                const message = nextJob.errorMessage ?? tr("app.s0064");
                setSourceError(message);
                setError(message);
            }
            if (nextJob.status === "interrupted") {
                const message = nextJob.errorMessage ?? tr("app.s0065");
                setSourceError(message);
                setError(message);
            }
            if (nextJob.status === "cancelled")
                setNotice(tr("app.s0066"));
            if (nextJob.status === "completed" && nextJob.projectId) {
                const imported = await loadProject(nextJob.projectId);
                setProjects((current) => [imported, ...current.filter((item) => item.id !== imported.id)]);
                setProject(imported);
                setSelectedId(imported.transcript.segments[0]?.id ?? null);
                setMediaUrl(await authorizeArtifact(imported.id, "preview") ?? await authorizeMedia(imported.id));
                setWaveformUrl(await authorizeArtifact(imported.id, "waveform"));
                await Promise.all([refreshLatestExport(imported.id), refreshLatestAudioAnalysis(imported.id)]);
                setShowSourceImport(false);
                setNotice(tr("app.s0067"));
            }
        }).catch((cause) => {
            const message = cause instanceof Error ? cause.message : String(cause);
            setSourceError(message);
            setError(message);
        }), 600);
    }, [refreshLatestAudioAnalysis, refreshLatestExport, sourceJob?.id, sourceJob?.status]);
    useEffect(() => {
        if (!autoWorkflow || !["queued", "running", "needs_agent", "needs_review"].includes(autoWorkflow.status))
            return;
        return startSerialPolling(() => runCore(["auto", "status", autoWorkflow.id]).then(async (envelope) => {
            setError(clearTransientCoreError);
            setAutoError(clearTransientCoreError);
            if (!envelope.workflow)
                return;
            const next = envelope.workflow;
            setAutoWorkflow({ ...next });
            if (next.projectId && (project?.id !== next.projectId || ["needs_review", "completed"].includes(next.status))) {
                await refreshProject(next.projectId, next.status === "completed");
            }
            if (next.status === "completed")
                setNotice(tr("app.s0068", { "0": next.outputPath }));
            if (next.status === "failed")
                setError(next.errorMessage ?? tr("app.s0069"));
            if (next.status === "interrupted")
                setError(next.errorMessage ?? tr("app.s0070"));
        }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))), 800);
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
        ? tr("app.s0071", { "0": selectedSegments.length, "1": formatTime(selectedSegments[0].start), "2": formatTime(selectedSegments.at(-1)!.end) }) : tr("app.s0072");
    const firstSelectedIndex = project?.transcript.segments.findIndex((segment) => segment.id === selectedSegments[0]?.id) ?? -1;
    const secondSelectedIndex = project?.transcript.segments.findIndex((segment) => segment.id === selectedSegments[1]?.id) ?? -1;
    const mergeCandidatesAdjacent = selectedSegments.length === 2 && firstSelectedIndex >= 0 && secondSelectedIndex === firstSelectedIndex + 1;
    const replaceMatchCount = useMemo(() => search && project
        ? project.transcript.segments.reduce((count, segment) => count + (segment.text.split(search).length - 1), 0)
        : 0, [project, search]);
    const splitTextOffset = Number(structureTextOffset);
    const splitCharacters = Array.from(selectedSegments[0]?.text ?? "");
    const splitLeftText = splitCharacters.slice(0, Number.isInteger(splitTextOffset) ? splitTextOffset : 0).join("").trim();
    const splitRightText = splitCharacters.slice(Number.isInteger(splitTextOffset) ? splitTextOffset : 0).join("").trim();
    const splitInputsValid = Number.isInteger(splitTextOffset)
        && splitTextOffset > 0
        && splitTextOffset < splitCharacters.length
        && Number(structureStart) > (selectedSegments[0]?.start ?? Number.POSITIVE_INFINITY)
        && Number(structureStart) < (selectedSegments[0]?.end ?? Number.NEGATIVE_INFINITY)
        && hasMeaningfulSubtitleText(splitLeftText)
        && hasMeaningfulSubtitleText(splitRightText);
    const timingStart = Number(structureStart);
    const timingEnd = Number(structureEnd);
    const timingInputsValid = Number.isFinite(timingStart)
        && Number.isFinite(timingEnd)
        && timingStart >= 0
        && timingEnd > timingStart;
    const timingChanged = Boolean(selectedSegments[0])
        && (Math.abs(timingStart - selectedSegments[0].start) >= 0.0005 || Math.abs(timingEnd - selectedSegments[0].end) >= 0.0005);
    const structureSubmitDisabled = structureBusy || (structureEditMode === "split" && !splitInputsValid)
        || (structureEditMode === "merge" && !mergeCandidatesAdjacent)
        || (structureEditMode === "timing" && (!timingInputsValid || !timingChanged))
        || (structureEditMode === "offset" && (!Number.isFinite(Number(structureDelta)) || Number(structureDelta) === 0));
    useEffect(() => {
        const segmentIds = new Set(project?.transcript.segments.map((segment) => segment.id) ?? []);
        setSelectedSegmentIds((current) => {
            const valid = current.filter((id) => segmentIds.has(id));
            if (selectedId && valid.includes(selectedId))
                return valid;
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
    const capabilities = useMemo(() => getProjectCapabilities(project, {
        mediaUrl,
        modelPath,
        translationTarget: subtitleLanguage,
        agentWorkflowKind,
    }), [agentWorkflowKind, mediaUrl, modelPath, project, subtitleLanguage]);
    const mediaCapabilityTitle = capabilities.hasBoundMedia ? undefined : tr("app.capability.mediaRequired");
    const transcribeCapabilityTitle = !capabilities.hasBoundMedia
        ? tr("app.capability.mediaRequired")
        : !capabilities.hasModel ? tr("app.capability.modelRequired") : undefined;
    const agentCapabilityTitle = !capabilities.hasBoundMedia
        ? tr("app.capability.mediaRequired")
        : !capabilities.hasTranscript
            ? tr("app.capability.transcriptRequired")
            : agentWorkflowKind === "translate" && !capabilities.hasTranslationTarget
                ? tr("app.capability.translationTargetRequired") : undefined;
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
        ? tr("app.s0073", { "0": deleteActiveTaskCount }) : currentDeleteCandidate && activeExport?.projectId === currentDeleteCandidate.id && ["queued", "running"].includes(activeExport.status)
        ? tr("app.s0074") : currentDeleteCandidate && audioAnalysisJob?.projectId === currentDeleteCandidate.id && ["queued", "running"].includes(audioAnalysisJob.status)
        ? tr("app.s0075") : currentDeleteCandidate && speakerJob?.projectId === currentDeleteCandidate.id && ["queued", "running"].includes(speakerJob.status)
        ? tr("app.s0076") : currentDeleteCandidate && autoWorkflow?.projectId === currentDeleteCandidate.id && ["queued", "running", "needs_agent", "needs_review", "failed", "interrupted"].includes(autoWorkflow.status)
        ? tr("app.s0077") : null;
    const humanState = busy ? tr("app.s0001") : taskLabel(project);
    const humanStateTone = humanState === tr("app.s0003") ? "warning" : humanState === tr("app.s0002") ? "agent" : humanState === tr("app.s0001") ? "info" : "success";
    const orderedPatchSets = project?.patchSets
        .map((set) => ({ ...set, items: set.items.filter((item) => ["pending", "conflict"].includes(item.status)).sort((left, right) => Number(right.status === "conflict") - Number(left.status === "conflict")) }))
        .filter((set) => set.items.length)
        .sort((left, right) => Number(right.items.some((item) => item.status === "conflict")) - Number(left.items.some((item) => item.status === "conflict"))) ?? [];
    const pendingEdits = project?.edits.filter((edit) => ["suggested", "proposed"].includes(edit.status)) ?? [];
    const failedTasks = project?.tasks.filter((task) => ["failed", "interrupted"].includes(task.status)) ?? [];
    const processingTasks = project?.tasks.filter((task) => ["queued", "claimed", "running"].includes(task.status)) ?? [];
    const workerTask = processingTasks[0];
    const workerWorkflow = workerTask ? project?.workflows.find((workflow) => workflow.taskId === workerTask.id) : undefined;
    const workerClaimCommand = "siaocut-core task claim --worker codex-worker";
    const workerContinueCommand = workerWorkflow ? `siaocut-core workflow continue ${workerWorkflow.id}` : workerTask ? `siaocut-core task retry ${workerTask.id}` : "";
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
        if (!project || subtitleMode === "source")
            return;
        if (!translationLanguageOptions.length) {
            setSubtitleMode("source");
            return;
        }
        if (!translationLanguageOptions.includes(subtitleLanguage))
            setSubtitleLanguage(translationLanguageOptions[0]);
    }, [project, subtitleLanguage, subtitleMode, translationLanguageOptions]);
    useEffect(() => {
        if (!showExportPanel)
            return;
        const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        window.requestAnimationFrame(() => exportPanelRef.current?.querySelector<HTMLElement>("button, select")?.focus());
        const closeOnEscape = (event: KeyboardEvent) => {
            if (event.key !== "Escape")
                return;
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
        if (videoRef.current)
            videoRef.current.currentTime = segment.start;
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
        }
        else {
            const alreadySelected = selectedSegmentIds.includes(segment.id);
            if (alreadySelected && selectedSegmentIds.length > 1) {
                const next = selectedSegmentIds.filter((id) => id !== segment.id);
                setSelectedSegmentIds(next);
                setSelectedId(next.at(-1) ?? null);
            }
            else if (!alreadySelected) {
                setSelectedSegmentIds([...selectedSegmentIds, segment.id]);
                setSelectedId(segment.id);
            }
            setSelectionAnchorId(segment.id);
        }
        setWordRange(null);
        if (videoRef.current)
            videoRef.current.currentTime = segment.start;
    };
    const moveSegmentSelection = (direction: -1 | 1) => {
        if (!project?.transcript.segments.length)
            return;
        const currentIndex = project.transcript.segments.findIndex((segment) => segment.id === selectedId);
        const nextIndex = Math.min(project.transcript.segments.length - 1, Math.max(0, (currentIndex < 0 ? 0 : currentIndex) + direction));
        selectSegment(project.transcript.segments[nextIndex]);
    };
    const locateSpeechEvidence = (evidence: SpeechEvidence) => {
        const segment = project?.transcript.segments.find((candidate) => candidate.id === evidence.segmentId);
        if (segment)
            selectSegment(segment);
    };
    const locateSpeechPause = (pause: SpeechPause) => {
        const word = project?.transcript.words.find((candidate) => candidate.id === pause.nextWordId);
        const segment = word && project?.transcript.segments.find((candidate) => candidate.id === word.segmentId);
        if (segment)
            selectSegment(segment);
    };
    const locateAudioRisk = (risk: AudioRisk) => {
        if (videoRef.current)
            videoRef.current.currentTime = risk.start;
    };
    const selectWordForCut = (index: number) => {
        if (!selectedId)
            return;
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
        try {
            await action();
        }
        catch (cause) {
            setError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            setBusy(null);
        }
    };
    const importMedia = () => withBusy(tr("app.s0078"), async () => {
        const path = await pickMedia();
        if (!path)
            return;
        const envelope = await runCore(["import", path]);
        if (!envelope.project)
            throw new Error(tr("app.s0079"));
        videoRef.current?.pause();
        setPlayback({ playing: false, currentTime: 0, duration: envelope.project.media.durationSeconds ?? 0 });
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
        setNotice(tr("app.s0080"));
    });
    const switchProject = (projectId: string) => {
        if (project?.id === projectId)
            return;
        setNotice(null);
        setError(null);
        void withBusy(tr("app.s0081"), async () => {
            await refreshProject(projectId, true);
        });
    };
    const openDeleteDialog = (candidate: Project) => {
        setDeleteError(null);
        setDeleteCandidate(candidate);
    };
    const closeDeleteDialog = () => {
        if (deleteBusy)
            return;
        setDeleteCandidate(null);
        setDeleteError(null);
    };
    const deleteProject = async () => {
        if (!currentDeleteCandidate || deleteBlockMessage)
            return;
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
                setPlayback({ playing: false, currentTime: 0, duration: 0 });
                setProject(null);
                setSelectedId(null);
                setMediaUrl(null);
                setWaveformUrl(null);
                setActiveExport(null);
                setAudioAnalysisJob(null);
                setSpeakerTrack(null);
                setWordRange(null);
                setCutPreview(null);
                if (remaining[0])
                    await refreshProject(remaining[0].id, true);
            }
            setNotice(tr("app.s0082", { "0": deleting.title }));
        }
        catch (cause) {
            const message = cause instanceof Error ? cause.message : String(cause);
            setDeleteError(message.replace(/^project_busy:\s*/, ""));
        }
        finally {
            setDeleteBusy(false);
        }
    };
    const withSourceBusy = async (label: string, action: () => Promise<void>) => {
        setSourceBusy(label);
        setSourceError(null);
        try {
            await action();
        }
        catch (cause) {
            setSourceError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            setSourceBusy(null);
        }
    };
    const inspectSource = () => withSourceBusy(tr("app.s0083"), async () => {
        const url = sourceUrl.trim();
        if (!runtime?.ytDlpConfigured)
            throw new Error(tr("app.s0084"));
        if (!isHttpsSourceUrl(url))
            throw new Error(tr("app.s0085"));
        const envelope = await runCore(["source", "inspect", url]);
        if (!envelope.source)
            throw new Error(tr("app.s0086"));
        setSourcePreview(envelope.source);
        setSourceJob(null);
        setSourceAuthorized(false);
    });
    const startSourceImport = () => sourcePreview && withSourceBusy(tr("app.s0087"), async () => {
        if (!sourceAuthorized)
            throw new Error(tr("app.s0088"));
        const envelope = await runCore(["source", "start", sourcePreview.originalUrl, "--confirm-media-id", sourcePreview.siteMediaId]);
        if (!envelope.sourceJob)
            throw new Error(tr("app.s0089"));
        setSourceJob(envelope.sourceJob);
        setNotice(tr("app.s0090"));
    });
    const cancelSourceImport = () => sourceJob && withSourceBusy(tr("app.s0091"), async () => {
        const envelope = await runCore(["source", "cancel", sourceJob.id]);
        if (!envelope.sourceJob)
            throw new Error(tr("app.s0092"));
        setSourceJob(envelope.sourceJob);
    });
    const resumeSourceImport = () => sourceJob && withSourceBusy(tr("app.s0093"), async () => {
        const envelope = await runCore(["source", "resume", sourceJob.id]);
        if (!envelope.sourceJob)
            throw new Error(tr("app.s0094"));
        setSourceJob(envelope.sourceJob);
        setNotice(tr("app.s0095", { "0": envelope.sourceJob.attemptCount }));
    });
    const resetSourceImport = () => {
        if (sourceJob && ["queued", "running", "finalizing"].includes(sourceJob.status))
            return;
        setSourcePreview(null);
        setSourceJob(null);
        setSourceUrl("");
        setSourceAuthorized(false);
        setSourceError(null);
    };
    const withAutoBusy = async (label: string, action: () => Promise<void>) => {
        setAutoBusy(label);
        setAutoError(null);
        try {
            await action();
        }
        catch (cause) {
            setAutoError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            setAutoBusy(null);
        }
    };
    const chooseAutoMedia = () => withAutoBusy(tr("app.s0096"), async () => {
        const path = await pickMedia();
        if (path)
            setAutoMediaPath(path);
    });
    const inspectAutoSource = () => withAutoBusy(tr("app.s0083"), async () => {
        if (!runtime?.ytDlpConfigured)
            throw new Error(tr("app.s0084"));
        if (!autoUrl.trim())
            throw new Error(tr("app.s0085"));
        const envelope = await runCore(["source", "inspect", autoUrl.trim()]);
        if (!envelope.source)
            throw new Error(tr("app.s0086"));
        setAutoSourcePreview(envelope.source);
        setAutoAuthorized(false);
    });
    const startAutoWorkflow = () => withAutoBusy(tr("app.s0097"), async () => {
        if (!modelPath)
            throw new Error(tr("app.s0098"));
        if (autoTranslate && !autoTranslationLanguage.trim())
            throw new Error(tr("app.s0099"));
        if (autoInputKind === "local" && !autoMediaPath)
            throw new Error(tr("app.s0100"));
        if (autoInputKind === "url" && (!autoSourcePreview || !autoAuthorized))
            throw new Error(tr("app.s0101"));
        const output = await pickVideoPath(autoSourcePreview?.title ?? tr("app.s0102"));
        if (!output)
            return;
        const inputArgs = autoInputKind === "local"
            ? ["--media", autoMediaPath, "--title", tr("app.s0103")]
            : ["--url", autoSourcePreview!.originalUrl, "--confirm-media-id", autoSourcePreview!.siteMediaId];
        const envelope = await runCore([
            "auto", "start", ...inputArgs,
            "--model", modelPath,
            "--language", transcriptionLanguage,
            "--locale", uiLocale,
            "--output", output,
            "--subtitle-mode", autoTranslate ? autoSubtitleMode : "source",
            ...(autoTranslate ? ["--translate", autoTranslationLanguage] : []),
            ...(autoBurnSubtitles ? ["--burn-subtitles"] : []),
        ]);
        if (!envelope.workflow)
            throw new Error(tr("app.s0104"));
        setAutoWorkflow({ ...envelope.workflow });
        setShowAutoWorkflow(false);
        setNotice(tr("app.s0105"));
    });
    const cancelAutoWorkflow = () => autoWorkflow && withAutoBusy(tr("app.s0106"), async () => {
        const envelope = await runCore(["auto", "cancel", autoWorkflow.id]);
        if (!envelope.workflow)
            throw new Error(tr("app.s0107"));
        setAutoWorkflow(null);
        setNotice(tr("app.s0108"));
    });
    const continueAutoWorkflow = () => autoWorkflow && withAutoBusy(tr("app.s0109"), async () => {
        const envelope = await runCore(["auto", "continue", autoWorkflow.id]);
        if (!envelope.workflow)
            throw new Error(tr("app.s0110"));
        setAutoWorkflow({ ...envelope.workflow });
        setNotice(tr("app.s0111", { "0": envelope.workflow.attemptCount }));
    });
    const openAutoProject = () => autoWorkflow?.projectId && withAutoBusy(tr("app.s0112"), async () => {
        await refreshProject(autoWorkflow.projectId!, true);
    });
    const changeAsrBackend = (backend: "cpu" | "vulkan") => withBusy(tr("app.s0113"), async () => {
        const next = await selectAsrBackend(backend);
        setRuntime(next);
        setNotice(backend === "vulkan" ? tr("app.s0114") : tr("app.s0115"));
    });
    const openDiagnostics = () => withBusy(tr("app.s0116"), async () => {
        await openLogDirectory();
        setNotice(tr("app.s0117"));
    });
    const relinkMedia = () => project && withBusy(tr("app.s0118"), async () => {
        const path = await pickMedia();
        if (!path)
            return;
        await runCore(["project", "relink", project.id, path]);
        await refreshProject(project.id, true);
        setNotice(tr("app.s0119"));
    });
    const transcribe = () => project && withBusy(tr("app.s0120"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        if (!runtime?.ffmpegConfigured)
            throw new Error(tr("app.s0121"));
        if (!runtime?.asrConfigured)
            throw new Error(tr("app.s0122"));
        if (!modelPath)
            throw new Error(tr("app.s0123"));
        const result = await runCore(["transcribe", project.id, "--model", modelPath, "--language", transcriptionLanguage]);
        await refreshProject(project.id);
        setNotice(Number(result.segments ?? 0) === 0 ? tr("app.s0124") : tr("app.s0125"));
    });
    const startAudioAnalysis = () => project && withBusy(tr("app.s0126"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        if (!runtime?.ffmpegConfigured)
            throw new Error(tr("app.s0121"));
        const envelope = await runCore(["speech", "audio-start", project.id]);
        if (!envelope.audioAnalysisJob)
            throw new Error(tr("app.s0127"));
        setAudioAnalysisJob(envelope.audioAnalysisJob);
        setNotice(tr("app.s0128"));
    });
    const cancelAudioAnalysis = () => audioAnalysisJob && withBusy(tr("app.s0129"), async () => {
        const envelope = await runCore(["speech", "audio-cancel", audioAnalysisJob.id]);
        if (envelope.audioAnalysisJob)
            setAudioAnalysisJob(envelope.audioAnalysisJob);
    });
    const resumeAudioAnalysis = () => audioAnalysisJob && withBusy(tr("app.s0130"), async () => {
        const envelope = await runCore(["speech", "audio-resume", audioAnalysisJob.id]);
        if (!envelope.audioAnalysisJob)
            throw new Error(tr("app.s0131"));
        setAudioAnalysisJob(envelope.audioAnalysisJob);
        setNotice(tr("app.s0132", { "0": envelope.audioAnalysisJob.attemptCount }));
    });
    const installSpeakerPackage = () => withBusy(tr("app.s0133"), async () => {
        const envelope = await runCore(["speaker", "install"]);
        if (!envelope.speakerJob)
            throw new Error(tr("app.s0134"));
        setSpeakerJob(envelope.speakerJob);
        if (envelope.speakerJob.status === "completed") {
            const status = await runCore(["speaker", "package", "--verify"]);
            setSpeakerPackage(status.speakerPackage ?? null);
            setNotice(tr("app.s0135"));
        }
        else {
            setNotice(tr("app.s0136"));
        }
    });
    const startSpeakerAnalysis = () => project && withBusy(tr("app.s0137"), async () => {
        if (!speakerPackage?.installed || speakerPackage.verified !== true)
            throw new Error(tr("app.s0138"));
        const envelope = await runCore(["speaker", "analyze", project.id]);
        if (!envelope.speakerJob)
            throw new Error(tr("app.s0139"));
        setSpeakerJob(envelope.speakerJob);
        if (envelope.speakerJob.status === "completed") {
            await Promise.all([refreshProject(project.id), refreshSpeakerTrack(project.id)]);
            setNotice(tr("app.s0061"));
        }
        else {
            setNotice(tr("app.s0140"));
        }
    });
    const cancelSpeakerJob = () => speakerJob && withBusy(tr("app.s0141"), async () => {
        const envelope = await runCore(["speaker", "cancel", speakerJob.id]);
        if (envelope.speakerJob)
            setSpeakerJob(envelope.speakerJob);
    });
    const resumeSpeakerJob = () => speakerJob && withBusy(tr("app.s0142"), async () => {
        const envelope = await runCore(["speaker", "resume", speakerJob.id]);
        if (!envelope.speakerJob)
            throw new Error(tr("app.s0143"));
        setSpeakerJob(envelope.speakerJob);
        setNotice(tr("app.s0144", { "0": envelope.speakerJob.attemptCount }));
    });
    const renameSpeaker = (speakerId: string, name: string) => project && withBusy(tr("app.s0145"), async () => {
        const envelope = await runCore(["speaker", "rename", project.id, speakerId, "--name", name]);
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0146"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0147"));
    });
    const mergeSpeaker = (fromId: string, intoId: string) => project && withBusy(tr("app.s0148"), async () => {
        const envelope = await runCore(["speaker", "merge", project.id, "--from", fromId, "--into", intoId]);
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0149"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0150"));
    });
    const assignSpeaker = (segmentId: string, speakerId: string) => project && withBusy(tr("app.s0151"), async () => {
        const envelope = await runCore(["speaker", "assign", project.id, segmentId, speakerId]);
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0146"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0152"));
    });
    const editSegment = (segment: Segment, text: string) => project && text.trim() !== segment.text && withBusy(tr("app.s0153"), async () => {
        await runCore(["transcript", "edit", project.id, segment.id, "--text", text.trim()]);
        await refreshProject(project.id);
        setNotice(tr("app.s0154"));
    });
    const replaceAll = () => project && search && (replacement || emptyReplacementConfirmed) && withBusy(tr("app.s0155"), async () => {
        const result = await runCore(["transcript", "replace", project.id, "--find", search, "--replace", replacement]);
        await refreshProject(project.id);
        setEmptyReplacementConfirmed(false);
        setNotice(Number(result.changedSegments ?? 0) === 0 ? tr("app.s0156") : tr("app.s0157", { "0": result.changedSegments }));
    });
    const openStructureEdit = (mode: StructureEditMode) => {
        const target = selectedSegments[0];
        if (!project || !target)
            return;
        setStructureError(null);
        if (mode === "split") {
            const characterCount = Array.from(target.text).length;
            let splitAt = target.start + (target.end - target.start) / 2;
            const crossingWord = project.transcript.words.find((word) => word.segmentId === target.id && word.start < splitAt && word.end > splitAt);
            if (crossingWord) {
                if (crossingWord.end < target.end)
                    splitAt = crossingWord.end;
                else if (crossingWord.start > target.start)
                    splitAt = crossingWord.start;
            }
            setStructureTextOffset(String(Math.max(1, Math.floor(characterCount / 2))));
            setStructureStart(splitAt.toFixed(3));
        }
        else if (mode === "timing") {
            setStructureStart(target.start.toFixed(3));
            setStructureEnd(target.end.toFixed(3));
        }
        else if (mode === "offset") {
            setStructureDelta("0.100");
        }
        setStructureEditMode(mode);
    };
    const applyStructureEdit = async () => {
        if (!project || !structureEditMode || !selectedSegments.length)
            return;
        setStructureBusy(true);
        setStructureError(null);
        try {
            let args: string[];
            if (structureEditMode === "split") {
                const textOffset = Number(structureTextOffset);
                const at = Number(structureStart);
                if (!Number.isInteger(textOffset) || textOffset <= 0 || !Number.isFinite(at))
                    throw new Error(tr("app.s0158"));
                if (!hasMeaningfulSubtitleText(splitLeftText) || !hasMeaningfulSubtitleText(splitRightText))
                    throw new Error(tr("app.structure.splitMeaningful"));
                args = ["transcript", "split", project.id, selectedSegments[0].id, "--text-offset", String(textOffset), "--at", String(at)];
            }
            else if (structureEditMode === "merge") {
                if (!mergeCandidatesAdjacent)
                    throw new Error(tr("app.s0159"));
                args = ["transcript", "merge", project.id, selectedSegments[0].id, selectedSegments[1].id];
            }
            else if (structureEditMode === "timing") {
                const start = Number(structureStart);
                const end = Number(structureEnd);
                if (!Number.isFinite(start) || !Number.isFinite(end) || start < 0 || end <= start)
                    throw new Error(tr("app.s0160"));
                if (!timingChanged)
                    throw new Error(tr("app.structure.timingUnchanged"));
                args = ["transcript", "timing", project.id, selectedSegments[0].id, "--start", String(start), "--end", String(end)];
            }
            else {
                const delta = Number(structureDelta);
                if (!Number.isFinite(delta) || delta === 0)
                    throw new Error(tr("app.s0161"));
                args = ["transcript", "offset", project.id, ...selectedSegments.flatMap((segment) => ["--segment", segment.id]), "--delta", String(delta)];
            }
            const envelope = await runCore(args);
            if (!envelope.structureEdit?.project)
                throw new Error(tr("app.s0162"));
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
                split: tr("app.s0163"),
                merge: tr("app.s0164"),
                timing: tr("app.s0165"),
                offset: tr("app.s0166", { "0": selectedSegments.length, "1": Number(structureDelta) > 0 ? "+" : "", "2": Number(structureDelta).toFixed(3) }),
            };
            setNotice(messages[structureEditMode]);
        }
        catch (cause) {
            setStructureError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
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
        if (!project)
            return;
        setSubtitleImportBusy(tr("app.s0167"));
        setSubtitleImportError(null);
        try {
            const path = await pickSubtitleFile();
            if (!path)
                return;
            setSubtitleImportPath(path);
            setSubtitleReplaceConfirmed(false);
            const envelope = await runCore(["transcript", "inspect-file", project.id, path]);
            if (!envelope.subtitleImportPreview)
                throw new Error(tr("app.s0168"));
            setSubtitleImportPreview(envelope.subtitleImportPreview);
        }
        catch (cause) {
            setSubtitleImportPreview(null);
            setSubtitleImportError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            setSubtitleImportBusy(null);
        }
    };
    const confirmSubtitleImport = async () => {
        if (!project || !subtitleImportPreview || !subtitleReplaceConfirmed)
            return;
        setSubtitleImportBusy(tr("app.s0169"));
        setSubtitleImportError(null);
        try {
            const envelope = await runCore([
                "transcript", "import-file", project.id, subtitleImportPath,
                "--confirm-replace", "--expected-sha256", subtitleImportPreview.sha256,
            ]);
            if (!envelope.project)
                throw new Error(tr("app.s0170"));
            setProject(envelope.project);
            setProjects((current) => current.map((item) => item.id === envelope.project?.id ? envelope.project : item) as Project[]);
            setSelectedId(envelope.project.transcript.segments[0]?.id ?? null);
            setWordRange(null);
            setCutPreview(null);
            await refreshSpeakerTrack(project.id);
            setShowSubtitleImport(false);
            setQualityFilter("all");
            setNotice(tr("app.s0171", { "0": envelope.project.transcript.segments.length }));
        }
        catch (cause) {
            setSubtitleImportError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            setSubtitleImportBusy(null);
        }
    };
    const locateSubtitleIssue = (issue: SubtitleQualityIssue) => {
        const segment = project?.transcript.segments.find((candidate) => candidate.id === issue.segmentId);
        if (segment)
            selectSegment(segment);
    };
    const exportTranscript = () => project && withBusy(tr("app.s0172"), async () => {
        const output = await pickTranscriptPath(project.title, exportFormat);
        if (!output)
            return;
        await runCore(["transcript", "export", project.id, "--format", exportFormat, "--output", output, ...subtitleArgs()]);
        setNotice(tr("app.s0173", { "0": exportFormat === "markdown" ? tr("app.s0174") : tr("app.s0175"), "1": output }));
    });
    const subtitleArgs = () => {
        if (subtitleMode === "source")
            return ["--subtitle-mode", "source"];
        if (!selectedSubtitleLanguage)
            throw new Error(tr("app.s0176"));
        if (selectedTranslationPending)
            throw new Error(tr("app.s0177", { "0": selectedSubtitleLanguage.toUpperCase() }));
        return ["--subtitle-mode", subtitleMode, "--lang", selectedSubtitleLanguage];
    };
    const changeCanvas = (settings: CanvasSettings) => project && withBusy(tr("app.s0178"), async () => {
        const envelope = await runCore(["canvas", "set", project.id, "--aspect-ratio", settings.aspectRatio, "--framing", settings.framing]);
        if (!envelope.project)
            throw new Error(tr("app.canvas.projectMissing"));
        setProject(envelope.project);
        setProjects((current) => current.map((item) => item.id === envelope.project!.id ? envelope.project! : item));
        const authorization = await resolveCanvasMedia(project.id);
        setMediaUrl(authorization.mediaUrl);
        const savedNotice = settings.aspectRatio === "9:16" ? tr("app.s0179") : tr("app.s0180");
        setNotice(authorization.warning ? `${savedNotice} ${tr("app.canvas.previewUnavailable")}` : savedNotice);
    });
    const changeSubtitleStyle = (preset: Project["subtitleStyle"]["preset"], position: Project["subtitleStyle"]["position"]) => project && withBusy(tr("app.s0181"), async () => {
        const envelope = await runCore(["transcript", "set-style", project.id, "--preset", preset, "--position", position]);
        if (!envelope.project)
            throw new Error(tr("app.s0182"));
        setProject(envelope.project);
        setProjects((current) => current.map((item) => item.id === envelope.project!.id ? envelope.project! : item));
        setNotice(tr("app.s0183"));
    });
    const preparePreview = () => project && withBusy(tr("app.s0184"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        await runCore(["media", "prepare", project.id]);
        await refreshProject(project.id, true);
        setNotice(tr("app.s0185"));
    });
    const exportVideo = () => project && withBusy(tr("app.s0186"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        const output = await pickVideoPath(project.title);
        if (!output)
            return;
        const envelope = await runCore(["video", "export", project.id, "--output", output, "--burn-subtitles", ...subtitleArgs()]);
        if (!envelope.job)
            throw new Error(tr("app.s0187"));
        setActiveExport(envelope.job);
        setNotice(envelope.job.status === "completed" ? tr("app.s0051", { "0": envelope.job.outputPath }) : tr("app.s0188"));
    });
    const cancelExport = () => activeExport && withBusy(tr("app.s0189"), async () => {
        const envelope = await runCore(["video", "cancel", activeExport.id]);
        if (envelope.job)
            setActiveExport(envelope.job);
        setNotice(tr("app.s0190"));
    });
    const retryExport = () => activeExport && withBusy(tr("app.s0191"), async () => {
        const envelope = await runCore(["video", "retry", activeExport.id]);
        if (!envelope.job)
            throw new Error(tr("app.s0187"));
        setActiveExport(envelope.job);
        setNotice(tr("app.s0192"));
    });
    const updateCut = (editId: string, action: "apply" | "restore") => project && withBusy(action === "apply" ? tr("app.s0193") : tr("app.s0194"), async () => {
        await runCore(["cut", action, project.id, editId]);
        await refreshProject(project.id);
        setNotice(action === "apply" ? tr("app.s0195") : tr("app.s0196"));
    });
    const detectSuggestions = () => project && withBusy(tr("app.s0197"), async () => {
        const envelope = await runCore(["cut", "detect", project.id]);
        const count = envelope.suggestions?.length ?? 0;
        await refreshProject(project.id);
        setNotice(count ? tr("app.s0198", { "0": count }) : tr("app.s0199"));
    });
    const startCutPreview = async (editId: string) => {
        if (!project)
            return;
        const envelope = await runCore(["cut", "preview", project.id, editId]);
        if (!envelope.preview)
            throw new Error(tr("app.s0200"));
        setCutPreview(envelope.preview);
        const video = videoRef.current;
        if (!video) {
            setNotice(tr("app.s0201"));
            return;
        }
        video.currentTime = envelope.preview.previewStart;
        await video.play();
        setNotice(tr("app.s0202"));
    };
    const previewCut = (editId: string) => withBusy(tr("app.s0203"), async () => {
        await startCutPreview(editId);
    });
    const createWordCut = () => project && selected && activeWordRange && withBusy(tr("app.s0204"), async () => {
        const from = selectedWords[activeWordRange.start];
        const to = selectedWords[activeWordRange.end];
        if (!from || !to)
            throw new Error(tr("app.s0205"));
        const envelope = await runCore([
            "cut", "create", project.id,
            "--segment", selected.id,
            "--from-word", from.id,
            "--to-word", to.id,
            "--padding-ms", String(cutPadding),
        ]);
        if (!envelope.cut)
            throw new Error(tr("app.s0206"));
        await refreshProject(project.id);
        setWordRange(null);
        await startCutPreview(envelope.cut.id);
    });
    const handleVideoTimeUpdate = () => {
        const video = videoRef.current;
        if (!video || !project)
            return;
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
        if (cut)
            video.currentTime = cut.sourceEnd;
        setPlayback((current) => ({
            ...current,
            currentTime: video.currentTime,
            duration: Number.isFinite(video.duration) ? video.duration : current.duration,
        }));
    };
    const handleVideoLoadedMetadata = (event: SyntheticEvent<HTMLVideoElement>) => {
        // React clears SyntheticEvent.currentTarget after this callback returns. Capture the
        // DOM value before entering a state updater, which React may invoke later.
        const mediaDuration = event.currentTarget.duration;
        const fallbackDuration = project?.media.durationSeconds;
        setPlayback((current) => ({
            ...current,
            duration: resolvePlaybackDuration(mediaDuration, fallbackDuration),
        }));
    };
    const restoreVersion = (versionId: string) => project && withBusy(tr("app.s0207"), async () => {
        await runCore(["project", "restore", project.id, versionId]);
        await refreshProject(project.id);
        setNotice(tr("app.s0208"));
    });
    const navigateHistory = (action: "undo" | "redo") => project && withBusy(action === "undo" ? tr("app.s0209") : tr("app.s0210"), async () => {
        const envelope = await runCore(["project", action, project.id]);
        if (!envelope.project)
            throw new Error(tr("app.s0211"));
        setProject(envelope.project);
        setProjects((current) => current.map((item) => item.id === envelope.project?.id ? envelope.project : item) as Project[]);
        setWordRange(null);
        setCutPreview(null);
        setNotice(action === "undo" ? tr("app.s0212") : tr("app.s0213"));
    });
    useEffect(() => {
        if (!showMoreMenu)
            return;
        const closeOnOutsidePointer = (event: PointerEvent) => {
            if (!commandMoreRef.current?.contains(event.target as Node))
                setShowMoreMenu(false);
        };
        document.addEventListener("pointerdown", closeOnOutsidePointer);
        return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
    }, [showMoreMenu]);
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
                commandMoreRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
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
                if (project)
                    setShowExportPanel(true);
                return;
            }
            if (!dialogOpen && !busy && !editingTarget && modifier && event.shiftKey && ["s", "m", "t", "o"].includes(key)) {
                event.preventDefault();
                const mode = ({ s: "split", m: "merge", t: "timing", o: "offset" } as const)[key as "s" | "m" | "t" | "o"];
                if (mode === "split" && selectedSegments.length === 1 && Array.from(selectedSegments[0].text).length > 1)
                    openStructureEdit(mode);
                if (mode === "merge" && mergeCandidatesAdjacent)
                    openStructureEdit(mode);
                if (mode === "timing" && selectedSegments.length === 1)
                    openStructureEdit(mode);
                if (mode === "offset" && selectedSegments.length > 0)
                    openStructureEdit(mode);
                return;
            }
            if (!dialogOpen && !busy && !editingTarget && event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
                event.preventDefault();
                moveSegmentSelection(event.key === "ArrowUp" ? -1 : 1);
                return;
            }
            if (target instanceof HTMLElement && (editingTarget || target.matches("button")))
                return;
            if (dialogOpen || busy)
                return;
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
            if (!video || modifier || event.altKey)
                return;
            if (event.code === "Space") {
                event.preventDefault();
                if (video.paused)
                    void video.play();
                else
                    video.pause();
            }
            else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
                event.preventDefault();
                const change = event.key === "ArrowLeft" ? -1 : 1;
                video.currentTime = Math.max(0, Math.min(video.duration || project?.timeline.sourceDuration || 0, video.currentTime + change));
            }
        };
        window.addEventListener("keydown", handleShortcut);
        return () => window.removeEventListener("keydown", handleShortcut);
    }, [busy, currentDeleteCandidate, mergeCandidatesAdjacent, project, selectedSegmentIds, showAutoWorkflow, showMoreMenu, showRuntime, showSourceImport, showSubtitleImport, structureEditMode]);
    const chooseModel = () => withBusy(tr("app.s0214"), async () => {
        const path = await pickModel();
        if (!path)
            return;
        localStorage.setItem("siaocut.modelPath", path);
        setModelPath(path);
        setNotice(tr("app.s0215"));
    });
    const installModel = (modelId: string) => withBusy(tr("app.s0216"), async () => {
        const envelope = await runCore(["model", "install", modelId]);
        if (!envelope.modelJob)
            throw new Error(tr("app.s0217"));
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
            setNotice(tr("app.s0057"));
            return;
        }
        setNotice(tr("app.s0218"));
    });
    const cancelModel = () => modelJob && withBusy(tr("app.s0219"), async () => {
        const envelope = await runCore(["model", "cancel", modelJob.id]);
        if (envelope.modelJob)
            setModelJob(envelope.modelJob);
    });
    const removeModel = (modelId: string) => withBusy(tr("app.s0220"), async () => {
        await runCore(["model", "remove", modelId]);
        const catalog = await runCore(["model", "list"]);
        const available = catalog.models ?? [];
        setModels(available);
        const selected = models.find((item) => item.id === modelId)?.path;
        if (selected && selected === modelPath) {
            localStorage.removeItem("siaocut.modelPath");
            setModelPath(null);
        }
        setNotice(tr("app.s0221"));
    });
    const createAgentTask = () => project && withBusy(tr("app.s0222"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        if (!capabilities.hasTranscript)
            throw new Error(tr("app.capability.transcriptRequired"));
        if (agentWorkflowKind === "translate" && !capabilities.hasTranslationTarget)
            throw new Error(tr("app.capability.translationTargetRequired"));
        await runCore([
            "workflow", "create", project.id,
            "--kind", agentWorkflowKind,
            "--locale", uiLocale,
            ...(agentWorkflowKind === "translate" ? ["--lang", subtitleLanguage] : []),
        ]);
        await refreshProject(project.id);
        setNotice({
            polish: tr("app.workflow.created.polish"),
            proofread: tr("app.workflow.created.proofread"),
            edit: tr("app.workflow.created.edit"),
            translate: tr("app.workflow.created.translate", { language: subtitleLanguage.toUpperCase() }),
        }[agentWorkflowKind]);
    });
    const updateTask = (taskId: string, action: "retry" | "cancel") => project && withBusy(action === "retry" ? tr("app.s0224") : tr("app.s0225"), async () => {
        await runCore(["task", action, taskId]);
        await refreshProject(project.id);
        setNotice(action === "retry" ? tr("app.s0226") : tr("app.s0227"));
    });
    const reviewPatch = (patchItemId: string, action: "apply" | "keep") => project && withBusy(action === "apply" ? tr("app.s0228") : tr("app.s0229"), async () => {
        await runCore(["task", "review", patchItemId, "--action", action]);
        await refreshProject(project.id);
        setNotice(action === "apply" ? tr("app.s0230") : tr("app.s0231"));
    });
    const reviewAll = (taskId: string, action: "apply" | "keep") => project && withBusy(action === "apply" ? tr("app.s0232") : tr("app.s0233"), async () => {
        await runCore(["task", "review-all", taskId, "--action", action]);
        await refreshProject(project.id);
        setNotice(action === "apply" ? tr("app.s0234") : tr("app.s0235"));
    });
    const inspectorTabs = ["segment", "analysis", "history"] as const;
    const changeInspectorTabFromKeyboard = (event: ReactKeyboardEvent<HTMLButtonElement>, tab: typeof inspectorTabs[number]) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight")
            return;
        event.preventDefault();
        const currentIndex = inspectorTabs.indexOf(tab);
        const direction = event.key === "ArrowRight" ? 1 : -1;
        const nextTab = inspectorTabs[(currentIndex + direction + inspectorTabs.length) % inspectorTabs.length];
        setInspectorTab(nextTab);
        requestAnimationFrame(() => document.getElementById(`inspector-tab-${nextTab}`)?.focus());
    };
    return (<main className="app-shell">
      <aside className="rail">
        <div className="brand"><span className="brand-mark">S</span><span>SiaoCut</span></div>
        <div className="new-project-actions">
          <button ref={autoButtonRef} className="new-project auto" onClick={() => setShowAutoWorkflow(true)}><Sparkles size={16}/>{tr("app.s0236")}</button>
          <button className="new-project" onClick={importMedia}><FolderPlus size={16}/>{tr("app.s0237")}</button>
          <button ref={sourceButtonRef} className="new-project url" onClick={() => setShowSourceImport(true)}><Link2 size={16}/>{tr("app.s0238")}</button>
        </div>
        <div className="rail-heading">{tr("app.s0239")}</div>
        <nav aria-label={tr("app.s0240")}>
          {projects.map((item) => (<div className={`project-entry ${project?.id === item.id ? "active" : ""}`} key={item.id}>
              <button className="project-link" onClick={() => switchProject(item.id)}>
                <span className="project-dot"/><span><strong>{item.title}</strong><small>{subtitleCountLabel(item.transcript.segments.length)}</small></span><ChevronRight size={14}/>
              </button>
              <button className="project-delete" aria-label={tr("app.s0242", { "0": item.title })} title={tr("app.s0243")} onClick={() => openDeleteDialog(item)}><Trash2 size={14}/></button>
            </div>))}
          {!projects.length && !busy && <p className="empty-rail">{tr("app.s0244")}</p>}
        </nav>
        <button ref={runtimeButtonRef} className="runtime-link" onClick={() => setShowRuntime(true)}><Settings2 size={15}/><span>{tr("app.s0245")}</span></button>
        <label className="locale-switch"><span>{tr("app.locale.label")}</span><select aria-label={tr("app.locale.label")} value={uiLocale} onChange={(event) => selectUiLocale(event.target.value as UiLocale)}><option value="zh-CN">{tr("app.locale.zhCN")}</option><option value="en-US">{tr("app.locale.enUS")}</option></select></label>
        <div className="privacy"><ShieldCheck size={15}/><span>{tr("app.s0246")}</span></div>
      </aside>

      <section className="workbench">
        <header className="topbar">
          <div className="topbar-heading"><p className="eyebrow">{tr("app.s0247")}</p><h1>{project?.title ?? tr("app.s0248")}</h1></div>
          <div className="command-bar" aria-label={tr("app.s0249")}>
            <StatusBadge tone={humanStateTone}>{humanState}</StatusBadge>
            <div className="command-history" aria-label={tr("app.s0250")}>
              <IconButton label={tr("app.s0251")} shortcut="Ctrl+Z" disabled={!project?.history.canUndo || Boolean(busy)} onClick={() => navigateHistory("undo")}><Undo2 size={15}/></IconButton>
              <IconButton label={tr("app.s0252")} shortcut="Ctrl+Shift+Z" disabled={!project?.history.canRedo || Boolean(busy)} onClick={() => navigateHistory("redo")}><Redo2 size={15}/></IconButton>
            </div>
            <label className="command-select"><span>{tr("app.transcription.language")}</span><select aria-label={tr("app.transcription.language")} value={transcriptionLanguage} onChange={(event) => selectTranscriptionLanguage(event.target.value as TranscriptionLanguage)}><option value="auto">{tr("app.transcription.auto")}</option><option value="en">{tr("app.transcription.english")}</option><option value="zh">{tr("app.transcription.chinese")}</option></select></label>
            <Button disabled={!capabilities.canTranscribe || Boolean(busy)} title={transcribeCapabilityTitle} onClick={transcribe}><RefreshCw size={15}/>{tr("app.s0253")}</Button>
            <div className="agent-command"><select aria-label={tr("app.workflow.label")} value={agentWorkflowKind} onChange={(event) => setAgentWorkflowKind(event.target.value as typeof agentWorkflowKind)}><option value="polish">{tr("app.workflow.polish")}</option><option value="proofread">{tr("app.workflow.proofread")}</option><option value="edit">{tr("app.workflow.edit")}</option><option value="translate">{tr("app.workflow.translate")}</option></select>{agentWorkflowKind === "translate" && <select className="agent-language" aria-label={tr("app.workflow.targetLanguage")} value={subtitleLanguage} onChange={(event) => setSubtitleLanguage(event.target.value)}><option value="en">EN</option><option value="zh">ZH</option><option value="ja">JA</option><option value="ko">KO</option></select>}<Button variant="agent" disabled={!capabilities.canCreateAgentTask || Boolean(busy)} title={agentCapabilityTitle} onClick={createAgentTask}><Bot size={15}/>{tr("app.s0255")}</Button></div>
            <div className="command-more" ref={commandMoreRef}>
              <IconButton label={tr("app.s0256")} onClick={() => setShowMoreMenu((current) => !current)}><MoreHorizontal size={17}/></IconButton>
              {showMoreMenu && <div className="command-menu" role="menu">
                <button role="menuitem" disabled={!project?.transcript.words.length || Boolean(busy)} onClick={() => { setShowMoreMenu(false); void detectSuggestions(); }}><Scissors size={14}/>{tr("app.s0254")}</button>
                <button role="menuitem" disabled={!capabilities.canPreparePreview || Boolean(busy)} title={mediaCapabilityTitle} onClick={() => { setShowMoreMenu(false); void preparePreview(); }}><Film size={14}/>{tr("app.s0257")}</button>
                <button role="menuitem" disabled={!capabilities.canRelinkMedia || Boolean(busy)} onClick={() => { setShowMoreMenu(false); void relinkMedia(); }}><Link2 size={14}/>{tr("app.s0258")}</button>
              </div>}
            </div>
            <div className="export-split">
              <Button ref={exportButtonRef} variant="primary" className="export-main" disabled={!capabilities.canExportVideo || Boolean(busy) || selectedTranslationPending || Boolean(activeExport && ["queued", "running"].includes(activeExport.status))} title={mediaCapabilityTitle} onClick={exportVideo}><Download size={15}/>{tr("app.s0259")}</Button>
              <button className="export-settings" aria-label={tr("app.s0260")} title={tr("app.s0261")} disabled={!project} onClick={() => setShowExportPanel(true)}><ChevronDown size={15}/></button>
            </div>
          </div>
        </header>

        {(notice || error) && <div className={`notice ${error ? "error" : ""}`} role="status" aria-live="polite">{error && <CircleAlert size={15}/>}<span>{error ? tr("app.error.unknownSummary") : notice}</span>{error && <details><summary>{tr("app.error.technicalDetails")}</summary><code>{error}</code></details>}{error && <button className="notice-action" onClick={() => void initialize()}>{tr("app.s0262")}</button>}<button aria-label={tr("app.s0263")} title={tr("app.s0263")} onClick={() => { setNotice(null); setError(null); }}>×</button></div>}
        {busy && <div className="progress-strip" role="status" aria-live="polite"><LoaderCircle size={14} className="spin"/>{busy}</div>}
        {activeExport && ["queued", "running"].includes(activeExport.status) && <div className="export-progress" role="status"><Film size={15}/><span>{tr("app.s0264") + " "}{Math.round(activeExport.progress * 100)}%</span><progress value={activeExport.progress} max={1}/><button onClick={cancelExport}>{tr("app.s0265")}</button></div>}
        {activeExport && ["failed", "interrupted"].includes(activeExport.status) && <div className="export-progress interrupted" role="status"><CircleAlert size={15}/><span>{activeExport.status === "interrupted" ? tr("app.s0266") : tr("app.s0267")}</span><JobFailureDetails context="export" status={activeExport.status} errorCode={activeExport.errorCode} errorMessage={activeExport.errorMessage}/><button onClick={retryExport}>{tr("app.s0269")}</button></div>}
        {sourceJob && !showSourceImport && ["queued", "running", "finalizing"].includes(sourceJob.status) && <div className="source-progress" role="status"><Link2 size={15}/><span><strong>{sourceStatusLabel(sourceJob.status)} · {Math.round(sourceJob.progress * 100)}%</strong><small>{sourceJob.title}</small></span><progress value={sourceJob.progress} max={1}/><button onClick={() => setShowSourceImport(true)}>{tr("app.s0270")}</button></div>}
        {autoWorkflow && <section className={`auto-progress ${autoWorkflow.status}`} aria-label={tr("app.s0271")}>
          <Sparkles size={17}/>
          <div className="auto-progress-copy"><strong>{autoStatusLabel(autoWorkflow.status)} · {autoStageLabel(autoWorkflow.currentStage)}</strong>{["failed", "interrupted"].includes(autoWorkflow.status) ? <JobFailureDetails context="auto" status={autoWorkflow.status} errorCode={autoWorkflow.errorCode} errorMessage={autoWorkflow.errorMessage}/> : <small>{!autoWorkflow.projectId && ["needs_agent", "needs_review"].includes(autoWorkflow.status) ? tr("app.s0272") : autoWorkflow.status === "needs_agent" ? tr("app.s0273") : autoWorkflow.status === "needs_review" ? tr("app.s0274") : autoWorkflow.outputPath}</small>}</div>
          <progress value={autoWorkflow.progress} max={1} aria-label={tr("app.s0275")}/>
          <span className="auto-progress-percent">{Math.round(autoWorkflow.progress * 100)}%</span>
          <div className="auto-progress-actions">
            {autoWorkflow.projectId && ["needs_agent", "needs_review"].includes(autoWorkflow.status) && <button onClick={() => void openAutoProject()}>{tr("app.s0276")}</button>}
            {["queued", "running", "needs_agent", "needs_review", "failed", "interrupted"].includes(autoWorkflow.status) && <button disabled={Boolean(autoBusy)} onClick={() => void cancelAutoWorkflow()}>{tr("app.s0277")}</button>}
            {autoWorkflow.status === "needs_review" && <button className="primary" disabled={Boolean(autoBusy)} onClick={() => void continueAutoWorkflow()}>{tr("app.s0278")}</button>}
            {["failed", "interrupted"].includes(autoWorkflow.status) && <button className="primary" disabled={Boolean(autoBusy)} onClick={() => void continueAutoWorkflow()}>{tr("app.s0279")}</button>}
            {["completed", "cancelled"].includes(autoWorkflow.status) && <button onClick={() => setShowAutoWorkflow(true)}>{tr("app.s0280")}</button>}
          </div>
          {autoError && <JobFailureDetails className="auto-progress-error" context="auto" status="failed" errorMessage={autoError}/>}
        </section>}

        {!project ? (<section className="welcome-card">
            <div className="welcome-icon"><FileVideo2 size={30}/></div>
            <p className="eyebrow">{tr("app.s0281")}</p><h2>{tr("app.s0282")}</h2>
            <p>{tr("app.s0283")}</p>
            <RuntimeChecklist runtime={runtime} modelPath={modelPath} onChooseModel={chooseModel} compact/>
            <div className="welcome-actions"><button className="button primary" onClick={() => setShowAutoWorkflow(true)}><Sparkles size={16}/>{tr("app.s0236")}</button><button className="button quiet" onClick={importMedia}><FolderPlus size={16}/>{tr("app.s0284")}</button><button className="button quiet" onClick={() => setShowSourceImport(true)}><Link2 size={16}/>{tr("app.s0285")}</button></div>
          </section>) : (<>
            <section className="stage-grid">
              <article className="video-panel">
                <div className="video-frame">
                  {mediaUrl ? <video key={project.id} ref={videoRef} src={mediaUrl} controls preload="metadata" onLoadedMetadata={handleVideoLoadedMetadata} onPlay={() => setPlayback((current) => ({ ...current, playing: true }))} onPause={() => setPlayback((current) => ({ ...current, playing: false }))} onTimeUpdate={handleVideoTimeUpdate}/> : <div className="video-placeholder"><Play size={30}/><span>{tr("app.s0286")}</span></div>}
                  {showSubtitleSafeArea && <div className="subtitle-safe-area" aria-label={tr("app.s0287")} style={{ inset: `${project.subtitleStyle.safeMarginPercent}% 6%` }}/>}
                  {selected && captionPrimaryText && <div className={`caption-overlay ${project.subtitleStyle.position}`} data-preset={project.subtitleStyle.preset} data-position={project.subtitleStyle.position} data-outline-width={project.subtitleStyle.outlineWidth} style={captionPreviewStyle}>
                    <span className="caption-primary">{captionPrimaryText}</span>
                    {captionSecondaryText && <span className="caption-secondary" style={{ color: project.subtitleStyle.secondaryColor, fontSize: `${Math.max(12, Math.round(project.subtitleStyle.secondaryFontSize * 0.36))}px` }}>{captionSecondaryText}</span>}
                  </div>}
                </div>
                <div className="transport-summary"><Clock3 size={14}/><span>{selected ? `${formatTime(selected.start)} — ${formatTime(selected.end)}` : tr("app.s0288")}</span><span className="playback-state" role="status" aria-live="polite" aria-label={tr("app.playback.status")}>{playback.playing ? tr("app.playback.playing") : tr("app.playback.paused")} · {formatTime(playback.currentTime)} / {formatTime(playback.duration || project.media.durationSeconds || 0)}</span><button className="relink-media" onClick={relinkMedia}>{tr("app.s0258")}</button><span className="shortcut-hint">{tr("app.s0289")}</span><span className="spacer"/><span>{tr("app.composite.timelineSummary", { output: formatTime(project.timeline.outputDuration), source: formatTime(project.timeline.sourceDuration) })}</span></div>
                {audioRisks.length > 0 && <div className="audio-risk-strip" role="status"><CircleAlert size={14}/><strong>{tr("app.composite.audioRiskCount", { count: audioRisks.length })}</strong><span>{audioRiskLabel(audioRisks[0].kind)} · {formatTime(audioRisks[0].start)}</span><button onClick={() => locateAudioRisk(audioRisks[0])}>{tr("app.s0293")}</button></div>}
              </article>

              <aside className="review-panel">
                <div className="section-title"><div><p className="eyebrow">{tr("app.s0294")}</p><h2>{tr("app.s0295")}</h2></div>{actionableReviewCount > 0 && <span className="state-count" aria-label={tr("app.s0296", { "0": actionableReviewCount })}>{actionableReviewCount}</span>}</div>
                <div className="review-panel-scroll" role="region" aria-label={tr("app.s0297")} tabIndex={0}>
                  {workerTask && <section className="codex-worker-status" aria-label={workerTask.status === "queued" ? tr("app.worker.waiting") : tr("app.worker.connected")}><header><Bot size={14}/><strong>{workerTask.status === "queued" ? tr("app.worker.waiting") : tr("app.worker.connected")}</strong></header><p>{tr("app.worker.privacy")}</p><label><span>{tr("app.worker.claimCommand")}</span><code>{workerClaimCommand}</code><button type="button" onClick={() => void navigator.clipboard?.writeText(workerClaimCommand)}>{tr("app.worker.copy")}</button></label>{workerContinueCommand && <label><span>{tr("app.worker.continueCommand")}</span><code>{workerContinueCommand}</code><button type="button" onClick={() => void navigator.clipboard?.writeText(workerContinueCommand)}>{tr("app.worker.copy")}</button></label>}</section>}
                  {orderedPatchSets.map((set) => <section className="patch-set" key={set.id}>
                    <header><span>{set.kind}{set.language ? ` · ${set.language.toUpperCase()}` : ""}</span>{set.items.length > 1 && <div><button onClick={() => reviewAll(set.taskId, "keep")}>{tr("app.s0298")}</button><button onClick={() => reviewAll(set.taskId, "apply")}>{tr("app.s0299")}</button></div>}</header>
                    {set.items.map((item) => <PatchReviewCard key={item.id} item={item} onReview={(action) => reviewPatch(item.id, action)} onSelect={() => { const segment = project.transcript.segments.find((candidate) => candidate.id === item.segmentId); if (segment)
                selectSegment(segment); }}/>)}
                  </section>)}
                  {pendingEdits.map((edit) => <article className="review-item" key={edit.id}><span className="review-tag">{tr("app.composite.reviewSuggestion", { kind: cutSuggestionLabel(edit.suggestion?.suggestionType) })}</span><strong>{editReasonLabel(edit)}</strong><p>{edit.suggestion ? tr("app.composite.suggestionEvidence", { range: `${formatTime(edit.start)} — ${formatTime(edit.end)}`, confidence: Math.round(edit.suggestion.confidence * 100) }) : `${formatTime(edit.start)} — ${formatTime(edit.end)} ${tr("app.s0302")}`}</p><div className="cut-actions"><button onClick={() => selectSegment(project.transcript.segments.find((segment) => segment.id === edit.segmentId)!)}>{tr("app.s0303")}</button>{edit.kind === "word_cut" && <button onClick={() => previewCut(edit.id)}><Headphones size={11}/>{tr("app.s0304")}</button>}<button onClick={() => updateCut(edit.id, "apply")}>{tr("app.s0305")}</button></div></article>)}
                  {audioRisks.map((risk, index) => <article className="review-item audio-risk-item" key={`${risk.kind}-${risk.start}-${index}`}><span className="review-tag warning"><CircleAlert size={12}/>{tr("app.s0306")}</span><strong>{audioRiskLabel(risk.kind)}</strong><p>{tr("app.composite.audioRiskEvidence", { range: `${formatTime(risk.start)} — ${formatTime(risk.end)}`, measured: risk.measuredValue, threshold: risk.threshold, unit: audioUnitLabel(risk.unit) })}</p><button onClick={() => locateAudioRisk(risk)}>{tr("app.s0309")}</button></article>)}
                  {audioAnalysisJob && ["failed", "interrupted"].includes(audioAnalysisJob.status) && <article className="review-item task-item failure"><span className="review-tag failure"><CircleAlert size={12}/>{tr("app.s0310")}{audioAnalysisJob.status === "interrupted" ? tr("app.s0018") : tr("app.s0311")}</span><strong>{tr("app.s0312")}</strong><JobFailureDetails context="audio" status={audioAnalysisJob.status} errorCode={audioAnalysisJob.errorCode} errorMessage={audioAnalysisJob.errorMessage}/><button onClick={resumeAudioAnalysis}><RefreshCw size={11}/>{tr("app.s0279")}</button></article>}
                  {audioAnalysisJob && ["queued", "running"].includes(audioAnalysisJob.status) && <article className="review-item task-item processing"><span className="review-tag info"><Activity size={12}/>{tr("app.s0314")}</span><strong>{tr("app.s0315")}</strong><p>{Math.round(audioAnalysisJob.progress * 100)}{tr("app.s0316")}</p><button disabled={Boolean(audioAnalysisJob.cancelRequestedAt)} onClick={cancelAudioAnalysis}>{audioAnalysisJob.cancelRequestedAt ? tr("app.s0317") : tr("app.s0318")}</button></article>}
                  {projectSpeakerJob && ["failed", "interrupted"].includes(projectSpeakerJob.status) && <article className="review-item task-item failure"><span className="review-tag failure"><CircleAlert size={12}/>{tr("app.s0319")}{projectSpeakerJob.status === "interrupted" ? tr("app.s0018") : tr("app.s0311")}</span><strong>{tr("app.s0320")}</strong><JobFailureDetails context="speaker" status={projectSpeakerJob.status} errorCode={projectSpeakerJob.errorCode} errorMessage={projectSpeakerJob.errorMessage}/><button onClick={resumeSpeakerJob}><RefreshCw size={11}/>{tr("app.s0279")}</button></article>}
                  {projectSpeakerJob && ["queued", "running"].includes(projectSpeakerJob.status) && <article className="review-item task-item processing"><span className="review-tag info"><Users size={12}/>{tr("app.s0314")}</span><strong>{tr("app.s0322")}</strong><p>{projectSpeakerJob.stage} · {Math.round(projectSpeakerJob.progress * 100)}{tr("app.s0323")}</p><button onClick={cancelSpeakerJob}>{tr("app.s0318")}</button></article>}
                  {failedTasks.map((task) => <article className="review-item task-item failure" key={task.id}><span className="review-tag failure"><CircleAlert size={12}/>Agent {task.status === "interrupted" ? tr("app.s0018") : tr("app.s0324")}</span><strong>{task.kind} · {Math.round(task.progress * 100)}%</strong><JobFailureDetails context="agent" status={task.status} errorCode={task.errorCode} errorMessage={task.errorMessage}/><button onClick={() => updateTask(task.id, "retry")}><RefreshCw size={11}/>{tr("app.s0325")}</button></article>)}
                  {processingTasks.map((task) => <article className="review-item task-item processing" key={task.id}><span className="review-tag agent"><Bot size={12}/>{task.status === "queued" ? tr("app.s0326") : tr("app.s0327")}</span><strong>{task.kind}</strong><p>{Math.round(task.progress * 100)}%</p><button onClick={() => updateTask(task.id, "cancel")}>{tr("app.s0328")}</button></article>)}
                  {actionableReviewCount === 0 && processingTasks.length === 0 && !audioAnalysisJob?.status.match(/queued|running|failed|interrupted/) && !projectSpeakerJob?.status.match(/queued|running|failed|interrupted/) && <div className="all-clear"><Check size={20}/><span>{tr("app.s0329")}</span></div>}
                  {recentTasks.length > 0 && <details className="review-history"><summary>{tr("app.s0330") + " "}{recentTasks.length}</summary><div>{recentTasks.map((task) => <article className="review-item recent" key={task.id}><span className="review-tag success"><Check size={12}/>{task.status === "completed" ? tr("app.s0032") : tr("app.s0017")}</span><strong>{task.kind}</strong><p>{Math.round(task.progress * 100)}%</p></article>)}</div></details>}
                  <div className="review-principle"><Sparkles size={16}/><span>{tr("app.s0331")}</span></div>
                </div>
              </aside>
            </section>

            <section className="editor-grid">
              <article className="transcript-panel">
                <header className="panel-header"><div><p className="eyebrow">{tr("app.s0332")}</p><h2>{tr("app.s0253")}</h2></div><div className="find-replace"><button ref={subtitleImportButtonRef} className="subtitle-import-command" disabled={Boolean(busy)} onClick={openSubtitleImport}><FileText size={12}/>{tr("app.s0333")}</button><button className="detect-suggestions" disabled={!project.transcript.words.length || Boolean(busy)} onClick={detectSuggestions}><Scissors size={12}/>{tr("app.s0334")}</button><label className="search"><Search size={14}/><input ref={searchInputRef} value={search} onChange={(event) => { setSearch(event.target.value); setEmptyReplacementConfirmed(false); }} placeholder={tr("app.s0335")} title="Ctrl+F"/></label><input ref={replacementInputRef} aria-label={tr("app.s0336")} value={replacement} onChange={(event) => { setReplacement(event.target.value); setEmptyReplacementConfirmed(false); }} placeholder={tr("app.s0336")} title="Ctrl+H"/>{search && <span className="replace-match-count">{tr("app.replace.matches", { count: replaceMatchCount })}</span>}{search && !replacement && <label className="replace-empty-confirm"><input type="checkbox" checked={emptyReplacementConfirmed} onChange={(event) => setEmptyReplacementConfirmed(event.target.checked)}/><span>{tr("app.replace.confirmDelete")}</span></label>}<button disabled={!search || (!replacement && !emptyReplacementConfirmed) || Boolean(busy)} onClick={replaceAll}>{tr("app.s0337")}</button></div></header>
                <div className="transcript-meta"><span>{tr("app.s0338")}</span><span>{tr("app.composite.transcriptStats", { language: project.transcript.sourceLanguage.toUpperCase(), segments: segmentCountLabel(project.transcript.segments.length), words: wordCountLabel(project.transcript.words.length) })}</span></div>
                <section className="subtitle-workbench-toolbar" aria-label={tr("app.s0341")}>
                  <div className="subtitle-selection-summary"><ListChecks size={15}/><span><strong>{selectedScopeLabel}</strong><small>{tr("app.s0342")}</small></span></div>
                  <div className="subtitle-selection-controls">
                    <button aria-label={tr("app.s0343")} title={tr("app.s0344")} disabled={!selectedId || project.transcript.segments[0]?.id === selectedId || Boolean(busy)} onClick={() => moveSegmentSelection(-1)}><ChevronUp size={14}/></button>
                    <button aria-label={tr("app.s0345")} title={tr("app.s0346")} disabled={!selectedId || project.transcript.segments.at(-1)?.id === selectedId || Boolean(busy)} onClick={() => moveSegmentSelection(1)}><ChevronDown size={14}/></button>
                    <button className="selection-scope" disabled={!filteredSegments.length || Boolean(busy)} onClick={() => {
                if (allVisibleSegmentsSelected && selected) {
                    setSelectedSegmentIds([selected.id]);
                    setSelectionAnchorId(selected.id);
                }
                else {
                    const ids = filteredSegments.map((segment) => segment.id);
                    setSelectedSegmentIds(ids);
                    setSelectedId(filteredSegments[0]?.id ?? null);
                    setSelectionAnchorId(filteredSegments[0]?.id ?? null);
                }
            }}>{allVisibleSegmentsSelected ? tr("app.s0347") : tr("app.s0348", { "0": filteredSegments.length })}</button>
                  </div>
                  <div className="subtitle-structure-actions">
                    <button disabled={selectedSegments.length !== 1 || Array.from(selectedSegments[0]?.text ?? "").length < 2 || Boolean(busy)} title={tr("app.s0349")} onClick={() => openStructureEdit("split")}><Scissors size={13}/>{tr("app.s0350")}</button>
                    <button disabled={!mergeCandidatesAdjacent || Boolean(busy)} title={tr("app.s0351")} onClick={() => openStructureEdit("merge")}><Link2 size={13}/>{tr("app.s0352")}</button>
                    <button disabled={selectedSegments.length !== 1 || Boolean(busy)} title={tr("app.s0353")} onClick={() => openStructureEdit("timing")}><Clock3 size={13}/>{tr("app.s0354")}</button>
                    <button disabled={!selectedSegments.length || Boolean(busy)} title={tr("app.s0355")} onClick={() => openStructureEdit("offset")}><MoveHorizontal size={13}/>{tr("app.s0356")}</button>
                  </div>
                </section>
                <section className={`subtitle-quality-summary ${project.subtitleQuality.status}`} aria-label={tr("app.s0357")}>
                  <div className="subtitle-quality-state">{project.subtitleQuality.status === "good" ? <Check size={15}/> : <CircleAlert size={15}/>}<span><strong>{subtitleQualityStatusLabel(project.subtitleQuality)}</strong><small>{project.subtitleQuality.errorCount}{tr("app.s0358") + " "}{project.subtitleQuality.warningCount}{tr("app.s0359")}</small></span></div>
                  <div className="subtitle-quality-filters" aria-label={tr("app.s0360")}><button className={qualityFilter === "all" ? "active" : ""} onClick={() => setQualityFilter("all")}>{tr("app.s0361")}</button><button className={qualityFilter === "error" ? "active" : ""} disabled={!project.subtitleQuality.errorCount} onClick={() => setQualityFilter("error")}>{tr("app.s0362") + " "}{project.subtitleQuality.errorCount}</button><button className={qualityFilter === "warning" ? "active" : ""} disabled={!project.subtitleQuality.warningCount} onClick={() => setQualityFilter("warning")}>{tr("app.s0363") + " "}{project.subtitleQuality.warningCount}</button></div>
                  {visibleQualityIssues.length > 0 && <div className="subtitle-quality-issues">{visibleQualityIssues.slice(0, 4).map((issue) => <button className={issue.severity} key={issue.id} onClick={() => locateSubtitleIssue(issue)}><CircleAlert size={12}/><span><strong>{subtitleIssueLabel(issue.kind)}</strong><small>{formatTime(issue.start)}{tr("app.s0364")}</small></span></button>)}</div>}
                </section>
                <div className="segment-list" aria-label={tr("app.s0365")}>
                  {filteredSegments.map((segment) => { const association = associationBySegment.get(segment.id); return <SegmentRow key={segment.id} segment={segment} speaker={association ? speakerById.get(association.speakerId) : undefined} speakerManual={association?.source === "manual"} selected={selectedSegmentIds.includes(segment.id)} active={segment.id === selectedId} translation={translation?.[1]} onSelect={(mode) => selectSegmentInWorkbench(segment, mode)} onSave={(text) => editSegment(segment, text)}/>; })}
                  {!filteredSegments.length && <p className="empty-list">{project.transcript.segments.length ? tr("app.s0366") : tr("app.s0367")}</p>}
                </div>
              </article>

              <aside className="context-panel">
                <div className="inspector-tabs" role="tablist" aria-label={tr("app.s0368")}>
                  {inspectorTabs.map((tab) => <button id={`inspector-tab-${tab}`} key={tab} role="tab" aria-controls={`inspector-panel-${tab}`} aria-selected={inspectorTab === tab} tabIndex={inspectorTab === tab ? 0 : -1} className={inspectorTab === tab ? "active" : ""} onKeyDown={(event) => changeInspectorTabFromKeyboard(event, tab)} onClick={() => setInspectorTab(tab)}>{tr(`app.inspector.${tab}`)}</button>)}
                </div>
                {inspectorTab === "segment" && <div id="inspector-panel-segment" aria-labelledby="inspector-tab-segment" className="inspector-view" role="tabpanel">
                  <p className="eyebrow">{tr("app.s0368")}</p><h2>{selected?.text ?? tr("app.s0369")}</h2>
                  <dl><div><dt>{tr("app.s0354")}</dt><dd>{selected ? `${formatTime(selected.start)} — ${formatTime(selected.end)}` : "—"}</dd></div><div><dt>{tr("app.s0370")}</dt><dd>{selected?.confidence == null ? tr("app.s0371") : `${Math.round(selected.confidence * 100)}%`}</dd></div><div><dt>{tr("app.s0372")}</dt><dd className={translation?.[1].status === "stale" ? "stale" : ""}>{translation ? (translation[1].status === "stale" ? tr("app.s0373") : tr("app.s0374", { "0": translation[0].toUpperCase() })) : tr("app.s0375")}</dd></div></dl>
                  {selectedWords.length > 0 && <section className="word-evidence" aria-label={tr("app.s0376")}><div className="word-heading"><div><p className="eyebrow">{tr("app.s0376")}</p><small>{tr("app.s0377")}</small></div>{activeWordRange && <button className="clear-range" onClick={() => setWordRange(null)}>{tr("app.s0378")}</button>}</div><div className="word-tokens">{selectedWords.map((word, index) => <button className={activeWordRange && index >= activeWordRange.start && index <= activeWordRange.end ? "selected" : ""} key={word.id} onClick={() => selectWordForCut(index)} title={`${formatTime(word.start)} — ${formatTime(word.end)}${word.confidence == null ? "" : ` · ${Math.round(word.confidence * 100)}%`}`}>{word.text}</button>)}</div>{activeWordRange && <div className="word-cut-controls"><label>{tr("app.s0379")}<input aria-label={tr("app.s0380")} type="range" min="0" max={selectedWords.length - 1} value={activeWordRange.start} onChange={(event) => setWordRange({ ...activeWordRange, start: Math.min(Number(event.target.value), activeWordRange.end) })}/><small>{selectedWords[activeWordRange.start]?.text}</small></label><label>{tr("app.s0381")}<input aria-label={tr("app.s0382")} type="range" min="0" max={selectedWords.length - 1} value={activeWordRange.end} onChange={(event) => setWordRange({ ...activeWordRange, end: Math.max(Number(event.target.value), activeWordRange.start) })}/><small>{selectedWords[activeWordRange.end]?.text}</small></label><label className="padding-select">{tr("app.s0383")}<select aria-label={tr("app.s0383")} value={cutPadding} onChange={(event) => setCutPadding(Number(event.target.value) as 30 | 100 | 200)}><option value="30">30 ms</option><option value="100">100 ms</option><option value="200">200 ms</option></select></label><button className="create-word-cut" disabled={Boolean(busy)} onClick={createWordCut}><Scissors size={12}/>{tr("app.s0384")}</button></div>}</section>}
                </div>}
                {inspectorTab === "analysis" && <div id="inspector-panel-analysis" aria-labelledby="inspector-tab-analysis" className="inspector-view" role="tabpanel"><SpeechInsightsPanel insights={project.speechInsights} onLocateEvidence={locateSpeechEvidence} onLocatePause={locateSpeechPause}/><AudioQualityPanel job={audioAnalysisJob} onStart={startAudioAnalysis} onCancel={cancelAudioAnalysis} onResume={resumeAudioAnalysis} onLocate={locateAudioRisk} disabled={!capabilities.canAnalyzeAudio || Boolean(busy)}/><SpeakerTrackPanel packageStatus={speakerPackage} track={speakerTrack} job={projectSpeakerJob} selectedSegmentId={selectedId} disabled={Boolean(busy)} onOpenRuntime={() => setShowRuntime(true)} onAnalyze={startSpeakerAnalysis} onCancel={cancelSpeakerJob} onResume={resumeSpeakerJob} onRename={renameSpeaker} onMerge={mergeSpeaker} onAssign={assignSpeaker}/></div>}
                {inspectorTab === "history" && <div id="inspector-panel-history" aria-labelledby="inspector-tab-history" className="inspector-view" role="tabpanel"><div className="version-block"><div className="section-title"><div><p className="eyebrow">{tr("app.s0385")}</p><h2>{tr("app.s0386")}</h2></div><div className="history-controls"><button aria-label={tr("app.s0251")} title="Ctrl+Z" disabled={!project.history.canUndo || Boolean(busy)} onClick={() => navigateHistory("undo")}><Undo2 size={14}/></button><button aria-label={tr("app.s0252")} title="Ctrl+Shift+Z / Ctrl+Y" disabled={!project.history.canRedo || Boolean(busy)} onClick={() => navigateHistory("redo")}><Redo2 size={14}/></button><History size={16}/></div></div>{project.versions.slice().reverse().slice(0, 4).map((version) => <button className="version-row" key={version.id} onClick={() => restoreVersion(version.id)}><span><strong>{versionReasonLabel(version.reason)}</strong><small>{new Date(version.createdAt).toLocaleString(uiLocale)}</small></span><RotateCcw size={14}/></button>)}</div></div>}
              </aside>
            </section>

            <section className="timeline-panel">
              <div className="section-title"><div><p className="eyebrow">{tr("app.s0387")}</p><h2>{tr("app.s0388")}</h2></div><span className="timeline-note">{tr("app.s0389")}</span></div>
              {waveformUrl && <img className="waveform" src={waveformUrl} alt={tr("app.s0390")}/>}
              <div className="timeline-track">{project.transcript.segments.map((segment) => { const edit = project.edits.find((candidate) => candidate.segmentId === segment.id && ["suggested", "proposed", "applied"].includes(candidate.status)); const association = associationBySegment.get(segment.id); const speaker = association ? speakerById.get(association.speakerId) : undefined; return <div className="timeline-segment-shell" key={segment.id} style={{ flexGrow: Math.max(1, segment.end - segment.start) }}><button className={`timeline-segment ${edit && ["suggested", "proposed"].includes(edit.status) ? "suggested" : ""} ${edit?.status === "applied" ? "applied" : ""} ${selectedSegmentIds.includes(segment.id) ? "selected" : ""} ${selectedId === segment.id ? "active" : ""}`} onClick={() => selectSegment(segment)} title={`${speaker ? `${speaker.label} · ` : ""}${segment.text}`}>{speaker && <i className={`speaker-color speaker-${speaker.colorIndex % 6}`}/>}{segment.text}</button>{edit?.status === "applied" && <button className="timeline-restore" onClick={() => void updateCut(edit.id, "restore")}><Scissors size={11}/>{tr("app.s0391")}</button>}</div>; })}</div>
            </section>
          </>)}
      </section>
      {showExportPanel && project && <aside ref={exportPanelRef} className="export-panel" aria-label={tr("app.s0392")}>
        <header className="export-panel-header"><div><p className="eyebrow">{tr("app.s0393")}</p><h2>{tr("app.s0392")}</h2></div><IconButton label={tr("app.s0394")} onClick={() => setShowExportPanel(false)}><X size={17}/></IconButton></header>
        <div className="export-panel-body">
          <section className="export-group" aria-labelledby="export-canvas-heading">
            <div><h3 id="export-canvas-heading">{tr("app.s0395")}</h3><p>{tr("app.s0396")}</p></div>
            <label><span>{tr("app.s0397")}</span><select aria-label={tr("app.s0397")} value={project.canvasSettings.aspectRatio} onChange={(event) => void changeCanvas({ ...project.canvasSettings, aspectRatio: event.target.value as CanvasSettings["aspectRatio"] })}><option value="source">{tr("app.s0398")}</option><option value="9:16">{tr("app.s0399")}</option></select></label>
            <label><span>{tr("app.s0400")}</span><select aria-label={tr("app.s0400")} disabled={project.canvasSettings.aspectRatio === "source"} value={project.canvasSettings.framing} onChange={(event) => void changeCanvas({ ...project.canvasSettings, framing: event.target.value as CanvasSettings["framing"] })}><option value="contain-blur">{tr("app.s0401")}</option><option value="cover-center">{tr("app.s0402")}</option></select></label>
          </section>
          <section className="export-group" aria-labelledby="export-subtitle-heading">
            <div><h3 id="export-subtitle-heading">{tr("app.s0175")}</h3><p>{tr("app.s0403")}</p></div>
            <label><span>{tr("app.s0404")}</span><select aria-label={tr("app.s0405")} value={subtitleMode} onChange={(event) => setSubtitleMode(event.target.value as typeof subtitleMode)}><option value="source">{tr("app.s0406")}</option><option value="translated">{tr("app.s0407")}</option><option value="bilingual">{tr("app.s0408")}</option></select></label>
            <label><span>{tr("app.s0409")}</span><select aria-label={tr("app.s0409")} disabled={!translationLanguageOptions.length} value={selectedSubtitleLanguage} onChange={(event) => setSubtitleLanguage(event.target.value)}>{translationLanguageOptions.length ? translationLanguageOptions.map((language) => <option value={language} key={language}>{language.toUpperCase()}{translationLanguages.includes(language) ? "" : tr("app.s0410")}</option>) : <option value="">{tr("app.s0411")}</option>}</select></label>
            <label><span>{tr("app.s0412")}</span><select aria-label={tr("app.s0413")} value={exportFormat} onChange={(event) => setExportFormat(event.target.value as typeof exportFormat)}><option value="srt">SRT</option><option value="vtt">VTT</option><option value="ass">ASS</option><option value="markdown">Markdown</option></select></label>
            {selectedTranslationPending && <p className="export-warning"><CircleAlert size={14}/>{selectedSubtitleLanguage.toUpperCase()}{tr("app.s0414")}</p>}
          </section>
          <section className="export-group subtitle-style-group" aria-labelledby="export-subtitle-style-heading">
            <div><h3 id="export-subtitle-style-heading">{tr("app.s0415")}</h3><p>{tr("app.s0416")}</p></div>
            <label><span>{tr("app.s0417")}</span><select aria-label={tr("app.s0418")} disabled={Boolean(busy)} value={project.subtitleStyle.preset} onChange={(event) => void changeSubtitleStyle(event.target.value as Project["subtitleStyle"]["preset"], project.subtitleStyle.position)}><option value="compact">{tr("app.s0419")}</option><option value="standard">{tr("app.s0420")}</option><option value="emphasis">{tr("app.s0421")}</option></select></label>
            <label><span>{tr("app.s0422")}</span><select aria-label={tr("app.s0422")} disabled={Boolean(busy)} value={project.subtitleStyle.position} onChange={(event) => void changeSubtitleStyle(project.subtitleStyle.preset, event.target.value as Project["subtitleStyle"]["position"])}><option value="bottom">{tr("app.s0423")}</option><option value="center">{tr("app.s0424")}</option></select></label>
            <label className="subtitle-safe-toggle"><input type="checkbox" checked={showSubtitleSafeArea} onChange={(event) => setShowSubtitleSafeArea(event.target.checked)}/><span>{tr("app.s0425")}</span></label>
            <div className="subtitle-style-summary"><span><strong>{project.subtitleStyle.fontSize} px</strong><small>{tr("app.s0426")}</small></span><span><strong>{project.subtitleStyle.secondaryFontSize} px</strong><small>{tr("app.s0427")}</small></span><span><strong>{project.subtitleStyle.outlineWidth} px</strong><small>{tr("app.s0428")}</small></span><span><strong>{project.subtitleStyle.safeMarginPercent}%</strong><small>{tr("app.s0429")}</small></span></div>
          </section>
          <section className="export-safety"><ShieldCheck size={17}/><span><strong>{tr("app.s0430")}</strong><small>{tr("app.s0431")}</small></span></section>
        </div>
        <footer className="export-panel-actions">
          <Button disabled={Boolean(busy) || selectedTranslationPending} onClick={exportTranscript}><Download size={15}/>{tr("app.s0432")}</Button>
          <Button variant="primary" disabled={!capabilities.canExportVideo || Boolean(busy) || selectedTranslationPending || Boolean(activeExport && ["queued", "running"].includes(activeExport.status))} title={mediaCapabilityTitle} onClick={exportVideo}><Film size={15}/>{tr("app.s0259")}</Button>
        </footer>
      </aside>}
      {structureEditMode && project && <Dialog label={structureEditLabel(structureEditMode)} className="runtime-dialog subtitle-structure-dialog" onClose={() => { if (!structureBusy)
            setStructureEditMode(null); }}>
        <button autoFocus className="dialog-close" aria-label={tr("app.s0433", { "0": structureEditLabel(structureEditMode) })} title={tr("app.s0434")} disabled={structureBusy} onClick={() => setStructureEditMode(null)}><X size={18}/></button>
        <p className="eyebrow">{tr("app.s0435")}</p><h2>{structureEditLabel(structureEditMode)}</h2>
        <section className="subtitle-operation-scope" aria-label={tr("app.s0436")}>
          <header><ListChecks size={15}/><span><strong>{tr("app.s0437")}{selectedScopeLabel}</strong><small>{selectedSegments.length > 4 ? tr("app.s0438", { "0": selectedSegments.length }) : tr("app.s0439")}</small></span></header>
          <div>{selectedSegments.slice(0, 4).map((segment) => <span key={segment.id}><code>{formatTime(segment.start)}—{formatTime(segment.end)}</code><small>{segment.text}</small></span>)}</div>
        </section>
        {structureEditMode === "split" && selectedSegments[0] && <div className="subtitle-structure-form">
          <label><span>{tr("app.s0440")}</span><input type="number" min="1" max={Math.max(1, Array.from(selectedSegments[0].text).length - 1)} step="1" value={structureTextOffset} onChange={(event) => setStructureTextOffset(event.target.value)}/><small>{tr("app.s0441")}</small></label>
          <label><span>{tr("app.s0442")}</span><input type="number" min={selectedSegments[0].start} max={selectedSegments[0].end} step="0.001" value={structureStart} onChange={(event) => setStructureStart(event.target.value)}/><small>{tr("app.s0443")}</small></label>
          <div className="subtitle-split-preview" role="region" aria-label={tr("app.s0444")}><span><small>{tr("app.s0445")}</small>{Array.from(selectedSegments[0].text).slice(0, Number(structureTextOffset) || 0).join("")}</span><span><small>{tr("app.s0446")}</small>{Array.from(selectedSegments[0].text).slice(Number(structureTextOffset) || 0).join("")}</span></div>
          {!hasMeaningfulSubtitleText(splitLeftText) || !hasMeaningfulSubtitleText(splitRightText) ? <p className="source-error" role="alert">{tr("app.structure.splitMeaningful")}</p> : null}
        </div>}
        {structureEditMode === "merge" && <div className="subtitle-merge-preview" aria-label={tr("app.s0447")}><small>{tr("app.s0448")}</small><p>{selectedSegments.map((segment) => segment.text.trim()).join(" ")}</p></div>}
        {structureEditMode === "timing" && selectedSegments[0] && <div className="subtitle-structure-form timing">
          <label><span>{tr("app.s0449")}</span><input type="number" min="0" step="0.001" value={structureStart} onChange={(event) => setStructureStart(event.target.value)}/><small>{tr("app.s0450") + " "}{selectedSegments[0].start.toFixed(3)}{tr("app.s0037")}</small></label>
          <label><span>{tr("app.s0451")}</span><input type="number" min="0" max={project.media.durationSeconds ?? undefined} step="0.001" value={structureEnd} onChange={(event) => setStructureEnd(event.target.value)}/><small>{tr("app.s0450") + " "}{selectedSegments[0].end.toFixed(3)}{tr("app.s0037")}</small></label>
          {!timingChanged && <p className="source-error" role="alert">{tr("app.structure.timingUnchanged")}</p>}
        </div>}
        {structureEditMode === "offset" && <div className="subtitle-structure-form offset">
          <label><span>{tr("app.s0452")}</span><input type="number" step="0.001" value={structureDelta} onChange={(event) => setStructureDelta(event.target.value)}/><small>{tr("app.s0453")}</small></label>
          <p><MoveHorizontal size={14}/>{tr("app.s0454")}{formatTime(Math.max(0, (selectedSegments[0]?.start ?? 0) + (Number(structureDelta) || 0)))} — {formatTime(Math.max(0, (selectedSegments.at(-1)?.end ?? 0) + (Number(structureDelta) || 0)))}</p>
        </div>}
        <p className="subtitle-structure-impact"><History size={14}/>{tr("app.s0455")}</p>
        {structureError && <div className="source-error" role="alert"><CircleAlert size={15}/>{structureError}</div>}
        <button className="button primary full" disabled={structureSubmitDisabled} onClick={() => void applyStructureEdit()}>{structureBusy ? <><LoaderCircle className="spin" size={14}/>{tr("app.s0456")}</> : tr("app.s0457", { "0": structureEditMode === "offset" ? tr("app.s0458", { "0": selectedSegments.length }) : structureEditMode === "merge" ? tr("app.s0459") : structureEditMode === "split" ? tr("app.s0460") : tr("app.s0461") })}</button>
      </Dialog>}
      {showSubtitleImport && project && <Dialog label={tr("app.s0333")} className="runtime-dialog subtitle-import-dialog" onClose={() => setShowSubtitleImport(false)} returnFocusRef={subtitleImportButtonRef}>
        <button autoFocus className="dialog-close" aria-label={tr("app.s0462")} title={tr("app.s0463")} onClick={() => setShowSubtitleImport(false)}><X size={18}/></button>
        <p className="eyebrow">{tr("app.s0464")}</p><h2>{tr("app.s0465")}</h2>
        <p className="dialog-copy">{tr("app.s0466")}</p>
        <div className="subtitle-import-file"><span><small>{tr("app.s0467")}</small><strong title={subtitleImportPath}>{subtitleImportPath ? subtitleImportPath.split(/[\\/]/).at(-1) : tr("app.s0468")}</strong></span><button className="button quiet" disabled={Boolean(subtitleImportBusy)} onClick={() => void inspectSubtitleFile()}><FolderOpen size={14}/>{subtitleImportPath ? tr("app.s0469") : tr("app.s0470")}</button></div>
        {subtitleImportBusy && <div className="subtitle-import-progress" role="status"><LoaderCircle className="spin" size={14}/>{subtitleImportBusy}</div>}
        {subtitleImportPreview && <section className={`subtitle-import-preview ${subtitleImportPreview.quality.status}`} aria-label={tr("app.s0471")}>
          <header><span><small>{subtitleImportPreview.format.toUpperCase()} · SHA-256 {subtitleImportPreview.sha256.slice(0, 10)}…</small><strong>{tr("app.composite.subtitleSegments", { label: subtitleCountLabel(subtitleImportPreview.segmentCount) })}</strong></span>{subtitleImportPreview.quality.status === "good" ? <Check size={18}/> : <CircleAlert size={18}/>}</header>
          <div className="subtitle-import-quality"><strong>{subtitleQualityStatusLabel(subtitleImportPreview.quality)}</strong><span>{subtitleImportPreview.quality.errorCount}{tr("app.s0358") + " "}{subtitleImportPreview.quality.warningCount}{tr("app.s0359")}</span></div>
          {subtitleImportPreview.quality.issues.length > 0 && <div className="subtitle-import-issues">{subtitleImportPreview.quality.issues.slice(0, 5).map((issue) => <div className={issue.severity} key={issue.id}><CircleAlert size={12}/><span><strong>{subtitleIssueLabel(issue.kind)}</strong><small>{formatTime(issue.start)} — {formatTime(issue.end)}</small></span></div>)}</div>}
          <label className="subtitle-replace-confirm"><input type="checkbox" checked={subtitleReplaceConfirmed} disabled={!subtitleImportPreview.canImport || Boolean(subtitleImportBusy)} onChange={(event) => setSubtitleReplaceConfirmed(event.target.checked)}/><span>{tr("app.s0472")}</span></label>
          <button className="button primary full" disabled={!subtitleImportPreview.canImport || !subtitleReplaceConfirmed || Boolean(subtitleImportBusy)} onClick={() => void confirmSubtitleImport()}>{subtitleImportPreview.canImport ? tr("app.s0473") : tr("app.s0474")}</button>
          <p className="runtime-disclosure">{tr("app.s0475")}</p>
        </section>}
        {subtitleImportError && <div className="source-error" role="alert"><CircleAlert size={15}/>{subtitleImportError}</div>}
      </Dialog>}
      {showAutoWorkflow && <Dialog label={tr("app.s0476")} className="runtime-dialog auto-dialog" onClose={() => setShowAutoWorkflow(false)} returnFocusRef={autoButtonRef}><button autoFocus className="dialog-close" aria-label={tr("app.s0477")} title={tr("app.s0478")} onClick={() => setShowAutoWorkflow(false)}><X size={18}/></button><p className="eyebrow">{tr("app.s0479")}</p><h2>{tr("app.s0480")}</h2><p className="dialog-copy">{tr("app.s0481")}</p>
        <div className="auto-form">
          <label><span>{tr("app.s0482")}</span><select aria-label={tr("app.s0483")} value={autoInputKind} disabled={Boolean(autoBusy)} onChange={(event) => { setAutoInputKind(event.target.value as "local" | "url"); setAutoSourcePreview(null); setAutoAuthorized(false); setAutoError(null); }}><option value="local">{tr("app.s0484")}</option><option value="url">{tr("app.s0485")}</option></select></label>
          {autoInputKind === "local" ? <div className="auto-file-row"><span><small>{tr("app.s0486")}</small><strong title={autoMediaPath}>{autoMediaPath || tr("app.s0468")}</strong></span><button className="button quiet" disabled={Boolean(autoBusy)} onClick={() => void chooseAutoMedia()}><FolderOpen size={14}/>{tr("app.s0470")}</button></div> : <>
            <form className="source-form" onSubmit={(event) => { event.preventDefault(); void inspectAutoSource(); }}><label><span>{tr("app.s0487")}</span><input autoComplete="url" aria-label={tr("app.s0488")} placeholder="https://…" value={autoUrl} disabled={Boolean(autoBusy)} onChange={(event) => { setAutoUrl(event.target.value); setAutoSourcePreview(null); setAutoAuthorized(false); setAutoError(null); }}/></label><button className="button quiet" type="submit" disabled={Boolean(autoBusy) || !autoUrl.trim()}><Search size={14}/>{tr("app.s0489")}</button></form>
            {autoSourcePreview && <section className="source-preview auto-source-preview" aria-label={tr("app.s0490")}><header><span><small>{autoSourcePreview.extractor}</small><strong>{autoSourcePreview.title}</strong></span><ShieldCheck size={19}/></header><dl><div><dt>{tr("app.s0491")}</dt><dd>{formatTime(autoSourcePreview.durationSeconds)}</dd></div><div><dt>{tr("app.s0492")}</dt><dd>{autoSourcePreview.siteMediaId}</dd></div></dl><label className="source-consent"><input type="checkbox" checked={autoAuthorized} onChange={(event) => setAutoAuthorized(event.target.checked)}/><span>{tr("app.s0493")}</span></label></section>}
          </>}
          <div className="auto-file-row"><span><small>{tr("app.s0494")}</small><strong title={modelPath ?? undefined}>{modelPath ?? tr("app.s0468")}</strong></span><button className="button quiet" onClick={() => { setShowAutoWorkflow(false); setShowRuntime(true); }}>{tr("app.s0495")}</button></div>
          <div className="auto-options">
            <label><span>{tr("app.transcription.language")}</span><select aria-label={`${tr("app.transcription.language")} · ${tr("app.s0236")}`} value={transcriptionLanguage} disabled={Boolean(autoBusy)} onChange={(event) => selectTranscriptionLanguage(event.target.value as TranscriptionLanguage)}><option value="auto">{tr("app.transcription.auto")}</option><option value="en">{tr("app.transcription.english")}</option><option value="zh">{tr("app.transcription.chinese")}</option></select></label>
            <label className="auto-check"><input type="checkbox" checked={autoTranslate} onChange={(event) => { setAutoTranslate(event.target.checked); if (!event.target.checked)
            setAutoSubtitleMode("source"); }}/><span>{tr("app.s0496")}</span></label>
            <label><span>{tr("app.s0497")}</span><input aria-label={tr("app.s0498")} value={autoTranslationLanguage} disabled={!autoTranslate} onChange={(event) => setAutoTranslationLanguage(event.target.value)}/></label>
            <label><span>{tr("app.s0499")}</span><select aria-label={tr("app.s0500")} value={autoSubtitleMode} disabled={!autoTranslate} onChange={(event) => setAutoSubtitleMode(event.target.value as typeof autoSubtitleMode)}><option value="source">{tr("app.s0406")}</option><option value="translated">{tr("app.s0407")}</option><option value="bilingual">{tr("app.s0408")}</option></select></label>
            <label className="auto-check"><input type="checkbox" checked={autoBurnSubtitles} onChange={(event) => setAutoBurnSubtitles(event.target.checked)}/><span>{tr("app.s0501")}</span></label>
          </div>
          <button className="button primary full" disabled={Boolean(autoBusy) || !modelPath || (autoTranslate && !autoTranslationLanguage.trim()) || (autoInputKind === "local" ? !autoMediaPath : !autoSourcePreview || !autoAuthorized)} onClick={() => void startAutoWorkflow()}>{autoBusy ? <LoaderCircle className="spin" size={14}/> : <Sparkles size={14}/>}{tr("app.s0502")}</button>
          {autoError && <div className="source-error" role="alert"><CircleAlert size={15}/><JobFailureDetails context="auto" status="failed" errorMessage={autoError}/></div>}
        </div>
      </Dialog>}
      {showRuntime && <Dialog label={tr("app.s0245")} className="runtime-dialog runtime-settings-dialog" onClose={() => setShowRuntime(false)} returnFocusRef={runtimeButtonRef}>
        <header className="runtime-dialog-header"><div><p className="eyebrow">{tr("app.s0505")}</p><h2>{tr("app.s0245")}</h2></div><button autoFocus className="dialog-close" aria-label={tr("app.s0503")} title={tr("app.s0504")} onClick={() => setShowRuntime(false)}><X size={18}/></button></header>
        <div className="runtime-dialog-content"><p className="dialog-copy">{tr("app.s0506")}</p><RuntimeChecklist runtime={runtime} modelPath={modelPath} onChooseModel={chooseModel}/><AsrBackendPicker runtime={runtime} onSelect={changeAsrBackend}/><ModelManager models={models} selectedPath={modelPath} job={modelJob} onSelect={(path) => { localStorage.setItem("siaocut.modelPath", path); setModelPath(path); }} onInstall={installModel} onCancel={cancelModel} onRemove={removeModel}/><SpeakerPackageManager packageStatus={speakerPackage} job={speakerJob?.kind === "install" ? speakerJob : null} disabled={Boolean(busy)} onInstall={installSpeakerPackage} onCancel={cancelSpeakerJob} onResume={resumeSpeakerJob}/><DiagnosticsPanel runtime={runtime} onOpen={openDiagnostics}/><UpdatePanel policy={updatePolicy} update={availableUpdate} busy={updateBusy} error={updateError} onCheck={() => void checkUpdates()} onInstall={() => void confirmUpdateInstall()}/><button className="button quiet full" onClick={() => void initialize()}><RefreshCw size={14}/>{tr("app.s0507")}</button></div>
      </Dialog>}
      {currentDeleteCandidate && <Dialog label={tr("app.s0243")} className="confirm-dialog" onClose={closeDeleteDialog}><div className="confirm-icon"><Trash2 size={20}/></div><p className="eyebrow">{tr("app.s0508")}</p><h2>{tr("app.s0509")}{currentDeleteCandidate.title}」？</h2><p className="dialog-copy">{tr("app.s0510")}</p>{(deleteBlockMessage || deleteError) && <div className="confirm-error" role="alert"><CircleAlert size={16}/><span>{deleteBlockMessage ?? deleteError}</span></div>}<div className="confirm-actions"><button className="button quiet" disabled={deleteBusy} onClick={closeDeleteDialog}>{tr("app.s0511")}</button><button className="button danger" disabled={deleteBusy || Boolean(deleteBlockMessage)} onClick={() => void deleteProject()}>{deleteBusy ? <LoaderCircle className="spin" size={14}/> : <Trash2 size={14}/>}{tr("app.s0512")}</button></div></Dialog>}
      {showSourceImport && <Dialog label={tr("app.s0513")} className="runtime-dialog source-dialog" onClose={() => setShowSourceImport(false)} returnFocusRef={sourceButtonRef}><button autoFocus className="dialog-close" aria-label={tr("app.s0514")} title={tr("app.s0515")} onClick={() => setShowSourceImport(false)}><X size={18}/></button><p className="eyebrow">{tr("app.s0516")}</p><h2>{tr("app.s0517")}</h2><p className="dialog-copy">{tr("app.s0518")}</p>
        {!sourceJob && <form className="source-form" onSubmit={(event) => { event.preventDefault(); void inspectSource(); }}><label><span>{tr("app.s0487")}</span><input autoComplete="url" aria-label={tr("app.s0487")} placeholder="https://…" value={sourceUrl} disabled={Boolean(sourceBusy)} onChange={(event) => { setSourceUrl(event.target.value); setSourcePreview(null); setSourceAuthorized(false); setSourceError(null); }}/></label><button className="button primary" type="submit" disabled={Boolean(sourceBusy) || !isHttpsSourceUrl(sourceUrl)}>{sourceBusy && !sourcePreview ? <LoaderCircle className="spin" size={14}/> : <Search size={14}/>}{tr("app.s0489")}</button></form>}
        {sourcePreview && !sourceJob && <section className="source-preview" aria-label={tr("app.s0519")}><header><span><small>{sourcePreview.extractor}</small><strong>{sourcePreview.title}</strong></span><ShieldCheck size={19}/></header><dl><div><dt>{tr("app.s0491")}</dt><dd>{formatTime(sourcePreview.durationSeconds)}</dd></div><div><dt>{sourcePreview.fileSizeKnown ? tr("app.s0520") : tr("app.s0521")}</dt><dd>{formatBytes(sourcePreview.fileSizeBytes)}</dd></div><div><dt>{tr("app.s0492")}</dt><dd>{sourcePreview.siteMediaId}</dd></div><div><dt>{tr("app.s0522")}</dt><dd>yt-dlp {sourcePreview.toolVersion}</dd></div></dl><p className="source-url" title={sourcePreview.webpageUrl}>{sourcePreview.webpageUrl}</p><label className="source-consent"><input type="checkbox" checked={sourceAuthorized} onChange={(event) => setSourceAuthorized(event.target.checked)}/><span>{tr("app.s0523")}</span></label><button className="button primary full" disabled={!sourceAuthorized || Boolean(sourceBusy)} onClick={() => void startSourceImport()}>{sourceBusy ? <LoaderCircle className="spin" size={14}/> : <Download size={14}/>}{tr("app.s0524")}</button></section>}
        {sourceJob && <section className="source-job" aria-label={tr("app.s0525")}><header><span className={`source-state ${sourceJob.status}`}><i />{sourceStatusLabel(sourceJob.status)}</span><strong>{sourceJob.title}</strong><small>{tr("app.composite.sourceAttempt", { attempt: sourceJob.attemptCount, mediaId: sourceJob.siteMediaId })}</small></header><div className="source-job-progress"><progress value={sourceJob.progress} max={1}/><span>{Math.round(sourceJob.progress * 100)}% · {formatBytes(sourceJob.bytesDownloaded)} / {formatBytes(sourceJob.totalBytes ?? sourceJob.fileSizeBytes)}</span></div><dl><div><dt>{tr("app.s0528")}</dt><dd>yt-dlp {sourceJob.toolVersion}</dd></div><div><dt>{tr("app.s0239")}</dt><dd>{sourceJob.projectId ?? tr("app.s0529")}</dd></div></dl>{["failed", "interrupted"].includes(sourceJob.status) && <JobFailureDetails className="source-job-error" context="source" status={sourceJob.status} errorCode={sourceJob.errorCode} errorMessage={sourceJob.errorMessage}/>}<div className="source-job-actions">{["queued", "running"].includes(sourceJob.status) && <button disabled={Boolean(sourceBusy) || Boolean(sourceJob.cancelRequestedAt)} onClick={() => void cancelSourceImport()}>{sourceJob.cancelRequestedAt ? tr("app.s0317") : tr("app.s0530")}</button>}{["cancelled", "failed", "interrupted"].includes(sourceJob.status) && <button className="primary" disabled={Boolean(sourceBusy)} onClick={() => void resumeSourceImport()}><RefreshCw size={13}/>{tr("app.s0279")}</button>}{!["queued", "running", "finalizing"].includes(sourceJob.status) && <button onClick={resetSourceImport}>{tr("app.s0531")}</button>}</div></section>}
        {sourceError && <div className="source-error" role="alert"><CircleAlert size={15}/><JobFailureDetails context="source" status="failed" errorMessage={sourceError}/></div>}
        <p className="runtime-disclosure">{tr("app.s0532")}</p>
      </Dialog>}
    </main>);
}
export default App;
