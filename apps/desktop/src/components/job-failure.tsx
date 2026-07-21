import { tr } from "../i18n";
import type { TranslationKey } from "../locales";

export type JobFailureContext = "export" | "source" | "model" | "audio" | "speaker" | "transcription" | "agent" | "auto";

const contextMessageKeys: Record<JobFailureContext, TranslationKey> = {
  export: "app.error.exportFailed",
  source: "app.error.sourceImportFailed",
  model: "app.error.modelDownloadFailed",
  audio: "app.error.audioAnalysisFailed",
  speaker: "app.error.speakerTaskFailed",
  transcription: "app.error.transcriptionFailed",
  agent: "app.error.agentTaskFailed",
  auto: "app.error.autoWorkflowFailed",
};

const codeMessageKeys: Partial<Record<string, TranslationKey>> = {
  job_failed: "app.error.jobFailed",
  job_interrupted: "app.error.jobInterrupted",
  disk_space_low: "app.error.diskSpaceLow",
  model_hash_mismatch: "app.error.integrityCheckFailed",
  source_tool_hash_mismatch: "app.error.integrityCheckFailed",
  speaker_package_hash_mismatch: "app.error.integrityCheckFailed",
};

export function jobErrorSummary(context: JobFailureContext, status: string, errorCode?: string | null): string {
  if (status === "cancelled") return tr("app.error.jobCancelled");
  if (status === "interrupted") return tr("app.error.jobInterrupted");
  return tr((errorCode && codeMessageKeys[errorCode]) || contextMessageKeys[context]);
}

export function JobFailureDetails({ context, status, errorCode, errorMessage, className = "" }: {
  context: JobFailureContext;
  status: string;
  errorCode?: string | null;
  errorMessage?: string | null;
  className?: string;
}) {
  const technicalDetails = errorMessage?.trim();
  return <div className={`job-failure-details ${className}`.trim()}>
    <span className="job-failure-summary">{jobErrorSummary(context, status, errorCode)}</span>
    {technicalDetails && <details className="job-failure-technical"><summary>{tr("app.error.technicalDetails")}</summary><code>{technicalDetails}</code></details>}
  </div>;
}
