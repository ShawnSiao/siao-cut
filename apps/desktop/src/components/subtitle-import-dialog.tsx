import { Check, CircleAlert, FolderOpen, LoaderCircle, X } from "lucide-react";
import type { RefObject } from "react";
import { formatTime, subtitleCountLabel, subtitleIssueLabel, subtitleQualityStatusLabel } from "../app-view-model";
import { tr } from "../i18n";
import type { SubtitleImportPreview } from "../types";
import { Dialog } from "./ui";

type SubtitleImportDialogProps = {
  returnFocusRef: RefObject<HTMLElement | null>;
  path: string;
  busy: string | null;
  error: string | null;
  preview: SubtitleImportPreview | null;
  confirmed: boolean;
  onClose: () => void;
  onInspect: () => void;
  onConfirmedChange: (value: boolean) => void;
  onConfirm: () => void;
};

export default function SubtitleImportDialog({
  returnFocusRef,
  path,
  busy,
  error,
  preview,
  confirmed,
  onClose,
  onInspect,
  onConfirmedChange,
  onConfirm,
}: SubtitleImportDialogProps) {
  return <Dialog label={tr("app.s0333")} className="runtime-dialog subtitle-import-dialog" onClose={onClose} returnFocusRef={returnFocusRef}>
    <button autoFocus className="dialog-close" aria-label={tr("app.s0462")} title={tr("app.s0463")} onClick={onClose}><X size={18}/></button>
    <p className="eyebrow">{tr("app.s0464")}</p><h2>{tr("app.s0465")}</h2>
    <p className="dialog-copy">{tr("app.s0466")}</p>
    <div className="subtitle-import-file"><span><small>{tr("app.s0467")}</small><strong title={path}>{path ? path.split(/[\\/]/).at(-1) : tr("app.s0468")}</strong></span><button className="button quiet" disabled={Boolean(busy)} onClick={onInspect}><FolderOpen size={14}/>{path ? tr("app.s0469") : tr("app.s0470")}</button></div>
    {busy && <div className="subtitle-import-progress" role="status"><LoaderCircle className="spin" size={14}/>{busy}</div>}
    {preview && <section className={`subtitle-import-preview ${preview.quality.status}`} aria-label={tr("app.s0471")}>
      <header><span><small>{preview.format.toUpperCase()} · SHA-256 {preview.sha256.slice(0, 10)}…</small><strong>{tr("app.composite.subtitleSegments", { label: subtitleCountLabel(preview.segmentCount) })}</strong></span>{preview.quality.status === "good" ? <Check size={18}/> : <CircleAlert size={18}/>}</header>
      <div className="subtitle-import-quality"><strong>{subtitleQualityStatusLabel(preview.quality)}</strong><span>{preview.quality.errorCount}{tr("app.s0358") + " "}{preview.quality.warningCount}{tr("app.s0359")}</span></div>
      {preview.quality.issues.length > 0 && <div className="subtitle-import-issues">{preview.quality.issues.slice(0, 5).map((issue) => <div className={issue.severity} key={issue.id}><CircleAlert size={12}/><span><strong>{subtitleIssueLabel(issue.kind)}</strong><small>{formatTime(issue.start)} — {formatTime(issue.end)}</small></span></div>)}</div>}
      <label className="subtitle-replace-confirm"><input type="checkbox" checked={confirmed} disabled={!preview.canImport || Boolean(busy)} onChange={(event) => onConfirmedChange(event.target.checked)}/><span>{tr("app.s0472")}</span></label>
      <button className="button primary full" disabled={!preview.canImport || !confirmed || Boolean(busy)} onClick={onConfirm}>{preview.canImport ? tr("app.s0473") : tr("app.s0474")}</button>
      <p className="runtime-disclosure">{tr("app.s0475")}</p>
    </section>}
    {error && <div className="source-error" role="alert"><CircleAlert size={15}/>{error}</div>}
  </Dialog>;
}
