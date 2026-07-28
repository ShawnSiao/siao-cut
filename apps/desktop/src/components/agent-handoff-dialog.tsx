import { Bot, Copy, ShieldCheck, X } from "lucide-react";
import type { RefObject } from "react";
import { tr } from "../i18n";
import { Dialog } from "./ui";

type AgentHandoffDialogProps = {
  returnFocusRef: RefObject<HTMLElement | null>;
  taskReady: boolean;
  ready: boolean;
  busy: boolean;
  identity: string;
  identityValid: boolean;
  identityLocked: boolean;
  handoffText: string;
  copied: boolean;
  onClose: () => void;
  onReadyChange: (value: boolean) => void;
  onIdentityChange: (value: string) => void;
  onCreate: () => void;
  onCopy: () => void;
};

export default function AgentHandoffDialog({
  returnFocusRef,
  taskReady,
  ready,
  busy,
  identity,
  identityValid,
  identityLocked,
  handoffText,
  copied,
  onClose,
  onReadyChange,
  onIdentityChange,
  onCreate,
  onCopy,
}: AgentHandoffDialogProps) {
  return <Dialog label={tr("app.agent.handoff.title")} className="runtime-dialog agent-handoff-dialog" onClose={onClose} returnFocusRef={returnFocusRef}>
    <button autoFocus className="dialog-close" aria-label={tr("app.agent.handoff.close")} title={tr("app.agent.handoff.close")} onClick={onClose}><X size={18}/></button>
    <p className="eyebrow">{tr("app.agent.handoff.eyebrow")}</p><h2>{taskReady ? tr("app.agent.handoff.readyTitle") : tr("app.agent.handoff.title")}</h2>
    <p className="dialog-copy">{tr("app.agent.handoff.description")}</p>
    <section className="agent-handoff-boundary"><ShieldCheck size={16}/><span>{tr("app.agent.handoff.boundary")}</span></section>
    {!taskReady ? <>
      <label className="agent-handoff-confirm"><input type="checkbox" checked={ready} onChange={(event) => onReadyChange(event.target.checked)}/><span>{tr("app.agent.handoff.confirm")}</span></label>
      <div className="confirm-actions"><button className="button quiet" onClick={onClose}>{tr("app.s0511")}</button><button className="button agent" disabled={!ready || busy} onClick={onCreate}><Bot size={14}/>{tr("app.agent.handoff.create")}</button></div>
    </> : <>
      <label className="agent-identity-field"><span>{tr("app.agent.handoff.identity")}</span><input value={identity} disabled={identityLocked} onChange={(event) => onIdentityChange(event.target.value)} aria-invalid={!identityValid}/><small>{tr("app.agent.handoff.identityHelp")}</small></label>
      {!identityValid && <p className="source-error" role="alert">{tr("app.agent.handoff.identityInvalid")}</p>}
      <label className="agent-handoff-prompt"><span>{tr("app.agent.handoff.promptLabel")}</span><textarea readOnly value={handoffText} aria-label={tr("app.agent.handoff.promptLabel")}/></label>
      <div className="confirm-actions"><button className="button quiet" onClick={onClose}>{tr("app.agent.handoff.later")}</button><button className="button agent" disabled={!handoffText} onClick={onCopy}><Copy size={14}/>{copied ? tr("app.agent.handoff.copied") : tr("app.agent.handoff.copy")}</button></div>
    </>}
  </Dialog>;
}
