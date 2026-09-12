import { useCallback,useState,type RefObject } from "react";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { tr } from "../../i18n";
import type { Project,TranscriptionJob,TranscriptionProviderConfig,TranscriptionProviderHealth,TranscriptionReviewItem } from "../../types";
import type { EditingSession } from "../editing/editing-session";
import type { useTranscriptionCommands } from "./use-transcription-commands";
type Inputs = {
  project: Project | null; editing: EditingSession; transcriptionJob: TranscriptionJob | null;
  setTranscriptionJob: (job: TranscriptionJob | null) => void; transcriptionCommands: ReturnType<typeof useTranscriptionCommands>;
  activeProjectIdRef: RefObject<string | null>; isCurrentProjectLoad: (id: string, sequence: number) => boolean;
  refreshProject: (id: string) => Promise<Project>; refreshSpeakerTrack: (id: string) => Promise<void>;
  withBusy: (label: string, action: () => Promise<void>) => Promise<void>; setNotice: (message: string | null) => void;
};
/** Provider preferences and candidate review belong to the transcription domain. */
export function useTranscriptionReviewSession({project, editing, transcriptionJob, setTranscriptionJob, transcriptionCommands, activeProjectIdRef, isCurrentProjectLoad, refreshProject, refreshSpeakerTrack, withBusy, setNotice}: Inputs) {
    const [transcriptionMode, setTranscriptionMode] = useState<"quick" | "multispeaker">(() => localStorage.getItem("siaocut.transcriptionMode") === "multispeaker" ? "multispeaker" : "quick");
    const [transcriptionConfig, setTranscriptionConfig] = useState<TranscriptionProviderConfig | null>(null);
    const [transcriptionHealth, setTranscriptionHealth] = useState<TranscriptionProviderHealth | null>(null);
    const [pendingCandidateJobId, setPendingCandidateJobId] = useState<string | null>(null);
    const [showTranscriptionCandidate, setShowTranscriptionCandidate] = useState(false);
    const [transcriptionApplyConfirmed, setTranscriptionApplyConfirmed] = useState(false);
    const [transcriptionReviews, setTranscriptionReviews] = useState<TranscriptionReviewItem[]>([]);
    const [transcriptionPrompt, setTranscriptionPrompt] = useState("");
    const [transcriptionHotwords, setTranscriptionHotwords] = useState("");
    const refreshTranscription = useCallback(async (projectId: string, loadSequence?: number) => {
        const [latest, reviews] = await Promise.all([
            backgroundTaskClient.latestTranscription(projectId),
            backgroundTaskClient.listTranscriptionReviews(projectId),
        ]);
        if (activeProjectIdRef.current === projectId && (loadSequence === undefined || isCurrentProjectLoad(projectId, loadSequence))) {
            setTranscriptionJob(latest.transcriptionJob ?? null);
            setTranscriptionReviews(reviews.reviewItems ?? []);
        }
    }, [isCurrentProjectLoad]);
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
        const envelope = await transcriptionCommands.retry(transcriptionJob.id, transcriptionJob.attemptCount);
        if (!envelope.transcriptionJob)
            throw new Error(tr("app.moss.job.missing"));
        setTranscriptionJob(envelope.transcriptionJob);
    });
    const applyTranscriptionCandidate = (versionId: string) => transcriptionJob?.candidate && project && withBusy(tr("app.moss.candidate.applying"), async () => {
        await editing.flush(project.id);
        const envelope = await transcriptionCommands.apply(transcriptionJob.id, versionId);
        if (!envelope.transcriptionJob)
            throw new Error(tr("app.moss.job.missing"));
        setShowTranscriptionCandidate(false);
        setTranscriptionApplyConfirmed(false);
        setTranscriptionJob(envelope.transcriptionJob);
        await refreshProject(project.id);
        await refreshTranscription(project.id);
        await refreshSpeakerTrack(project.id);
        setNotice(tr("app.moss.candidate.applied"));
    });
    const discardTranscriptionCandidate = () => transcriptionJob && withBusy(tr("app.moss.candidate.discarding"), async () => {
        const envelope = await transcriptionCommands.discard(transcriptionJob.id);
        if (!envelope.transcriptionJob)
            throw new Error(tr("app.moss.job.missing"));
        setTranscriptionJob(envelope.transcriptionJob);
        setShowTranscriptionCandidate(false);
        setTranscriptionApplyConfirmed(false);
        setNotice(tr("app.moss.candidate.discarded"));
    });
    const resolveTranscriptionReview = (itemId: string, action: "resolved" | "ignored") => withBusy(tr("app.moss.review.saving"), async () => {
        if (!project) return;
        await editing.mutate(project.id, {kind:"resolve_transcription_review",itemId,action});
        if (project)
            await refreshTranscription(project.id);
    });
    return { transcriptionMode, setTranscriptionMode, transcriptionConfig, setTranscriptionConfig, transcriptionHealth, setTranscriptionHealth, pendingCandidateJobId, setPendingCandidateJobId, showTranscriptionCandidate, setShowTranscriptionCandidate, transcriptionApplyConfirmed, setTranscriptionApplyConfirmed, transcriptionReviews, setTranscriptionReviews, transcriptionPrompt, setTranscriptionPrompt, transcriptionHotwords, setTranscriptionHotwords, refreshTranscription, saveTranscriptionProvider, checkTranscriptionProvider, cancelTranscription, resumeTranscription, applyTranscriptionCandidate, discardTranscriptionCandidate, resolveTranscriptionReview };
}
