import { useCallback,type RefObject } from "react";
import { getProjectCapabilities } from "../../app-view-model";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { tr } from "../../i18n";
import type { Project,RuntimeInfo } from "../../types";
import type { useBackgroundSession } from "./use-background-session";
type Inputs = Pick<ReturnType<typeof useBackgroundSession>, "audioAnalysisJob" | "setAudioAnalysisJob"> & {project:Project|null;mediaUrl:string|null;runtime:RuntimeInfo|null;
activeProjectIdRef:RefObject<string|null>;isCurrentProjectLoad:(id:string,sequence:number)=>boolean;setNotice:(value:string|null)=>void;withBusy:(label:string,action:()=>Promise<void>)=>Promise<void>};
export function useAudioAnalysisSession({project, mediaUrl, runtime, audioAnalysisJob, setAudioAnalysisJob, activeProjectIdRef, isCurrentProjectLoad, setNotice, withBusy}:Inputs) {
    const capabilities = getProjectCapabilities(project,{mediaUrl});
    const refreshLatestAudioAnalysis = useCallback(async (projectId: string, loadSequence?: number) => {
        const envelope = await backgroundTaskClient.latestAudioAnalysis(projectId);
        if (activeProjectIdRef.current === projectId && (loadSequence === undefined || isCurrentProjectLoad(projectId, loadSequence)))
            setAudioAnalysisJob(envelope.audioAnalysisJob ?? null);
    }, [isCurrentProjectLoad]);
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
    return { refreshLatestAudioAnalysis, startAudioAnalysis, cancelAudioAnalysis, resumeAudioAnalysis };
}
