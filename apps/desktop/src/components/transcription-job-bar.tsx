import { CircleAlert, LoaderCircle, RefreshCw } from "lucide-react";
import { tr } from "../i18n";
import type { TranscriptionJob } from "../types";
import { JobFailureDetails } from "./job-failure";

const stageLabel = (stage: string) => ({
  queued: tr("app.moss.stage.queued"),
  preparing_audio: tr("app.moss.stage.preparing_audio"),
  requesting_model: tr("app.moss.stage.requesting_model"),
  validating_result: tr("app.moss.stage.validating_result"),
  awaiting_apply: tr("app.moss.stage.awaitingApply"),
  completed: tr("app.moss.stage.completed"),
  failed: tr("app.moss.stage.failed"),
  interrupted: tr("app.moss.stage.interrupted"),
  cancelled: tr("app.moss.stage.cancelled"),
  discarded: tr("app.moss.stage.discarded"),
}[stage] ?? stage);

type Props = {
  job: TranscriptionJob;
  busy: boolean;
  onCancel: () => void;
  onResume: () => void;
  onInspectCandidate: () => void;
  onDiscardCandidate: () => void;
};

export default function TranscriptionJobBar({ job, busy, onCancel, onResume, onInspectCandidate, onDiscardCandidate }: Props) {
  const active = ["queued", "running", "finalizing"].includes(job.status);
  const waiting = job.status === "awaiting_apply";
  const visible = active || waiting || ["failed", "interrupted", "cancelled"].includes(job.status);
  if (!visible) return null;

  return <section className={`transcription-job-bar ${job.status}`} role="status" aria-label={tr("app.moss.job.statusLabel")}>
    {active ? <LoaderCircle size={15} className="spin"/> : <CircleAlert size={15}/>}
    <span>
      <strong>{stageLabel(job.stage)}</strong>
      <small>{waiting && job.candidate
        ? tr("app.moss.candidate.summary", { segments: job.candidate.segmentCount, speakers: job.candidate.speakerCount, warnings: job.candidate.warningCount })
        : `${tr("app.moss.job.attempt", { attempt: job.attemptCount })} · ${job.modelId}`}</small>
    </span>
    {job.errorMessage && <JobFailureDetails context="transcription" status={job.status} errorCode={job.errorCode} errorMessage={job.errorMessage}/>}
    {waiting && <div className="transcription-job-actions">
      <button className="primary" disabled={busy} onClick={onInspectCandidate}>{tr("app.moss.candidate.inspect")}</button>
      <button disabled={busy} onClick={onDiscardCandidate}>{tr("app.moss.candidate.discard")}</button>
    </div>}
    {active && <button disabled={busy} onClick={onCancel}>{tr("app.moss.job.cancel")}</button>}
    {["failed", "interrupted", "cancelled"].includes(job.status) && <button disabled={busy} onClick={onResume}><RefreshCw size={13}/>{tr("app.moss.job.resume")}</button>}
  </section>;
}
