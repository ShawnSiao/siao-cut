import { CheckCircle2, CircleAlert, Download, FolderOpen, HardDrive, LoaderCircle, RefreshCw, ShieldCheck, Trash2, X } from "lucide-react";
import { useState, type RefObject } from "react";
import { formatBytes } from "../app-view-model";
import { getUiLocale, tr } from "../i18n";
import type { LocalCapabilityId, LocalCapabilityStatus, LocalResourceJob, LocalResourcePlan, LocalResourceStatus, LocalTranscriptionProfile } from "../types";
import { localResourceClient } from "../domains/local-resource-client";
import { Dialog } from "./ui";

const resourceUpdateCopy = {
  "zh-CN": {
    check: "检查更新",
    checkAll: "检查全部",
    checking: "正在检查更新",
    current: "已是最新版本",
    failed: "检查失败，现有组件仍可使用。",
    download: "下载并更新",
  },
  "en-US": {
    check: "Check updates",
    checkAll: "Check all",
    checking: "Checking for updates",
    current: "Latest compatible version",
    failed: "The check failed. The current component remains available.",
    download: "Download and update",
  },
} as const;

function updateCopy(key: keyof (typeof resourceUpdateCopy)["zh-CN"]) {
  return resourceUpdateCopy[getUiLocale()][key];
}

function capabilityLabel(id: LocalCapabilityId) {
  return {
    basic_media: tr("app.resources.capability.basic_media"),
    url_import: tr("app.resources.capability.url_import"),
    local_transcription: tr("app.resources.capability.local_transcription"),
    speaker_identity: tr("app.resources.capability.speaker_identity"),
  }[id];
}

function capabilityDescription(id: LocalCapabilityId) {
  return {
    basic_media: tr("app.resources.capability.basic_media.description"),
    url_import: tr("app.resources.capability.url_import.description"),
    local_transcription: tr("app.resources.capability.local_transcription.description"),
    speaker_identity: tr("app.resources.capability.speaker_identity.description"),
  }[id];
}

function stateLabel(capability: LocalCapabilityStatus) {
  if (capability.state === "ready") return tr("app.resources.state.ready");
  if (capability.state === "preparing") return tr("app.resources.state.preparing");
  if (capability.state === "needs_repair") return tr("app.resources.state.needsRepair");
  if (capability.state === "update_available") return tr("app.resources.state.updateAvailable");
  return tr("app.resources.state.notReady");
}

function jobStageLabel(job: LocalResourceJob) {
  if (job.status === "cancelled") return tr("app.resources.job.cancelled");
  if (job.status === "failed" || job.status === "interrupted") return tr("app.resources.job.interrupted");
  if (job.stage === "installing" || job.stage === "verified") return tr("app.resources.job.installing");
  return tr("app.resources.job.downloading");
}

const transcriptionProfiles: LocalTranscriptionProfile[] = ["fast", "standard", "quality"];

type LocalResourceSetupDialogProps = {
  returnFocusRef?: RefObject<HTMLElement | null>;
  reason: "first_run" | "on_demand" | "manage";
  capability: LocalCapabilityId;
  status: LocalResourceStatus | null;
  plan: LocalResourcePlan | null;
  job: LocalResourceJob | null;
  profile: LocalTranscriptionProfile;
  selectedRoot: string;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onChooseLocation: () => void;
  onConfirmLocation: () => void;
  onProfileChange: (profile: LocalTranscriptionProfile) => void;
  onStart: () => void;
  onCancel: () => void;
  onResume: () => void;
  onDefer: () => void;
};

