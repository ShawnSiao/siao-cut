import { CircleAlert, Download, LoaderCircle, RefreshCw, Search, ShieldCheck, X } from "lucide-react";
import type { RefObject } from "react";
import { formatBytes, formatTime, isHttpsSourceUrl, sourceStatusLabel } from "../app-view-model";
import { tr } from "../i18n";
import type { SourceImportJob, SourcePreview } from "../types";
import { JobFailureDetails } from "./job-failure";
import { Dialog } from "./ui";

type SourceImportDialogProps = {
  returnFocusRef: RefObject<HTMLElement | null>;
  sourceUrl: string;
  sourcePreview: SourcePreview | null;
  sourceJob: SourceImportJob | null;
  sourceAuthorized: boolean;
  sourceBusy: string | null;
  sourceError: string | null;
  onClose: () => void;
  onSourceUrlChange: (value: string) => void;
  onAuthorizedChange: (value: boolean) => void;
  onInspect: () => void;
  onStart: () => void;
  onCancel: () => void;
  onResume: () => void;
  onReset: () => void;
};

export default function SourceImportDialog({
  returnFocusRef,
  sourceUrl,
  sourcePreview,
  sourceJob,
  sourceAuthorized,
  sourceBusy,
  sourceError,
  onClose,
  onSourceUrlChange,
  onAuthorizedChange,
  onInspect,
  onStart,
  onCancel,
  onResume,
  onReset,
}: SourceImportDialogProps) {
  return <Dialog label={tr("app.s0513")} className="runtime-dialog source-dialog" onClose={onClose} returnFocusRef={returnFocusRef}>
    <button autoFocus className="dialog-close" aria-label={tr("app.s0514")} title={tr("app.s0515")} onClick={onClose}><X size={18}/></button>
    <p className="eyebrow">{tr("app.s0516")}</p><h2>{tr("app.s0517")}</h2><p className="dialog-copy">{tr("app.s0518")}</p>
    {!sourceJob && <form className="source-form" onSubmit={(event) => { event.preventDefault(); onInspect(); }}><label><span>{tr("app.s0487")}</span><input autoComplete="url" aria-label={tr("app.s0487")} placeholder="https://…" value={sourceUrl} disabled={Boolean(sourceBusy)} onChange={(event) => onSourceUrlChange(event.target.value)}/></label><button className="button primary" type="submit" disabled={Boolean(sourceBusy) || !isHttpsSourceUrl(sourceUrl)}>{sourceBusy && !sourcePreview ? <LoaderCircle className="spin" size={14}/> : <Search size={14}/>}{tr("app.s0489")}</button></form>}
    {sourcePreview && !sourceJob && <section className="source-preview" aria-label={tr("app.s0519")}><header><span><small>{sourcePreview.extractor}</small><strong>{sourcePreview.title}</strong></span><ShieldCheck size={19}/></header><dl><div><dt>{tr("app.s0491")}</dt><dd>{formatTime(sourcePreview.durationSeconds)}</dd></div><div><dt>{sourcePreview.fileSizeKnown ? tr("app.s0520") : tr("app.s0521")}</dt><dd>{formatBytes(sourcePreview.fileSizeBytes)}</dd></div><div><dt>{tr("app.s0492")}</dt><dd>{sourcePreview.siteMediaId}</dd></div><div><dt>{tr("app.s0522")}</dt><dd>yt-dlp {sourcePreview.toolVersion}</dd></div></dl><p className="source-url" title={sourcePreview.webpageUrl}>{sourcePreview.webpageUrl}</p><label className="source-consent"><input type="checkbox" checked={sourceAuthorized} onChange={(event) => onAuthorizedChange(event.target.checked)}/><span>{tr("app.s0523")}</span></label><button className="button primary full" disabled={!sourceAuthorized || Boolean(sourceBusy)} onClick={onStart}>{sourceBusy ? <LoaderCircle className="spin" size={14}/> : <Download size={14}/>}{tr("app.s0524")}</button></section>}
    {sourceJob && <section className="source-job" aria-label={tr("app.s0525")}><header><span className={`source-state ${sourceJob.status}`}><i />{sourceStatusLabel(sourceJob.status)}</span><strong>{sourceJob.title}</strong><small>{tr("app.composite.sourceAttempt", { attempt: sourceJob.attemptCount, mediaId: sourceJob.siteMediaId })}</small></header><div className="source-job-progress"><progress value={sourceJob.progress} max={1}/><span>{Math.round(sourceJob.progress * 100)}% · {formatBytes(sourceJob.bytesDownloaded)} / {formatBytes(sourceJob.totalBytes ?? sourceJob.fileSizeBytes)}</span></div><dl><div><dt>{tr("app.s0528")}</dt><dd>yt-dlp {sourceJob.toolVersion}</dd></div><div><dt>{tr("app.s0239")}</dt><dd>{sourceJob.projectId ?? tr("app.s0529")}</dd></div></dl>{["failed", "interrupted"].includes(sourceJob.status) && <JobFailureDetails className="source-job-error" context="source" status={sourceJob.status} errorCode={sourceJob.errorCode} errorMessage={sourceJob.errorMessage}/>}<div className="source-job-actions">{["queued", "running"].includes(sourceJob.status) && <button disabled={Boolean(sourceBusy) || Boolean(sourceJob.cancelRequestedAt)} onClick={onCancel}>{sourceJob.cancelRequestedAt ? tr("app.s0317") : tr("app.s0530")}</button>}{["cancelled", "failed", "interrupted"].includes(sourceJob.status) && <button className="primary" disabled={Boolean(sourceBusy)} onClick={onResume}><RefreshCw size={13}/>{tr("app.s0279")}</button>}{!["queued", "running", "finalizing"].includes(sourceJob.status) && <button onClick={onReset}>{tr("app.s0531")}</button>}</div></section>}
    {sourceError && <div className="source-error" role="alert"><CircleAlert size={15}/>{sourceJob?.status === "completed" ? <div className="job-failure-details"><span className="job-failure-summary">{tr("app.error.sourceImportOpenFailed")}</span><details className="job-failure-technical"><summary>{tr("app.error.technicalDetails")}</summary><code>{sourceError}</code></details></div> : <JobFailureDetails context="source" status="failed" errorMessage={sourceError}/>}</div>}
    <p className="runtime-disclosure">{tr("app.s0532")}</p>
  </Dialog>;
}
