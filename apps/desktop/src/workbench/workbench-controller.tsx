import { changeUiLocale, getUiLocale, tr, type UiLocale } from "../i18n";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type SyntheticEvent } from "react";
import { Activity, Bot, Check, ChevronDown, ChevronRight, ChevronUp, CircleAlert, Clock3, Copy, Cpu, Database, Download, FileVideo2, FileText, Film, FolderOpen, FolderPlus, HardDrive, History, Link2, LoaderCircle, Play, RefreshCw, RotateCcw, Search, Scissors, Settings2, ShieldCheck, Sparkles, Trash2, Undo2, Redo2, Headphones, ListChecks, MoreHorizontal, MoveHorizontal, Users, X, } from "lucide-react";
import { authorizeArtifact, authorizeMedia, openLogDirectory, pickMedia, pickModel, pickSubtitleFile, pickTranscriptPath, pickVideoPath, runtimeInfo, selectAsrBackend, updaterPolicy } from "../core";
import type { AgentRun, AudioAnalysisJob, AudioRisk, AutoWorkflow, CanvasSettings, CodexHealth, CutPreview, ExportJob, ModelDownloadJob, ModelStatus, Project, ProjectDeletionPreflight, RuntimeInfo, Segment, SourceImportJob, SourcePreview, SpeakerIdentity, SpeakerJob, SpeakerPackageStatus, SpeakerTrack, SpeechEvidence, SpeechInsights, SpeechPause, SubtitleImportPreview, SubtitleQualityIssue, TranscriptionJob, TranscriptionLanguage, TranscriptionProviderConfig, TranscriptionProviderHealth, TranscriptionReviewItem } from "../types";
import { Button, Dialog, IconButton, StatusBadge } from "../components/ui";
import { JobFailureDetails } from "../components/job-failure";
import RuntimeSettingsDialog from "../components/runtime-settings-dialog";
import { AudioQualityPanel, PatchReviewCard, RuntimeChecklist, SegmentRow, SpeakerTrackPanel, SpeechInsightsPanel, TranscriptionReviewPanel } from "../components/workbench-panels";
import { useAppUpdater } from "../hooks/use-app-updater";
export { AudioQualityPanel, PatchReviewCard, SpeakerPackageManager, SpeakerTrackPanel, SpeechInsightsPanel } from "../components/workbench-panels";
import { audioRiskLabel, audioUnitLabel, autoStageLabel, autoStatusLabel, clearTransientCoreError, cutSuggestionLabel, DEFAULT_EXPORT_PREFERENCES, editReasonLabel, formatBytes, formatTime, getProjectCapabilities, hasMeaningfulSubtitleText, isHttpsSourceUrl, modelDescription, modelName, parseExportPreferences, parseTranscriptionLanguage, patchReasonLabel, segmentCountLabel, sourceStatusLabel, structureEditLabel, subtitleCountLabel, subtitleIssueLabel, subtitleQualityStatusLabel, taskLabel, TRANSCRIPTION_LANGUAGE_STORAGE_KEY, versionReasonLabel, wordCountLabel, type ExportPreferencesV1, type SegmentSelectionMode, type StructureEditMode } from "../app-view-model";
import { agentReviewClient } from "../domains/agent-review-client";
import { backgroundTaskClient } from "../domains/background-task-client";
import { exportRuntimeClient } from "../domains/export-runtime-client";
import { projectSessionClient } from "../domains/project-session-client";
import { transcriptEditingClient } from "../domains/transcript-editing-client";
import { translationClient } from "../domains/translation-client";
import { useBackgroundTaskRegistry } from "../hooks/use-background-task-registry";
import { useWorkbenchFeedback } from "../hooks/use-workbench-feedback";

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

const isValidAgentIdentity = (value: string) => /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(value);
const TranscriptionCandidateDialog = lazy(() => import("../components/transcription-candidate-dialog"));
const ExportPanel = lazy(() => import("../components/export-panel"));
const TranscriptionJobBar = lazy(() => import("../components/transcription-job-bar"));
const ProjectDeleteDialog = lazy(() => import("../components/project-delete-dialog"));
const AppCommandMenu = lazy(() => import("../components/app-command-menu"));