export function LocalResourceSetupDialog(props: LocalResourceSetupDialogProps) {
  const active = props.job && ["queued", "running"].includes(props.job.status);
  const resumable = props.job && ["cancelled", "failed", "interrupted"].includes(props.job.status);
  const pendingLocation = Boolean(props.selectedRoot) && props.selectedRoot !== props.status?.root;
  const canStart = Boolean(props.status?.configured && props.status.rootAvailable && !pendingLocation && !props.job && !props.busy);
  const updating = props.status?.capabilities.some((capability) => capability.id === props.capability && capability.state === "update_available");
  const actionLabel = updating ? updateCopy("download") : props.reason === "on_demand" ? tr("app.resources.prepareAndContinue") : tr("app.resources.prepareRecommended");
  return <Dialog label={tr("app.resources.setupTitle")} className="runtime-dialog resource-setup-dialog" onClose={props.onClose} returnFocusRef={props.returnFocusRef}>
    <button autoFocus className="dialog-close" aria-label={tr("app.resources.close")} onClick={props.onClose}><X size={18}/></button>
    <div className="resource-setup-mark"><HardDrive size={22}/></div>
    <p className="eyebrow">{props.reason === "on_demand" ? tr("app.resources.requiredForAction") : tr("app.resources.firstRun")}</p>
    <h2>{tr("app.resources.setupTitle")}</h2>
    <p className="dialog-copy">{props.reason === "on_demand" ? tr("app.resources.onDemandDescription", { capability: capabilityLabel(props.capability) }) : tr("app.resources.setupDescription")}</p>
    <div className="resource-boundaries">
      <p><ShieldCheck size={15}/><span>{tr("app.resources.installerBoundary")}</span></p>
      <p><FolderOpen size={15}/><span>{tr("app.resources.storageBoundary")}</span></p>
    </div>
    <section className="resource-location" aria-label={tr("app.resources.location")}>
      <span><small>{props.status?.configured ? tr("app.resources.confirmedLocation") : tr("app.resources.location")}</small><strong title={props.selectedRoot || props.status?.root || undefined}>{props.selectedRoot || props.status?.root || tr("app.resources.locationMissing")}</strong></span>
      <button className="button quiet" disabled={props.busy || Boolean(active)} onClick={props.onChooseLocation}><FolderOpen size={14}/>{props.status?.configured ? tr("app.resources.changeLocation") : tr("app.resources.chooseLocation")}</button>
    </section>
    {pendingLocation && <button className="button primary full resource-confirm-location" disabled={props.busy} onClick={props.onConfirmLocation}><CheckCircle2 size={14}/>{tr("app.resources.confirmLocation")}</button>}
    {props.capability === "local_transcription" && <fieldset className="resource-profiles" disabled={props.busy || Boolean(active)}>
      <legend>{tr("app.resources.profile.title")}</legend>
      <p>{tr("app.resources.profile.description")}</p>
      <div>{transcriptionProfiles.map((profile) => <label key={profile} className={props.profile === profile ? "selected" : ""}>
        <input type="radio" name="resource-transcription-profile" value={profile} checked={props.profile === profile} onChange={() => props.onProfileChange(profile)}/>
        <span><strong>{tr(`app.resources.profile.${profile}`)}</strong><small>{tr(`app.resources.profile.${profile}.description`)}</small></span>
        {profile === "standard" && <em>{tr("app.resources.profile.recommended")}</em>}
      </label>)}</div>
    </fieldset>}
    <section className="resource-recommendation" aria-label={tr("app.resources.preparationPlan")}>
      <header><span><small>{tr("app.resources.preparationPlan")}</small><strong>{capabilityLabel(props.capability)}</strong></span><Download size={18}/></header>
      <p>{capabilityDescription(props.capability)}</p>
      <dl><div><dt>{tr("app.resources.downloadSize")}</dt><dd>{props.plan ? (props.plan.unknownSize ? tr("app.resources.sizeEstimate", { size: formatBytes(props.plan.downloadBytes) }) : formatBytes(props.plan.downloadBytes)) : tr("app.resources.calculating")}</dd></div><div><dt>{tr("app.resources.saveTo")}</dt><dd>{props.status?.configured && !pendingLocation ? tr("app.resources.confirmed") : tr("app.resources.confirmRequired")}</dd></div></dl>
    </section>
    {props.job && <section className={`resource-job ${props.job.status}`} aria-label={tr("app.resources.progress")}>
      <header><strong>{jobStageLabel(props.job)}</strong><span>{Math.round(props.job.progress * 100)}%</span></header>
      <progress value={props.job.progress} max={1}/>
      <small>{formatBytes(props.job.bytesDownloaded)} / {formatBytes(props.job.totalBytes)}</small>
      {active && <button className="button quiet" disabled={props.busy || Boolean(props.job.cancelRequestedAt)} onClick={props.onCancel}>{props.job.cancelRequestedAt ? tr("app.resources.cancelling") : tr("app.resources.cancel")}</button>}
      {resumable && <button className="button primary" disabled={props.busy} onClick={props.onResume}><RefreshCw size={14}/>{tr("app.resources.resume")}</button>}
    </section>}
    {props.error && <div className="resource-error" role="alert"><CircleAlert size={15}/><span>{props.error}</span></div>}
    {!props.job && <button className="button primary full" disabled={!canStart} onClick={props.onStart}>{props.busy ? <LoaderCircle className="spin" size={14}/> : <Download size={14}/>} {actionLabel}</button>}
    <button className="button quiet full" disabled={props.busy || Boolean(active)} onClick={props.onDefer}>{props.reason === "first_run" ? tr("app.resources.defer") : tr("app.resources.back")}</button>
  </Dialog>;
}

type LocalResourcePanelProps = {
  status: LocalResourceStatus | null;
  job: LocalResourceJob | null;
  busy: boolean;
  onPrepare: (capability: LocalCapabilityId) => void;
  checkUpdates?: typeof localResourceClient.checkUpdates;
  onStatusChange?: (status: LocalResourceStatus) => void;
  onChangeLocation: () => void;
  onRemove: (capability: LocalCapabilityId) => void;
  onRollback: (capability: LocalCapabilityId) => void;
  onCleanup: () => void;
};

