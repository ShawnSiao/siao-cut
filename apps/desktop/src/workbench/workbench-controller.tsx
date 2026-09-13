import { TaskRecords } from "../features/ai-assistance/TaskRecords";
import { TranscriptionStatus } from "../features/background-tasks/TranscriptionStatus";
import { Bot,Check,ChevronDown,ChevronRight,ChevronUp,CircleAlert,Clock3,Copy,Cpu,Download,FileText,FileVideo2,FolderOpen,FolderPlus,Headphones,History,Link2,ListChecks,LoaderCircle,MoreHorizontal,MoveHorizontal,Play,Redo2,RefreshCw,RotateCcw,Scissors,Search,Settings2,ShieldCheck,Sparkles,Trash2,Undo2,Users,X } from "lucide-react";
import { lazy,Suspense,useCallback,useEffect,useMemo,useRef,useState,type CSSProperties,type KeyboardEvent as ReactKeyboardEvent } from "react";
import { agentTaskStatusLabel,audioRiskLabel,audioUnitLabel,autoStageLabel,autoStatusLabel,cutSuggestionLabel,editReasonLabel,formatTime,getProjectCapabilities,hasMeaningfulSubtitleText,parseTranscriptionLanguage,segmentCountLabel,structureEditLabel,subtitleCountLabel,subtitleIssueLabel,subtitleQualityStatusLabel,taskLabel,TRANSCRIPTION_LANGUAGE_STORAGE_KEY,versionReasonLabel,wordCountLabel,workflowProfileLabel,type SegmentSelectionMode } from "../app-view-model";
import { JobFailureDetails } from "../components/job-failure";
import { Button,Dialog,IconButton,StatusBadge } from "../components/ui";
import { AudioQualityPanel,PatchReviewCard,RuntimeChecklist,SegmentRow,SpeakerTrackPanel,SpeechInsightsPanel,TranscriptionReviewPanel } from "../components/workbench-panels";
import { agentReviewClient } from "../domains/agent-review-client";
import { authorizeArtifact,authorizeMedia,pickMedia } from "../domains/desktop-platform-client";
import { exportRuntimeClient } from "../domains/export-runtime-client";
import { projectSessionClient } from "../domains/project-session-client";
import { isValidAgentIdentity,useAiReviewSession } from "../features/ai-assistance/use-ai-review-session";
import { ACTIVE_AUTO_WORKFLOW_STATUSES,AUTO_WORKFLOW_DISMISSED_STORAGE_KEY,TERMINAL_AUTO_WORKFLOW_STATUSES } from "../features/background-tasks/auto-workflow-snapshots";
import { resourceMaintenance } from "../features/background-tasks/resource-maintenance";
import { localResourceError } from "../features/background-tasks/resource-messages";
import { useAudioAnalysisSession } from "../features/background-tasks/use-audio-analysis-session";
import { useAutoWorkflowSession } from "../features/background-tasks/use-auto-workflow-session";
import { useBackgroundSession } from "../features/background-tasks/use-background-session";
import { useResourceCompletion } from "../features/background-tasks/use-resource-completion";
import { useResourceSession } from "../features/background-tasks/use-resource-session";
import { useRuntimeSession } from "../features/background-tasks/use-runtime-session";
import { useSourceImportSession } from "../features/background-tasks/use-source-import-session";
import { useSpeakerSession } from "../features/background-tasks/use-speaker-session";
import { useTranscriptionReviewSession } from "../features/background-tasks/use-transcription-review-session";
import { useTranscriptionStartSession } from "../features/background-tasks/use-transcription-start-session";
import { EditingStatus } from "../features/editing/EditingStatus";
import { createTranscriptCommands } from "../features/editing/transcript-commands";
import { useEditingSession } from "../features/editing/use-editing-session";
import { useStructureEditing } from "../features/editing/use-structure-editing";
import { useSubtitleImportSession } from "../features/editing/use-subtitle-import-session";
import { VirtualTranscript } from "../features/editing/virtual-transcript";
import { createPresentationCommands } from "../features/export/presentation-commands";
import { useExportSession } from "../features/export/use-export-session";
import { createMediaCommands } from "../features/playback/media-commands";
import { resolveImportedProjectMedia,usePlaybackSession } from "../features/playback/use-playback-session";
import { useProjectDeletion } from "../features/project-session/use-project-deletion";
import { useProjectLifecycle } from "../features/project-session/use-project-lifecycle";
import { useProjectReadModels } from "../features/project-session/use-project-read-models";
import { useProjectSession } from "../features/project-session/use-project-session";
import { useWorkbenchStartup } from "../features/project-session/use-workbench-startup";
import { useAppUpdater } from "../hooks/use-app-updater";
import { useWorkbenchFeedback } from "../hooks/use-workbench-feedback";
import { changeUiLocale,getUiLocale,tr,type UiLocale } from "../i18n";
import type { AudioRisk,AutoWorkflow,Project,Segment,SourceImportJob,SpeechEvidence,SpeechPause,SubtitleQualityIssue,TranscriptionLanguage } from "../types";
import type { ReviewQueueItem } from "./review-queue";
import { groupSubtitleQualityIssues } from "./subtitle-quality-groups";
import type { TimelineReviewMarker } from "./subtitle-timeline-panel";
import { useFocusReviewState } from "./use-focus-review-state";
import { cycleWorkbenchFocus,useTranscriptNavigation } from "./use-transcript-navigation";
import type { WorkbenchActivity,WorkbenchActivityInputs } from "./workbench-activity";
import type { WorkbenchActivityAction } from "./workbench-activity-center";
export { AudioQualityPanel,PatchReviewCard,SpeakerPackageManager,SpeakerTrackPanel,SpeechInsightsPanel } from "../components/workbench-panels";
export { AUTO_WORKFLOW_DISMISSED_STORAGE_KEY,parseDismissedAutoWorkflowIds,upsertAutoWorkflowSnapshot } from "../features/background-tasks/auto-workflow-snapshots";


export { resolveCanvasMedia,resolveImportedProjectMedia } from "../features/playback/use-playback-session";
export function resolveCaptionKaraokeStyle(
    playing: boolean,
    progress: number,
    primaryColor: string,
    secondaryColor: string,
): CSSProperties | undefined {
    if (!playing)
        return undefined;
    const clampedProgress = Math.max(0, Math.min(1, progress));
    return {
        color: secondaryColor,
        "--caption-progress": `${clampedProgress * 100}%`,
        "--caption-primary-color": primaryColor,
    } as CSSProperties;
}

export function resolveCaptionSegment(
    segments: Segment[],
    selected: Segment | null | undefined,
    currentTime: number,
    playing: boolean,
) {
    const timedSegment = segments.find((segment) => currentTime >= segment.start && currentTime < segment.end) ?? null;
    return timedSegment ?? (playing ? null : selected ?? null);
}

export function resolveFocusCaptionText(
    mode: "source" | "translated" | "bilingual",
    sourceText: string,
    translatedText: string,
    missingTranslationText: string,
) {
    if (mode === "translated")
        return { primary: translatedText || missingTranslationText, secondary: "", missingTranslation: !translatedText };
    if (mode === "bilingual")
        return { primary: sourceText, secondary: translatedText || missingTranslationText, missingTranslation: !translatedText };
    return { primary: sourceText, secondary: "", missingTranslation: false };
}

export { resolvePlaybackDuration } from "../features/playback/use-playback-session";