function WorkbenchController() {
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
    const selectTranscriptionMode = (mode: "quick" | "multispeaker") => {
        localStorage.setItem("siaocut.transcriptionMode", mode);
        setTranscriptionMode(mode);
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
    const [transcriptionMode, setTranscriptionMode] = useState<"quick" | "multispeaker">(() => localStorage.getItem("siaocut.transcriptionMode") === "multispeaker" ? "multispeaker" : "quick");
    const [transcriptionConfig, setTranscriptionConfig] = useState<TranscriptionProviderConfig | null>(null);
    const [transcriptionHealth, setTranscriptionHealth] = useState<TranscriptionProviderHealth | null>(null);
    const [transcriptionJob, setTranscriptionJob] = useState<TranscriptionJob | null>(null);
    const [showTranscriptionCandidate, setShowTranscriptionCandidate] = useState(false);
    const [transcriptionApplyConfirmed, setTranscriptionApplyConfirmed] = useState(false);
    const [transcriptionReviews, setTranscriptionReviews] = useState<TranscriptionReviewItem[]>([]);
    const [transcriptionPrompt, setTranscriptionPrompt] = useState("");
    const [transcriptionHotwords, setTranscriptionHotwords] = useState("");
    const { busy, notice, error, setBusy, setNotice, setError } = useWorkbenchFeedback(tr("app.s0038"));
    const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
    const { updatePolicy, setUpdatePolicy, availableUpdate, updateBusy, updateError, checkUpdates, confirmUpdateInstall } = useAppUpdater(setNotice);
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
    const [agentWorkflowKind, setAgentWorkflowKind] = useState<"polish" | "proofread" | "edit" | "translate" | "punctuate" | "speaker_names">("polish");
    const [codexHealth, setCodexHealth] = useState<CodexHealth | null>(null);
    const [agentRun, setAgentRun] = useState<AgentRun | null>(null);
    const [showAgentHandoff, setShowAgentHandoff] = useState(false);
    const [agentHandoffTaskId, setAgentHandoffTaskId] = useState<string | null>(null);
    const [agentIdentity, setAgentIdentity] = useState("external-agent");
    const [agentHandoffReady, setAgentHandoffReady] = useState(false);
    const [agentHandoffCopied, setAgentHandoffCopied] = useState(false);
    const [autoBurnSubtitles, setAutoBurnSubtitles] = useState(true);
    const [autoSubtitleMode, setAutoSubtitleMode] = useState<"source" | "translated" | "bilingual">("source");
    const [autoBusy, setAutoBusy] = useState<string | null>(null);
    const [autoError, setAutoError] = useState<string | null>(null);
    const [modelPath, setModelPath] = useState<string | null>(() => localStorage.getItem("siaocut.modelPath"));
    const [showRuntime, setShowRuntime] = useState(false);
    const [showExportPanel, setShowExportPanel] = useState(false);
    const [drawerTab, setDrawerTab] = useState<"review" | "quality" | "analysis" | "history" | "export">("review");
    const [playerExpanded, setPlayerExpanded] = useState(true);
    const [timelineExpanded, setTimelineExpanded] = useState(false);
    const [showSubtitleSafeArea, setShowSubtitleSafeArea] = useState(true);
    const [showMoreMenu, setShowMoreMenu] = useState(false);
    const [search, setSearch] = useState("");
    const [replacement, setReplacement] = useState("");
    const [emptyReplacementConfirmed, setEmptyReplacementConfirmed] = useState(false);
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
    const [exportFormat, setExportFormat] = useState<"srt" | "vtt" | "ass" | "markdown" | "json">(() => parseExportPreferences(localStorage.getItem("siaocut.exportPreferences.v1")).transcriptFormat);
    const [includeSpeakerLabels, setIncludeSpeakerLabels] = useState(true);
    const [confirmTranscriptionWarnings, setConfirmTranscriptionWarnings] = useState(false);
    const [confirmStaleTranslation, setConfirmStaleTranslation] = useState(false);
    const [glossaryDraft, setGlossaryDraft] = useState("");
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
    const [deletionPreflight, setDeletionPreflight] = useState<ProjectDeletionPreflight | null>(null);
    const [deletePreflightBusy, setDeletePreflightBusy] = useState(false);
    const videoRef = useRef<HTMLVideoElement>(null);
    const runtimeButtonRef = useRef<HTMLButtonElement>(null);
    const sourceButtonRef = useRef<HTMLButtonElement>(null);
    const autoButtonRef = useRef<HTMLButtonElement>(null);
    const agentButtonRef = useRef<HTMLButtonElement>(null);
    const exportButtonRef = useRef<HTMLButtonElement>(null);
    const exportPanelRef = useRef<HTMLElement>(null);
    const commandMoreRef = useRef<HTMLDivElement>(null);
    const searchInputRef = useRef<HTMLInputElement>(null);
    const replacementInputRef = useRef<HTMLInputElement>(null);
    const subtitleImportButtonRef = useRef<HTMLButtonElement>(null);
    const refreshLatestExport = useCallback(async (projectId: string) => {
        const envelope = await exportRuntimeClient.listVideoExports(projectId);
        setActiveExport(envelope.jobs?.[0] ?? null);
    }, []);
    const refreshLatestAudioAnalysis = useCallback(async (projectId: string) => {
        const envelope = await backgroundTaskClient.latestAudioAnalysis(projectId);
        setAudioAnalysisJob(envelope.audioAnalysisJob ?? null);
    }, []);
    const refreshSpeakerTrack = useCallback(async (projectId: string) => {
        const envelope = await transcriptEditingClient.getSpeakerTrack(projectId);
        setSpeakerTrack(envelope.speakerTrack ?? null);
    }, []);
    const refreshTranscription = useCallback(async (projectId: string) => {
        const [latest, reviews] = await Promise.all([
            backgroundTaskClient.latestTranscription(projectId),
            backgroundTaskClient.listTranscriptionReviews(projectId),
        ]);
        setTranscriptionJob(latest.transcriptionJob ?? null);
        setTranscriptionReviews(reviews.reviewItems ?? []);
    }, []);
    const refreshProject = useCallback(async (projectId: string, refreshMedia = false) => {
        const next = await projectSessionClient.loadProject(projectId);
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
        const [, , , , runs] = await Promise.all([
            refreshLatestExport(next.id),
            refreshLatestAudioAnalysis(next.id),
            refreshSpeakerTrack(next.id),
            refreshTranscription(next.id),
            agentReviewClient.listAgentRuns(next.id).catch(() => null),
        ]);
        setAgentRun(runs?.agentRuns?.[0] ?? null);
    }, [refreshLatestAudioAnalysis, refreshLatestExport, refreshSpeakerTrack, refreshTranscription]);
    const initialize = useCallback(async () => {
        setBusy(tr("app.s0039"));
        setError(null);
        const [projectsResult, runtimeResult, modelsResult, modelJobsResult, sourceJobsResult, autoWorkflowsResult, updatePolicyResult, speakerPackageResult, speakerJobsResult, transcriptionHealthResult, codexHealthResult] = await Promise.allSettled([
            projectSessionClient.listProjects(),
            runtimeInfo(),
            backgroundTaskClient.listModels(true),
            backgroundTaskClient.listModelJobs(),
            backgroundTaskClient.listSourceJobs(),
            backgroundTaskClient.listAutoWorkflows(),
            updaterPolicy(),
            backgroundTaskClient.getSpeakerPackage(),
            backgroundTaskClient.listSpeakerJobs(),
            backgroundTaskClient.getTranscriptionHealth(),
            agentReviewClient.getCodexHealth(),
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
        if (transcriptionHealthResult.status === "fulfilled" && transcriptionHealthResult.value.providerHealth) {
            const next = transcriptionHealthResult.value.providerHealth;
            setTranscriptionHealth(next);
            setTranscriptionConfig({ providerId: next.providerId, endpoint: next.endpoint, modelId: next.modelId, updatedAt: next.checkedAt });
        }
        setCodexHealth(codexHealthResult.status === "fulfilled" ? codexHealthResult.value.codex ?? null : null);
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
                    await Promise.all([refreshLatestExport(first.id), refreshLatestAudioAnalysis(first.id), refreshSpeakerTrack(first.id), refreshTranscription(first.id)]);
                    const runs = await agentReviewClient.listAgentRuns(first.id).catch(() => null);
                    setAgentRun(runs?.agentRuns?.[0] ?? null);
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
    }, [refreshLatestAudioAnalysis, refreshLatestExport, refreshSpeakerTrack, refreshTranscription]);
    useEffect(() => {
        void initialize();
    }, [initialize]);
    useBackgroundTaskRegistry([
        agentRun && ["queued", "running", "submitting"].includes(agentRun.status) ? {
            key: `codex-agent:${agentRun.id}`,
            intervalMs: 1200,
            poll: () => agentReviewClient.getAgentRun(agentRun.id).then(async (envelope) => {
                if (!envelope.agentRun)
                    return;
                const next = envelope.agentRun;
                setAgentRun(next);
                if (next.status === "completed") {
                    await refreshProject(next.projectId);
                    setDrawerTab("review");
                    setNotice(tr("app.creator.agent.completed"));
                }
                if (["failed", "interrupted"].includes(next.status))
                    setError(next.errorMessage ?? tr("app.creator.agent.failed"));
                if (next.status === "cancelled")
                    setNotice(tr("app.creator.agent.cancelled"));
            }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
        } : null,
        project?.tasks.some((task) => ["queued", "claimed", "running", "interrupted"].includes(task.status)) ? {
            key: `agent-project:${project.id}`,
            intervalMs: 2500,
            poll: () => projectSessionClient.loadProject(project.id).then((next) => {
                setError(clearTransientCoreError);
                setProject(next);
                setProjects((current) => current.map((item) => item.id === next.id ? next : item));
            }).catch(() => undefined),
        } : null,
        activeExport && ["queued", "running"].includes(activeExport.status) ? {
            key: `video-export:${activeExport.id}`,
            intervalMs: 1000,
            poll: () => exportRuntimeClient.getVideoExport(activeExport.id).then((envelope) => {
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
            }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
        } : null,
        audioAnalysisJob && ["queued", "running"].includes(audioAnalysisJob.status) ? {
            key: `audio-analysis:${audioAnalysisJob.id}`,
            intervalMs: 700,
            poll: () => backgroundTaskClient.getAudioAnalysis(audioAnalysisJob.id).then((envelope) => {
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
            }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
        } : null,
        modelJob && ["queued", "running"].includes(modelJob.status) ? {
            key: `model:${modelJob.id}`,
            intervalMs: 800,
            poll: () => backgroundTaskClient.getModelJob(modelJob.id).then(async (envelope) => {
                setError(clearTransientCoreError);
                if (!envelope.modelJob)
                    return;
                setModelJob(envelope.modelJob);
                if (envelope.modelJob.status === "completed") {
                    const catalog = await backgroundTaskClient.listModels(true);
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
            }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
        } : null,
        speakerJob && ["queued", "running"].includes(speakerJob.status) ? {
            key: `speaker:${speakerJob.id}`,
            intervalMs: 800,
            poll: () => backgroundTaskClient.getSpeakerJob(speakerJob.id).then(async (envelope) => {
                setError(clearTransientCoreError);
                if (!envelope.speakerJob)
                    return;
                const next = envelope.speakerJob;
                setSpeakerJob(next);
                if (next.status === "completed" && next.kind === "install") {
                    const status = await backgroundTaskClient.getSpeakerPackage();
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
            }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
        } : null,
        transcriptionJob && ["queued", "running", "finalizing"].includes(transcriptionJob.status) ? {
            key: `transcription:${transcriptionJob.id}`,
            intervalMs: 1000,
            poll: () => backgroundTaskClient.getTranscriptionJob(transcriptionJob.id).then(async (envelope) => {
                setError(clearTransientCoreError);
                if (!envelope.transcriptionJob)
                    return;
                const next = envelope.transcriptionJob;
                setTranscriptionJob(next);
                if (next.status === "completed") {
                    await Promise.all([refreshProject(next.projectId, true), refreshSpeakerTrack(next.projectId), refreshTranscription(next.projectId)]);
                    setNotice(tr("app.moss.job.completed"));
                }
                if (["failed", "interrupted"].includes(next.status))
                    setError(next.errorMessage ?? tr("app.moss.job.failed"));
                if (next.status === "cancelled")
                    setNotice(tr("app.moss.job.cancelled"));
            }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
        } : null,
        sourceJob && ["queued", "running", "finalizing"].includes(sourceJob.status) ? {
            key: `source:${sourceJob.id}`,
            intervalMs: 600,
            poll: () => backgroundTaskClient.getSourceJob(sourceJob.id).then(async (envelope) => {
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
                    const imported = await projectSessionClient.loadProject(nextJob.projectId);
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
            }),
        } : null,
        autoWorkflow && ["queued", "running", "needs_agent", "needs_review"].includes(autoWorkflow.status) ? {
            key: `auto:${autoWorkflow.id}`,
            intervalMs: 800,
            poll: () => backgroundTaskClient.getAutoWorkflow(autoWorkflow.id).then(async (envelope) => {
                setError(clearTransientCoreError);
                setAutoError(clearTransientCoreError);
                if (!envelope.workflow)
                    return;
                const next = envelope.workflow;
                setAutoWorkflow({ ...next });
                if (next.projectId && (project?.id !== next.projectId || ["needs_review", "completed"].includes(next.status)))
                    await refreshProject(next.projectId, next.status === "completed");
                if (next.status === "completed")
                    setNotice(tr("app.s0068", { "0": next.outputPath }));
                if (next.status === "failed")
                    setError(next.errorMessage ?? tr("app.s0069"));
                if (next.status === "interrupted")
                    setError(next.errorMessage ?? tr("app.s0070"));
            }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
        } : null,
    ]);
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
    const selectedTranslationIncomplete = Boolean(selectedTranslation && project?.transcript.segments.some(
        (source) => !selectedTranslation.segments.some((translated) => translated.segmentId === source.id),
    ));
    const selectedTranslationPending = Boolean(subtitleMode !== "source" && selectedSubtitleLanguage && (!selectedTranslation || selectedTranslationIncomplete));
    const selectedTranslationStale = Boolean(subtitleMode !== "source" && !selectedTranslationIncomplete && selectedTranslation && (
        selectedTranslation.status !== "current"
        || selectedTranslation.segments.some((segment) => segment.status !== "current")
    ));
    const capabilities = useMemo(() => getProjectCapabilities(project, {
        mediaUrl,
        modelPath,
        translationTarget: subtitleLanguage,
        agentWorkflowKind,
    }), [agentWorkflowKind, mediaUrl, modelPath, project, subtitleLanguage]);
    const mediaCapabilityTitle = capabilities.hasBoundMedia ? undefined : tr("app.capability.mediaRequired");
    const transcribeCapabilityTitle = !capabilities.hasBoundMedia
        ? tr("app.capability.mediaRequired")
        : transcriptionMode === "multispeaker"
            ? transcriptionHealth?.state !== "healthy" ? tr("app.moss.health.required") : undefined
            : !capabilities.hasModel ? tr("app.capability.modelRequired") : undefined;
    const transcriptionActive = Boolean(transcriptionJob && ["queued", "running", "finalizing"].includes(transcriptionJob.status));
    const canStartTranscription = capabilities.hasBoundMedia && (transcriptionMode === "multispeaker" ? transcriptionHealth?.state === "healthy" : capabilities.hasModel);
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
    const deleteBlockMessage = deletionPreflight?.blockers.length
        ? deletionPreflight.blockers.map((blocker) => ({
            agent_task: tr("app.delete.blocker.agent"),
            export: tr("app.delete.blocker.export"),
            audio_analysis: tr("app.delete.blocker.audio"),
            speaker_analysis: tr("app.delete.blocker.speaker"),
            auto_workflow: tr("app.delete.blocker.workflow"),
            transcription: blocker.status === "awaiting_apply" ? tr("app.delete.blocker.transcriptionCandidate") : tr("app.delete.blocker.transcription"),
        }[blocker.kind] ?? tr("app.delete.blocker.unknown", { kind: blocker.kind, status: blocker.status }))).join(" ")
        : null;
    const humanState = busy ? tr("app.s0001") : taskLabel(project);
    const humanStateTone = humanState === tr("app.s0003") ? "warning" : humanState === tr("app.s0002") ? "agent" : humanState === tr("app.s0001") ? "info" : "success";
    const orderedPatchSets = project?.patchSets
        .map((set) => ({ ...set, items: set.items.filter((item) => ["pending", "conflict"].includes(item.status)).sort((left, right) => Number(right.status === "conflict") - Number(left.status === "conflict")) }))
        .filter((set) => set.items.length)
        .sort((left, right) => Number(right.items.some((item) => item.status === "conflict")) - Number(left.items.some((item) => item.status === "conflict"))) ?? [];
    const pendingEdits = project?.edits.filter((edit) => ["suggested", "proposed"].includes(edit.status)) ?? [];
    const failedTasks = project?.tasks.filter((task) => ["failed", "interrupted"].includes(task.status)) ?? [];
    const processingTasks = project?.tasks.filter((task) => ["queued", "claimed", "running"].includes(task.status)) ?? [];
    const agentTask = processingTasks[0];
    const agentActivityAgeMs = agentTask?.lastActivity?.createdAt ? Date.now() - new Date(agentTask.lastActivity.createdAt).getTime() : null;
    const agentActivityStale = agentTask?.status === "running" && agentActivityAgeMs != null && Number.isFinite(agentActivityAgeMs) && agentActivityAgeMs > 5 * 60 * 1000;
    const agentStatus = !agentTask ? null : agentTask.status === "queued" ? tr("app.agent.status.queued") : agentTask.status === "claimed" ? tr("app.agent.status.claimed") : agentActivityStale ? tr("app.agent.status.stale") : tr("app.agent.status.running");
    const recentTasks = project?.tasks.filter((task) => ["completed", "cancelled", "canceled"].includes(task.status)).slice(-5).reverse() ?? [];
    const audioRisks = audioAnalysisJob?.status === "completed" ? audioAnalysisJob.report?.risks ?? [] : [];
    const projectSpeakerJob = speakerJob?.projectId === project?.id ? speakerJob : null;
    const speakerById = new Map(speakerTrack?.speakers.map((speaker) => [speaker.id, speaker]) ?? []);
    const associationBySegment = new Map(speakerTrack?.associations.map((association) => [association.segmentId, association]) ?? []);
    const actionableReviewCount = orderedPatchSets.reduce((count, set) => count + set.items.length, 0) + pendingEdits.length + failedTasks.length + audioRisks.length + transcriptionReviews.length + Number(Boolean(projectSpeakerJob && ["failed", "interrupted"].includes(projectSpeakerJob.status)));
    const mossWordTimingUnavailable = speakerTrack?.providerId === "moss_openai" && speakerTrack.sourceKind === "end_to_end";
    const transcriptionExportErrors = transcriptionReviews.filter((item) => item.status === "open" && item.severity === "error");
    const transcriptionExportWarnings = transcriptionReviews.filter((item) => item.status === "open" && item.severity === "warning");
    const structuredExport = exportFormat === "json" || (exportFormat === "markdown" && speakerTrack?.status === "ready");
    const transcriptionExportBlocked = structuredExport && (transcriptionExportErrors.length > 0 || (transcriptionExportWarnings.length > 0 && !confirmTranscriptionWarnings));
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
        const entries = project?.glossary.entries.filter((entry) => entry.language === subtitleLanguage) ?? [];
        setGlossaryDraft(entries.map((entry) => `${entry.source}=${entry.target}`).join("\n"));
    }, [project?.id, project?.glossary.version, subtitleLanguage]);
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
        const envelope = await projectSessionClient.importMedia(path);
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
    const refreshDeletionPreflight = async (projectId: string) => {
        const envelope = await projectSessionClient.deletePreflight(projectId);
        if (!envelope.deletionPreflight)
            throw new Error(tr("app.delete.preflightMissing"));
        setDeletionPreflight(envelope.deletionPreflight);
        return envelope.deletionPreflight;
    };
    const openDeleteDialog = (candidate: Project) => {
        setDeleteError(null);
        setDeletionPreflight(null);
        setDeleteCandidate(candidate);
        setDeletePreflightBusy(true);
        void refreshDeletionPreflight(candidate.id).catch((cause) => {
            setDeleteError(cause instanceof Error ? cause.message : String(cause));
        }).finally(() => setDeletePreflightBusy(false));
    };
    const closeDeleteDialog = () => {
        if (deleteBusy || deletePreflightBusy)
            return;
        setDeleteCandidate(null);
        setDeleteError(null);
        setDeletionPreflight(null);
    };
    const deleteProject = async () => {
        if (!currentDeleteCandidate || deletePreflightBusy)
            return;
        const deleting = currentDeleteCandidate;
        setDeleteBusy(true);
        setDeleteError(null);
        try {
            const latestPreflight = await refreshDeletionPreflight(deleting.id);
            if (!latestPreflight.deletable)
                return;
            await projectSessionClient.deleteProject(deleting.id);
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
            await refreshDeletionPreflight(deleting.id).catch(() => undefined);
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
        const envelope = await backgroundTaskClient.inspectSource(url);
        if (!envelope.source)
            throw new Error(tr("app.s0086"));
        setSourcePreview(envelope.source);
        setSourceJob(null);
        setSourceAuthorized(false);
    });
    const startSourceImport = () => sourcePreview && withSourceBusy(tr("app.s0087"), async () => {
        if (!sourceAuthorized)
            throw new Error(tr("app.s0088"));
        const envelope = await backgroundTaskClient.startSourceImport(sourcePreview.originalUrl, sourcePreview.siteMediaId);
        if (!envelope.sourceJob)
            throw new Error(tr("app.s0089"));
        setSourceJob(envelope.sourceJob);
        setNotice(tr("app.s0090"));
    });
    const cancelSourceImport = () => sourceJob && withSourceBusy(tr("app.s0091"), async () => {
        const envelope = await backgroundTaskClient.cancelSourceImport(sourceJob.id);
        if (!envelope.sourceJob)
            throw new Error(tr("app.s0092"));
        setSourceJob(envelope.sourceJob);
    });
    const resumeSourceImport = () => sourceJob && withSourceBusy(tr("app.s0093"), async () => {
        const envelope = await backgroundTaskClient.resumeSourceImport(sourceJob.id);
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
        const envelope = await backgroundTaskClient.inspectSource(autoUrl.trim());
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
        const input = autoInputKind === "local"
            ? { kind: "local" as const, mediaPath: autoMediaPath, title: tr("app.s0103") }
            : { kind: "url" as const, url: autoSourcePreview!.originalUrl, confirmedMediaId: autoSourcePreview!.siteMediaId };
        const envelope = await backgroundTaskClient.startAutoWorkflow({
            input,
            modelPath,
            language: transcriptionLanguage,
            locale: uiLocale,
            output,
            subtitleMode: autoTranslate ? autoSubtitleMode : "source",
            translationLanguage: autoTranslate ? autoTranslationLanguage : undefined,
            burnSubtitles: autoBurnSubtitles,
        });
        if (!envelope.workflow)
            throw new Error(tr("app.s0104"));
        setAutoWorkflow({ ...envelope.workflow });
        setShowAutoWorkflow(false);
        setNotice(tr("app.s0105"));
    });
    const cancelAutoWorkflow = () => autoWorkflow && withAutoBusy(tr("app.s0106"), async () => {
        const envelope = await backgroundTaskClient.cancelAutoWorkflow(autoWorkflow.id);
        if (!envelope.workflow)
            throw new Error(tr("app.s0107"));
        setAutoWorkflow(null);
        setNotice(tr("app.s0108"));
    });
    const continueAutoWorkflow = () => autoWorkflow && withAutoBusy(tr("app.s0109"), async () => {
        const envelope = await backgroundTaskClient.continueAutoWorkflow(autoWorkflow.id);
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
        await projectSessionClient.relinkMedia(project.id, path);
        await refreshProject(project.id, true);
        setNotice(tr("app.s0119"));
    });
    const transcribe = () => project && withBusy(tr("app.s0120"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        if (!runtime?.ffmpegConfigured)
            throw new Error(tr("app.s0121"));
        if (transcriptionMode === "multispeaker") {
            if (transcriptionHealth?.state !== "healthy")
                throw new Error(tr("app.moss.health.required"));
            const envelope = await backgroundTaskClient.startTranscription({
                projectId: project.id,
                language: transcriptionLanguage,
                prompt: transcriptionPrompt.trim() || undefined,
                hotwords: transcriptionHotwords.split(/[,，\n]/).map((value) => value.trim()).filter(Boolean),
            });
            if (!envelope.transcriptionJob)
                throw new Error(tr("app.moss.job.missing"));
            setTranscriptionJob(envelope.transcriptionJob);
            if (envelope.transcriptionJob.status === "completed") {
                await Promise.all([refreshProject(project.id, true), refreshSpeakerTrack(project.id), refreshTranscription(project.id)]);
                setNotice(tr("app.moss.job.completed"));
            }
            else if (envelope.transcriptionJob.status === "awaiting_apply") {
                setNotice(tr("app.moss.job.awaitingApply"));
            }
            else {
                setNotice(tr("app.moss.job.started"));
            }
            return;
        }
        if (!runtime?.asrConfigured)
            throw new Error(tr("app.s0122"));
        if (!modelPath)
            throw new Error(tr("app.s0123"));
        const result = await transcriptEditingClient.quickTranscribe(project.id, modelPath, transcriptionLanguage);
        await refreshProject(project.id);
        setNotice(Number(result.segments ?? 0) === 0 ? tr("app.s0124") : tr("app.s0125"));
    });
    const saveTranscriptionProvider = (endpoint: string, modelId: string) => withBusy(tr("app.moss.settings.saving"), async () => {
        const envelope = await backgroundTaskClient.configureTranscription(endpoint, modelId);
        if (!envelope.config)
            throw new Error(tr("app.moss.settings.missing"));
        setTranscriptionConfig(envelope.config);
        const checked = await backgroundTaskClient.getTranscriptionHealth();
        setTranscriptionHealth(checked.providerHealth ?? null);
        setNotice(tr("app.moss.settings.saved"));
    });
    const checkTranscriptionProvider = () => withBusy(tr("app.moss.health.checking"), async () => {
        const envelope = await backgroundTaskClient.getTranscriptionHealth();
        setTranscriptionHealth(envelope.providerHealth ?? null);
    });
    const cancelTranscription = () => transcriptionJob && withBusy(tr("app.moss.job.cancelling"), async () => {
        const envelope = await backgroundTaskClient.cancelTranscription(transcriptionJob.id);
        setTranscriptionJob(envelope.transcriptionJob ?? null);
    });
    const resumeTranscription = () => transcriptionJob && withBusy(tr("app.moss.job.resuming"), async () => {
        const envelope = await backgroundTaskClient.resumeTranscription(transcriptionJob.id);
        if (!envelope.transcriptionJob)
            throw new Error(tr("app.moss.job.missing"));
        setTranscriptionJob(envelope.transcriptionJob);
    });
    const applyTranscriptionCandidate = () => transcriptionJob?.candidate && project && withBusy(tr("app.moss.candidate.applying"), async () => {
        const envelope = await backgroundTaskClient.applyTranscription(transcriptionJob.id, transcriptionJob.candidate!.currentVersionId ?? "");
        if (!envelope.transcriptionJob)
            throw new Error(tr("app.moss.job.missing"));
        setShowTranscriptionCandidate(false);
        setTranscriptionApplyConfirmed(false);
        await refreshProject(project.id);
        setNotice(tr("app.moss.candidate.applied"));
    });
    const discardTranscriptionCandidate = () => transcriptionJob && withBusy(tr("app.moss.candidate.discarding"), async () => {
        const envelope = await backgroundTaskClient.discardTranscription(transcriptionJob.id);
        if (!envelope.transcriptionJob)
            throw new Error(tr("app.moss.job.missing"));
        setTranscriptionJob(envelope.transcriptionJob);
        setShowTranscriptionCandidate(false);
        setTranscriptionApplyConfirmed(false);
        setNotice(tr("app.moss.candidate.discarded"));
    });
    const resolveTranscriptionReview = (itemId: string, action: "resolved" | "ignored") => withBusy(tr("app.moss.review.saving"), async () => {
        await backgroundTaskClient.resolveTranscriptionReview(itemId, action);
        if (project)
            await refreshTranscription(project.id);
    });
    const startAudioAnalysis = () => project && withBusy(tr("app.s0126"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        if (!runtime?.ffmpegConfigured)
            throw new Error(tr("app.s0121"));
        const envelope = await backgroundTaskClient.startAudioAnalysis(project.id);
        if (!envelope.audioAnalysisJob)
            throw new Error(tr("app.s0127"));
        setAudioAnalysisJob(envelope.audioAnalysisJob);
        setNotice(tr("app.s0128"));
    });
    const cancelAudioAnalysis = () => audioAnalysisJob && withBusy(tr("app.s0129"), async () => {
        const envelope = await backgroundTaskClient.cancelAudioAnalysis(audioAnalysisJob.id);
        if (envelope.audioAnalysisJob)
            setAudioAnalysisJob(envelope.audioAnalysisJob);
    });
    const resumeAudioAnalysis = () => audioAnalysisJob && withBusy(tr("app.s0130"), async () => {
        const envelope = await backgroundTaskClient.resumeAudioAnalysis(audioAnalysisJob.id);
        if (!envelope.audioAnalysisJob)
            throw new Error(tr("app.s0131"));
        setAudioAnalysisJob(envelope.audioAnalysisJob);
        setNotice(tr("app.s0132", { "0": envelope.audioAnalysisJob.attemptCount }));
    });
    const installSpeakerPackage = () => withBusy(tr("app.s0133"), async () => {
        const envelope = await backgroundTaskClient.installSpeakerPackage();
        if (!envelope.speakerJob)
            throw new Error(tr("app.s0134"));
        setSpeakerJob(envelope.speakerJob);
        if (envelope.speakerJob.status === "completed") {
            const status = await backgroundTaskClient.getSpeakerPackage();
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
        const envelope = await backgroundTaskClient.startSpeakerAnalysis(project.id);
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
        const envelope = await backgroundTaskClient.cancelSpeakerJob(speakerJob.id);
        if (envelope.speakerJob)
            setSpeakerJob(envelope.speakerJob);
    });
    const resumeSpeakerJob = () => speakerJob && withBusy(tr("app.s0142"), async () => {
        const envelope = await backgroundTaskClient.resumeSpeakerJob(speakerJob.id);
        if (!envelope.speakerJob)
            throw new Error(tr("app.s0143"));
        setSpeakerJob(envelope.speakerJob);
        setNotice(tr("app.s0144", { "0": envelope.speakerJob.attemptCount }));
    });
    const renameSpeaker = (speakerId: string, name: string) => project && withBusy(tr("app.s0145"), async () => {
        const envelope = await transcriptEditingClient.renameSpeaker(project.id, speakerId, name);
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0146"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0147"));
    });
    const mergeSpeaker = (fromId: string, intoId: string) => project && withBusy(tr("app.s0148"), async () => {
        const envelope = await transcriptEditingClient.mergeSpeaker(project.id, fromId, intoId);
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0149"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0150"));
    });
    const assignSpeaker = (segmentId: string, speakerId: string) => project && withBusy(tr("app.s0151"), async () => {
        const envelope = await transcriptEditingClient.assignSpeaker(project.id, segmentId, speakerId);
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0146"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0152"));
    });
    const editSegment = (segment: Segment, text: string) => project && text.trim() !== segment.text && withBusy(tr("app.s0153"), async () => {
        await transcriptEditingClient.editSegment(project.id, segment.id, text.trim());
        await refreshProject(project.id);
        setNotice(tr("app.s0154"));
    });
    const replaceAll = () => project && search && (replacement || emptyReplacementConfirmed) && withBusy(tr("app.s0155"), async () => {
        const result = await transcriptEditingClient.replaceAll(project.id, search, replacement);
        await refreshProject(project.id);
        setEmptyReplacementConfirmed(false);
        setNotice(Number(result.changedSegments ?? 0) === 0 ? tr("app.s0156") : tr("app.s0157", { "0": result.changedSegments }));
    });
    const openStructureEdit = (mode: StructureEditMode, targetOverride?: Segment, textOffsetOverride?: number, useWordTiming = true) => {
        const target = targetOverride ?? selectedSegments[0];
        if (!project || !target)
            return;
        if (targetOverride) {
            setSelectedId(target.id);
            setSelectedSegmentIds([target.id]);
            setSelectionAnchorId(target.id);
        }
        setStructureError(null);
        if (mode === "split") {
            const characterCount = Array.from(target.text).length;
            const requestedOffset = Math.max(1, Math.min(characterCount - 1, textOffsetOverride ?? Math.floor(characterCount / 2)));
            const targetWords = useWordTiming ? project.transcript.words
                .filter((word) => word.segmentId === target.id && Number.isFinite(word.start) && Number.isFinite(word.end) && word.start >= target.start && word.end <= target.end && word.end > word.start)
                .sort((left, right) => left.start - right.start) : [];
            let scanFrom = 0;
            const wordBoundaries = targetWords.slice(0, -1).flatMap((word) => {
                const index = target.text.indexOf(word.text, scanFrom);
                if (index < 0)
                    return [];
                scanFrom = index + word.text.length;
                return [{ textOffset: Array.from(target.text.slice(0, scanFrom)).length, at: word.end }];
            });
            const credibleBoundary = wordBoundaries
                .filter((boundary) => boundary.textOffset > 0 && boundary.textOffset < characterCount && boundary.at > target.start && boundary.at < target.end)
                .sort((left, right) => Math.abs(left.textOffset - requestedOffset) - Math.abs(right.textOffset - requestedOffset))[0];
            setStructureTextOffset(String(credibleBoundary?.textOffset ?? requestedOffset));
            setStructureStart(credibleBoundary ? credibleBoundary.at.toFixed(3) : "");
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
    const saveBeforeStructureEdit = async (segment: Segment, draft: string) => {
        const text = draft.trim();
        if (!project || text === segment.text)
            return { saved: true, segment };
        let saved = false;
        await withBusy(tr("app.s0153"), async () => {
            await transcriptEditingClient.editSegment(project.id, segment.id, text);
            await refreshProject(project.id);
            setNotice(tr("app.s0154"));
            saved = true;
        });
        return { saved, segment: { ...segment, text } };
    };
    const splitSegmentFromEditor = async (segment: Segment, draft: string, textOffset: number) => {
        const changed = draft.trim() !== segment.text;
        const result = await saveBeforeStructureEdit(segment, draft);
        if (!result.saved)
            return;
        openStructureEdit("split", result.segment, textOffset, !changed);
    };
    const mergePreviousFromEditor = async (segment: Segment, draft: string) => {
        if (!project)
            return;
        const result = await saveBeforeStructureEdit(segment, draft);
        if (!result.saved)
            return;
        const index = project.transcript.segments.findIndex((candidate) => candidate.id === segment.id);
        const previous = project.transcript.segments[index - 1];
        if (!previous) {
            setNotice(tr("app.creator.editor.noPrevious"));
            return;
        }
        setSelectedId(previous.id);
        setSelectedSegmentIds([previous.id, segment.id]);
        setSelectionAnchorId(previous.id);
        setStructureError(null);
        setStructureEditMode("merge");
    };
    const applyStructureEdit = async () => {
        if (!project || !structureEditMode || !selectedSegments.length)
            return;
        setStructureBusy(true);
        setStructureError(null);
        try {
            let request: Promise<Awaited<ReturnType<typeof transcriptEditingClient.splitSegment>>>;
            if (structureEditMode === "split") {
                const textOffset = Number(structureTextOffset);
                const at = Number(structureStart);
                if (!Number.isInteger(textOffset) || textOffset <= 0 || !Number.isFinite(at))
                    throw new Error(tr("app.s0158"));
                if (!hasMeaningfulSubtitleText(splitLeftText) || !hasMeaningfulSubtitleText(splitRightText))
                    throw new Error(tr("app.structure.splitMeaningful"));
                request = transcriptEditingClient.splitSegment(project.id, selectedSegments[0].id, textOffset, at);
            }
            else if (structureEditMode === "merge") {
                if (!mergeCandidatesAdjacent)
                    throw new Error(tr("app.s0159"));
                request = transcriptEditingClient.mergeSegments(project.id, selectedSegments[0].id, selectedSegments[1].id);
            }
            else if (structureEditMode === "timing") {
                const start = Number(structureStart);
                const end = Number(structureEnd);
                if (!Number.isFinite(start) || !Number.isFinite(end) || start < 0 || end <= start)
                    throw new Error(tr("app.s0160"));
                if (!timingChanged)
                    throw new Error(tr("app.structure.timingUnchanged"));
                request = transcriptEditingClient.updateTiming(project.id, selectedSegments[0].id, start, end);
            }
            else {
                const delta = Number(structureDelta);
                if (!Number.isFinite(delta) || delta === 0)
                    throw new Error(tr("app.s0161"));
                request = transcriptEditingClient.offsetSegments(project.id, selectedSegments.map((segment) => segment.id), delta);
            }
            const envelope = await request;
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
            const envelope = await transcriptEditingClient.inspectSubtitleFile(project.id, path);
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
            const envelope = await transcriptEditingClient.importSubtitleFile(project.id, subtitleImportPath, subtitleImportPreview.sha256);
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
        if (structuredExport) {
            await exportRuntimeClient.exportStructuredTranscript(project.id, exportFormat, output, includeSpeakerLabels, confirmTranscriptionWarnings);
        }
        else {
            const subtitle = subtitleExportOptions();
            await exportRuntimeClient.exportTranscript(project.id, exportFormat, output, subtitle.mode, subtitle.language, subtitle.confirmStaleTranslation);
        }
        setNotice(tr("app.s0173", { "0": exportFormat === "markdown" ? tr("app.s0174") : exportFormat === "json" ? tr("app.moss.export.json") : tr("app.s0175"), "1": output }));
    });
    const subtitleExportOptions = () => {
        if (subtitleMode === "source")
            return { mode: "source" as const, language: undefined, confirmStaleTranslation: false };
        if (!selectedSubtitleLanguage)
            throw new Error(tr("app.s0176"));
        if (selectedTranslationPending)
            throw new Error(tr("app.s0177", { "0": selectedSubtitleLanguage.toUpperCase() }));
        return { mode: subtitleMode, language: selectedSubtitleLanguage, confirmStaleTranslation };
    };
    const changeCanvas = (settings: CanvasSettings) => project && withBusy(tr("app.s0178"), async () => {
        const envelope = await transcriptEditingClient.setCanvas(project.id, settings);
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
        const envelope = await transcriptEditingClient.setSubtitleStyle(project.id, preset, position);
        if (!envelope.project)
            throw new Error(tr("app.s0182"));
        setProject(envelope.project);
        setProjects((current) => current.map((item) => item.id === envelope.project!.id ? envelope.project! : item));
        setNotice(tr("app.s0183"));
    });
    const preparePreview = () => project && withBusy(tr("app.s0184"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        await transcriptEditingClient.prepareMedia(project.id);
        await refreshProject(project.id, true);
        setNotice(tr("app.s0185"));
    });
    const exportVideo = () => project && withBusy(tr("app.s0186"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        const output = await pickVideoPath(project.title);
        if (!output)
            return;
        const subtitle = subtitleExportOptions();
        const envelope = await exportRuntimeClient.exportVideo(project.id, output, subtitle.mode, subtitle.language, subtitle.confirmStaleTranslation);
        if (!envelope.job)
            throw new Error(tr("app.s0187"));
        setActiveExport(envelope.job);
        setNotice(envelope.job.status === "completed" ? tr("app.s0051", { "0": envelope.job.outputPath }) : tr("app.s0188"));
    });
    const cancelExport = () => activeExport && withBusy(tr("app.s0189"), async () => {
        const envelope = await exportRuntimeClient.cancelVideoExport(activeExport.id);
        if (envelope.job)
            setActiveExport(envelope.job);
        setNotice(tr("app.s0190"));
    });
    const retryExport = () => activeExport && withBusy(tr("app.s0191"), async () => {
        const envelope = await exportRuntimeClient.retryVideoExport(activeExport.id);
        if (!envelope.job)
            throw new Error(tr("app.s0187"));
        setActiveExport(envelope.job);
        setNotice(tr("app.s0192"));
    });
    const updateCut = (editId: string, action: "apply" | "restore") => project && withBusy(action === "apply" ? tr("app.s0193") : tr("app.s0194"), async () => {
        await transcriptEditingClient.updateCut(project.id, editId, action);
        await refreshProject(project.id);
        setNotice(action === "apply" ? tr("app.s0195") : tr("app.s0196"));
    });
    const detectSuggestions = () => project && withBusy(tr("app.s0197"), async () => {
        const envelope = await transcriptEditingClient.detectCuts(project.id);
        const count = envelope.suggestions?.length ?? 0;
        await refreshProject(project.id);
        setNotice(count ? tr("app.s0198", { "0": count }) : tr("app.s0199"));
    });
    const startCutPreview = async (editId: string) => {
        if (!project)
            return;
        const envelope = await transcriptEditingClient.previewCut(project.id, editId);
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
        const envelope = await transcriptEditingClient.createWordCut(project.id, selected.id, from.id, to.id, cutPadding);
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
        await projectSessionClient.restoreVersion(project.id, versionId);
        await refreshProject(project.id);
        setNotice(tr("app.s0208"));
    });
    const navigateHistory = (action: "undo" | "redo") => project && withBusy(action === "undo" ? tr("app.s0209") : tr("app.s0210"), async () => {
        const envelope = await projectSessionClient.navigateHistory(project.id, action);
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
            const dialogOpen = showRuntime || showSourceImport || showAutoWorkflow || showSubtitleImport || showAgentHandoff || Boolean(structureEditMode) || Boolean(currentDeleteCandidate);
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
                if (project) {
                    setDrawerTab("export");
                    setShowExportPanel(true);
                }
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
        const envelope = await backgroundTaskClient.installModel(modelId);
        if (!envelope.modelJob)
            throw new Error(tr("app.s0217"));
        setModelJob(envelope.modelJob);
        if (envelope.modelJob.status === "completed") {
            const catalog = await backgroundTaskClient.listModels();
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
        const envelope = await backgroundTaskClient.cancelModel(modelJob.id);
        if (envelope.modelJob)
            setModelJob(envelope.modelJob);
    });
    const removeModel = (modelId: string) => withBusy(tr("app.s0220"), async () => {
        await backgroundTaskClient.removeModel(modelId);
        const catalog = await backgroundTaskClient.listModels();
        const available = catalog.models ?? [];
        setModels(available);
        const selected = models.find((item) => item.id === modelId)?.path;
        if (selected && selected === modelPath) {
            localStorage.removeItem("siaocut.modelPath");
            setModelPath(null);
        }
        setNotice(tr("app.s0221"));
    });
    const openAgentHandoff = () => {
        setAgentHandoffTaskId(null);
        setAgentHandoffReady(false);
        setAgentHandoffCopied(false);
        setShowAgentHandoff(true);
    };
    const openExistingAgentHandoff = (taskId: string) => {
        setAgentHandoffTaskId(taskId);
        setAgentHandoffReady(true);
        setAgentHandoffCopied(false);
        setShowAgentHandoff(true);
    };
    const assertAgentWorkflowReady = () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        if (!capabilities.hasTranscript)
            throw new Error(tr("app.capability.transcriptRequired"));
        if (agentWorkflowKind === "translate" && !capabilities.hasTranslationTarget)
            throw new Error(tr("app.capability.translationTargetRequired"));
    };
    const saveGlossary = () => project && withBusy(tr("app.creator.glossary.saving"), async () => {
        const entries = glossaryDraft
            .split(/\r?\n/)
            .map((line) => line.trim())
            .filter(Boolean)
            .map((line) => {
                const separator = line.indexOf("=");
                if (separator <= 0 || separator === line.length - 1)
                    throw new Error(tr("app.creator.glossary.invalid"));
                return { source: line.slice(0, separator).trim(), target: line.slice(separator + 1).trim() };
            });
        const envelope = await translationClient.replaceGlossary(
            project.id,
            subtitleLanguage,
            project.glossary.version,
            entries,
        );
        if (!envelope.project)
            throw new Error(tr("app.canvas.projectMissing"));
        setProject(envelope.project);
        setProjects((current) => current.map((item) => item.id === envelope.project!.id ? envelope.project! : item));
        setConfirmStaleTranslation(false);
        setNotice(tr("app.creator.glossary.saved", { version: envelope.project.glossary.version }));
    });
    const createAgentTask = () => project && withBusy(tr("app.s0222"), async () => {
        assertAgentWorkflowReady();
        const envelope = await agentReviewClient.createWorkflow(project.id, agentWorkflowKind, uiLocale, agentWorkflowKind === "translate" ? subtitleLanguage : undefined);
        await refreshProject(project.id);
        setAgentHandoffTaskId(envelope.taskId ?? null);
        setNotice({
            polish: tr("app.workflow.created.polish"),
            proofread: tr("app.workflow.created.proofread"),
            edit: tr("app.workflow.created.edit"),
            translate: tr("app.workflow.created.translate", { language: subtitleLanguage.toUpperCase() }),
            punctuate: tr("app.workflow.created.punctuate"),
            speaker_names: tr("app.workflow.created.speakerNames"),
        }[agentWorkflowKind]);
    });
    const startCodexAgent = () => project && withBusy(tr("app.creator.agent.starting"), async () => {
        assertAgentWorkflowReady();
        const workflow = await agentReviewClient.createWorkflow(project.id, agentWorkflowKind, uiLocale, agentWorkflowKind === "translate" ? subtitleLanguage : undefined);
        if (!workflow.taskId)
            throw new Error(tr("app.creator.agent.taskMissing"));
        await refreshProject(project.id);
        if (!codexHealth?.available || !codexHealth.authenticated) {
            setAgentHandoffTaskId(workflow.taskId);
            setAgentHandoffReady(true);
            setAgentHandoffCopied(false);
            setShowAgentHandoff(true);
            setNotice(tr("app.creator.agent.manualFallback"));
            return;
        }
        const envelope = await agentReviewClient.startAgent(workflow.taskId);
        if (!envelope.agentRun)
            throw new Error(tr("app.creator.agent.runMissing"));
        setAgentRun(envelope.agentRun);
        setDrawerTab("review");
        setNotice(tr("app.creator.agent.started"));
    });
    const cancelCodexAgent = () => agentRun && withBusy(tr("app.creator.agent.cancelling"), async () => {
        const envelope = await agentReviewClient.cancelAgent(agentRun.id);
        if (envelope.agentRun)
            setAgentRun(envelope.agentRun);
        if (project)
            await refreshProject(project.id);
        setNotice(tr("app.creator.agent.cancelled"));
    });
    const resumeCodexAgent = () => agentRun && withBusy(tr("app.creator.agent.resuming"), async () => {
        const envelope = await agentReviewClient.resumeAgent(agentRun.id);
        if (!envelope.agentRun)
            throw new Error(tr("app.creator.agent.runMissing"));
        setAgentRun(envelope.agentRun);
        setDrawerTab("review");
        setNotice(tr("app.creator.agent.resumed"));
    });
    const handoffTask = agentHandoffTaskId ? project?.tasks.find((task) => task.id === agentHandoffTaskId) ?? null : null;
    const handoffIdentity = agentIdentity.trim();
    const handoffText = handoffTask && isValidAgentIdentity(handoffIdentity) ? [
        tr("app.agent.handoff.prompt.title", { taskId: handoffTask.id }),
        tr("app.agent.handoff.prompt.context", { worker: handoffIdentity }),
        tr("app.agent.handoff.prompt.claim", { taskId: handoffTask.id, worker: handoffIdentity }),
        tr("app.agent.handoff.prompt.verify", { taskId: handoffTask.id }),
        tr("app.agent.handoff.prompt.heartbeat", { taskId: handoffTask.id, worker: handoffIdentity }),
        tr("app.agent.handoff.prompt.process"),
        tr("app.agent.handoff.prompt.submit", { taskId: handoffTask.id, worker: handoffIdentity }),
        tr("app.agent.handoff.prompt.review", { taskId: handoffTask.id }),
    ].join("\n\n") : "";
    const copyAgentHandoff = async () => {
        if (!handoffText) return;
        try {
            await navigator.clipboard.writeText(handoffText);
            setAgentHandoffCopied(true);
        }
        catch {
            setError(tr("app.agent.handoff.copyFailed"));
        }
    };
    const updateTask = (taskId: string, action: "retry" | "cancel") => project && withBusy(action === "retry" ? tr("app.s0224") : tr("app.s0225"), async () => {
        await agentReviewClient.updateTask(taskId, action);
        await refreshProject(project.id);
        setNotice(action === "retry" ? tr("app.s0226") : tr("app.s0227"));
    });
    const reviewPatch = (patchItemId: string, action: "apply" | "keep") => project && withBusy(action === "apply" ? tr("app.s0228") : tr("app.s0229"), async () => {
        await agentReviewClient.reviewPatch(patchItemId, action);
        await refreshProject(project.id);
        setNotice(action === "apply" ? tr("app.s0230") : tr("app.s0231"));
    });
    const reviewAll = (taskId: string, action: "apply" | "keep") => project && withBusy(action === "apply" ? tr("app.s0232") : tr("app.s0233"), async () => {
        await agentReviewClient.reviewAll(taskId, action);
        await refreshProject(project.id);
        setNotice(action === "apply" ? tr("app.s0234") : tr("app.s0235"));
    });
    const drawerTabs = ["review", "quality", "analysis", "history", "export"] as const;
    const changeDrawerTabFromKeyboard = (event: ReactKeyboardEvent<HTMLButtonElement>, tab: typeof drawerTabs[number]) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight")
            return;
        event.preventDefault();
        const currentIndex = drawerTabs.indexOf(tab);
        const direction = event.key === "ArrowRight" ? 1 : -1;
        const nextTab = drawerTabs[(currentIndex + direction + drawerTabs.length) % drawerTabs.length];
        setDrawerTab(nextTab);
        setShowExportPanel(nextTab === "export");
        requestAnimationFrame(() => document.getElementById(`creator-drawer-tab-${nextTab}`)?.focus());
    };
    const openCreatorDrawer = (tab: typeof drawerTabs[number]) => {
        setDrawerTab(tab);
        setShowExportPanel(tab === "export");
    };
    const agentRunActive = Boolean(agentRun && ["queued", "running", "submitting"].includes(agentRun.status));
    const creatorPhase = !project ? "prepare"
        : !capabilities.hasTranscript || transcriptionActive ? "transcribe"
            : agentRunActive ? "agent"
                : actionableReviewCount > 0 ? "review" : "export";
    const creatorSteps = ["prepare", "transcribe", "agent", "review", "export"] as const;
    const creatorStepIndex = creatorSteps.indexOf(creatorPhase);
    const runCreatorPrimaryAction = () => {
        if (!project) {
            void importMedia();
            return;
        }
        if (!capabilities.hasTranscript) {
            void transcribe();
            return;
        }
        if (agentRunActive || actionableReviewCount > 0) {
            openCreatorDrawer("review");
            return;
        }
        openCreatorDrawer("quality");
    };
    const creatorPrimaryLabel = !project ? tr("app.creator.action.import")
        : !capabilities.hasTranscript ? tr("app.creator.action.transcribe")
            : agentRunActive ? tr("app.creator.action.viewAgent")
                : actionableReviewCount > 0 ? tr("app.creator.action.review") : tr("app.creator.action.checkExport");
    return (<main className="app-shell">
      <aside className="rail">
        <div className="brand"><span className="brand-mark">S</span><span>SiaoCut</span></div>
        <div className="new-project-actions">
          <button className="new-project auto" aria-label={tr("app.s0237")} onClick={importMedia}><FolderPlus size={16}/>{tr("app.creator.action.import")}</button>
          <details className="rail-advanced-actions"><summary><Settings2 size={14}/>{tr("app.creator.advanced")}</summary><div><button ref={sourceButtonRef} onClick={() => setShowSourceImport(true)}><Link2 size={14}/>{tr("app.s0238")}</button><button ref={autoButtonRef} onClick={() => setShowAutoWorkflow(true)}><Sparkles size={14}/>{tr("app.s0236")}</button></div></details>
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
        <section className="creator-readiness" aria-label={tr("app.creator.readiness.title")}>
          <header><Cpu size={14}/><strong>{tr("app.creator.readiness.title")}</strong></header>
          <span className={runtime ? "ready" : "pending"}><i/>{tr("app.creator.readiness.core")}</span>
          <span className={runtime?.ffmpegConfigured ? "ready" : "pending"}><i/>{tr("app.creator.readiness.ffmpeg")}</span>
          <span className={runtime?.asrBackend === "vulkan" ? "ready" : "default"}><i/>{runtime?.asrBackend === "vulkan" ? tr("app.creator.readiness.vulkan") : tr("app.creator.readiness.cpu")}</span>
          <span className={codexHealth?.available && codexHealth.authenticated ? "ready" : "optional"}><i/>{codexHealth?.available && codexHealth.authenticated ? tr("app.creator.readiness.codexReady") : tr("app.creator.readiness.codexOptional")}</span>
        </section>
        <button ref={runtimeButtonRef} className="runtime-link" aria-label={tr("app.s0245")} onClick={() => setShowRuntime(true)}><Settings2 size={15}/><span>{tr("app.creator.advancedSettings")}</span></button>
        <label className="locale-switch"><span>{tr("app.locale.label")}</span><select aria-label={tr("app.locale.label")} value={uiLocale} onChange={(event) => selectUiLocale(event.target.value as UiLocale)}><option value="zh-CN">{tr("app.locale.zhCN")}</option><option value="en-US">{tr("app.locale.enUS")}</option></select></label>
        <div className="privacy"><ShieldCheck size={15}/><span>{tr("app.s0246")}</span></div>
      </aside>

      <section className="workbench">
        <header className="topbar">
          <div className="topbar-heading"><p className="eyebrow">{tr("app.s0247")}</p><h1>{project?.title ?? tr("app.s0248")}</h1></div>
	          <div className="command-bar creator-command-bar" aria-label={tr("app.s0249")}>
	            <StatusBadge tone={humanStateTone}>{humanState}</StatusBadge>
	            <div className="command-history" aria-label={tr("app.s0250")}>
	              <IconButton label={tr("app.s0251")} shortcut="Ctrl+Z" disabled={!project?.history.canUndo || Boolean(busy)} onClick={() => navigateHistory("undo")}><Undo2 size={15}/></IconButton>
	              <IconButton label={tr("app.s0252")} shortcut="Ctrl+Shift+Z" disabled={!project?.history.canRedo || Boolean(busy)} onClick={() => navigateHistory("redo")}><Redo2 size={15}/></IconButton>
	            </div>
	            <Button variant="primary" className="creator-primary-action" disabled={Boolean(busy) || (creatorPhase === "transcribe" && (!canStartTranscription || transcriptionActive))} title={creatorPhase === "transcribe" ? transcribeCapabilityTitle : undefined} onClick={runCreatorPrimaryAction}>{creatorPhase === "review" ? <ListChecks size={15}/> : creatorPhase === "export" ? <Download size={15}/> : <Sparkles size={15}/>} {creatorPrimaryLabel}</Button>
	            <div className="command-more" ref={commandMoreRef}><IconButton label={tr("app.s0256")} onClick={() => setShowMoreMenu((current) => !current)}><MoreHorizontal size={17}/></IconButton>{showMoreMenu && <Suspense fallback={null}><AppCommandMenu canDetectSuggestions={Boolean(project?.transcript.words.length) && !busy} canPreparePreview={capabilities.canPreparePreview && !busy} canRelinkMedia={capabilities.canRelinkMedia && !busy} mediaCapabilityTitle={mediaCapabilityTitle} onDetectSuggestions={() => { setShowMoreMenu(false); void detectSuggestions(); }} onPreparePreview={() => { setShowMoreMenu(false); void preparePreview(); }} onRelinkMedia={() => { setShowMoreMenu(false); void relinkMedia(); }}/></Suspense>}</div>
	          </div>
	        </header>
	        <nav className="creator-flow" aria-label={tr("app.creator.flow.label")}>{creatorSteps.map((step, index) => <span key={step} className={index < creatorStepIndex ? "done" : index === creatorStepIndex ? "active" : "pending"}><i>{index < creatorStepIndex ? <Check size={12}/> : index + 1}</i>{tr(`app.creator.step.${step}`)}</span>)}</nav>

        {(notice || error) && <div className={`notice ${error ? "error" : ""}`} role="status" aria-live="polite">{error && <CircleAlert size={15}/>}<span>{error ? tr("app.error.unknownSummary") : notice}</span>{error && <details><summary>{tr("app.error.technicalDetails")}</summary><code>{error}</code></details>}{error && <button className="notice-action" onClick={() => void initialize()}>{tr("app.s0262")}</button>}<button aria-label={tr("app.s0263")} title={tr("app.s0263")} onClick={() => { setNotice(null); setError(null); }}>×</button></div>}
        {busy && <div className="progress-strip" role="status" aria-live="polite"><LoaderCircle size={14} className="spin"/>{busy}</div>}
        {transcriptionJob && <Suspense fallback={null}><TranscriptionJobBar job={transcriptionJob} busy={Boolean(busy)} onCancel={cancelTranscription} onResume={resumeTranscription} onInspectCandidate={() => { setTranscriptionApplyConfirmed(false); setShowTranscriptionCandidate(true); }} onDiscardCandidate={discardTranscriptionCandidate}/></Suspense>}
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
	            <div className="welcome-actions"><button className="button primary" onClick={importMedia}><FolderPlus size={16}/>{tr("app.creator.action.import")}</button><button className="button quiet" onClick={() => setShowSourceImport(true)}><Link2 size={16}/>{tr("app.s0285")}</button></div>
          </section>) : (<>
	            <section className="stage-grid">
	              <article className={`video-panel creator-player ${playerExpanded ? "expanded" : "collapsed"}`}>
	                <header className="creator-player-header"><span><Play size={14}/><strong>{tr("app.creator.player.title")}</strong><small>{selected ? `${formatTime(selected.start)} — ${formatTime(selected.end)}` : tr("app.s0288")}</small></span><button aria-expanded={playerExpanded} onClick={() => setPlayerExpanded((current) => !current)}>{playerExpanded ? <ChevronUp size={14}/> : <ChevronDown size={14}/>}{playerExpanded ? tr("app.creator.player.collapse") : tr("app.creator.player.expand")}</button></header>
	                {playerExpanded && <>
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
	                </>}
	              </article>

            </section>

            <section className="editor-grid">
              <article className="transcript-panel">
	                <header className="panel-header"><div><p className="eyebrow">{tr("app.s0332")}</p><h2>{tr("app.s0253")}</h2></div><div className="find-replace">{transcriptionMode === "multispeaker" && <button className="moss-transcribe-command" disabled={!canStartTranscription || transcriptionActive || Boolean(busy)} title={transcribeCapabilityTitle} onClick={transcribe}><Users size={12}/>{tr("app.moss.action.start")}</button>}<button ref={subtitleImportButtonRef} className="subtitle-import-command" disabled={Boolean(busy)} onClick={openSubtitleImport}><FileText size={12}/>{tr("app.s0333")}</button><button className="detect-suggestions" disabled={!project.transcript.words.length || Boolean(busy)} onClick={detectSuggestions}><Scissors size={12}/>{tr("app.s0334")}</button><label className="search"><Search size={14}/><input ref={searchInputRef} value={search} onChange={(event) => { setSearch(event.target.value); setEmptyReplacementConfirmed(false); }} placeholder={tr("app.s0335")} title="Ctrl+F"/></label><input ref={replacementInputRef} aria-label={tr("app.s0336")} value={replacement} onChange={(event) => { setReplacement(event.target.value); setEmptyReplacementConfirmed(false); }} placeholder={tr("app.s0336")} title="Ctrl+H"/>{search && <span className="replace-match-count">{tr("app.replace.matches", { count: replaceMatchCount })}</span>}{search && !replacement && <label className="replace-empty-confirm"><input type="checkbox" checked={emptyReplacementConfirmed} onChange={(event) => setEmptyReplacementConfirmed(event.target.checked)}/><span>{tr("app.replace.confirmDelete")}</span></label>}<button disabled={!search || (!replacement && !emptyReplacementConfirmed) || Boolean(busy)} onClick={replaceAll}>{tr("app.s0337")}</button></div></header>
                <div className="transcript-meta"><span>{tr("app.s0338")}</span><span>{tr("app.composite.transcriptStats", { language: project.transcript.sourceLanguage.toUpperCase(), segments: segmentCountLabel(project.transcript.segments.length), words: wordCountLabel(project.transcript.words.length) })}</span></div>
                {transcriptionMode === "multispeaker" && <details className="moss-advanced"><summary>{tr("app.moss.advanced.title")}</summary><div><label><span>{tr("app.moss.advanced.prompt")}</span><textarea value={transcriptionPrompt} maxLength={1200} onChange={(event) => setTranscriptionPrompt(event.target.value)} placeholder={tr("app.moss.advanced.promptPlaceholder")}/></label><label><span>{tr("app.moss.advanced.hotwords")}</span><input value={transcriptionHotwords} maxLength={500} onChange={(event) => setTranscriptionHotwords(event.target.value)} placeholder={tr("app.moss.advanced.hotwordsPlaceholder")}/></label><p>{tr("app.moss.advanced.experimental")}</p></div></details>}
                {mossWordTimingUnavailable && <div className="capability-notice"><CircleAlert size={14}/><span><strong>{tr("app.moss.words.unavailable")}</strong><small>{tr("app.moss.words.explanation")}</small></span></div>}
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
                <div className="segment-list" aria-label={tr("app.s0365")}>
                  {filteredSegments.map((segment) => { const association = associationBySegment.get(segment.id); return <SegmentRow key={segment.id} segment={segment} speaker={association ? speakerById.get(association.speakerId) : undefined} speakerManual={association?.source === "manual"} selected={selectedSegmentIds.includes(segment.id)} active={segment.id === selectedId} translation={translation?.[1]} onSelect={(mode) => selectSegmentInWorkbench(segment, mode)} onSave={(text) => editSegment(segment, text)} onSplitAt={(text, offset) => void splitSegmentFromEditor(segment, text, offset)} onMergePrevious={(text) => void mergePreviousFromEditor(segment, text)}/>; })}
                  {!filteredSegments.length && <p className="empty-list">{project.transcript.segments.length ? tr("app.s0366") : tr("app.s0367")}</p>}
                </div>
              </article>

	              <aside className="creator-drawer" aria-label={tr("app.creator.drawer.label")}>
	                <div className="creator-drawer-tabs" role="tablist" aria-label={tr("app.creator.drawer.tabs")}>
	                  {drawerTabs.map((tab) => <button id={`creator-drawer-tab-${tab}`} key={tab} role="tab" aria-controls={`creator-drawer-panel-${tab}`} aria-selected={drawerTab === tab} tabIndex={drawerTab === tab ? 0 : -1} className={drawerTab === tab ? "active" : ""} onKeyDown={(event) => changeDrawerTabFromKeyboard(event, tab)} onClick={() => openCreatorDrawer(tab)}>{tr(({ review: "app.creator.drawer.review", quality: "app.creator.drawer.quality", analysis: "app.creator.drawer.analysis", history: "app.creator.drawer.history", export: "app.creator.drawer.export" } as const)[tab])}{tab === "review" && actionableReviewCount > 0 ? <i>{actionableReviewCount}</i> : null}{tab === "quality" && project.subtitleQuality.issueCount > 0 ? <i>{project.subtitleQuality.issueCount}</i> : null}</button>)}
	                </div>
	                <div className="creator-drawer-body" id={`creator-drawer-panel-${drawerTab}`} role="tabpanel" aria-labelledby={`creator-drawer-tab-${drawerTab}`}>
	                  {drawerTab === "review" && <>
	                    <section className="creator-agent-control">
	                      <header><span><Bot size={15}/><strong>{tr("app.creator.agent.title")}</strong><small>{codexHealth?.available && codexHealth.authenticated ? tr("app.creator.agent.ready", { version: codexHealth.version ?? "Codex CLI" }) : tr("app.creator.agent.unavailable")}</small></span>{agentRunActive && agentRun ? <StatusBadge tone="agent">{Math.round(agentRun.progress * 100)}%</StatusBadge> : null}</header>
	                      <label><span>{tr("app.workflow.label")}</span><select aria-label={tr("app.workflow.label")} value={agentWorkflowKind} disabled={agentRunActive} onChange={(event) => setAgentWorkflowKind(event.target.value as typeof agentWorkflowKind)}><option value="polish">{tr("app.workflow.polish")}</option><option value="proofread">{tr("app.workflow.proofread")}</option><option value="punctuate">{tr("app.workflow.punctuate")}</option><option value="edit">{tr("app.workflow.edit")}</option><option value="translate">{tr("app.workflow.translate")}</option><option value="speaker_names">{tr("app.workflow.speakerNames")}</option></select></label>
	                      {agentWorkflowKind === "translate" && <>
	                        <label><span>{tr("app.workflow.targetLanguage")}</span><select aria-label={tr("app.workflow.targetLanguage")} value={subtitleLanguage} disabled={agentRunActive} onChange={(event) => setSubtitleLanguage(event.target.value)}><option value="en">EN</option><option value="zh">ZH</option><option value="ja">JA</option><option value="ko">KO</option></select></label>
	                        <div className="creator-glossary">
	                          <div><strong>{tr("app.creator.glossary.title")}</strong><small>{tr("app.creator.glossary.version", { version: project.glossary.version })}</small></div>
	                          <textarea aria-label={tr("app.creator.glossary.title")} value={glossaryDraft} disabled={agentRunActive || Boolean(busy)} placeholder={tr("app.creator.glossary.placeholder")} onChange={(event) => setGlossaryDraft(event.target.value)}/>
	                          <button className="button quiet" disabled={agentRunActive || Boolean(busy)} onClick={saveGlossary}>{tr("app.creator.glossary.save")}</button>
	                        </div>
	                      </>}
	                      {agentRun && <div className={`creator-agent-run ${agentRun.status}`} role="status"><span><strong>{tr(`app.creator.agent.status.${agentRun.status}` as Parameters<typeof tr>[0])}</strong><small>{tr("app.creator.agent.batch", { current: agentRun.currentBatch, total: agentRun.batchCount })}</small></span><progress max={1} value={agentRun.progress}/>{["queued", "running", "submitting"].includes(agentRun.status) ? <button onClick={() => void cancelCodexAgent()}>{tr("app.creator.agent.cancel")}</button> : ["failed", "interrupted", "cancelled"].includes(agentRun.status) ? <button onClick={() => void resumeCodexAgent()}><RefreshCw size={12}/>{tr("app.creator.agent.resume")}</button> : null}{agentRun.errorMessage && <JobFailureDetails context="agent" status={agentRun.status} errorCode={agentRun.errorCode} errorMessage={agentRun.errorMessage}/>}</div>}
	                      <div className="creator-agent-actions"><Button ref={agentButtonRef} variant="agent" disabled={!capabilities.canCreateAgentTask || agentRunActive || Boolean(busy) || (agentWorkflowKind === "speaker_names" && speakerTrack?.status !== "ready")} title={agentCapabilityTitle} onClick={startCodexAgent}><Bot size={14}/>{tr("app.creator.agent.start")}</Button><button className="button quiet" disabled={agentRunActive || Boolean(busy)} onClick={openAgentHandoff}>{tr("app.creator.agent.manual")}</button></div>
	                      <p className="runtime-disclosure"><ShieldCheck size={13}/>{tr("app.creator.agent.boundary")}</p>
	                    </section>
	                    <div className="review-panel-scroll creator-review-list" role="region" aria-label={tr("app.s0297")} tabIndex={0}>
	                      {orderedPatchSets.map((set) => <section className="patch-set" key={set.id}><header><span>{set.kind}{set.language ? ` · ${set.language.toUpperCase()}` : ""}</span>{set.items.length > 1 && <div><button onClick={() => reviewAll(set.taskId, "keep")}>{tr("app.s0298")}</button><button onClick={() => reviewAll(set.taskId, "apply")}>{tr("app.s0299")}</button></div>}</header>{set.items.map((item) => <PatchReviewCard key={item.id} item={item} onReview={(action) => reviewPatch(item.id, action)} onSelect={() => { const segment = project.transcript.segments.find((candidate) => candidate.id === item.segmentId); if (segment) selectSegment(segment); }}/>)}</section>)}
	                      {pendingEdits.map((edit) => <article className="review-item" key={edit.id}><span className="review-tag">{tr("app.composite.reviewSuggestion", { kind: cutSuggestionLabel(edit.suggestion?.suggestionType) })}</span><strong>{editReasonLabel(edit)}</strong><p>{edit.suggestion ? tr("app.composite.suggestionEvidence", { range: `${formatTime(edit.start)} — ${formatTime(edit.end)}`, confidence: Math.round(edit.suggestion.confidence * 100) }) : `${formatTime(edit.start)} — ${formatTime(edit.end)}`}</p><div className="cut-actions"><button onClick={() => selectSegment(project.transcript.segments.find((segment) => segment.id === edit.segmentId)!)}>{tr("app.s0303")}</button>{edit.kind === "word_cut" && <button onClick={() => previewCut(edit.id)}><Headphones size={11}/>{tr("app.s0304")}</button>}<button onClick={() => updateCut(edit.id, "apply")}>{tr("app.s0305")}</button></div></article>)}
	                      {audioRisks.map((risk, index) => <article className="review-item audio-risk-item" key={`${risk.kind}-${risk.start}-${index}`}><span className="review-tag warning"><CircleAlert size={12}/>{tr("app.s0306")}</span><strong>{audioRiskLabel(risk.kind)}</strong><p>{tr("app.composite.audioRiskEvidence", { range: `${formatTime(risk.start)} — ${formatTime(risk.end)}`, measured: risk.measuredValue, threshold: risk.threshold, unit: audioUnitLabel(risk.unit) })}</p><button onClick={() => locateAudioRisk(risk)}>{tr("app.s0309")}</button></article>)}
	                      <TranscriptionReviewPanel items={transcriptionReviews} disabled={Boolean(busy)} onLocate={(segmentId) => { const segment = project.transcript.segments.find((item) => item.id === segmentId); if (segment) selectSegment(segment); }} onResolve={resolveTranscriptionReview}/>
	                      {failedTasks.map((task) => <article className="review-item task-item failure" key={task.id}><span className="review-tag failure"><CircleAlert size={12}/>Agent {task.status === "interrupted" ? tr("app.s0018") : tr("app.s0324")}</span><strong>{task.kind}</strong><JobFailureDetails context="agent" status={task.status} errorCode={task.errorCode} errorMessage={task.errorMessage}/><button onClick={() => updateTask(task.id, "retry")}><RefreshCw size={11}/>{tr("app.s0325")}</button></article>)}
	                      {processingTasks.filter((task) => task.id !== agentRun?.taskId).map((task) => <article className="review-item task-item processing" key={task.id}><span className="review-tag agent"><Bot size={12}/>{task.status === "queued" ? tr("app.s0326") : tr("app.s0327")}</span><strong>{task.kind}</strong><p>{Math.round(task.progress * 100)}%</p><button onClick={() => updateTask(task.id, "cancel")}>{tr("app.s0328")}</button></article>)}
	                      {actionableReviewCount === 0 && !agentRunActive && <div className="all-clear"><Check size={20}/><span>{tr("app.s0329")}</span></div>}
	                    </div>
	                  </>}
	                  {drawerTab === "quality" && <section className={`subtitle-quality-summary creator-quality ${project.subtitleQuality.status}`} aria-label={tr("app.s0357")}><div className="subtitle-quality-state">{project.subtitleQuality.status === "good" ? <Check size={15}/> : <CircleAlert size={15}/>}<span><strong>{subtitleQualityStatusLabel(project.subtitleQuality)}</strong><small>{project.subtitleQuality.errorCount}{tr("app.s0358") + " "}{project.subtitleQuality.warningCount}{tr("app.s0359")}</small></span></div><div className="subtitle-quality-filters" aria-label={tr("app.s0360")}><button className={qualityFilter === "all" ? "active" : ""} onClick={() => setQualityFilter("all")}>{tr("app.s0361")}</button><button className={qualityFilter === "error" ? "active" : ""} disabled={!project.subtitleQuality.errorCount} onClick={() => setQualityFilter("error")}>{tr("app.s0362") + " "}{project.subtitleQuality.errorCount}</button><button className={qualityFilter === "warning" ? "active" : ""} disabled={!project.subtitleQuality.warningCount} onClick={() => setQualityFilter("warning")}>{tr("app.s0363") + " "}{project.subtitleQuality.warningCount}</button></div>{visibleQualityIssues.length > 0 ? <div className="subtitle-quality-issues">{visibleQualityIssues.map((issue) => <button className={issue.severity} key={issue.id} onClick={() => locateSubtitleIssue(issue)}><CircleAlert size={12}/><span><strong>{subtitleIssueLabel(issue.kind)}</strong><small>{formatTime(issue.start)}{tr("app.s0364")}</small></span></button>)}</div> : <div className="all-clear"><Check size={20}/><span>{tr("app.creator.quality.ready")}</span></div>}<button className="button primary full" onClick={() => openCreatorDrawer("export")}>{tr("app.creator.quality.continue")}</button></section>}
	                  {drawerTab === "analysis" && <div className="inspector-view creator-analysis">
	                    <SpeechInsightsPanel insights={project.speechInsights} onLocateEvidence={locateSpeechEvidence} onLocatePause={locateSpeechPause}/>
	                    <AudioQualityPanel job={audioAnalysisJob} onStart={startAudioAnalysis} onCancel={cancelAudioAnalysis} onResume={resumeAudioAnalysis} onLocate={locateAudioRisk} disabled={!capabilities.canAnalyzeAudio || Boolean(busy)}/>
	                    <SpeakerTrackPanel packageStatus={speakerPackage} track={speakerTrack} job={projectSpeakerJob} selectedSegmentId={selectedId} disabled={Boolean(busy)} onOpenRuntime={() => setShowRuntime(true)} onAnalyze={startSpeakerAnalysis} onCancel={cancelSpeakerJob} onResume={resumeSpeakerJob} onRename={renameSpeaker} onMerge={mergeSpeaker} onAssign={assignSpeaker}/>
	                    {selectedWords.length > 0 && <section className="word-evidence" aria-label={tr("app.s0376")}>
	                      <div className="word-heading"><div><p className="eyebrow">{tr("app.s0376")}</p><small>{tr("app.s0377")}</small></div>{activeWordRange && <button className="clear-range" onClick={() => setWordRange(null)}>{tr("app.s0378")}</button>}</div>
	                      <div className="word-tokens">{selectedWords.map((word, index) => <button className={activeWordRange && index >= activeWordRange.start && index <= activeWordRange.end ? "selected" : ""} key={word.id} onClick={() => selectWordForCut(index)} title={`${formatTime(word.start)} — ${formatTime(word.end)}${word.confidence == null ? "" : ` · ${Math.round(word.confidence * 100)}%`}`}>{word.text}</button>)}</div>
	                      {activeWordRange && <div className="word-cut-controls"><label>{tr("app.s0379")}<input aria-label={tr("app.s0380")} type="range" min="0" max={selectedWords.length - 1} value={activeWordRange.start} onChange={(event) => setWordRange({ ...activeWordRange, start: Math.min(Number(event.target.value), activeWordRange.end) })}/><small>{selectedWords[activeWordRange.start]?.text}</small></label><label>{tr("app.s0381")}<input aria-label={tr("app.s0382")} type="range" min="0" max={selectedWords.length - 1} value={activeWordRange.end} onChange={(event) => setWordRange({ ...activeWordRange, end: Math.max(Number(event.target.value), activeWordRange.start) })}/><small>{selectedWords[activeWordRange.end]?.text}</small></label><label className="padding-select">{tr("app.s0383")}<select aria-label={tr("app.s0383")} value={cutPadding} onChange={(event) => setCutPadding(Number(event.target.value) as 30 | 100 | 200)}><option value="30">30 ms</option><option value="100">100 ms</option><option value="200">200 ms</option></select></label><button className="create-word-cut" disabled={Boolean(busy)} onClick={createWordCut}><Scissors size={12}/>{tr("app.s0384")}</button></div>}
	                    </section>}
	                  </div>}
	                  {drawerTab === "history" && <div className="inspector-view"><div className="version-block"><div className="section-title"><div><p className="eyebrow">{tr("app.s0385")}</p><h2>{tr("app.s0386")}</h2></div><History size={16}/></div>{project.versions.slice().reverse().map((version) => <button className="version-row" key={version.id} onClick={() => restoreVersion(version.id)}><span><strong>{versionReasonLabel(version.reason)}</strong><small>{new Date(version.createdAt).toLocaleString(uiLocale)}</small></span><RotateCcw size={14}/></button>)}</div></div>}
	                  {drawerTab === "export" && showExportPanel && <Suspense fallback={null}><ExportPanel embedded ref={exportPanelRef} project={project} busy={Boolean(busy)} subtitleMode={subtitleMode} translationLanguageOptions={translationLanguageOptions} translationLanguages={translationLanguages} selectedSubtitleLanguage={selectedSubtitleLanguage} selectedTranslationPending={selectedTranslationPending} selectedTranslationStale={selectedTranslationStale} confirmStaleTranslation={confirmStaleTranslation} exportFormat={exportFormat} structuredExport={structuredExport} includeSpeakerLabels={includeSpeakerLabels} transcriptionExportErrorCount={transcriptionExportErrors.length} transcriptionExportWarningCount={transcriptionExportWarnings.length} confirmTranscriptionWarnings={confirmTranscriptionWarnings} showSubtitleSafeArea={showSubtitleSafeArea} transcriptionExportBlocked={transcriptionExportBlocked} canExportVideo={capabilities.canExportVideo} activeExportRunning={Boolean(activeExport && ["queued", "running"].includes(activeExport.status))} mediaCapabilityTitle={mediaCapabilityTitle} onClose={() => { setShowExportPanel(false); setDrawerTab("quality"); }} onChangeCanvas={(settings) => void changeCanvas(settings)} onSubtitleModeChange={(mode) => { setSubtitleMode(mode); setConfirmStaleTranslation(false); }} onSubtitleLanguageChange={(language) => { setSubtitleLanguage(language); setConfirmStaleTranslation(false); }} onExportFormatChange={(format) => { setExportFormat(format); setConfirmTranscriptionWarnings(false); }} onIncludeSpeakerLabelsChange={setIncludeSpeakerLabels} onConfirmWarningsChange={setConfirmTranscriptionWarnings} onConfirmStaleTranslationChange={setConfirmStaleTranslation} onSubtitleStyleChange={(preset, position) => void changeSubtitleStyle(preset, position)} onShowSafeAreaChange={setShowSubtitleSafeArea} onExportTranscript={exportTranscript} onExportVideo={exportVideo}/></Suspense>}
	                </div>
	              </aside>
	            </section>

            <section className={`timeline-panel ${timelineExpanded ? "expanded" : "collapsed"}`}>
              <div className="section-title"><div><p className="eyebrow">{tr("app.s0387")}</p><h2>{tr("app.s0388")}</h2></div><button className="timeline-toggle" aria-expanded={timelineExpanded} onClick={() => setTimelineExpanded((current) => !current)}>{timelineExpanded ? <ChevronDown size={14}/> : <ChevronUp size={14}/>}{timelineExpanded ? tr("app.creator.timeline.collapse") : tr("app.creator.timeline.expand")}</button></div>
              {timelineExpanded && <>{waveformUrl && <img className="waveform" src={waveformUrl} alt={tr("app.s0390")}/>}
              <div className="timeline-track">{project.transcript.segments.map((segment) => { const edit = project.edits.find((candidate) => candidate.segmentId === segment.id && ["suggested", "proposed", "applied"].includes(candidate.status)); const association = associationBySegment.get(segment.id); const speaker = association ? speakerById.get(association.speakerId) : undefined; return <div className="timeline-segment-shell" key={segment.id} style={{ flexGrow: Math.max(1, segment.end - segment.start) }}><button className={`timeline-segment ${edit && ["suggested", "proposed"].includes(edit.status) ? "suggested" : ""} ${edit?.status === "applied" ? "applied" : ""} ${selectedSegmentIds.includes(segment.id) ? "selected" : ""} ${selectedId === segment.id ? "active" : ""}`} onClick={() => selectSegment(segment)} title={`${speaker ? `${speaker.label} · ` : ""}${segment.text}`}>{speaker && <i className={`speaker-color speaker-${speaker.colorIndex % 6}`}/>}{segment.text}</button>{edit?.status === "applied" && <button className="timeline-restore" onClick={() => void updateCut(edit.id, "restore")}><Scissors size={11}/>{tr("app.s0391")}</button>}</div>; })}</div></>}
            </section>
          </>)}
      </section>
      {showAgentHandoff && project && <Dialog label={tr("app.agent.handoff.title")} className="runtime-dialog agent-handoff-dialog" onClose={() => setShowAgentHandoff(false)} returnFocusRef={agentButtonRef}>
        <button autoFocus className="dialog-close" aria-label={tr("app.agent.handoff.close")} title={tr("app.agent.handoff.close")} onClick={() => setShowAgentHandoff(false)}><X size={18}/></button>
        <p className="eyebrow">{tr("app.agent.handoff.eyebrow")}</p><h2>{agentHandoffTaskId ? tr("app.agent.handoff.readyTitle") : tr("app.agent.handoff.title")}</h2>
        <p className="dialog-copy">{tr("app.agent.handoff.description")}</p>
        <section className="agent-handoff-boundary"><ShieldCheck size={16}/><span>{tr("app.agent.handoff.boundary")}</span></section>
        {!agentHandoffTaskId ? <>
          <label className="agent-handoff-confirm"><input type="checkbox" checked={agentHandoffReady} onChange={(event) => setAgentHandoffReady(event.target.checked)}/><span>{tr("app.agent.handoff.confirm")}</span></label>
          <div className="confirm-actions"><button className="button quiet" onClick={() => setShowAgentHandoff(false)}>{tr("app.s0511")}</button><button className="button agent" disabled={!agentHandoffReady || Boolean(busy)} onClick={() => void createAgentTask()}><Bot size={14}/>{tr("app.agent.handoff.create")}</button></div>
        </> : <>
          <label className="agent-identity-field"><span>{tr("app.agent.handoff.identity")}</span><input value={agentIdentity} onChange={(event) => { setAgentIdentity(event.target.value); setAgentHandoffCopied(false); }} aria-invalid={!isValidAgentIdentity(handoffIdentity)} /><small>{tr("app.agent.handoff.identityHelp")}</small></label>
          {!isValidAgentIdentity(handoffIdentity) && <p className="source-error" role="alert">{tr("app.agent.handoff.identityInvalid")}</p>}
          <label className="agent-handoff-prompt"><span>{tr("app.agent.handoff.promptLabel")}</span><textarea readOnly value={handoffText} aria-label={tr("app.agent.handoff.promptLabel")}/></label>
          <div className="confirm-actions"><button className="button quiet" onClick={() => setShowAgentHandoff(false)}>{tr("app.agent.handoff.later")}</button><button className="button agent" disabled={!handoffText} onClick={() => void copyAgentHandoff()}><Copy size={14}/>{agentHandoffCopied ? tr("app.agent.handoff.copied") : tr("app.agent.handoff.copy")}</button></div>
        </>}
      </Dialog>}
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
	          {!structureStart && <p className="split-time-confirmation"><Clock3 size={14}/>{tr("app.creator.editor.confirmSplitTime")}</p>}
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
      {showRuntime && <RuntimeSettingsDialog
        returnFocusRef={runtimeButtonRef}
        runtime={runtime}
        modelPath={modelPath}
	        transcriptionConfig={transcriptionConfig}
	        transcriptionHealth={transcriptionHealth}
	        transcriptionMode={transcriptionMode}
	        transcriptionLanguage={transcriptionLanguage}
        busy={Boolean(busy)}
        models={models}
        modelJob={modelJob}
        speakerPackage={speakerPackage}
        speakerJob={speakerJob?.kind === "install" ? speakerJob : null}
        updatePolicy={updatePolicy}
        availableUpdate={availableUpdate}
        updateBusy={updateBusy}
        updateError={updateError}
        onClose={() => setShowRuntime(false)}
        onChooseModel={chooseModel}
        onSaveTranscriptionProvider={saveTranscriptionProvider}
	        onCheckTranscriptionProvider={checkTranscriptionProvider}
	        onSelectTranscriptionMode={selectTranscriptionMode}
	        onSelectTranscriptionLanguage={selectTranscriptionLanguage}
        onSelectAsrBackend={changeAsrBackend}
        onSelectModel={(path) => { localStorage.setItem("siaocut.modelPath", path); setModelPath(path); }}
        onInstallModel={installModel}
        onCancelModel={cancelModel}
        onRemoveModel={removeModel}
        onInstallSpeakerPackage={installSpeakerPackage}
        onCancelSpeakerJob={cancelSpeakerJob}
        onResumeSpeakerJob={resumeSpeakerJob}
        onOpenDiagnostics={openDiagnostics}
        onCheckUpdates={() => void checkUpdates()}
        onInstallUpdate={() => void confirmUpdateInstall()}
        onRefresh={() => void initialize()}
      />}
      {showTranscriptionCandidate && transcriptionJob?.status === "awaiting_apply" && <Suspense fallback={null}><TranscriptionCandidateDialog job={transcriptionJob} busy={Boolean(busy)} confirmed={transcriptionApplyConfirmed} onConfirmedChange={setTranscriptionApplyConfirmed} onApply={applyTranscriptionCandidate} onDiscard={discardTranscriptionCandidate} onClose={() => { if (!busy) { setShowTranscriptionCandidate(false); setTranscriptionApplyConfirmed(false); } }}/></Suspense>}
      {currentDeleteCandidate && <Suspense fallback={null}><ProjectDeleteDialog project={currentDeleteCandidate} checking={deletePreflightBusy} deleting={deleteBusy} deletable={Boolean(deletionPreflight?.deletable)} blockerMessage={deleteBlockMessage} error={deleteError} onClose={closeDeleteDialog} onDelete={() => void deleteProject()}/></Suspense>}
      {showSourceImport && <Dialog label={tr("app.s0513")} className="runtime-dialog source-dialog" onClose={() => setShowSourceImport(false)} returnFocusRef={sourceButtonRef}><button autoFocus className="dialog-close" aria-label={tr("app.s0514")} title={tr("app.s0515")} onClick={() => setShowSourceImport(false)}><X size={18}/></button><p className="eyebrow">{tr("app.s0516")}</p><h2>{tr("app.s0517")}</h2><p className="dialog-copy">{tr("app.s0518")}</p>
        {!sourceJob && <form className="source-form" onSubmit={(event) => { event.preventDefault(); void inspectSource(); }}><label><span>{tr("app.s0487")}</span><input autoComplete="url" aria-label={tr("app.s0487")} placeholder="https://…" value={sourceUrl} disabled={Boolean(sourceBusy)} onChange={(event) => { setSourceUrl(event.target.value); setSourcePreview(null); setSourceAuthorized(false); setSourceError(null); }}/></label><button className="button primary" type="submit" disabled={Boolean(sourceBusy) || !isHttpsSourceUrl(sourceUrl)}>{sourceBusy && !sourcePreview ? <LoaderCircle className="spin" size={14}/> : <Search size={14}/>}{tr("app.s0489")}</button></form>}
        {sourcePreview && !sourceJob && <section className="source-preview" aria-label={tr("app.s0519")}><header><span><small>{sourcePreview.extractor}</small><strong>{sourcePreview.title}</strong></span><ShieldCheck size={19}/></header><dl><div><dt>{tr("app.s0491")}</dt><dd>{formatTime(sourcePreview.durationSeconds)}</dd></div><div><dt>{sourcePreview.fileSizeKnown ? tr("app.s0520") : tr("app.s0521")}</dt><dd>{formatBytes(sourcePreview.fileSizeBytes)}</dd></div><div><dt>{tr("app.s0492")}</dt><dd>{sourcePreview.siteMediaId}</dd></div><div><dt>{tr("app.s0522")}</dt><dd>yt-dlp {sourcePreview.toolVersion}</dd></div></dl><p className="source-url" title={sourcePreview.webpageUrl}>{sourcePreview.webpageUrl}</p><label className="source-consent"><input type="checkbox" checked={sourceAuthorized} onChange={(event) => setSourceAuthorized(event.target.checked)}/><span>{tr("app.s0523")}</span></label><button className="button primary full" disabled={!sourceAuthorized || Boolean(sourceBusy)} onClick={() => void startSourceImport()}>{sourceBusy ? <LoaderCircle className="spin" size={14}/> : <Download size={14}/>}{tr("app.s0524")}</button></section>}
        {sourceJob && <section className="source-job" aria-label={tr("app.s0525")}><header><span className={`source-state ${sourceJob.status}`}><i />{sourceStatusLabel(sourceJob.status)}</span><strong>{sourceJob.title}</strong><small>{tr("app.composite.sourceAttempt", { attempt: sourceJob.attemptCount, mediaId: sourceJob.siteMediaId })}</small></header><div className="source-job-progress"><progress value={sourceJob.progress} max={1}/><span>{Math.round(sourceJob.progress * 100)}% · {formatBytes(sourceJob.bytesDownloaded)} / {formatBytes(sourceJob.totalBytes ?? sourceJob.fileSizeBytes)}</span></div><dl><div><dt>{tr("app.s0528")}</dt><dd>yt-dlp {sourceJob.toolVersion}</dd></div><div><dt>{tr("app.s0239")}</dt><dd>{sourceJob.projectId ?? tr("app.s0529")}</dd></div></dl>{["failed", "interrupted"].includes(sourceJob.status) && <JobFailureDetails className="source-job-error" context="source" status={sourceJob.status} errorCode={sourceJob.errorCode} errorMessage={sourceJob.errorMessage}/>}<div className="source-job-actions">{["queued", "running"].includes(sourceJob.status) && <button disabled={Boolean(sourceBusy) || Boolean(sourceJob.cancelRequestedAt)} onClick={() => void cancelSourceImport()}>{sourceJob.cancelRequestedAt ? tr("app.s0317") : tr("app.s0530")}</button>}{["cancelled", "failed", "interrupted"].includes(sourceJob.status) && <button className="primary" disabled={Boolean(sourceBusy)} onClick={() => void resumeSourceImport()}><RefreshCw size={13}/>{tr("app.s0279")}</button>}{!["queued", "running", "finalizing"].includes(sourceJob.status) && <button onClick={resetSourceImport}>{tr("app.s0531")}</button>}</div></section>}
        {sourceError && <div className="source-error" role="alert"><CircleAlert size={15}/><JobFailureDetails context="source" status="failed" errorMessage={sourceError}/></div>}
        <p className="runtime-disclosure">{tr("app.s0532")}</p>
      </Dialog>}
    </main>);
}
export default WorkbenchController;