export function LocalResourcePanel({ status, job, busy, checkUpdates = localResourceClient.checkUpdates, onStatusChange, onPrepare, onChangeLocation, onRemove, onRollback, onCleanup }: LocalResourcePanelProps) {
  const [updateCheckTarget, setUpdateCheckTarget] = useState<LocalCapabilityId | "all" | null>(null);
  const [updateCheckedAt, setUpdateCheckedAt] = useState<Partial<Record<LocalCapabilityId, string>>>({});
  const [updateCheckError, setUpdateCheckError] = useState<LocalCapabilityId | null | undefined>(undefined);
  const activeResourceJob = Boolean(job && ["queued", "running"].includes(job.status));
  const updateCheckBusy = updateCheckTarget !== null;
  const handleCheckUpdates = async (capability?: LocalCapabilityId) => {
    if (updateCheckBusy) return;
    setUpdateCheckTarget(capability ?? "all");
    setUpdateCheckError(undefined);
    try {
      const envelope = await checkUpdates(capability);
      if (envelope.localResources) onStatusChange?.(envelope.localResources);
      const checkedAt = envelope.resourceUpdateCheck?.checkedAt ?? new Date().toISOString();
      const checkedCapabilities = envelope.resourceUpdateCheck?.capabilities.map((item) => item.capabilityId)
        ?? (capability ? [capability] : status?.capabilities.map((item) => item.id) ?? []);
      setUpdateCheckedAt((current) => ({ ...current, ...Object.fromEntries(checkedCapabilities.map((item) => [item, checkedAt])) }));
    } catch {
      setUpdateCheckError(capability ?? null);
    } finally {
      setUpdateCheckTarget(null);
    }
  };
  return <section className="local-resource-panel" aria-label={tr("app.resources.title")}>
    <header><span><strong>{tr("app.resources.title")}</strong><small>{tr("app.resources.panelDescription")}</small></span><div className="local-resource-header-actions"><button className="button quiet" disabled={busy || activeResourceJob || updateCheckBusy} onClick={() => void handleCheckUpdates()}>{updateCheckTarget === "all" ? <LoaderCircle className="spin" size={14}/> : <RefreshCw size={14}/>} {updateCopy("checkAll")}</button><HardDrive size={19}/></div></header>
    {updateCheckError === null && <div className="resource-update-check-error" role="alert"><CircleAlert size={14}/><span>{updateCopy("failed")}</span></div>}
    <div className="local-resource-location"><span><small>{tr("app.resources.location")}</small><strong title={status?.root ?? undefined}>{status?.root ?? tr("app.resources.locationMissing")}</strong></span><button className="button quiet" disabled={busy || Boolean(job && ["queued", "running"].includes(job.status))} onClick={onChangeLocation}><FolderOpen size={14}/>{status?.configured ? tr("app.resources.changeLocation") : tr("app.resources.chooseLocation")}</button></div>
    <div className="local-capability-grid">{status?.capabilities.map((capability) => {
      const activeJob = job?.capabilityId === capability.id && ["queued", "running"].includes(job.status);
      const checking = updateCheckTarget === "all" || updateCheckTarget === capability.id;
      const checked = Boolean(updateCheckedAt[capability.id]);
      const checkFailed = updateCheckError === capability.id;
      const statusText = checking
        ? updateCopy("checking")
        : checkFailed
          ? updateCopy("failed")
          : checked && capability.state === "ready"
            ? updateCopy("current")
            : stateLabel(capability);
      return <article key={capability.id} className={capability.state}>
        <header><span><strong>{capabilityLabel(capability.id)}</strong><small>{capabilityDescription(capability.id)}</small></span><i>{checking || activeJob ? <LoaderCircle className="spin" size={16}/> : capability.state === "ready" ? <CheckCircle2 size={16}/> : <CircleAlert size={16}/>}</i></header>
        <footer><span className={checkFailed ? "check-failed" : ""} aria-live="polite">{activeJob ? tr("app.resources.state.preparing") : statusText}</span><div>{capability.state !== "ready" && <button disabled={busy || Boolean(job) || updateCheckBusy} onClick={() => onPrepare(capability.id)}>{capability.state === "needs_repair" ? tr("app.resources.repair") : capability.state === "update_available" ? tr("app.resources.update") : tr("app.resources.prepare")}</button>}{capability.enabled && <button className="quiet" aria-label={`${capabilityLabel(capability.id)}：${updateCopy("check")}`} disabled={busy || Boolean(job) || updateCheckBusy} onClick={() => void handleCheckUpdates(capability.id)}><RefreshCw size={13}/>{updateCopy("check")}</button>}{capability.canRollback && <button className="quiet" disabled={busy || Boolean(job) || updateCheckBusy} onClick={() => onRollback(capability.id)}><RefreshCw size={13}/>{tr("app.resources.rollback")}</button>}{capability.state === "ready" && <button className="danger-link" disabled={busy || Boolean(job) || updateCheckBusy} onClick={() => onRemove(capability.id)}><Trash2 size={13}/>{tr("app.resources.remove")}</button>}</div></footer>
      </article>;
    })}</div>
    {status?.configured && <button className="button quiet full" disabled={busy || Boolean(job && ["queued", "running"].includes(job.status))} onClick={onCleanup}><Trash2 size={14}/>{tr("app.resources.cleanup")}</button>}
  </section>;
}
