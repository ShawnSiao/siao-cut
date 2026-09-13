import { useCallback,useState,type Dispatch,type RefObject,type SetStateAction } from "react";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { transcriptEditingClient } from "../../domains/transcript-editing-client";
import { tr } from "../../i18n";
import type { Project,SpeakerJob,SpeakerPackageStatus,SpeakerTrack } from "../../types";
import type { EditingSession } from "../editing/editing-session";
import { upsertById } from "./auto-workflow-snapshots";
type Inputs = {
  project: Project | null; editing: EditingSession; speakerJob: SpeakerJob | null;
  setSpeakerJob: Dispatch<SetStateAction<SpeakerJob | null>>; setSpeakerJobs: Dispatch<SetStateAction<SpeakerJob[]>>;
  activeProjectIdRef: RefObject<string | null>; isCurrentProjectLoad: (id: string, sequence: number) => boolean;
  refreshProject: (id: string) => Promise<Project>; withBusy: (label: string, action: () => Promise<void>) => Promise<void>;
  setNotice: (message: string | null) => void;
};
/** Owns speaker package/track state; job snapshots stay in the background session. */
export function useSpeakerSession({project, editing, speakerJob, setSpeakerJob, setSpeakerJobs, activeProjectIdRef, isCurrentProjectLoad, refreshProject, withBusy, setNotice}: Inputs) {
    const [speakerPackage, setSpeakerPackage] = useState<SpeakerPackageStatus | null>(null);
    const [speakerTrack, setSpeakerTrack] = useState<SpeakerTrack | null>(null);
    const refreshSpeakerTrack = useCallback(async (projectId: string, loadSequence?: number) => {
        const envelope = await transcriptEditingClient.getSpeakerTrack(projectId);
        if (activeProjectIdRef.current === projectId && (loadSequence === undefined || isCurrentProjectLoad(projectId, loadSequence)))
            setSpeakerTrack(envelope.speakerTrack ?? null);
    }, [isCurrentProjectLoad]);
    const installSpeakerPackage = () => withBusy(tr("app.s0133"), async () => {
        const envelope = await backgroundTaskClient.installSpeakerPackage();
        if (!envelope.speakerJob)
            throw new Error(tr("app.s0134"));
        setSpeakerJobs((current) => upsertById(current, envelope.speakerJob!));
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
        setSpeakerJobs((current) => upsertById(current, envelope.speakerJob!));
        setSpeakerJob(envelope.speakerJob);
        if (envelope.speakerJob.status === "completed") {
            await Promise.all([refreshProject(project.id), refreshSpeakerTrack(project.id)]);
            setNotice(tr("app.s0061"));
        }
        else {
            setNotice(tr("app.s0140"));
        }
    });
    const cancelSpeakerJob = (target: SpeakerJob | null = speakerJob) => target && withBusy(tr("app.s0141"), async () => {
        const envelope = await backgroundTaskClient.cancelSpeakerJob(target.id);
        if (envelope.speakerJob) {
            setSpeakerJobs((current) => upsertById(current, envelope.speakerJob!));
            setSpeakerJob(envelope.speakerJob);
        }
    });
    const resumeSpeakerJob = (target: SpeakerJob | null = speakerJob) => target && withBusy(tr("app.s0142"), async () => {
        const envelope = await backgroundTaskClient.resumeSpeakerJob(target.id);
        if (!envelope.speakerJob)
            throw new Error(tr("app.s0143"));
        setSpeakerJobs((current) => upsertById(current, envelope.speakerJob!));
        setSpeakerJob(envelope.speakerJob);
        setNotice(tr("app.s0144", { "0": envelope.speakerJob.attemptCount }));
    });
    const renameSpeaker = (speakerId: string, name: string) => project && withBusy(tr("app.s0145"), async () => {
        const envelope = await editing.mutate(project.id, { kind: "rename_speaker", speakerId, name });
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0146"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0147"));
    });
    const mergeSpeaker = (fromId: string, intoId: string) => project && withBusy(tr("app.s0148"), async () => {
        const envelope = await editing.mutate(project.id, { kind: "merge_speaker", fromId, intoId });
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0149"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0150"));
    });
    const assignSpeaker = (segmentId: string, speakerId: string) => project && withBusy(tr("app.s0151"), async () => {
        const envelope = await editing.mutate(project.id, { kind: "assign_speaker", segmentId, speakerId });
        if (!envelope.speakerTrack)
            throw new Error(tr("app.s0146"));
        setSpeakerTrack(envelope.speakerTrack);
        await refreshProject(project.id);
        setNotice(tr("app.s0152"));
    });
    return { speakerPackage, setSpeakerPackage, speakerTrack, setSpeakerTrack, refreshSpeakerTrack, installSpeakerPackage, startSpeakerAnalysis, cancelSpeakerJob, resumeSpeakerJob, renameSpeaker, mergeSpeaker, assignSpeaker };
}
