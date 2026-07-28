import { CircleAlert, LoaderCircle, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { tr } from "../i18n";
import type { Project } from "../types";
import { Dialog } from "./ui";

const CHECKING_INDICATOR_DELAY_MS = 180;

type Props = {
  project: Project;
  checking: boolean;
  deleting: boolean;
  deletable: boolean;
  blockerMessage: string | null;
  error: string | null;
  onClose: () => void;
  onDelete: () => void;
};

export default function ProjectDeleteDialog({ project, checking, deleting, deletable, blockerMessage, error, onClose, onDelete }: Props) {
  const [showChecking, setShowChecking] = useState(false);

  useEffect(() => {
    if (!checking) {
      setShowChecking(false);
      return;
    }
    const timer = window.setTimeout(() => setShowChecking(true), CHECKING_INDICATOR_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [checking]);

  return <Dialog label={tr("app.s0243")} className="confirm-dialog" onClose={onClose}>
    <div className="confirm-icon"><Trash2 size={20}/></div>
    <p className="eyebrow">{tr("app.s0508")}</p>
    <h2>{tr("app.projectDelete.title", { title: project.title })}</h2>
    <p className="dialog-copy">{tr("app.s0510")}</p>
    {checking && showChecking && <div className="confirm-checking" role="status"><LoaderCircle className="spin" size={15}/>{tr("app.delete.checking")}</div>}
    {(blockerMessage || error) && <div className="confirm-error" role="alert"><CircleAlert size={16}/><span>{blockerMessage ?? error}</span></div>}
    <div className="confirm-actions">
      <button className="button quiet" disabled={deleting || checking} onClick={onClose}>{tr("app.s0511")}</button>
      <button className="button danger" disabled={deleting || checking || !deletable} onClick={onDelete}>{deleting ? <LoaderCircle className="spin" size={14}/> : <Trash2 size={14}/>}{tr("app.s0512")}</button>
    </div>
  </Dialog>;
}