const TranscriptionCandidateDialog = lazy(() => import("../components/transcription-candidate-dialog"));
const ExportPanel = lazy(() => import("../components/export-panel"));
const WorkbenchTaskMenu = lazy(() => import("./workbench-task-menu"));
const FocusReviewPanel = lazy(() => import("./focus-review-panel"));
const FocusReviewToolbar = lazy(() => import("./focus-review-panel").then((module) => ({ default: module.FocusReviewToolbar })));
const SubtitleTimelinePanel = lazy(() => import("./subtitle-timeline-panel").then((module) => ({ default: module.SubtitleTimelinePanel }))); const AutoWorkflowProfileSelector = lazy(() => import("./auto-workflow-profile-selector"));
const ProjectDeleteDialog = lazy(() => import("../components/project-delete-dialog"));
const AppCommandMenu = lazy(() => import("../components/app-command-menu"));
const RuntimeSettingsDialog = lazy(() => import("../components/runtime-settings-dialog"));
const SourceImportDialog = lazy(() => import("../components/source-import-dialog"));
const LocalResourceSetupDialog = lazy(() => import("../components/local-resource-ui").then((module) => ({ default: module.LocalResourceSetupDialog })));
const AgentHandoffDialog = lazy(() => import("../components/agent-handoff-dialog"));
const AiExecutionConfirm = lazy(() => import("../features/ai-assistance/AiExecutionConfirm"));
const AutoWorkflowAiTarget = lazy(() => import("../features/ai-assistance/AutoWorkflowAiTarget"));
const SubtitleImportDialog = lazy(() => import("../components/subtitle-import-dialog"));
const QuickRetranscriptionDialog = lazy(() => import("../components/quick-retranscription-dialog"));

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
    const { projects,setProjects,project,projectRef,setProject,activeProjectIdRef,beginProjectLoad,isCurrentProjectLoad,invalidateProjectLoads,updateProjectSummary,acknowledgeEdit,nextProjectOffset,replaceProjectPage,projectPageLoading,loadMoreProjects } = useProjectSession();
    const { mediaUrl,setMediaUrl,waveformUrl,setWaveformUrl,cutPreview,setCutPreview,playback,setPlayback,videoRef,handleVideoTimeUpdate,handleVideoLoadedMetadata,seekTimeline,toggleTimelinePlayback } = usePlaybackSession(project);
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [selectedSegmentIds, setSelectedSegmentIds] = useState<string[]>([]);
    const [selectionAnchorId, setSelectionAnchorId] = useState<string | null>(null);
    const { busy, notice, error, setBusy, setNotice, setError } = useWorkbenchFeedback(tr("app.s0038"));
    const { audioAnalysisJob,setAudioAnalysisJob,speakerJob,setSpeakerJob,speakerJobs,setSpeakerJobs,sourceJob,setSourceJob,modelJob,setModelJob,resourceJob,setResourceJob,autoWorkflow,setAutoWorkflow,autoWorkflows,setAutoWorkflows,transcriptionCommands,transcriptionTasks,transcriptionJob,setTranscriptionJob } = useBackgroundSession({projectId:project?.id,activeProjectIdRef,setError,setNotice,
      onSourceCompleted:(job)=>onSourceJobCompleted(job),onWorkflowTransition:(job)=>onWorkflowTransition(job),
      onModelsReady:(available,id)=>{setModels(available);const installed=available.find((model)=>model.id===id);if(installed){localStorage.setItem("siaocut.modelPath",installed.path);setModelPath(installed.path);setModelPathAvailable(installed.installed&&installed.verified===true);}},
      onSpeakerPackageReady:(status)=>setSpeakerPackage(status),onSpeakerAnalysisReady:async(id)=>{await refreshProject(id);await refreshSpeakerTrack(id);},
      onSourceError:(message)=>setSourceError(message),onResourceError:(error)=>setResourceError(localResourceError(error)),
      onTranscriptionApplied:async(job)=>{if(activeProjectIdRef.current!==job.projectId)return;await refreshProject(job.projectId,true);if(activeProjectIdRef.current===job.projectId)setNotice(tr("app.moss.job.completed"));},
    });
    const editing = useEditingSession(project, acknowledgeEdit);
    const { transcriptionMode, setTranscriptionMode, transcriptionConfig, setTranscriptionConfig, transcriptionHealth, setTranscriptionHealth, pendingCandidateJobId, setPendingCandidateJobId, showTranscriptionCandidate, setShowTranscriptionCandidate, transcriptionApplyConfirmed, setTranscriptionApplyConfirmed, transcriptionReviews, setTranscriptionReviews, transcriptionPrompt, setTranscriptionPrompt, transcriptionHotwords, setTranscriptionHotwords, refreshTranscription, saveTranscriptionProvider, checkTranscriptionProvider, cancelTranscription, resumeTranscription, applyTranscriptionCandidate, discardTranscriptionCandidate, resolveTranscriptionReview } = useTranscriptionReviewSession({project, editing: editing.session, transcriptionJob, setTranscriptionJob, transcriptionCommands, activeProjectIdRef, isCurrentProjectLoad, refreshProject: (id) => refreshProject(id), refreshSpeakerTrack: (id) => refreshSpeakerTrack(id), withBusy: (label, action) => withBusy(label, action), setNotice});
    const { speakerPackage, setSpeakerPackage, speakerTrack, setSpeakerTrack, refreshSpeakerTrack, installSpeakerPackage, startSpeakerAnalysis, cancelSpeakerJob, resumeSpeakerJob, renameSpeaker, mergeSpeaker, assignSpeaker } = useSpeakerSession({project, editing: editing.session, speakerJob, setSpeakerJob, setSpeakerJobs, activeProjectIdRef, isCurrentProjectLoad, refreshProject: (id) => refreshProject(id), withBusy: (label, action) => withBusy(label, action), setNotice});
    const { runtime, setRuntime, models, setModels, modelPath, setModelPath, modelPathAvailable, setModelPathAvailable, changeAsrBackend, openDiagnostics, chooseModel, installModel, cancelModel, removeModel } = useRuntimeSession({modelJob, setModelJob, setNotice, withBusy: (label, action) => withBusy(label, action)});
    const { refreshLatestAudioAnalysis, startAudioAnalysis, cancelAudioAnalysis, resumeAudioAnalysis } = useAudioAnalysisSession({project, mediaUrl, runtime, audioAnalysisJob, setAudioAnalysisJob, activeProjectIdRef, isCurrentProjectLoad, setNotice, withBusy: (label, action) => withBusy(label, action)});
    const { localResources, setLocalResources, resourcePlan, setResourcePlan, resourceCapability, setResourceCapability, resourceProfile, setResourceProfile, resourceSetupReason, setResourceSetupReason, resourceSelectedRoot, setResourceSelectedRoot, resourceBusy, setResourceBusy, resourceError, setResourceError, showResourceSetup, setShowResourceSetup, pendingResourceAction, setPendingResourceAction, handledResourceJobRef, openResourcePreparation, changeResourceProfile, chooseResourceLocation, confirmResourceLocation, startResourcePreparation, cancelResourcePreparation, resumeResourcePreparation, closeResourcePreparation } = useResourceSession({getJob: () => resourceJob, setJob: (job) => setResourceJob(job), setRuntime, setShowRuntime: (show) => setShowRuntime(show), setShowSourceImport: (show) => setShowSourceImport(show), setNotice});
    const {removeResourceCapability, cleanupLocalResources, rollbackResourceCapability} = resourceMaintenance({setResourceBusy, setLocalResources, setRuntime, setNotice, setError});
    const [resumeSourceInspection, setResumeSourceInspection] = useState(false);
    const { updatePolicy, setUpdatePolicy, availableUpdate, updateBusy, updateError, checkUpdates, confirmUpdateInstall } = useAppUpdater(setNotice);
    const { sourcePreview, setSourcePreview, sourceUrl, setSourceUrl, sourceAuthorized, setSourceAuthorized, sourceAuthMode, setSourceAuthMode, sourceBrowser, setSourceBrowser, sourceBrowserAuthorized, setSourceBrowserAuthorized, sourceBusy, setSourceBusy, sourceError, setSourceError, showSourceImport, setShowSourceImport, sourceJobOriginProjectIdsRef, inspectSource, startSourceImport, cancelSourceImport, resumeSourceImport, resetSourceImport } = useSourceImportSession({localResources, runtime, activeProjectIdRef, getJob: () => sourceJob, setJob: (job) => setSourceJob(job), setNotice, prepareResources: async () => {setPendingResourceAction("inspect_url"); await openResourcePreparation("url_import", "on_demand");}});
    const [transcriptionLanguage, setTranscriptionLanguage] = useState<TranscriptionLanguage>(() => parseTranscriptionLanguage(localStorage.getItem(TRANSCRIPTION_LANGUAGE_STORAGE_KEY)));
    const { showAutoWorkflow, setShowAutoWorkflow, trackedAutoWorkflowIds, setTrackedAutoWorkflowIds, dismissedAutoWorkflowIds, setDismissedAutoWorkflowIds, autoInputKind, setAutoInputKind, autoMediaPath, setAutoMediaPath, autoUrl, setAutoUrl, autoSourcePreview, setAutoSourcePreview, autoAuthorized, setAutoAuthorized, autoTranslate, setAutoTranslate, autoProfile, setAutoProfile, autoAiSelection, setAutoAiSelection, autoTranslationLanguage, setAutoTranslationLanguage, autoBurnSubtitles, setAutoBurnSubtitles, autoSubtitleMode, setAutoSubtitleMode, autoBusy, setAutoBusy, autoError, setAutoError, autoWorkflowErrors, setAutoWorkflowErrors, autoWorkflowOriginProjectIdsRef, chooseAutoMedia, inspectAutoSource, showAutoWorkflowStatus, dismissAutoWorkflowStatus, startAutoWorkflow, cancelAutoWorkflow, continueAutoWorkflow, openAutoProject } = useAutoWorkflowSession({runtime, modelPath, modelPathAvailable, setModelPathAvailable, transcriptionLanguage, uiLocale, activeProjectIdRef, getWorkflow: () => autoWorkflow, setWorkflow: setAutoWorkflow, setWorkflows: setAutoWorkflows, openProject: async (id) => {await activateProject(id);}, setNotice});
    const [showRuntime, setShowRuntime] = useState(false);
    const [showExportPanel, setShowExportPanel] = useState(false);
    const [drawerTab, setDrawerTab] = useState<"review" | "quality" | "analysis" | "history" | "export">("review");
    const readModels = useProjectReadModels(project,drawerTab,setProject,setError);
    const [playerExpanded, setPlayerExpanded] = useState(false);
    const [navigationCollapsed, setNavigationCollapsed] = useState(false);
    const [reviewFocusDetailId, setReviewFocusDetailId] = useState<string | null>(null);
    const [showSubtitleSafeArea, setShowSubtitleSafeArea] = useState(true);
    const [showMoreMenu, setShowMoreMenu] = useState(false);
    const [search, setSearch] = useState("");
    const [replacement, setReplacement] = useState("");
    const [emptyReplacementConfirmed, setEmptyReplacementConfirmed] = useState(false);
    const [qualityFilter, setQualityFilter] = useState<"all" | "warning" | "error">("all");
    const { showSubtitleImport, setShowSubtitleImport, subtitleImportPath, setSubtitleImportPath, subtitleImportPreview, setSubtitleImportPreview, subtitleImportBusy, setSubtitleImportBusy, subtitleImportError, setSubtitleImportError, subtitleReplaceConfirmed, setSubtitleReplaceConfirmed, openSubtitleImport, inspectSubtitleFile, confirmSubtitleImport } = useSubtitleImportSession({project, editing: editing.session, setNotice, onApplied: async (next) => {
        setProject(next); updateProjectSummary(next); setSelectedId(next.transcript.segments[0]?.id ?? null);
        setWordRange(null); setCutPreview(null); setQualityFilter("all");
        await Promise.all([refreshSpeakerTrack(next.id), refreshTranscription(next.id)]);
    }});
    const { activeExport,setActiveExport,exportFormat,setExportFormat,includeSpeakerLabels,setIncludeSpeakerLabels,confirmTranscriptionWarnings,setConfirmTranscriptionWarnings,confirmStaleTranslation,setConfirmStaleTranslation,confirmUncutExport,setConfirmUncutExport,subtitleDelivery,setSubtitleDelivery,subtitleMode,setSubtitleMode,subtitleLanguage,setSubtitleLanguage,translationLanguages,translationLanguageOptions,selectedSubtitleLanguage,selectedTranslation,translation,selectedTranslationPending,selectedTranslationStale,transcriptionExportErrors,transcriptionExportWarnings,structuredExport,transcriptionExportBlocked,exportTranscript,exportVideo,cancelExport,retryExport } = useExportSession({getVersion: (id) => projectRef.current?.id === id ? projectRef.current.history.currentVersionId ?? null : null,project,speakerTrack,transcriptionReviews,mediaUrl,setNotice,setError,activeProjectIdRef,withBusy:(label,action) => withBusy(label,action),flush:() => editing.session.flush(project?.id)});
    const { agentWorkflowKind,setAgentWorkflowKind,codexHealth,setCodexHealth,agentRun,setAgentRun,showAgentHandoff,setShowAgentHandoff,showAiExecutionConfirm,setShowAiExecutionConfirm,aiApprovalTaskId,setAiApprovalTaskId,agentHandoffTaskId,setAgentHandoffTaskId,agentIdentity,setAgentIdentity,agentHandoffReady,setAgentHandoffReady,agentHandoffCopied,setAgentHandoffCopied,taskActions,setTaskActions,glossaryDraft,setGlossaryDraft,agentButtonRef,agentHandoffReturnFocusRef,taskActionIdsRef,openAgentHandoff,openExistingAgentHandoff,saveGlossary,createAgentTask,startAiAssistance,cancelCodexAgent,resumeCodexAgent,handoffTask,lockedHandoffIdentity,handoffIdentity,handoffIdentityLocked,handoffText,aiConfirmationSegments,aiConfirmationCharacters,aiConfirmationLabel,aiConfirmationContext,copyAgentHandoff,updateTask,reviewPatch,reviewAll } = useAiReviewSession({project,mediaUrl,subtitleLanguage,uiLocale,editing:editing.session,setProject,updateProjectSummary,setConfirmStaleTranslation,setNotice,setError,activeProjectIdRef,invalidateProjectLoads,onReview:() => setDrawerTab("review"),refreshProject:(id) => refreshProject(id),withBusy:(label,action) => withBusy(label,action)});
    const [wordRange, setWordRange] = useState<{
        segmentId: string;
        start: number;
        end: number;
    } | null>(null);
    const [cutPadding, setCutPadding] = useState<30 | 100 | 200>(100);
    const transcriptNavigation = useTranscriptNavigation(project, playback.currentTime, playback.playing);
    const { currentDeleteCandidate, deleteBusy, deleteError, deletionPreflight, deletePreflightBusy, deleteBlockMessage, openDeleteDialog, closeDeleteDialog, deleteProject } = useProjectDeletion({
        projects, onDeleted: async (deleting) => {
            const remaining = projects.filter((item) => item.id !== deleting.id);
            setProjects(remaining);
            if (projectRef.current?.id === deleting.id) {
                activeProjectIdRef.current = remaining[0]?.id ?? null;
                resetProjectScopedState();
                if (remaining[0]) await refreshProject(remaining[0].id, true);
            }
            setNotice(tr("app.s0082", { "0": deleting.title }));
        },
    });
    const runtimeButtonRef = useRef<HTMLButtonElement>(null);
    const sourceButtonRef = useRef<HTMLButtonElement>(null);
    const autoButtonRef = useRef<HTMLButtonElement>(null);
    const exportButtonRef = useRef<HTMLButtonElement>(null);
    const exportPanelRef = useRef<HTMLElement>(null);
    const commandMoreRef = useRef<HTMLDivElement>(null);
    const searchInputRef = useRef<HTMLInputElement>(null);
    const replacementInputRef = useRef<HTMLInputElement>(null);
    const subtitleImportButtonRef = useRef<HTMLButtonElement>(null);
    const busyRef = useRef(false);
    const { showQuickRetranscription, quickRetranscriptionPreflight, quickRetranscriptionChecking, quickRetranscriptionConfirmed, setQuickRetranscriptionConfirmed, quickRetranscriptionError, resumeLocalTranscription, setResumeLocalTranscription, transcribe, openQuickRetranscription, closeQuickRetranscription, confirmQuickRetranscription } = useTranscriptionStartSession({project, projectRef, mediaUrl, runtime, modelPath, modelPathAvailable, setModelPathAvailable, transcriptionMode, transcriptionHealth, transcriptionLanguage, transcriptionPrompt, transcriptionHotwords, transcriptionCommands, setTranscriptionJob, localResources, setPendingResourceAction, openResourcePreparation, busyRef, setBusy, setError, setNotice, editing: editing.session, withBusy: (label, action) => withBusy(label, action), refreshProject: (id, media) => refreshProject(id, media), refreshSpeakerTrack: (id) => refreshSpeakerTrack(id), refreshTranscription: (id) => refreshTranscription(id)});
    const { focusReview, enterFocusReview, exitFocusReview, resetFocusReview } = useFocusReviewState({
        projectAvailable: Boolean(project), mediaAvailable: Boolean(mediaUrl), mediaMissingMessage: tr("app.focusReview.mediaMissing"),
        drawerTab, selectedId, selectedSegmentIds, playerExpanded, setDrawerTab, setSelectedId, setSelectedSegmentIds,
        setSelectionAnchorId, setPlayerExpanded, setShowExportPanel, setError,
    });
    const resetProjectScopedState = useCallback((next: Project | null = null) => {
        videoRef.current?.pause();
        setPlayback({ playing: false, currentTime: 0, duration: next?.media.durationSeconds ?? 0 });
        setProject(next);
        const firstSegmentId = next?.transcript.segments[0]?.id ?? null;
        setSelectedId(firstSegmentId);
        setSelectedSegmentIds(firstSegmentId ? [firstSegmentId] : []);
        setSelectionAnchorId(firstSegmentId);
        setMediaUrl(null);
        setWaveformUrl(null);
        setActiveExport(null);
        setAudioAnalysisJob(null);
        setSpeakerTrack(null);
        setTranscriptionJob(null);
        setTranscriptionReviews([]);
        setAgentRun(null);
        setTaskActions({});
        setWordRange(null);
        setCutPreview(null);
        resetFocusReview();
    }, [resetFocusReview]);
    const refreshLatestExport = useCallback(async (projectId: string, loadSequence?: number) => {
        const envelope = await exportRuntimeClient.listVideoExports(projectId);
        if (activeProjectIdRef.current === projectId && (loadSequence === undefined || isCurrentProjectLoad(projectId, loadSequence)))
            setActiveExport(envelope.jobs?.[0] ?? null);
    }, [isCurrentProjectLoad]);
    const { refreshProject, activateProject } = useProjectLifecycle(
        { projectRef, activeProjectIdRef, beginProjectLoad, isCurrentProjectLoad, setProject, updateProjectSummary },
        {
            reset: resetProjectScopedState,
            prepareMedia: async (next) => {
                const [mediaUrl, waveformUrl] = await Promise.all([
                    authorizeArtifact(next.id, "preview").then((preview) => preview ?? authorizeMedia(next.id)),
                    authorizeArtifact(next.id, "waveform"),
                ]);
                return { mediaUrl, waveformUrl };
            },
            mediaReady: (next, media) => {
                videoRef.current?.pause();
                setPlayback({ playing: false, currentTime: 0, duration: next.media.durationSeconds ?? 0 });
                setMediaUrl(media.mediaUrl); setWaveformUrl(media.waveformUrl);
                setActiveExport(null); setWordRange(null); setCutPreview(null);
            },
            projectReady: async (next, sequence, opening) => {
                setSelectedId((current) => next.transcript.segments.some((segment) => segment.id === current) ? current : next.transcript.segments[0]?.id ?? null);
                if (!opening) return;
                const [, , , , runs] = await Promise.all([
                    refreshLatestExport(next.id, sequence), refreshLatestAudioAnalysis(next.id, sequence),
                    refreshSpeakerTrack(next.id, sequence), refreshTranscription(next.id, sequence),
                    agentReviewClient.listAgentRuns(next.id).catch(() => null),
                ]);
                if (activeProjectIdRef.current === next.id && isCurrentProjectLoad(next.id, sequence))
                    setAgentRun(runs?.agentRuns?.[0] ?? null);
            },
        },
    );
    const initialize = useWorkbenchStartup({
        setBusy, setError, setUpdatePolicy, setAutoWorkflows, setAutoWorkflow, setTrackedAutoWorkflowIds, setModels, setModelJob, setSpeakerPackage, setSpeakerJobs, setSpeakerJob, setTranscriptionHealth, setTranscriptionConfig, setCodexHealth, setLocalResources, setResourceProfile, setResourceCapability, setResourceSetupReason, setResourcePlan, setShowResourceSetup, setResourceJob, setSourceJob, setRuntime, setModelPath, setModelPathAvailable, replaceProjectPage,
        flush: () => editing.session.flush(activeProjectIdRef.current ?? undefined),
        restoreProject: async (page) => {
            const id = activeProjectIdRef.current ?? page.items[0]?.id;
            if (!id) { resetProjectScopedState(); return; }
            if (projectRef.current?.id === id) await refreshProject(id);
            else await activateProject(id);
        },
    });
    useEffect(() => {
        if (dismissedAutoWorkflowIds.length)
            localStorage.setItem(AUTO_WORKFLOW_DISMISSED_STORAGE_KEY, JSON.stringify(dismissedAutoWorkflowIds));
        else
            localStorage.removeItem(AUTO_WORKFLOW_DISMISSED_STORAGE_KEY);
    }, [dismissedAutoWorkflowIds]);
    useEffect(() => {
        const activeIds = autoWorkflows
            .filter((workflow) => ACTIVE_AUTO_WORKFLOW_STATUSES.has(workflow.status))
            .map((workflow) => workflow.id);
        if (!activeIds.length)
            return;
        setTrackedAutoWorkflowIds((current) => {
            const next = Array.from(new Set([...current, ...activeIds]));
            return next.length === current.length ? current : next;
        });
        setDismissedAutoWorkflowIds((current) => {
            const next = current.filter((id) => !activeIds.includes(id));
            return next.length === current.length ? current : next;
        });
    }, [autoWorkflows]);
    const onSourceJobCompleted = async (nextJob:SourceImportJob) => {
                    setSourceError(null);
                    let imported: Project;
                    try {
                        imported = await projectSessionClient.loadProject(nextJob.projectId!);
                    }
                    catch (cause) {
                        setSourceError(cause instanceof Error ? cause.message : String(cause));
                        setNotice(tr("app.error.sourceImportOpenFailed"));
                        return;
                    }
                    updateProjectSummary(imported);
                    setShowSourceImport(false);
                    setNotice(tr("app.s0067"));
                    const hasOrigin = sourceJobOriginProjectIdsRef.current.has(nextJob.id);
                    const originProjectId = sourceJobOriginProjectIdsRef.current.get(nextJob.id);
                    const projectScopeBusy = busyRef.current || structureBusy || Boolean(subtitleImportBusy) || deleteBusy || deletePreflightBusy || Boolean(autoBusy) || Boolean(sourceBusy) || Object.keys(taskActions).length > 0;
                    const shouldActivate = activeProjectIdRef.current === imported.id
                        || (hasOrigin && activeProjectIdRef.current === originProjectId && !projectScopeBusy);
                    sourceJobOriginProjectIdsRef.current.delete(nextJob.id);
                    if (shouldActivate) {
                        activeProjectIdRef.current = imported.id;
                        resetProjectScopedState(imported);
                        const media = await resolveImportedProjectMedia(imported.id);
                        setMediaUrl(media.mediaUrl);
                        setWaveformUrl(media.waveformUrl);
                        await refreshProject(imported.id);
                        if (media.warning)
                            setError(tr("app.error.sourceImportPreviewUnavailable"));
                    }
    };
    const onWorkflowTransition = async (next:AutoWorkflow) => {
                if (next.projectId) {
                    const hasOrigin = autoWorkflowOriginProjectIdsRef.current.has(next.id);
                    const originProjectId = autoWorkflowOriginProjectIdsRef.current.get(next.id);
                    const projectScopeBusy = busyRef.current || structureBusy || Boolean(subtitleImportBusy) || deleteBusy || deletePreflightBusy || Boolean(autoBusy) || Boolean(sourceBusy) || Object.keys(taskActions).length > 0;
                    if (autoWorkflow?.id === next.id
                        && activeProjectIdRef.current !== next.projectId
                        && hasOrigin
                        && activeProjectIdRef.current === originProjectId
                        && !projectScopeBusy) {
                        activeProjectIdRef.current = next.projectId;
                        autoWorkflowOriginProjectIdsRef.current.delete(next.id);
                        resetProjectScopedState();
                        await refreshProject(next.projectId, true);
                    }
                    else if (activeProjectIdRef.current === next.projectId && ["needs_review", "completed"].includes(next.status)) {
                        await refreshProject(next.projectId, next.status === "completed");
                    }
                    else if (activeProjectIdRef.current !== next.projectId) {
                        await refreshProject(next.projectId);
                    }
                }
    };
    const selected = project?.transcript.segments.find((segment) => segment.id === selectedId) ?? null;
    const selectedWords = project?.transcript.words.filter((word) => word.segmentId === selectedId) ?? [];
    const activeWordRange = wordRange?.segmentId === selectedId ? wordRange : null;
    const filteredSegments = useMemo(() => {
        const issueSegmentIds = qualityFilter === "all" ? null : new Set(project?.subtitleQuality.issues.filter((issue) => issue.severity === qualityFilter).map((issue) => issue.segmentId));
        return project?.transcript.segments.filter((segment) => segment.text.toLowerCase().includes(search.toLowerCase()) && (!issueSegmentIds || issueSegmentIds.has(segment.id))) ?? [];
    }, [project, qualityFilter, search]);
    const visibleQualityIssues = project?.subtitleQuality.issues.filter((issue) => qualityFilter === "all" || issue.severity === qualityFilter) ?? [];
    const visibleQualityIssueGroups = groupSubtitleQualityIssues(visibleQualityIssues);
    const selectedSegments = useMemo(() => project?.transcript.segments.filter((segment) => selectedSegmentIds.includes(segment.id)) ?? [], [project, selectedSegmentIds]);
    const allVisibleSegmentsSelected = filteredSegments.length > 0 && filteredSegments.every((segment) => selectedSegmentIds.includes(segment.id));
    const selectedScopeLabel = selectedSegments.length
        ? tr("app.s0071", { "0": selectedSegments.length, "1": formatTime(selectedSegments[0].start), "2": formatTime(selectedSegments.at(-1)!.end) }) : tr("app.s0072");
    const { structureEditMode,setStructureEditMode,structureStart,setStructureStart,structureEnd,setStructureEnd,structureTextOffset,setStructureTextOffset,structureDelta,setStructureDelta,structureBusy,setStructureBusy,structureError,setStructureError,firstSelectedIndex,secondSelectedIndex,mergeCandidatesAdjacent,splitTextOffset,splitCharacters,splitLeftText,splitRightText,splitInputsValid,timingStart,timingEnd,timingInputsValid,timingChanged,structureSubmitDisabled,openStructureEdit,splitSegmentFromEditor,mergePreviousFromEditor,applyStructureEdit,nudgeTimelineSegment } = useStructureEditing({project,selectedSegments,editing:editing.session,setSelectedId,setSelectedSegmentIds,setSelectionAnchorId,setNotice,withBusy:(label,action)=>withBusy(label,action),onSaveField:(segment,text)=>editSegment(segment,text),onApplied:async(result)=>{const nextProject=result.project;            setProject(nextProject);
            updateProjectSummary(nextProject);
            const nextSelection = result.affectedSegmentIds.filter((id) => nextProject.transcript.segments.some((segment) => segment.id === id));
            setSelectedSegmentIds(nextSelection);
            setSelectedId(nextSelection[0] ?? nextProject.transcript.segments[0]?.id ?? null);
            setSelectionAnchorId(nextSelection[0] ?? null);
            setWordRange(null);
            setCutPreview(null);
            await Promise.all([refreshSpeakerTrack(nextProject.id), refreshTranscription(nextProject.id)]);
}});
    const replaceMatchCount = useMemo(() => search && project
        ? project.transcript.segments.reduce((count, segment) => count + (segment.text.split(search).length - 1), 0)
        : 0, [project, search]);
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
    const capabilities = useMemo(() => getProjectCapabilities(project, {
        mediaUrl,
        modelPath,
        modelAvailable: modelPathAvailable,
        translationTarget: subtitleLanguage,
        agentWorkflowKind,
    }), [agentWorkflowKind, mediaUrl, modelPath, modelPathAvailable, project, subtitleLanguage]);
    const mediaCapabilityTitle = capabilities.hasBoundMedia ? undefined : tr("app.capability.mediaRequired");
    const transcribeCapabilityTitle = !capabilities.hasBoundMedia
        ? tr("app.capability.mediaRequired")
        : transcriptionMode === "multispeaker"
            ? transcriptionHealth?.state !== "healthy" ? tr("app.moss.health.required") : undefined
            : !["ready", "update_available"].includes(localResources?.capabilities.find((capability) => capability.id === "local_transcription")?.state ?? "not_ready") || !capabilities.hasModel
                ? tr("app.resources.transcriptionRequired") : undefined;
    const transcriptionActive = Boolean(transcriptionJob && ["queued", "running", "finalizing"].includes(transcriptionJob.status));
    const canStartTranscription = capabilities.hasBoundMedia && (transcriptionMode === "multispeaker" ? transcriptionHealth?.state === "healthy" : true);
    const agentCapabilityTitle = !capabilities.hasBoundMedia
        ? tr("app.capability.mediaRequired")
        : !capabilities.hasTranscript
            ? tr("app.capability.transcriptRequired")
            : agentWorkflowKind === "translate" && !capabilities.hasTranslationTarget
                ? tr("app.capability.translationTargetRequired") : undefined;
    const captionSegment = resolveCaptionSegment(
        project?.transcript.segments ?? [],
        selected,
        playback.currentTime,
        playback.playing,
    );
    const captionWords = project?.transcript.words.filter((word) => word.segmentId === captionSegment?.id) ?? [];
    const selectedTranslationText = selectedTranslation?.segments.find((segment) => segment.segmentId === captionSegment?.id)?.text ?? "";
    const focusCaptionText = resolveFocusCaptionText(subtitleMode, captionSegment?.text ?? "", selectedTranslationText, tr("app.focusReview.noTranslation"));
    const captionPrimaryText = focusReview ? focusCaptionText.primary : subtitleMode === "translated" ? selectedTranslationText : captionSegment?.text ?? "";
    const captionSecondaryText = focusReview ? focusCaptionText.secondary : subtitleMode === "bilingual" ? selectedTranslationText : "";
    const captionProgress = (() => {
        if (!playback.playing || !captionSegment)
            return 1;
        if (!captionWords.length)
            return Math.max(0, Math.min(1, (playback.currentTime - captionSegment.start) / Math.max(0.01, captionSegment.end - captionSegment.start)));
        const units = captionWords.map((word) => Math.max(1, Array.from(word.text).filter((character) => !/\s/u.test(character)).length));
        const total = units.reduce((sum, value) => sum + value, 0);
        const completed = captionWords.reduce((sum, word, index) => {
            if (playback.currentTime >= word.end)
                return sum + units[index];
            if (playback.currentTime <= word.start)
                return sum;
            return sum + units[index] * ((playback.currentTime - word.start) / Math.max(0.01, word.end - word.start));
        }, 0);
        return Math.max(0, Math.min(1, completed / Math.max(1, total)));
    })();
    const captionKaraokeStyle = project
        ? resolveCaptionKaraokeStyle(
            playback.playing,
            captionProgress,
            project.subtitleStyle.primaryColor,
            project.subtitleStyle.secondaryColor,
        )
        : undefined;
    const captionPreviewStyle = project ? {
        color: project.subtitleStyle.primaryColor,
        fontFamily: `"${project.subtitleStyle.fontFamily}", "Microsoft YaHei UI", sans-serif`,
        fontSize: `${Math.max(14, Math.round(project.subtitleStyle.fontSize * 0.36))}px`,
        fontWeight: project.subtitleStyle.bold ? 700 : 400,
        bottom: project.subtitleStyle.position === "bottom" ? `${project.subtitleStyle.safeMarginPercent}%` : undefined,
        left: `${(100 - project.subtitleStyle.boxWidthPercent) / 2}%`,
        right: `${(100 - project.subtitleStyle.boxWidthPercent) / 2}%`,
        maxHeight: `${Math.round(Math.max(project.subtitleStyle.fontSize, captionSecondaryText ? project.subtitleStyle.secondaryFontSize : 0) * 0.36 * 1.35 * project.subtitleStyle.boxHeightLines + (captionSecondaryText ? 3 : 0))}px`,
        textShadow: `0 ${project.subtitleStyle.shadowDepth}px ${Math.max(1, project.subtitleStyle.shadowDepth * 2)}px ${project.subtitleStyle.outlineColor}, 0 0 ${project.subtitleStyle.outlineWidth * 2}px ${project.subtitleStyle.outlineColor}`,
    } : undefined;
    const captionPrimaryStyle = project && subtitleMode === "translated"
        ? { ...captionKaraokeStyle, fontSize: `${Math.max(12, Math.round(project.subtitleStyle.secondaryFontSize * 0.36))}px` }
        : captionKaraokeStyle;
    const quickRetranscriptionBlockMessage = quickRetranscriptionPreflight && !quickRetranscriptionPreflight.canReplace
        ? tr("app.quickRetranscribe.blocked", {
            edits: quickRetranscriptionPreflight.blockers.edits,
            patchItems: quickRetranscriptionPreflight.blockers.patchItems,
            taskSegments: quickRetranscriptionPreflight.blockers.taskSegments,
        })
        : null;
    const projectTransitionLocked = Boolean(busy || structureBusy || subtitleImportBusy || deleteBusy || deletePreflightBusy || autoBusy || sourceBusy || Object.keys(taskActions).length > 0);
    const visibleAutoWorkflows = autoWorkflows.filter((workflow) => (
        (trackedAutoWorkflowIds.includes(workflow.id) || ACTIVE_AUTO_WORKFLOW_STATUSES.has(workflow.status))
        && !dismissedAutoWorkflowIds.includes(workflow.id)
    ));
    const workbenchActivityInputs: WorkbenchActivityInputs = {
        busyMessage: busy,
        sourceJob,
        transcriptionJobs: transcriptionTasks.jobs,
        projectTitles: Object.fromEntries(projects.map((item) => [item.id, item.title])),
        agentRun,
        audioAnalysisJob,
        exportJob: activeExport,
        autoWorkflows: visibleAutoWorkflows,
        autoWorkflowErrors,
    };
    const recentAutoWorkflows = autoWorkflows.slice(0, 5);
    const activeTranscription = transcriptionTasks.jobs.some((job) => job.projectId === project?.id && ["queued", "running", "finalizing"].includes(job.status));
    const humanState = busy || activeTranscription ? tr("app.s0001") : taskLabel(project);
    const humanStateTone = humanState === tr("app.s0003") ? "warning" : humanState === tr("app.s0002") ? "agent" : humanState === tr("app.s0001") ? "info" : "success";
    const orderedPatchSets = project?.patchSets
        .map((set) => ({ ...set, items: set.items.filter((item) => ["pending", "conflict"].includes(item.status)).sort((left, right) => Number(right.status === "conflict") - Number(left.status === "conflict")) }))
        .filter((set) => set.items.length)
        .sort((left, right) => Number(right.items.some((item) => item.status === "conflict")) - Number(left.items.some((item) => item.status === "conflict"))) ?? [];
    const pendingEdits = project?.edits.filter((edit) => ["suggested", "proposed"].includes(edit.status)) ?? [];
    const processingTasks = project?.tasks.filter((task) => ["queued", "claimed", "running"].includes(task.status)) ?? [];
    const audioRisks = audioAnalysisJob?.status === "completed" ? audioAnalysisJob.report?.risks ?? [] : [];
    const projectSpeakerJob = speakerJobs
        .filter((job) => job.kind === "analyze" && job.projectId === project?.id)
        .sort((left, right) => Number(["queued", "running"].includes(right.status)) - Number(["queued", "running"].includes(left.status)))[0] ?? null;
    const speakerInstallJob = speakerJobs
        .filter((job) => job.kind === "install")
        .sort((left, right) => Number(["queued", "running"].includes(right.status)) - Number(["queued", "running"].includes(left.status)))[0] ?? null;
    const speakerById = new Map(speakerTrack?.speakers.map((speaker) => [speaker.id, speaker]) ?? []);
    const associationBySegment = new Map(speakerTrack?.associations.map((association) => [association.segmentId, association]) ?? []);
    const actionableReviewCount = orderedPatchSets.reduce((count, set) => count + set.items.length, 0) + pendingEdits.length + audioRisks.length + transcriptionReviews.length + Number(Boolean(projectSpeakerJob && ["failed", "interrupted"].includes(projectSpeakerJob.status)));
    const focusReviewCount = (project?.subtitleQuality.issues.filter((issue) => issue.severity === "error").length ?? 0)
        + orderedPatchSets.reduce((count, set) => count + set.items.length, 0)
        + pendingEdits.length
        + transcriptionReviews.filter((item) => item.status === "open").length
        + audioRisks.length;
    const mossWordTimingUnavailable = speakerTrack?.providerId === "moss_openai" && speakerTrack.sourceKind === "end_to_end";
    useEffect(() => {
        setConfirmUncutExport(false);
    }, [project?.id, project?.timeline.cuts.length]);
    useEffect(() => {
        setEmptyReplacementConfirmed(false);
        setSubtitleReplaceConfirmed(false);
        setConfirmTranscriptionWarnings(false);
        setConfirmStaleTranslation(false);
        setConfirmUncutExport(false);
        setTranscriptionApplyConfirmed(false);
        setAgentHandoffReady(false);
        setAgentHandoffCopied(false);
        setAgentHandoffTaskId(null);
        setShowAgentHandoff(false);
        setShowSubtitleImport(false);
        setSubtitleImportPreview(null);
        setSubtitleImportPath("");
        setStructureEditMode(null);
        setShowTranscriptionCandidate(false);
        resetFocusReview();
    }, [project?.id, resetFocusReview]);
    useEffect(() => {
        if (pendingCandidateJobId && transcriptionJob?.id === pendingCandidateJobId && transcriptionJob.status === "awaiting_apply") {
            setTranscriptionApplyConfirmed(false);
            setShowTranscriptionCandidate(true);
            setPendingCandidateJobId(null);
        }
    }, [project?.id, transcriptionJob?.id, transcriptionJob?.status, pendingCandidateJobId]);
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
    useEffect(() => {
        if (!reviewFocusDetailId)
            return;
        const frame = window.requestAnimationFrame(() => {
            const target = document.querySelector<HTMLElement>(`[data-review-detail-id="${reviewFocusDetailId}"]`);
            if (!target)
                return;
            target.scrollIntoView({ block: "nearest" });
            target.focus({ preventScroll: true });
        });
        return () => window.cancelAnimationFrame(frame);
    }, [drawerTab, reviewFocusDetailId]);
    const selectSegment = (segment: Segment) => {
        transcriptNavigation.locate(segment.id);
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
        if (busyRef.current)
            return;
        busyRef.current = true;
        setBusy(label);
        setError(null);
        try {
            await action();
        }
        catch (cause) {
            setError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            busyRef.current = false;
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
        activeProjectIdRef.current = envelope.project.id;
        resetProjectScopedState(envelope.project);
        updateProjectSummary(envelope.project!);
        setMediaUrl(await authorizeMedia(envelope.project.id));
        setNotice(tr("app.s0080"));
    });
    const switchProject = async (projectId: string) => {
        try { await editing.session.flush(project?.id); } catch (error) { setError(String(error)); return; }
        if (project?.id === projectId || busyRef.current || structureBusy || Boolean(subtitleImportBusy) || deleteBusy || deletePreflightBusy || Boolean(autoBusy) || Boolean(sourceBusy))
            return;
        setNotice(null);
        setError(null);
        void withBusy(tr("app.s0081"), async () => {
            await activateProject(projectId);
        });
    };
    useResourceCompletion({resourceJob, handledResourceJobRef, setResourceError, setLocalResources, setRuntime, models, setModels, setSpeakerPackage, setModelPath, setModelPathAvailable, setResourceJob, setShowResourceSetup, setResourceSelectedRoot, setNotice, pendingResourceAction, setPendingResourceAction, setShowSourceImport, setResumeSourceInspection, setResumeLocalTranscription, resourceSetupReason, setShowRuntime});
    useEffect(() => {
        if (!resumeSourceInspection || !showSourceImport)
            return;
        setResumeSourceInspection(false);
        void inspectSource();
    }, [resumeSourceInspection, showSourceImport]);
    const locateSubtitleIssue = (issue: SubtitleQualityIssue) => {
        const segment = project?.transcript.segments.find((candidate) => candidate.id === issue.segmentId);
        if (segment)
            selectSegment(segment);
    };
    const { changeCanvas, changeSubtitleStyle } = createPresentationCommands({project, setProject, updateProjectSummary, setMediaUrl, withBusy, setNotice, editing: editing.session});
    const {relinkMedia, preparePreview} = createMediaCommands({project, mediaUrl, editing:editing.session, withBusy, refreshProject, setNotice});
    const { editSegment, editTranslationSegment, replaceAll, updateCut, detectSuggestions, previewCut, createWordCut, restoreVersion, navigateHistory } = createTranscriptCommands({project, selected, selectedWords, activeWordRange, cutPadding, selectedSubtitleLanguage, search, replacement, emptyReplacementConfirmed, setEmptyReplacementConfirmed, setWordRange, videoRef, setCutPreview, withBusy, setNotice, refreshProject, refreshSpeakerTrack, refreshTranscription, editing: editing.session});
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
        if (focusReview)
            return;
        const handleShortcut = (event: KeyboardEvent) => {
            const target = event.target;
            const modifier = event.ctrlKey || event.metaKey;
            const key = event.key.toLowerCase();
            const dialogOpen = showRuntime || showResourceSetup || showSourceImport || showAutoWorkflow || showSubtitleImport || showAgentHandoff || showAiExecutionConfirm || showTranscriptionCandidate || Boolean(structureEditMode) || Boolean(currentDeleteCandidate);
            const editingTarget = target instanceof HTMLElement && (target.isContentEditable || target.matches("input, textarea, select"));
            if (event.key === "Escape" && showMoreMenu) {
                event.preventDefault();
                setShowMoreMenu(false);
                commandMoreRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
                return;
            }
            if (!dialogOpen && event.key === "F6" && !event.isComposing) { event.preventDefault(); cycleWorkbenchFocus(event.shiftKey); return; }
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
    }, [busy, currentDeleteCandidate, focusReview, mergeCandidatesAdjacent, project, selectedSegmentIds, showAgentHandoff, showAiExecutionConfirm, showAutoWorkflow, showMoreMenu, showResourceSetup, showRuntime, showSourceImport, showSubtitleImport, showTranscriptionCandidate, structureEditMode]);
    const activityActionsFor = (activity: WorkbenchActivity): WorkbenchActivityAction[] => {
        if (activity.kind === "local")
            return [];
        if (activity.kind === "source") {
            const actions: WorkbenchActivityAction[] = [{
                id: "open",
                label: tr("app.activity.open"),
                primary: true,
                disabled: Boolean(sourceBusy),
                onClick: () => setShowSourceImport(true),
            }];
            if (["queued", "running", "finalizing"].includes(activity.status))
                actions.push({ id: "cancel", label: tr("app.activity.cancel"), disabled: Boolean(sourceBusy), onClick: () => void cancelSourceImport() });
            else if (["failed", "interrupted", "cancelled", "canceled"].includes(activity.status))
                actions.push({ id: "resume", label: tr("app.activity.resume"), disabled: Boolean(sourceBusy), onClick: () => void resumeSourceImport() });
            return actions;
        }
        if (activity.kind === "transcription") {
            const job = transcriptionTasks.jobs.find((item) => `transcription:${item.id}` === activity.id);
            if (!job) return [];
            const run = async (action: "review" | "cancel" | "retry" | "discard" | "open") => {
                try {
                    if (action === "review" || action === "open") {
                        await editing.session.flush(project?.id);
                        if (activeProjectIdRef.current === job.projectId) await refreshProject(job.projectId);
                        else await activateProject(job.projectId);
                        if (activeProjectIdRef.current !== job.projectId) return;
                        if (action === "review") setPendingCandidateJobId(job.id);
                        return;
                    }
                    const result = action === "cancel" ? await transcriptionCommands.cancel(job.id)
                        : action === "retry" ? await transcriptionCommands.retry(job.id, job.attemptCount)
                        : await transcriptionCommands.discard(job.id);
                    setTranscriptionJob(result.transcriptionJob ?? null);
                } catch (cause) { setError(String(cause)); }
            };
            if (job.stage === "cancelling") return [];
            if (job.status === "awaiting_apply") return [
                { id: "inspect", label: tr("app.activity.inspect"), primary: true, onClick: () => void run("review") },
                { id: "discard", label: tr("app.activity.discard"), onClick: () => void run("discard") },
            ];
            if (["queued", "running", "finalizing"].includes(job.status)) return [{ id: "cancel", label: tr("app.activity.cancel"), onClick: () => void run("cancel") }];
            if (job.status === "completed") return [{ id: "open", label: tr("app.activity.open"), onClick: () => void run("open") }];
            return [{ id: "retry", label: tr("app.activity.resume"), onClick: () => void run("retry") }];
        }
        if (activity.kind === "agent") {
            if (["queued", "running", "submitting"].includes(activity.status))
                return [{ id: "cancel", label: tr("app.activity.cancel"), disabled: Boolean(busy), onClick: () => void cancelCodexAgent() }];
            return [{ id: "resume", label: tr("app.activity.resume"), primary: true, disabled: Boolean(busy), onClick: () => void resumeCodexAgent() }];
        }
        if (activity.kind === "audio") {
            if (["queued", "running"].includes(activity.status))
                return [{ id: "cancel", label: tr("app.activity.cancel"), disabled: Boolean(busy), onClick: () => void cancelAudioAnalysis() }];
            return [{ id: "resume", label: tr("app.activity.resume"), primary: true, disabled: Boolean(busy), onClick: () => void resumeAudioAnalysis() }];
        }
        if (activity.kind === "export") {
            if (["queued", "running"].includes(activity.status))
                return [{ id: "cancel", label: tr("app.activity.cancel"), disabled: Boolean(busy), onClick: () => void cancelExport() }];
            return [{ id: "retry", label: tr("app.activity.retry"), primary: true, disabled: Boolean(busy), onClick: () => void retryExport() }];
        }
        const workflow = visibleAutoWorkflows.find((candidate) => `auto:${candidate.id}` === activity.id);
        if (!workflow)
            return [];
        const actions: WorkbenchActivityAction[] = [];
        if (workflow.status === "awaiting_authorization" && workflow.projectId && workflow.agentTaskId)
            actions.push({ id: "authorize", label: "核对并授权发送", primary: true, disabled: Boolean(busy || autoBusy), onClick: () => {
                void (async () => {
                    await editing.session.flush(project?.id);
                    await activateProject(workflow.projectId!);
                    setAgentWorkflowKind("translate"); setSubtitleLanguage(workflow.translationLanguage ?? "en");
                    setAiApprovalTaskId(workflow.agentTaskId!); setShowAiExecutionConfirm(true);
                })().catch((cause) => setError(String(cause)));
            } });
        if (workflow.projectId && ["needs_agent", "awaiting_authorization", "needs_review", "cancelled"].includes(workflow.status))
            actions.push({ id: "open", label: tr("app.s0276"), primary: ["needs_agent", "awaiting_authorization", "needs_review"].includes(workflow.status), disabled: Boolean(autoBusy), onClick: () => void openAutoProject(workflow) });
        if (workflow.status === "needs_review")
            actions.push({ id: "continue", label: tr("app.s0278"), disabled: Boolean(autoBusy), onClick: () => void continueAutoWorkflow(workflow) });
        if (["failed", "interrupted", "cancelled"].includes(workflow.status))
            actions.push({ id: "resume", label: tr("app.s0279"), primary: true, disabled: Boolean(autoBusy), onClick: () => void continueAutoWorkflow(workflow) });
        if (["queued", "running", "needs_agent", "awaiting_authorization", "needs_review"].includes(workflow.status))
            actions.push({ id: "cancel", label: tr("app.s0277"), disabled: Boolean(autoBusy), onClick: () => void cancelAutoWorkflow(workflow) });
        if (["completed", "cancelled"].includes(workflow.status))
            actions.push({ id: "details", label: tr("app.s0280"), disabled: Boolean(autoBusy), onClick: () => { setAutoWorkflow(workflow); setShowAutoWorkflow(true); } });
        if (TERMINAL_AUTO_WORKFLOW_STATUSES.has(workflow.status))
            actions.push({ id: "dismiss", label: tr("app.auto.status.dismiss"), disabled: Boolean(autoBusy), onClick: () => dismissAutoWorkflowStatus(workflow) });
        return actions;
    };
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
    const locateFocusReviewItem = (item: ReviewQueueItem) => {
        const segment = item.segmentId ? project?.transcript.segments.find((candidate) => candidate.id === item.segmentId) : null;
        if (segment)
            selectSegment(segment);
        else
            seekTimeline(item.start);
    };
    const openFocusReviewEditor = (item: ReviewQueueItem) => {
        locateFocusReviewItem(item);
        if (item.kind === "quality") {
            setQualityFilter("all");
            setReviewFocusDetailId(`quality:${item.sourceId}`);
            setDrawerTab("quality");
        }
        else {
            setDrawerTab("analysis");
        }
        setShowExportPanel(false);
        exitFocusReview(false);
    };
    const openTimelineReviewDetail = (marker: TimelineReviewMarker) => {
        const segment = project?.transcript.segments.find((candidate) => candidate.id === marker.segmentId);
        if (segment)
            selectSegment(segment);
        if (marker.detailTarget === "quality")
            setQualityFilter("all");
        setReviewFocusDetailId(marker.detailId);
        openCreatorDrawer(marker.detailTarget);
    };
    const agentRunActive = Boolean(agentRun && ["queued", "running", "submitting"].includes(agentRun.status));
    const creatorPhase = !project ? "prepare"
        : !capabilities.hasTranscript || transcriptionActive ? "transcribe"
            : agentRunActive ? "agent"
                : actionableReviewCount > 0 ? "review" : "export";
    const runCreatorPrimaryAction = () => {
        if (!project) {
            void importMedia();
            return;
        }
        if (!capabilities.hasTranscript) {
            void transcribe();
            return;
        }
        if (agentRunActive) {
            openCreatorDrawer("review");
            return;
        }
        if (focusReviewCount > 0) {
            enterFocusReview();
            return;
        }
        if (actionableReviewCount > 0) {
            openCreatorDrawer("review");
            return;
        }
        openCreatorDrawer("quality");
    };
    const creatorPrimaryLabel = !project ? tr("app.creator.action.import")
        : !capabilities.hasTranscript ? tr("app.creator.action.transcribe")
            : agentRunActive ? tr("app.creator.action.viewAgent")
                : actionableReviewCount > 0 ? tr("app.creator.action.review") : tr("app.creator.action.checkExport");
    return (<main className={`app-shell${focusReview ? " focus-review" : ""}${navigationCollapsed ? " navigation-collapsed" : ""}`}>
      <aside className="rail">
        <div className="brand"><span className="brand-mark">S</span><span>SiaoCut</span><button className="navigation-toggle" aria-label={uiLocale === "zh-CN" ? "切换项目导航" : "Toggle project navigation"} aria-expanded={!navigationCollapsed} onClick={() => setNavigationCollapsed((value) => !value)}><ChevronRight size={15}/></button></div>
        <div className="new-project-actions">
          <button className="new-project auto" aria-label={tr("app.s0237")} disabled={projectTransitionLocked} onClick={importMedia}><FolderPlus size={16}/><span>{tr("app.creator.action.import")}</span></button>
          <details className="rail-advanced-actions"><summary aria-label={tr("app.creator.advanced")}><Settings2 size={14}/><span>{tr("app.creator.advanced")}</span></summary><div><button aria-label={tr("app.s0238")} ref={sourceButtonRef} disabled={projectTransitionLocked} onClick={() => setShowSourceImport(true)}><Link2 size={14}/><span>{tr("app.s0238")}</span></button><button aria-label={tr("app.s0236")} ref={autoButtonRef} disabled={projectTransitionLocked} onClick={() => setShowAutoWorkflow(true)}><Sparkles size={14}/><span>{tr("app.s0236")}</span></button></div></details>
        </div>
        <div className="rail-heading">{tr("app.s0239")}</div>
        <nav aria-label={tr("app.s0240")}>
          {projects.map((item) => (<div className={`project-entry ${project?.id === item.id ? "active" : ""}`} key={item.id}>
              <button className="project-link" title={item.title} aria-label={item.title} disabled={projectTransitionLocked} onClick={() => switchProject(item.id)}>
                <span className="project-dot"/><span><strong>{item.title}</strong><small>{subtitleCountLabel(item.segmentCount)}</small></span><ChevronRight size={14}/>
              </button>
              <button className="project-delete" disabled={projectTransitionLocked} aria-label={tr("app.s0242", { "0": item.title })} title={tr("app.s0243")} onClick={() => openDeleteDialog(item)}><Trash2 size={14}/></button>
            </div>))}
          {nextProjectOffset !== null && <button className="project-link" disabled={projectPageLoading} onClick={() => void loadMoreProjects().catch((cause) => setError(String(cause)))}>{projectPageLoading ? (uiLocale === "zh-CN" ? "加载中…" : "Loading…") : (uiLocale === "zh-CN" ? "加载更多项目" : "Load more projects")}</button>}
          {!projects.length && !busy && <p className="empty-rail">{tr("app.s0244")}</p>}
        </nav>
        <section className="creator-readiness" aria-label={tr("app.creator.readiness.title")}>
          <header><Cpu size={14}/><strong>{tr("app.creator.readiness.title")}</strong></header>
          <span className={runtime ? "ready" : "pending"}><i/>{tr("app.creator.readiness.core")}</span>
          <span className={runtime?.ffmpegConfigured ? "ready" : "pending"}><i/>{tr("app.creator.readiness.ffmpeg")}</span>
          <span className={runtime?.asrBackend === "vulkan" ? "ready" : "default"}><i/>{runtime?.asrBackend === "vulkan" ? tr("app.creator.readiness.vulkan") : tr("app.creator.readiness.cpu")}</span>
          <span className={codexHealth?.available && codexHealth.authenticated ? "ready" : "optional"}><i/>{codexHealth?.available && codexHealth.authenticated ? tr("app.creator.readiness.codexReady") : tr("app.creator.readiness.codexOptional")}</span>
        </section>
        <button ref={runtimeButtonRef} className="runtime-link" aria-label={tr("app.resources.title")} onClick={() => setShowRuntime(true)}><Settings2 size={15}/><span>{tr("app.resources.title")}</span></button>
        <label className="locale-switch"><span>{tr("app.locale.label")}</span><select aria-label={tr("app.locale.label")} value={uiLocale} onChange={(event) => selectUiLocale(event.target.value as UiLocale)}><option value="zh-CN">{tr("app.locale.zhCN")}</option><option value="en-US">{tr("app.locale.enUS")}</option></select></label>
        <div className="privacy"><ShieldCheck size={15}/><span>{tr("app.s0246")}</span></div>
      </aside>

      <section className={`workbench${project ? "" : " empty-workbench"}${playerExpanded && !focusReview ? " preview-expanded" : ""}`}>
        {focusReview && project && <Suspense fallback={null}><FocusReviewToolbar remaining={focusReviewCount} subtitleMode={subtitleMode} translationPending={selectedTranslationPending} translationStale={selectedTranslationStale} onSubtitleModeChange={(mode) => { setSubtitleMode(mode); setConfirmStaleTranslation(false); }} onExit={() => exitFocusReview()}/></Suspense>}
        <header className="topbar">
          <div className="topbar-heading"><p className="eyebrow">{tr("app.s0247")}</p><h1 title={project?.title}>{project?.title ?? tr("app.s0248")}</h1><EditingStatus session={editing.session} projectId={project?.id} closeError={editing.closeError} onCancelClose={editing.clearCloseError} onCloseWithDrafts={editing.closeWithDrafts}/></div>
	          <div className="command-bar creator-command-bar" aria-label={tr("app.s0249")}>
	            {project && <TaskRecords key={project.id} projectId={project.id} tasks={project.tasks} onRetry={(id) => { if (id === agentRun?.taskId) void resumeCodexAgent(); else void updateTask(id, "retry"); }} pending={taskActions} english={uiLocale === "en-US"}/>}<StatusBadge tone={humanStateTone}>{humanState}</StatusBadge><Suspense fallback={null}><WorkbenchTaskMenu error={transcriptionTasks.error} onRefresh={transcriptionTasks.refresh} inputs={workbenchActivityInputs} actionsFor={activityActionsFor}/></Suspense>
	            <div className="command-history" aria-label={tr("app.s0250")}>
	              <IconButton label={tr("app.s0251")} shortcut="Ctrl+Z" disabled={!project?.history.canUndo || Boolean(busy)} onClick={() => navigateHistory("undo")}><Undo2 size={15}/></IconButton>
	              <IconButton label={tr("app.s0252")} shortcut="Ctrl+Shift+Z" disabled={!project?.history.canRedo || Boolean(busy)} onClick={() => navigateHistory("redo")}><Redo2 size={15}/></IconButton>
	            </div>
	            <Button variant="primary" className="creator-primary-action" disabled={Boolean(busy) || (creatorPhase === "transcribe" && (!canStartTranscription || transcriptionActive))} title={creatorPhase === "transcribe" ? transcribeCapabilityTitle : undefined} onClick={runCreatorPrimaryAction}>{creatorPhase === "review" ? <ListChecks size={15}/> : creatorPhase === "export" ? <Download size={15}/> : <Sparkles size={15}/>} {creatorPrimaryLabel}</Button>
	            <div className="command-more" ref={commandMoreRef}><IconButton label={tr("app.s0256")} onClick={() => setShowMoreMenu((current) => !current)}><MoreHorizontal size={17}/></IconButton>{showMoreMenu && <Suspense fallback={null}><AppCommandMenu canDetectSuggestions={Boolean(project?.transcript.words.length) && !busy} canPreparePreview={capabilities.canPreparePreview && !busy} canRelinkMedia={capabilities.canRelinkMedia && !busy} canRetranscribe={Boolean(project?.transcript.segments.length) && capabilities.hasBoundMedia && !busy} mediaCapabilityTitle={mediaCapabilityTitle} onDetectSuggestions={() => { setShowMoreMenu(false); void detectSuggestions(); }} onPreparePreview={() => { setShowMoreMenu(false); void preparePreview(); }} onRelinkMedia={() => { setShowMoreMenu(false); void relinkMedia(); }} onRetranscribe={() => { setShowMoreMenu(false); void openQuickRetranscription(); }}/></Suspense>}</div>
	          </div>
	        </header>

        <TranscriptionStatus projectId={project?.id} tasks={transcriptionTasks} titles={workbenchActivityInputs.projectTitles} actionsFor={activityActionsFor}/>
        {(notice || error) && <div className={`notice ${error ? "error" : ""}`} role="status" aria-live="polite">{error && <CircleAlert size={15}/>}<span>{error ? tr("app.error.unknownSummary") : notice}</span>{error && <details><summary>{tr("app.error.technicalDetails")}</summary><code>{error}</code></details>}{error && <button className="notice-action" onClick={() => void initialize()}>{tr("app.s0262")}</button>}<button aria-label={tr("app.s0263")} title={tr("app.s0263")} onClick={() => { setNotice(null); setError(null); }}>×</button></div>}

        {!project ? (<section className="welcome-card">
            <div className="welcome-icon"><FileVideo2 size={30}/></div>
            <p className="eyebrow">{tr("app.s0281")}</p><h2>{tr("app.s0282")}</h2>
            <p>{tr("app.s0283")}</p>
            <RuntimeChecklist runtime={runtime} modelPath={modelPath} modelAvailable={modelPathAvailable} onChooseModel={chooseModel} compact/>
	            <div className="welcome-actions"><button className="button primary" onClick={importMedia}><FolderPlus size={16}/><span>{tr("app.creator.action.import")}</span></button><button className="button quiet" onClick={() => setShowSourceImport(true)}><Link2 size={16}/>{tr("app.s0285")}</button></div>
          </section>) : (<>
	            <section className="stage-grid">
	              <article className={`video-panel creator-player ${playerExpanded ? "expanded" : "collapsed"}`}>
	                <header className="creator-player-header"><span><Play size={14}/><strong>{tr("app.creator.player.title")}</strong><small>{selected ? `${formatTime(selected.start)} — ${formatTime(selected.end)}` : tr("app.s0288")}</small></span><button aria-expanded={playerExpanded} onClick={() => setPlayerExpanded((current) => !current)}>{playerExpanded ? <ChevronUp size={14}/> : <ChevronDown size={14}/>}{playerExpanded ? tr("app.creator.player.collapse") : tr("app.creator.player.expand")}</button></header>
	                <div className="video-frame">
                  {mediaUrl ? <video key={project.id} ref={videoRef} src={mediaUrl} controls preload="metadata" onLoadedMetadata={handleVideoLoadedMetadata} onPlay={() => setPlayback((current) => ({ ...current, playing: true }))} onPause={() => setPlayback((current) => ({ ...current, playing: false }))} onTimeUpdate={handleVideoTimeUpdate}/> : <div className="video-placeholder"><Play size={30}/><span>{tr("app.s0286")}</span></div>}
                  {showSubtitleSafeArea && (
                    <div className="subtitle-safe-area" aria-label={tr("app.s0287")} data-label={tr("app.s0287")} style={{ top: `${project.subtitleStyle.safeMarginPercent}%`, bottom: `${project.subtitleStyle.safeMarginPercent}%`, left: `${(100 - project.subtitleStyle.boxWidthPercent) / 2}%`, right: `${(100 - project.subtitleStyle.boxWidthPercent) / 2}%` }}/>
                  )}
                  {captionSegment && captionPrimaryText && <div className={`caption-overlay ${project.subtitleStyle.position}`} data-preset={project.subtitleStyle.preset} data-position={project.subtitleStyle.position} data-outline-width={project.subtitleStyle.outlineWidth} data-box-width={project.subtitleStyle.boxWidthPercent} data-box-height-lines={project.subtitleStyle.boxHeightLines} aria-label={tr("app.creator.preview.subtitleBox")} style={captionPreviewStyle}>
                    <span className={`caption-primary${playback.playing ? " playing" : ""}`} data-caption-text={playback.playing ? captionPrimaryText : undefined} data-progress={captionProgress.toFixed(3)} style={captionPrimaryStyle}>{captionPrimaryText}</span>
                    {captionSecondaryText && <span className="caption-secondary" style={{ color: project.subtitleStyle.secondaryColor, fontSize: `${Math.max(12, Math.round(project.subtitleStyle.secondaryFontSize * 0.36))}px` }}>{captionSecondaryText}</span>}
                  </div>}
                </div>
	                <div className="transport-summary"><Clock3 size={14}/><span>{selected ? `${formatTime(selected.start)} — ${formatTime(selected.end)}` : tr("app.s0288")}</span><span className="playback-state" role="status" aria-live="polite" aria-label={tr("app.playback.status")}>{playback.playing ? tr("app.playback.playing") : tr("app.playback.paused")} · {formatTime(playback.currentTime)} / {formatTime(playback.duration || project.media.durationSeconds || 0)}</span><button className="relink-media" onClick={relinkMedia}>{tr("app.s0258")}</button><span className="shortcut-hint">{tr("app.s0289")}</span><span className="spacer"/><span className="timeline-duration">{tr("app.composite.timelineSummary", { output: formatTime(project.timeline.outputDuration), source: formatTime(project.timeline.sourceDuration) })}</span></div>
	                {audioRisks.length > 0 && <div className="audio-risk-strip" role="status"><CircleAlert size={14}/><strong>{tr("app.composite.audioRiskCount", { count: audioRisks.length })}</strong><span>{audioRiskLabel(audioRisks[0].kind)} · {formatTime(audioRisks[0].start)}</span><button onClick={() => locateAudioRisk(audioRisks[0])}>{tr("app.s0293")}</button></div>}
	              </article>

	              <aside className="creator-drawer" aria-label={tr("app.creator.drawer.label")}>
	                {focusReview && readModels.ready && <Suspense fallback={null}><FocusReviewPanel project={project} transcriptionReviews={transcriptionReviews} audioRisks={audioRisks} busy={Boolean(busy)} error={error} onLocate={locateFocusReviewItem} onAgentReview={(item, action) => void reviewPatch(item.sourceId, action)} onCutReview={(item, action) => void updateCut(item.sourceId, action)} onTranscriptionReview={(item, action) => void resolveTranscriptionReview(item.sourceId, action)} onOpenEditor={openFocusReviewEditor} onTogglePlayback={toggleTimelinePlayback} onSeekDelta={(delta) => seekTimeline(playback.currentTime + delta)} onExit={() => exitFocusReview()}/></Suspense>}
	                <div className="creator-drawer-tabs" role="tablist" aria-label={tr("app.creator.drawer.tabs")}>
	                  {drawerTabs.map((tab) => <button id={`creator-drawer-tab-${tab}`} key={tab} role="tab" aria-controls={`creator-drawer-panel-${tab}`} aria-selected={drawerTab === tab} tabIndex={drawerTab === tab ? 0 : -1} className={drawerTab === tab ? "active" : ""} onKeyDown={(event) => changeDrawerTabFromKeyboard(event, tab)} onClick={() => openCreatorDrawer(tab)}>{tr(({ review: "app.creator.drawer.review", quality: "app.creator.drawer.quality", analysis: "app.creator.drawer.analysis", history: "app.creator.drawer.history", export: "app.creator.drawer.export" } as const)[tab])}{tab === "review" && actionableReviewCount > 0 ? <i>{actionableReviewCount}</i> : null}{tab === "quality" && project.subtitleQuality.errorCount > 0 ? <i>{project.subtitleQuality.errorCount}</i> : null}</button>)}
	                </div>
	                <div className="creator-drawer-body" id={`creator-drawer-panel-${drawerTab}`} role="tabpanel" aria-labelledby={`creator-drawer-tab-${drawerTab}`}>
                      {readModels.loading && <p role="status">{uiLocale === "zh-CN" ? "正在加载…" : "Loading…"}</p>}
                      {readModels.error && <div role="alert"><p>{readModels.error}</p><button onClick={readModels.retry}>{uiLocale === "zh-CN" ? "重试" : "Retry"}</button></div>}
	                  {drawerTab === "review" && readModels.reviewReady && <>
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
                      <div className="creator-agent-actions"><Button ref={agentButtonRef} variant="agent" disabled={!capabilities.canCreateAgentTask || agentRunActive || Boolean(busy) || (agentWorkflowKind === "speaker_names" && speakerTrack?.status !== "ready")} title={agentCapabilityTitle} onClick={() => { agentHandoffReturnFocusRef.current = agentButtonRef.current; void editing.session.flush(project!.id).then(() => { setAiApprovalTaskId(null); setShowAiExecutionConfirm(true); }).catch((cause) => setError(String(cause))); }}><Bot size={14}/>{tr("app.creator.agent.start")}</Button><button className="button quiet" disabled={agentRunActive || Boolean(busy)} onClick={(event) => openAgentHandoff(event.currentTarget)}>{tr("app.creator.agent.manual")}</button></div>
	                      <p className="runtime-disclosure"><ShieldCheck size={13}/>{tr("app.creator.agent.boundary")}</p>
	                    </section>
	                    <div className="review-panel-scroll creator-review-list" role="region" aria-label={tr("app.s0297")} tabIndex={0}>
	                      {orderedPatchSets.map((set) => <section className="patch-set" key={set.id}><header><span>{set.kind}{set.language ? ` · ${set.language.toUpperCase()}` : ""}</span>{set.items.length > 1 && <div><button onClick={() => reviewAll(set.taskId, "keep")}>{tr("app.s0298")}</button>{!set.items.some((item) => item.status === "conflict") && <button onClick={() => reviewAll(set.taskId, "apply")}>{tr("app.s0299")}</button>}</div>}</header>{set.items.map((item) => <div key={item.id} data-review-detail-id={`agent:${item.id}`} tabIndex={-1}><PatchReviewCard item={item} onReview={(action) => reviewPatch(item.id, action)} onSelect={() => { const segment = project.transcript.segments.find((candidate) => candidate.id === item.segmentId); if (segment) selectSegment(segment); }}/></div>)}</section>)}
	                      {pendingEdits.map((edit) => <article className="review-item" key={edit.id} data-review-detail-id={`edit:${edit.id}`} tabIndex={-1}><span className="review-tag">{tr("app.composite.reviewSuggestion", { kind: cutSuggestionLabel(edit.suggestion?.suggestionType) })}</span><strong>{editReasonLabel(edit)}</strong><p>{edit.suggestion ? tr("app.composite.suggestionEvidence", { range: `${formatTime(edit.start)} — ${formatTime(edit.end)}`, confidence: Math.round(edit.suggestion.confidence * 100) }) : `${formatTime(edit.start)} — ${formatTime(edit.end)}`}</p><div className="cut-actions"><button onClick={() => selectSegment(project.transcript.segments.find((segment) => segment.id === edit.segmentId)!)}>{tr("app.s0303")}</button>{edit.kind === "word_cut" && <button onClick={() => previewCut(edit.id)}><Headphones size={11}/>{tr("app.s0304")}</button>}<button onClick={() => updateCut(edit.id, "dismiss")}>{tr("app.cut.dismiss")}</button><button onClick={() => updateCut(edit.id, "apply")}>{tr("app.s0305")}</button></div></article>)}
	                      {audioRisks.map((risk, index) => <article className="review-item audio-risk-item" key={`${risk.kind}-${risk.start}-${index}`}><span className="review-tag warning"><CircleAlert size={12}/>{tr("app.s0306")}</span><strong>{audioRiskLabel(risk.kind)}</strong><p>{tr("app.composite.audioRiskEvidence", { range: `${formatTime(risk.start)} — ${formatTime(risk.end)}`, measured: risk.measuredValue, threshold: risk.threshold, unit: audioUnitLabel(risk.unit) })}</p><button onClick={() => locateAudioRisk(risk)}>{tr("app.s0309")}</button></article>)}
	                      <TranscriptionReviewPanel items={transcriptionReviews} disabled={Boolean(busy)} onLocate={(segmentId) => { const segment = project.transcript.segments.find((item) => item.id === segmentId); if (segment) selectSegment(segment); }} onResolve={resolveTranscriptionReview}/>
	                      {processingTasks.filter((task) => task.id !== agentRun?.taskId).map((task) => <article className={`agent-task-status ${task.status}`} key={task.id}>
                            <header><Bot size={14}/><strong>{agentTaskStatusLabel(task)}</strong><small>{task.kind}</small></header>
                            <p className="agent-task-next">{task.status === "claimed"
                                ? tr("app.agent.task.claimedHelp", { worker: task.lease?.worker ?? tr("app.agent.task.unknownWorker") })
                                : task.status === "running" && agentTaskStatusLabel(task) === tr("app.agent.status.stale")
                                    ? tr("app.agent.task.staleHelp")
                                    : task.lastActivity?.message ?? (task.status === "queued" ? tr("app.agent.task.queuedHelp") : tr("app.agent.task.runningHelp"))}</p>
                            <progress max={1} value={task.progress}/>
                            <small>{tr("app.agent.task.attempt", { attempt: task.status === "queued" ? (task.attemptCount ?? 0) + 1 : Math.max(1, task.attemptCount ?? 1) })}{task.lease?.worker ? ` · ${tr("app.agent.task.worker")}：${task.lease.worker}` : ""}{task.lastActivity?.createdAt ? ` · ${tr("app.agent.task.lastActivity", { time: new Date(task.lastActivity.createdAt).toLocaleString(uiLocale) })}` : ""}</small>
                            <div className="agent-task-actions">
                              <button onClick={(event) => openExistingAgentHandoff(task.id, event.currentTarget)}><Copy size={11}/>{tr(task.status === "queued" ? "app.agent.task.copyHandoff" : "app.agent.task.recoverHandoff")}</button>
                              <button disabled={Boolean(taskActions[task.id])} onClick={() => void updateTask(task.id, "cancel")}>{taskActions[task.id] === "cancel" ? <LoaderCircle className="spin" size={11}/> : null}{taskActions[task.id] === "cancel" ? tr("app.agent.task.cancelling") : tr("app.s0328")}</button>
                            </div>
                            <details><summary>{tr("app.agent.task.technical")}</summary><dl><div><dt>{tr("app.agent.task.id")}</dt><dd><code>{task.id}</code></dd></div>{task.lease && <><div><dt>{tr("app.agent.task.worker")}</dt><dd>{task.lease.worker}</dd></div><div><dt>{tr("app.agent.task.lease")}</dt><dd>{new Date(task.lease.expiresAt).toLocaleString(uiLocale)}</dd></div></>}</dl></details>
                          </article>)}
                      {actionableReviewCount === 0 && processingTasks.length === 0 && !agentRunActive && <div className="all-clear"><Check size={20}/><span>{tr("app.s0329")}</span></div>}
                    </div>
                  </>}
                  {drawerTab === "quality" && readModels.ready && <section className={`subtitle-quality-summary creator-quality ${project.subtitleQuality.status}`} aria-label={tr("app.s0357")}><div className="subtitle-quality-state">{project.subtitleQuality.status === "good" ? <Check size={15}/> : <CircleAlert size={15}/>}<span><strong>{project.subtitleQuality.errorCount > 0 ? subtitleQualityStatusLabel(project.subtitleQuality) : project.subtitleQuality.warningCount > 0 ? tr("app.creator.quality.advisorySummary", { count: project.subtitleQuality.warningCount }) : subtitleQualityStatusLabel(project.subtitleQuality)}</strong><small>{project.subtitleQuality.errorCount}{tr("app.s0358") + " "}{project.subtitleQuality.warningCount}{tr("app.s0359")}</small></span></div><div className="subtitle-quality-filters" aria-label={tr("app.s0360")}><button className={qualityFilter === "all" ? "active" : ""} onClick={() => setQualityFilter("all")}>{tr("app.s0361")}</button><button className={qualityFilter === "error" ? "active" : ""} disabled={!project.subtitleQuality.errorCount} onClick={() => setQualityFilter("error")}>{tr("app.s0362") + " "}{project.subtitleQuality.errorCount}</button><button className={qualityFilter === "warning" ? "active" : ""} disabled={!project.subtitleQuality.warningCount} onClick={() => setQualityFilter("warning")}>{tr("app.s0363") + " "}{project.subtitleQuality.warningCount}</button></div>{visibleQualityIssueGroups.length > 0 ? <><p className="quality-review-policy">{tr("app.creator.quality.reviewPolicy")}</p><div className="subtitle-quality-issues">{visibleQualityIssueGroups.map((group) => <button className={group.severity} key={group.id} data-review-detail-id={`quality:${group.first.id}`} onClick={() => locateSubtitleIssue(group.first)}><CircleAlert size={12}/><span><strong>{subtitleIssueLabel(group.kind)}{group.count > 1 ? ` · ${tr("app.creator.quality.groupCount", { count: group.count })}` : ""}</strong><small>{formatTime(group.start)} — {formatTime(group.end)}</small></span></button>)}</div></> : <div className="all-clear"><Check size={20}/><span>{tr("app.creator.quality.ready")}</span></div>}<button className="button primary full" onClick={() => openCreatorDrawer("export")}>{tr("app.creator.quality.continue")}</button></section>}
                  {drawerTab === "analysis" && readModels.ready && <div className="inspector-view creator-analysis">
                    <SpeechInsightsPanel insights={project.speechInsights} onLocateEvidence={locateSpeechEvidence} onLocatePause={locateSpeechPause}/>
                    <AudioQualityPanel job={audioAnalysisJob} onStart={startAudioAnalysis} onCancel={cancelAudioAnalysis} onResume={resumeAudioAnalysis} onLocate={locateAudioRisk} disabled={!capabilities.canAnalyzeAudio || Boolean(busy)}/>
                    <SpeakerTrackPanel packageStatus={speakerPackage} track={speakerTrack} job={projectSpeakerJob} selectedSegmentId={selectedId} disabled={Boolean(busy)} onOpenRuntime={() => void openResourcePreparation("speaker_identity", "on_demand")} onAnalyze={startSpeakerAnalysis} onCancel={() => void cancelSpeakerJob(projectSpeakerJob)} onResume={() => void resumeSpeakerJob(projectSpeakerJob)} onRename={renameSpeaker} onMerge={mergeSpeaker} onAssign={assignSpeaker}/>
                    {selectedWords.length > 0 && <section className="word-evidence" aria-label={tr("app.s0376")}>
                      <div className="word-heading"><div><p className="eyebrow">{tr("app.s0376")}</p><small>{tr("app.s0377")}</small></div>{activeWordRange && <button className="clear-range" onClick={() => setWordRange(null)}>{tr("app.s0378")}</button>}</div>
                      <div className="word-tokens">{selectedWords.map((word, index) => <button className={activeWordRange && index >= activeWordRange.start && index <= activeWordRange.end ? "selected" : ""} key={word.id} onClick={() => selectWordForCut(index)} title={`${formatTime(word.start)} — ${formatTime(word.end)}${word.confidence == null ? "" : ` · ${Math.round(word.confidence * 100)}%`}`}>{word.text}</button>)}</div>
                      {activeWordRange && <div className="word-cut-controls"><label>{tr("app.s0379")}<input aria-label={tr("app.s0380")} type="range" min="0" max={selectedWords.length - 1} value={activeWordRange.start} onChange={(event) => setWordRange({ ...activeWordRange, start: Math.min(Number(event.target.value), activeWordRange.end) })}/><small>{selectedWords[activeWordRange.start]?.text}</small></label><label>{tr("app.s0381")}<input aria-label={tr("app.s0382")} type="range" min="0" max={selectedWords.length - 1} value={activeWordRange.end} onChange={(event) => setWordRange({ ...activeWordRange, end: Math.max(Number(event.target.value), activeWordRange.start) })}/><small>{selectedWords[activeWordRange.end]?.text}</small></label><label className="padding-select">{tr("app.s0383")}<select aria-label={tr("app.s0383")} value={cutPadding} onChange={(event) => setCutPadding(Number(event.target.value) as 30 | 100 | 200)}><option value="30">30 ms</option><option value="100">100 ms</option><option value="200">200 ms</option></select></label><button className="create-word-cut" disabled={Boolean(busy)} onClick={createWordCut}><Scissors size={12}/>{tr("app.s0384")}</button></div>}
                    </section>}
                  </div>}
                  {drawerTab === "history" && readModels.ready && <div className="inspector-view"><div className="version-block"><div className="section-title"><div><p className="eyebrow">{tr("app.s0385")}</p><h2>{tr("app.s0386")}</h2></div><History size={16}/></div>{project.versions.slice().reverse().map((version) => <button className="version-row" key={version.id} onClick={() => restoreVersion(version.id)}><span><strong>{versionReasonLabel(version.reason)}</strong><small>{new Date(version.createdAt).toLocaleString(uiLocale)}</small></span><RotateCcw size={14}/></button>)}</div></div>}
                  {drawerTab === "export" && showExportPanel && <Suspense fallback={null}><ExportPanel embedded ref={exportPanelRef} project={project} busy={Boolean(busy)} subtitleDelivery={subtitleDelivery} subtitleMode={subtitleMode} translationLanguageOptions={translationLanguageOptions} translationLanguages={translationLanguages} selectedSubtitleLanguage={selectedSubtitleLanguage} selectedTranslationPending={selectedTranslationPending} selectedTranslationStale={selectedTranslationStale} confirmStaleTranslation={confirmStaleTranslation} confirmUncutExport={confirmUncutExport} exportFormat={exportFormat} structuredExport={structuredExport} includeSpeakerLabels={includeSpeakerLabels} transcriptionExportErrorCount={transcriptionExportErrors.length} transcriptionExportWarningCount={transcriptionExportWarnings.length} confirmTranscriptionWarnings={confirmTranscriptionWarnings} showSubtitleSafeArea={showSubtitleSafeArea} transcriptionExportBlocked={transcriptionExportBlocked} canExportVideo={capabilities.canExportVideo} activeExportRunning={Boolean(activeExport && ["queued", "running"].includes(activeExport.status))} mediaCapabilityTitle={mediaCapabilityTitle} onClose={() => { setShowExportPanel(false); setDrawerTab("quality"); }} onChangeCanvas={(settings) => void changeCanvas(settings)} onSubtitleDeliveryChange={setSubtitleDelivery} onSubtitleModeChange={(mode) => { setSubtitleMode(mode); setConfirmStaleTranslation(false); }} onSubtitleLanguageChange={(language) => { setSubtitleLanguage(language); setConfirmStaleTranslation(false); }} onExportFormatChange={(format) => { setExportFormat(format); setConfirmTranscriptionWarnings(false); }} onIncludeSpeakerLabelsChange={setIncludeSpeakerLabels} onConfirmWarningsChange={setConfirmTranscriptionWarnings} onConfirmStaleTranslationChange={setConfirmStaleTranslation} onConfirmUncutExportChange={setConfirmUncutExport} onSubtitleStyleChange={(preset, position, sourceFontSize, translationFontSize, boxWidthPercent, boxHeightLines) => void changeSubtitleStyle(preset, position, sourceFontSize, translationFontSize, boxWidthPercent, boxHeightLines)} onShowSafeAreaChange={setShowSubtitleSafeArea} onExportTranscript={exportTranscript} onExportVideo={exportVideo}/></Suspense>}
	                </div>
	              </aside>

            <section className="editor-grid">
              <article className="transcript-panel">
	                <header className="panel-header"><div><h2>{uiLocale === "zh-CN" ? "字幕文稿" : "Transcript"}</h2><label className="transcript-follow"><input type="checkbox" checked={transcriptNavigation.followPlayback} onChange={(event) => transcriptNavigation.setFollowPlayback(event.target.checked)}/>{uiLocale === "zh-CN" ? "跟随播放" : "Follow playback"}</label></div><div className="find-replace">{transcriptionMode === "multispeaker" && <button className="moss-transcribe-command" disabled={!canStartTranscription || transcriptionActive || Boolean(busy)} title={transcribeCapabilityTitle} onClick={transcribe}><Users size={12}/>{tr("app.moss.action.start")}</button>}<button ref={subtitleImportButtonRef} className="subtitle-import-command" disabled={Boolean(busy)} onClick={openSubtitleImport}><FileText size={12}/>{tr("app.s0333")}</button><button className="detect-suggestions" disabled={!project.transcript.words.length || Boolean(busy)} onClick={detectSuggestions}><Scissors size={12}/>{tr("app.s0334")}</button><label className="search"><Search size={14}/><input ref={searchInputRef} value={search} onChange={(event) => { setSearch(event.target.value); setEmptyReplacementConfirmed(false); }} placeholder={tr("app.s0335")} title="Ctrl+F"/></label><input ref={replacementInputRef} aria-label={tr("app.s0336")} value={replacement} onChange={(event) => { setReplacement(event.target.value); setEmptyReplacementConfirmed(false); }} placeholder={tr("app.s0336")} title="Ctrl+H"/>{search && <span className="replace-match-count">{tr("app.replace.matches", { count: replaceMatchCount })}</span>}{search && !replacement && <label className="replace-empty-confirm"><input type="checkbox" checked={emptyReplacementConfirmed} onChange={(event) => setEmptyReplacementConfirmed(event.target.checked)}/><span>{tr("app.replace.confirmDelete")}</span></label>}<button disabled={!search || (!replacement && !emptyReplacementConfirmed) || Boolean(busy)} onClick={replaceAll}>{tr("app.s0337")}</button></div></header>
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
                <VirtualTranscript listRef={transcriptNavigation.listRef} label={tr("app.s0365")} segments={filteredSegments}
                  empty={<p className="empty-list">{project.transcript.segments.length ? tr("app.s0366") : tr("app.s0367")}</p>}
                  renderRow={(segment) => { const association = associationBySegment.get(segment.id); return <SegmentRow playbackActive={segment.id === transcriptNavigation.playbackSegmentId} editingSession={editing.session} projectId={project.id} key={segment.id} segment={segment} speaker={association ? speakerById.get(association.speakerId) : undefined} speakerManual={association?.source === "manual"} selected={selectedSegmentIds.includes(segment.id)} active={segment.id === selectedId} translation={translation?.[1]} translationLanguage={translation?.[0]} onSelect={(mode) => selectSegmentInWorkbench(segment, mode)} onSave={(text) => editSegment(segment, text)} onSaveTranslation={(text) => editTranslationSegment(segment, text)} onSplitAt={(text, offset) => void splitSegmentFromEditor(segment, text, offset)} onMergePrevious={(text) => void mergePreviousFromEditor(segment, text)}/>; }} />
              </article>

	            </section>

            </section>

            <Suspense fallback={null}><SubtitleTimelinePanel
              project={project}
              speakerTrack={speakerTrack}
              transcriptionReviews={transcriptionReviews}
              audioRisks={audioRisks}
              waveformUrl={waveformUrl}
              playback={playback}
              selectedId={selectedId}
              selectedSegmentIds={selectedSegmentIds}
              busy={Boolean(busy) || structureBusy}
              onSelectSegment={selectSegment}
              onSeek={seekTimeline}
              onTogglePlayback={toggleTimelinePlayback}
              onNudgeSelected={(segmentId, delta) => void nudgeTimelineSegment(segmentId, delta)}
              onOpenTiming={(segment) => {
                selectSegment(segment);
                openStructureEdit("timing", segment);
              }}
              onOpenReviewDetail={openTimelineReviewDetail}
              onRestoreCut={(editId) => void updateCut(editId, "restore")}
              canEnterFocusReview={Boolean(project)}
              onEnterFocusReview={enterFocusReview}
            /></Suspense>
          </>)}
      </section>
      {showAiExecutionConfirm && project && <Suspense fallback={null}><AiExecutionConfirm
        scope={{ projectId: project.id, expectedVersionId: project.history.currentVersionId ?? "", kind: agentWorkflowKind,
          language: agentWorkflowKind === "translate" ? subtitleLanguage : null, instructionLocale: uiLocale, taskId: aiApprovalTaskId }}
        returnFocusRef={agentHandoffReturnFocusRef}
        codexReady={Boolean(codexHealth?.available && codexHealth.authenticated)}
        taskLabel={`AI 辅助 · ${aiConfirmationLabel}`}
        segmentCount={aiConfirmationSegments.length}
        characterCount={aiConfirmationCharacters}
        startTime={aiConfirmationSegments[0]?.start ?? 0}
        endTime={aiConfirmationSegments.at(-1)?.end ?? 0}
        contextLabel={aiConfirmationContext}
        onClose={() => setShowAiExecutionConfirm(false)}
        onConfirm={startAiAssistance}
      /></Suspense>}
      {showAgentHandoff && project && <Suspense fallback={null}><AgentHandoffDialog
        returnFocusRef={agentHandoffReturnFocusRef}
        taskReady={Boolean(agentHandoffTaskId)}
        ready={agentHandoffReady}
        busy={Boolean(busy)}
        identity={lockedHandoffIdentity ?? agentIdentity}
        identityValid={isValidAgentIdentity(handoffIdentity)}
        identityLocked={handoffIdentityLocked}
        handoffText={handoffText}
        copied={agentHandoffCopied}
        onClose={() => setShowAgentHandoff(false)}
        onReadyChange={setAgentHandoffReady}
        onIdentityChange={(value) => { setAgentIdentity(value); setAgentHandoffCopied(false); }}
        onCreate={() => void createAgentTask()}
        onCopy={() => void copyAgentHandoff()}
      /></Suspense>}
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
      {showSubtitleImport && project && <Suspense fallback={null}><SubtitleImportDialog
        returnFocusRef={subtitleImportButtonRef}
        path={subtitleImportPath}
        busy={subtitleImportBusy}
        error={subtitleImportError}
        preview={subtitleImportPreview}
        confirmed={subtitleReplaceConfirmed}
        onClose={() => setShowSubtitleImport(false)}
        onInspect={() => void inspectSubtitleFile()}
        onConfirmedChange={setSubtitleReplaceConfirmed}
        onConfirm={() => void confirmSubtitleImport()}
      /></Suspense>}
      {showAutoWorkflow && <Dialog label={tr("app.s0476")} className="runtime-dialog auto-dialog" onClose={() => setShowAutoWorkflow(false)} returnFocusRef={autoButtonRef}><button autoFocus className="dialog-close" aria-label={tr("app.s0477")} title={tr("app.s0478")} onClick={() => setShowAutoWorkflow(false)}><X size={18}/></button><p className="eyebrow">{tr("app.s0479")}</p><h2>{tr("app.s0480")}</h2><p className="dialog-copy">{tr("app.s0481")}</p>
        {recentAutoWorkflows.length > 0 && <section className="auto-history" aria-label={tr("app.auto.history.title")}>
          <header><span><strong>{tr("app.auto.history.title")}</strong><small>{tr("app.auto.history.help")}</small></span><History size={15}/></header>
          <div>{recentAutoWorkflows.map((workflow) => <article key={workflow.id}>
            <span><strong>{workflowProfileLabel(workflow.profile)} · {autoStatusLabel(workflow.status)} · {autoStageLabel(workflow.currentStage)}</strong><small>{workflow.title ?? workflow.outputPath} · {new Date(workflow.updatedAt).toLocaleString(uiLocale)}</small></span>
            <div>
              <button className="button quiet" onClick={() => { showAutoWorkflowStatus(workflow); setShowAutoWorkflow(false); }}>{tr("app.auto.history.show")}</button>
              {workflow.projectId && <button className="button quiet" onClick={() => { setShowAutoWorkflow(false); void openAutoProject(workflow); }}>{tr("app.s0276")}</button>}
              {["failed", "interrupted", "cancelled"].includes(workflow.status) && <button className="button primary" disabled={Boolean(autoBusy)} onClick={() => { setShowAutoWorkflow(false); void continueAutoWorkflow(workflow); }}>{tr("app.s0279")}</button>}
            </div>
          </article>)}</div>
        </section>}
        <div className="auto-form">
          <Suspense fallback={null}><AutoWorkflowProfileSelector value={autoProfile} onChange={(profile) => { setAutoProfile(profile); if (profile === "draft") { setAutoTranslate(false); setAutoSubtitleMode("source"); setAutoAiSelection(null); } }}/></Suspense>
          <label><span>{tr("app.s0482")}</span><select aria-label={tr("app.s0483")} value={autoInputKind} disabled={Boolean(autoBusy)} onChange={(event) => { setAutoInputKind(event.target.value as "local" | "url"); setAutoSourcePreview(null); setAutoAuthorized(false); setAutoError(null); }}><option value="local">{tr("app.s0484")}</option><option value="url">{tr("app.s0485")}</option></select></label>
          {autoInputKind === "local" ? <div className="auto-file-row"><span><small>{tr("app.s0486")}</small><strong title={autoMediaPath}>{autoMediaPath || tr("app.s0468")}</strong></span><button className="button quiet" disabled={Boolean(autoBusy)} onClick={() => void chooseAutoMedia()}><FolderOpen size={14}/>{tr("app.s0470")}</button></div> : <>
            <form className="source-form" onSubmit={(event) => { event.preventDefault(); void inspectAutoSource(); }}><label><span>{tr("app.s0487")}</span><input autoComplete="url" aria-label={tr("app.s0488")} placeholder="https://…" value={autoUrl} disabled={Boolean(autoBusy)} onChange={(event) => { setAutoUrl(event.target.value); setAutoSourcePreview(null); setAutoAuthorized(false); setAutoError(null); }}/></label><button className="button quiet" type="submit" disabled={Boolean(autoBusy) || !autoUrl.trim()}><Search size={14}/>{tr("app.s0489")}</button></form>
            {autoSourcePreview && <section className="source-preview auto-source-preview" aria-label={tr("app.s0490")}><header><span><small>{autoSourcePreview.extractor}</small><strong>{autoSourcePreview.title}</strong></span><ShieldCheck size={19}/></header><dl><div><dt>{tr("app.s0491")}</dt><dd>{formatTime(autoSourcePreview.durationSeconds)}</dd></div><div><dt>{tr("app.s0492")}</dt><dd>{autoSourcePreview.siteMediaId}</dd></div></dl><label className="source-consent"><input type="checkbox" checked={autoAuthorized} onChange={(event) => setAutoAuthorized(event.target.checked)}/><span>{tr("app.s0493")}</span></label></section>}
          </>}
          <div className="auto-file-row"><span><small>{tr("app.s0494")}</small><strong title={modelPath ?? undefined}>{modelPath ?? tr("app.s0468")}</strong></span><button className="button quiet" onClick={() => { setShowAutoWorkflow(false); setShowRuntime(true); }}>{tr("app.s0495")}</button></div>
          <div className="auto-options">
            <label><span>{tr("app.transcription.language")}</span><select aria-label={`${tr("app.transcription.language")} · ${tr("app.s0236")}`} value={transcriptionLanguage} disabled={Boolean(autoBusy)} onChange={(event) => selectTranscriptionLanguage(event.target.value as TranscriptionLanguage)}><option value="auto">{tr("app.transcription.auto")}</option><option value="en">{tr("app.transcription.english")}</option><option value="zh">{tr("app.transcription.chinese")}</option></select></label>
            {autoProfile !== "draft" && <><label className="auto-check"><input type="checkbox" checked={autoTranslate} onChange={(event) => { setAutoTranslate(event.target.checked); if (!event.target.checked)
            setAutoSubtitleMode("source"); setAutoAiSelection(null); }}/><span>{tr("app.s0496")}</span></label>
            <label><span>{tr("app.s0497")}</span><input aria-label={tr("app.s0498")} value={autoTranslationLanguage} disabled={!autoTranslate} onChange={(event) => setAutoTranslationLanguage(event.target.value)}/></label>
            <label><span>{tr("app.s0499")}</span><select aria-label={tr("app.s0500")} value={autoSubtitleMode} disabled={!autoTranslate} onChange={(event) => setAutoSubtitleMode(event.target.value as typeof autoSubtitleMode)}><option value="source">{tr("app.s0406")}</option><option value="translated">{tr("app.s0407")}</option><option value="bilingual">{tr("app.s0408")}</option></select></label></>}
            <label className="auto-check"><input type="checkbox" checked={autoBurnSubtitles} onChange={(event) => setAutoBurnSubtitles(event.target.checked)}/><span>{tr("app.s0501")}</span></label>
          </div>
          {autoTranslate && <Suspense fallback={null}><AutoWorkflowAiTarget codexReady={Boolean(codexHealth?.available && codexHealth.authenticated)} onChange={setAutoAiSelection}/></Suspense>}
          <button className="button primary full" disabled={Boolean(autoBusy) || !modelPathAvailable || (autoTranslate && (!autoTranslationLanguage.trim() || !autoAiSelection)) || (autoInputKind === "local" ? !autoMediaPath : !autoSourcePreview || !autoAuthorized)} onClick={() => void startAutoWorkflow()}>{autoBusy ? <LoaderCircle className="spin" size={14}/> : <Sparkles size={14}/>}{tr("app.s0502")}</button>
          {autoError && <div className="source-error" role="alert"><CircleAlert size={15}/><JobFailureDetails context="auto" status="failed" errorMessage={autoError}/></div>}
        </div>
      </Dialog>}
      {showRuntime && <Suspense fallback={null}><RuntimeSettingsDialog
        returnFocusRef={runtimeButtonRef}
        runtime={runtime}
        codexHealth={codexHealth}
        localResources={localResources}
        resourceJob={resourceJob}
        resourceBusy={resourceBusy}
        modelPath={modelPath}
        modelAvailable={modelPathAvailable}
	        transcriptionConfig={transcriptionConfig}
	        transcriptionHealth={transcriptionHealth}
	        transcriptionMode={transcriptionMode}
	        transcriptionLanguage={transcriptionLanguage}
        busy={Boolean(busy)}
        models={models}
        modelJob={modelJob}
        speakerPackage={speakerPackage}
        speakerJob={speakerInstallJob}
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
        onSelectModel={(path) => { localStorage.setItem("siaocut.modelPath", path); setModelPath(path); setModelPathAvailable(true); }}
        onInstallModel={installModel}
        onCancelModel={cancelModel}
        onRemoveModel={removeModel}
        onInstallSpeakerPackage={installSpeakerPackage}
        onCancelSpeakerJob={() => void cancelSpeakerJob(speakerInstallJob)}
        onResumeSpeakerJob={() => void resumeSpeakerJob(speakerInstallJob)}
        onOpenDiagnostics={openDiagnostics}
        onCheckUpdates={() => void checkUpdates()}
        onInstallUpdate={() => void confirmUpdateInstall()}
        onRefresh={() => void initialize()}
        onPrepareResource={(capability) => void openResourcePreparation(capability, "manage")}
        onLocalResourcesChange={setLocalResources}
        onChangeResourceLocation={() => void openResourcePreparation("basic_media", "manage")}
        onRemoveResource={(capability) => void removeResourceCapability(capability)}
        onRollbackResource={(capability) => void rollbackResourceCapability(capability)}
        onCleanupResources={() => void cleanupLocalResources()}
      /></Suspense>}
      {showResourceSetup && <Suspense fallback={null}><LocalResourceSetupDialog
        reason={resourceSetupReason}
        capability={resourceCapability}
        status={localResources}
        plan={resourcePlan}
        job={resourceJob}
        profile={resourceProfile}
        selectedRoot={resourceSelectedRoot}
        busy={resourceBusy}
        error={resourceError}
        onClose={closeResourcePreparation}
        onChooseLocation={() => void chooseResourceLocation()}
        onConfirmLocation={() => void confirmResourceLocation()}
        onProfileChange={(profile) => void changeResourceProfile(profile)}
        onStart={() => void startResourcePreparation()}
        onCancel={() => void cancelResourcePreparation()}
        onResume={() => void resumeResourcePreparation()}
        onDefer={closeResourcePreparation}
      /></Suspense>}
      {showTranscriptionCandidate && transcriptionJob?.status === "awaiting_apply" && <Suspense fallback={null}><TranscriptionCandidateDialog job={transcriptionJob} busy={Boolean(busy)} confirmed={transcriptionApplyConfirmed} onConfirmedChange={setTranscriptionApplyConfirmed} onApply={applyTranscriptionCandidate} onDiscard={discardTranscriptionCandidate} onClose={() => { if (!busy) { setShowTranscriptionCandidate(false); setTranscriptionApplyConfirmed(false); } }}/></Suspense>}
      {showQuickRetranscription && <Suspense fallback={null}><QuickRetranscriptionDialog preflight={quickRetranscriptionPreflight} checking={quickRetranscriptionChecking} busy={Boolean(busy)} confirmed={quickRetranscriptionConfirmed} blockerMessage={quickRetranscriptionBlockMessage} error={quickRetranscriptionError} onConfirmedChange={setQuickRetranscriptionConfirmed} onConfirm={() => void confirmQuickRetranscription()} onClose={closeQuickRetranscription}/></Suspense>}
      {currentDeleteCandidate && <Suspense fallback={null}><ProjectDeleteDialog project={currentDeleteCandidate} checking={deletePreflightBusy} deleting={deleteBusy} deletable={Boolean(deletionPreflight?.deletable)} blockerMessage={deleteBlockMessage} error={deleteError} onClose={closeDeleteDialog} onDelete={() => void deleteProject()}/></Suspense>}
      {showSourceImport && <Suspense fallback={null}><SourceImportDialog
        returnFocusRef={sourceButtonRef}
        sourceUrl={sourceUrl}
        sourcePreview={sourcePreview}
        sourceJob={sourceJob}
        sourceAuthorized={sourceAuthorized}
        sourceAuthMode={sourceAuthMode}
        sourceBrowser={sourceBrowser}
        sourceBrowserAuthorized={sourceBrowserAuthorized}
        sourceBusy={sourceBusy}
        sourceError={sourceError}
        onClose={() => { setShowSourceImport(false); setSourceBrowserAuthorized(false); }}
        onSourceUrlChange={(value) => { setSourceUrl(value); setSourcePreview(null); setSourceAuthorized(false); setSourceBrowserAuthorized(false); setSourceError(null); }}
        onAuthorizedChange={setSourceAuthorized}
        onAuthModeChange={(value) => { setSourceAuthMode(value); setSourcePreview(null); setSourceAuthorized(false); setSourceBrowserAuthorized(false); setSourceError(null); }}
        onBrowserChange={(value) => { setSourceBrowser(value); setSourcePreview(null); setSourceAuthorized(false); setSourceBrowserAuthorized(false); setSourceError(null); }}
        onBrowserAuthorizedChange={setSourceBrowserAuthorized}
        onInspect={() => void inspectSource()}
        onStart={() => void startSourceImport()}
        onCancel={() => void cancelSourceImport()}
        onResume={() => void resumeSourceImport()}
        onReset={resetSourceImport}
      /></Suspense>}
    </main>);
}
export default WorkbenchController;
