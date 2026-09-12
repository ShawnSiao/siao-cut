import { useEffect,useRef,useState,type RefObject } from "react";
import { getProjectCapabilities } from "../../app-view-model";
import { localFileAvailable } from "../../domains/desktop-platform-client";
import { transcriptEditingClient } from "../../domains/transcript-editing-client";
import { tr } from "../../i18n";
import type { Project,TranscriptionLanguage,TranscriptReplacementPreflight } from "../../types";
import type { EditingSession } from "../editing/editing-session";
import type { useBackgroundSession } from "./use-background-session";
import type { useResourceSession } from "./use-resource-session";
import type { useRuntimeSession } from "./use-runtime-session";
import type { useTranscriptionReviewSession } from "./use-transcription-review-session";
type Inputs = Pick<ReturnType<typeof useBackgroundSession>, "transcriptionCommands" | "setTranscriptionJob">
 & Pick<ReturnType<typeof useResourceSession>, "localResources" | "setPendingResourceAction" | "openResourcePreparation">
 & Pick<ReturnType<typeof useRuntimeSession>, "runtime" | "modelPath" | "modelPathAvailable" | "setModelPathAvailable">
 & Pick<ReturnType<typeof useTranscriptionReviewSession>, "transcriptionMode" | "transcriptionHealth" | "transcriptionPrompt" | "transcriptionHotwords" | "refreshTranscription">
 & {project:Project|null; projectRef:RefObject<Project|null>; mediaUrl:string|null; editing:EditingSession;transcriptionLanguage:TranscriptionLanguage;
   busyRef:RefObject<boolean>; setBusy:(value:string|null)=>void; setError:(value:string|null)=>void;setNotice:(value:string|null)=>void;
   withBusy:(label:string, action:()=>Promise<void>)=>Promise<void>; refreshProject:(id:string, media?:boolean)=>Promise<Project>; refreshSpeakerTrack:(id:string)=>Promise<void>};
