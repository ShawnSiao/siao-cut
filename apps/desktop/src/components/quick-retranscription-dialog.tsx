import { CircleAlert, LoaderCircle, RefreshCw, X } from "lucide-react";
import { tr } from "../i18n";
import type { TranscriptReplacementPreflight } from "../types";
import { Dialog } from "./ui";

type Props = {
  preflight: TranscriptReplacementPreflight | null;
  checking: boolean;
  busy: boolean;
  confirmed: boolean;
  blockerMessage: string | null;
  error: string | null;
  onConfirmedChange: (confirmed: boolean) => void;
  onConfirm: () => void;
  onClose: () => void;
};

export default function QuickRetranscriptionDialog({ preflight, checking, busy, confirmed, blockerMessage, error, onConfirmedChange, onConfirm, onClose }: Props) {
  const blocked = Boolean(preflight && !preflight.canReplace);

  return <Dialog label={tr("app.quickRetranscribe.dialogLabel")} className="confirm-dialog quick-retranscription-dialog" onClose={onClose}>
    <button className="dialog-close" aria-label={tr("app.quickRetranscribe.close")} onClick={onClose}><X size={18}/></button>
    <div className="confirm-icon warning"><RefreshCw size={20}/></div>
    <p className="eyebrow">{tr("app.quickRetranscribe.eyebrow")}</p>
    <h2>{tr("app.quickRetranscribe.title")}</h2>
    <p className="dialog-copy">{tr("app.quickRetranscribe.explanation")}</p>
    <div className="candidate-version-warning" role="note"><CircleAlert size={16}/><span>{tr("app.quickRetranscribe.safety")}</span></div>
    {checking && <div className="confirm-checking" role="status"><LoaderCircle className="spin" size={15}/>{tr("app.quickRetranscribe.checking")}</div>}
    {(blockerMessage || error) && <div className="confirm-error" role="alert"><CircleAlert size={16}/><span>{blockerMessage ?? error}</span></div>}
    <label className="source-consent">
      <input type="checkbox" checked={confirmed} disabled={checking || busy || blocked || !preflight} onChange={(event) => onConfirmedChange(event.target.checked)}/>
      <span>{tr("app.quickRetranscribe.confirm")}</span>
    </label>
    <div className="confirm-actions">
      <button className="button quiet" disabled={busy} onClick={onClose}>{tr("app.s0511")}</button>
      <button className="button primary" disabled={checking || busy || blocked || !preflight || !confirmed} onClick={onConfirm}>{busy ? <LoaderCircle className="spin" size={14}/> : <RefreshCw size={14}/>} {tr("app.quickRetranscribe.submit")}</button>
    </div>
  </Dialog>;
}
