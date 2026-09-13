import { useState, type Dispatch, type RefObject, type SetStateAction } from "react";
import { backgroundTaskClient } from "../../domains/background-task-client";
import { localResourceClient } from "../../domains/local-resource-client";
import { useBackgroundTaskRegistry } from "../../hooks/use-background-task-registry";
import { tr } from "../../i18n";
import type { AudioAnalysisJob, AutoWorkflow, LocalResourceJob, ModelDownloadJob, ModelStatus, SourceImportJob, SpeakerJob, SpeakerPackageStatus, TranscriptionJob } from "../../types";
import { useTranscriptionCommands } from "./use-transcription-commands";
import { useTranscriptionTasks } from "./use-transcription-tasks";

type Events = {
projectId: string | undefined; activeProjectIdRef: RefObject<string | null>; setError: Dispatch<SetStateAction<string | null>>; setNotice: (text: string | null) => void;
  onSourceCompleted: (job: SourceImportJob) => Promise<void>; onWorkflowTransition: (workflow: AutoWorkflow) => Promise<void>;
  onModelsReady: (models: ModelStatus[], modelId: string) => void; onSpeakerPackageReady: (status: SpeakerPackageStatus | null) => void;
  onSpeakerAnalysisReady: (projectId: string) => Promise<void>; onTranscriptionApplied: (job: TranscriptionJob) => Promise<void>;
  onSourceError: (message: string | null) => void; onResourceError: (error: unknown) => void;
};
const active = (status: string) => ["queued", "running", "finalizing"].includes(status);
function upsert<T extends { id: string }>(items: T[], next: T) { return items.some((item) => item.id === next.id) ? items.map((item) => item.id === next.id ? next : item) : [...items, next]; }