/** Starts persistent transcription and binds replacement consent to its reviewed baseline. */
export function useTranscriptionStartSession({project, projectRef, mediaUrl, runtime, modelPath, modelPathAvailable, setModelPathAvailable, transcriptionMode, transcriptionHealth, transcriptionLanguage, transcriptionPrompt, transcriptionHotwords, transcriptionCommands, setTranscriptionJob, localResources, setPendingResourceAction, openResourcePreparation, busyRef, setBusy, setError, setNotice, editing, withBusy, refreshProject, refreshSpeakerTrack, refreshTranscription}:Inputs) {
    const preflightEpoch = useRef(0);
    const capabilities = getProjectCapabilities(project, {mediaUrl});
    const [showQuickRetranscription, setShowQuickRetranscription] = useState(false);
    const [quickRetranscriptionPreflight, setQuickRetranscriptionPreflight] = useState<TranscriptReplacementPreflight | null>(null);
    const [quickRetranscriptionChecking, setQuickRetranscriptionChecking] = useState(false);
    const [quickRetranscriptionConfirmed, setQuickRetranscriptionConfirmed] = useState(false);
    const [quickRetranscriptionError, setQuickRetranscriptionError] = useState<string | null>(null);
    const [resumeLocalTranscription, setResumeLocalTranscription] = useState(false);
    useEffect(() => {
        preflightEpoch.current += 1;
        setQuickRetranscriptionChecking(false);
        setShowQuickRetranscription(false);
        setQuickRetranscriptionPreflight(null);
        setQuickRetranscriptionConfirmed(false);
        setQuickRetranscriptionError(null);
        return () => { preflightEpoch.current += 1; };
    }, [project?.id]);
    const transcribe = () => project && withBusy(tr("app.s0120"), async () => {
        if (!capabilities.hasBoundMedia)
            throw new Error(tr("app.capability.mediaRequired"));
        if (transcriptionMode === "multispeaker") {
            if (!runtime?.ffmpegConfigured)
                throw new Error(tr("app.s0121"));
            if (transcriptionHealth?.state !== "healthy")
                throw new Error(tr("app.moss.health.required"));
            await editing.flush(project.id);
            const expectedVersionId = projectRef.current?.id === project.id ? projectRef.current.history.currentVersionId : null;
            if (!expectedVersionId) throw new Error("editing_version_conflict");
            const envelope = await transcriptionCommands.startMultispeaker({
                expectedVersionId,
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
        const localTranscription = localResources?.capabilities.find((capability) => capability.id === "local_transcription");
        const activeModelPath = modelPath;
        const modelReady = Boolean(activeModelPath && modelPathAvailable && await localFileAvailable(activeModelPath));
        if (!["ready", "update_available"].includes(localTranscription?.state ?? "not_ready") || !runtime?.ffmpegConfigured || !runtime.asrConfigured || !activeModelPath || !modelReady) {
            setModelPathAvailable(false);
            setPendingResourceAction("transcribe");
            await openResourcePreparation("local_transcription", "on_demand");
            return;
        }
        await editing.flush(project.id);
        const expectedVersionId = projectRef.current?.id === project.id ? projectRef.current.history.currentVersionId : null;
        if (!expectedVersionId)
            throw new Error(tr("app.quickRetranscribe.versionMissing"));
        const result = await transcriptionCommands.start(project.id, activeModelPath, transcriptionLanguage, expectedVersionId);
        setTranscriptionJob(result.transcriptionJob ?? null);
        setNotice(tr("app.transcription.backgroundStarted"));
    });
    useEffect(() => {
        if (!resumeLocalTranscription)
            return;
        setResumeLocalTranscription(false);
        void transcribe();
    }, [resumeLocalTranscription]);
    const openQuickRetranscription = async () => {
        if (!project || quickRetranscriptionChecking)
            return;
        const epoch = ++preflightEpoch.current;
        setShowQuickRetranscription(true);
        setQuickRetranscriptionPreflight(null);
        setQuickRetranscriptionConfirmed(false);
        setQuickRetranscriptionError(null);
        setQuickRetranscriptionChecking(true);
        try {
            const envelope = await transcriptEditingClient.transcriptReplacementPreflight(project.id);
            if (epoch !== preflightEpoch.current) return;
            if (!envelope.transcriptReplacementPreflight)
                throw new Error(tr("app.quickRetranscribe.preflightMissing"));
            setQuickRetranscriptionPreflight(envelope.transcriptReplacementPreflight);
        }
        catch (cause) {
            if (epoch === preflightEpoch.current) setQuickRetranscriptionError(cause instanceof Error ? cause.message : String(cause));
        }
        finally {
            if (epoch === preflightEpoch.current) setQuickRetranscriptionChecking(false);
        }
    };
    const closeQuickRetranscription = () => {
        if (busyRef.current)
            return;
        setShowQuickRetranscription(false);
        setQuickRetranscriptionPreflight(null);
        setQuickRetranscriptionConfirmed(false);
        setQuickRetranscriptionError(null);
    };
    const confirmQuickRetranscription = async () => {
        if (!project || !quickRetranscriptionPreflight?.canReplace || !quickRetranscriptionConfirmed || busyRef.current)
            return;
        busyRef.current = true;
        setBusy(tr("app.quickRetranscribe.running"));
        setError(null);
        setQuickRetranscriptionError(null);
        try {
            if (!capabilities.hasBoundMedia)
                throw new Error(tr("app.capability.mediaRequired"));
            if (!runtime?.ffmpegConfigured)
                throw new Error(tr("app.s0121"));
            if (!runtime.asrConfigured)
                throw new Error(tr("app.s0122"));
            if (!modelPath || !modelPathAvailable || !await localFileAvailable(modelPath)) {
                setModelPathAvailable(false);
                throw new Error(tr("app.s0123"));
            }
            await editing.flush(project.id);
            const result = await transcriptionCommands.start(project.id, modelPath, transcriptionLanguage, quickRetranscriptionPreflight.currentVersionId);
            setTranscriptionJob(result.transcriptionJob ?? null);
            setShowQuickRetranscription(false);
            setQuickRetranscriptionPreflight(null);
            setQuickRetranscriptionConfirmed(false);
            setNotice(tr("app.transcription.backgroundStarted"));
        }
        catch (cause) {
            setQuickRetranscriptionError(cause instanceof Error ? cause.message : String(cause));
            if ((cause as {code?:string}).code?.includes("version")) { setQuickRetranscriptionPreflight(null); setQuickRetranscriptionConfirmed(false); }
        }
        finally {
            busyRef.current = false;
            setBusy(null);
        }
    };
    return { showQuickRetranscription, quickRetranscriptionPreflight, quickRetranscriptionChecking, quickRetranscriptionConfirmed, setQuickRetranscriptionConfirmed, quickRetranscriptionError, resumeLocalTranscription, setResumeLocalTranscription, transcribe, openQuickRetranscription, closeQuickRetranscription, confirmQuickRetranscription };
}
