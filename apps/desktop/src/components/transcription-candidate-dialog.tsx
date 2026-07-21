import { CircleAlert, FileText, LoaderCircle, Trash2, X } from "lucide-react";
import { tr } from "../i18n";
import type { TranscriptionJob } from "../types";
import { Dialog } from "./ui";

type Props = {
  job: TranscriptionJob;
  busy: boolean;
  confirmed: boolean;
  onConfirmedChange: (confirmed: boolean) => void;
  onApply: () => void;
  onDiscard: () => void;
  onClose: () => void;
};

export default function TranscriptionCandidateDialog({ job, busy, confirmed, onConfirmedChange, onApply, onDiscard, onClose }: Props) {
  const candidate = job.candidate;
  if (!candidate) return null;
  const duration = candidate.durationSeconds == null ? tr("app.moss.candidate.durationUnknown") : tr("app.moss.candidate.duration", { seconds: Math.round(candidate.durationSeconds) });

  return <Dialog label={tr("app.moss.candidate.dialogTitle")} className="confirm-dialog transcription-candidate-dialog" onClose={onClose}>
    <button className="dialog-close" aria-label={tr("app.moss.candidate.close")} onClick={onClose}><X size={18}/></button>
    <div className="confirm-icon warning"><FileText size={20}/></div>
    <p className="eyebrow">{tr("app.moss.candidate.eyebrow")}</p>
    <h2>{tr("app.moss.candidate.title")}</h2>
    <p className="dialog-copy">{tr("app.moss.candidate.explanation")}</p>
    <dl className="candidate-summary">
      <div><dt>{tr("app.moss.candidate.segments")}</dt><dd>{candidate.segmentCount}</dd></div>
      <div><dt>{tr("app.moss.candidate.speakers")}</dt><dd>{candidate.speakerCount}</dd></div>
      <div><dt>{tr("app.moss.candidate.length")}</dt><dd>{duration}</dd></div>
      <div><dt>{tr("app.moss.candidate.warnings")}</dt><dd>{candidate.warningCount}</dd></div>
    </dl>
    <div className="candidate-version-warning" role="note"><CircleAlert size={16}/><span>{tr("app.moss.candidate.versionWarning")}</span></div>
    <label className="source-consent"><input type="checkbox" checked={confirmed} onChange={(event) => onConfirmedChange(event.target.checked)}/><span>{tr("app.moss.candidate.confirmReplace")}</span></label>
    <div className="confirm-actions candidate-actions">
      <button className="button quiet" disabled={busy} onClick={onDiscard}>{busy ? <LoaderCircle className="spin" size={14}/> : <Trash2 size={14}/>}{tr("app.moss.candidate.discard")}</button>
      <button className="button primary" disabled={busy || !confirmed || !candidate.canApply} onClick={onApply}>{busy && <LoaderCircle className="spin" size={14}/>} {tr("app.moss.candidate.apply")}</button>
    </div>
  </Dialog>;
}