/** Owns persisted job snapshots; completion events carry IDs, never a second writable Project. */
export function useBackgroundSession(events: Events) {
  const [audioAnalysisJob, setAudioAnalysisJob] = useState<AudioAnalysisJob | null>(null);
  const [speakerJob, setSpeakerJob] = useState<SpeakerJob | null>(null);
  const [speakerJobs, setSpeakerJobs] = useState<SpeakerJob[]>([]);
  const [sourceJob, setSourceJob] = useState<SourceImportJob | null>(null);
  const [modelJob, setModelJob] = useState<ModelDownloadJob | null>(null);
  const [resourceJob, setResourceJob] = useState<LocalResourceJob | null>(null);
  const [autoWorkflow, setAutoWorkflow] = useState<AutoWorkflow | null>(null);
  const [autoWorkflows, setAutoWorkflows] = useState<AutoWorkflow[]>([]);
  const transcriptionCommands = useTranscriptionCommands();
  const transcriptionTasks = useTranscriptionTasks(events.onTranscriptionApplied);
  const transcriptionJob = transcriptionTasks.jobs.filter((job) => job.projectId === events.projectId).at(-1) ?? null;
  const setTranscriptionJob = transcriptionTasks.track;
  const failed = (error: unknown) => events.setError(error instanceof Error ? error.message : String(error));
  useBackgroundTaskRegistry([
    resourceJob && active(resourceJob.status) ? { key: `resource:${resourceJob.id}`, intervalMs: 800, poll: () => localResourceClient.getJob(resourceJob.id).then((result) => { if (result.resourceJob) setResourceJob(result.resourceJob); }).catch(events.onResourceError) } : null,
    audioAnalysisJob && active(audioAnalysisJob.status) ? {
key: `audio:${audioAnalysisJob.id}`, intervalMs: 700, poll: () => backgroundTaskClient.getAudioAnalysis(audioAnalysisJob.id).then((result) => {
        const job = result.audioAnalysisJob; if (!job) return;
        if (events.activeProjectIdRef.current === job.projectId) setAudioAnalysisJob(job);
        if (job.status === "completed") events.setNotice(tr("app.s0054"));
        if (["failed", "interrupted"].includes(job.status)) events.setError(job.errorMessage ?? tr("app.s0055"));
        if (job.status === "cancelled") events.setNotice(tr("app.s0056"));
      }).catch(failed)
} : null,
    modelJob && active(modelJob.status) ? {
key: `model:${modelJob.id}`, intervalMs: 800, poll: () => backgroundTaskClient.getModelJob(modelJob.id).then(async (result) => {
        const job = result.modelJob; if (!job) return; setModelJob(job);
        if (job.status === "completed") { const catalog = await backgroundTaskClient.listModels(true); events.onModelsReady(catalog.models ?? [], job.modelId); events.setNotice(tr("app.s0057")); }
        if (job.status === "failed") events.setError(job.errorMessage ?? tr("app.s0058"));
        if (job.status === "cancelled") events.setNotice(tr("app.s0059"));
      }).catch(failed)
} : null,
    ...speakerJobs.filter((job) => active(job.status)).map((job) => ({
key: `speaker:${job.id}`, intervalMs: 800, poll: () => backgroundTaskClient.getSpeakerJob(job.id).then(async (result) => {
        const next = result.speakerJob; if (!next) return; setSpeakerJobs((items) => upsert(items, next)); setSpeakerJob((current) => current?.id === next.id ? next : current);
        if (next.status === "completed" && next.kind === "install") { const result = await backgroundTaskClient.getSpeakerPackage(); events.onSpeakerPackageReady(result.speakerPackage ?? null); events.setNotice(tr("app.s0060")); }
        if (next.status === "completed" && next.kind === "analyze" && next.projectId) { await events.onSpeakerAnalysisReady(next.projectId); events.setNotice(tr("app.s0061")); }
        if (["failed", "interrupted"].includes(next.status)) events.setError(next.errorMessage ?? tr("app.s0062"));
        if (next.status === "cancelled") events.setNotice(tr("app.s0063"));
      }).catch(failed)
})),
    sourceJob && active(sourceJob.status) ? {
key: `source:${sourceJob.id}`, intervalMs: 600, poll: () => backgroundTaskClient.getSourceJob(sourceJob.id).then(async (result) => {
        const job = result.sourceJob; if (!job) return; setSourceJob(job);
        if (["failed", "interrupted"].includes(job.status)) { const message = job.errorMessage ?? tr("app.s0064"); events.onSourceError(message); events.setError(message); }
        if (job.status === "cancelled") events.setNotice(tr("app.s0066"));
        if (job.status === "completed" && job.projectId) await events.onSourceCompleted(job);
      }).catch((error) => { events.onSourceError(String(error)); failed(error); })
} : null,
    ...autoWorkflows.filter((job) => active(job.status) || ["needs_agent", "awaiting_authorization", "needs_review"].includes(job.status)).map((job) => ({
key: `auto:${job.id}`, intervalMs: 800, poll: () => backgroundTaskClient.getAutoWorkflow(job.id).then(async (result) => {
        const next = result.workflow; if (!next || Date.parse(next.updatedAt) < Date.parse(job.updatedAt)) return;
        setAutoWorkflows((items) => upsert(items, next)); setAutoWorkflow((current) => current?.id === next.id ? next : current);
        // A waiting workflow is stable; polling it must not reload all project domains.
        if (next.status !== job.status || next.currentStage !== job.currentStage || next.projectId !== job.projectId) await events.onWorkflowTransition(next);
        if (next.status === "completed") events.setNotice(tr("app.s0068", { "0": next.outputPath }));
        if (["failed", "interrupted"].includes(next.status)) events.setError(next.errorMessage ?? tr("app.s0069"));
      }).catch(failed)
})),
  ]);
  return { audioAnalysisJob, setAudioAnalysisJob, speakerJob, setSpeakerJob, speakerJobs, setSpeakerJobs, sourceJob, setSourceJob, modelJob, setModelJob, resourceJob, setResourceJob, autoWorkflow, setAutoWorkflow, autoWorkflows, setAutoWorkflows, transcriptionCommands, transcriptionTasks, transcriptionJob, setTranscriptionJob };
}
