import { useEffect, useState } from "react";
import { Activity, Check, CircleAlert, Clock3, Cpu, Database, Download, FileVideo2, FolderOpen, HardDrive, Headphones, LoaderCircle, RefreshCw, ShieldCheck, Users } from "lucide-react";
import { getUiLocale, tr } from "../i18n";
import type { AudioAnalysisJob, AudioRisk, ModelDownloadJob, ModelStatus, Project, RuntimeInfo, Segment, SpeakerIdentity, SpeakerJob, SpeakerPackageStatus, SpeakerTrack, SpeechEvidence, SpeechInsights, SpeechPause, UpdateMetadata, UpdatePolicy } from "../types";
import { JobFailureDetails } from "./job-failure";
import { audioRiskLabel, audioUnitLabel, formatBytes, formatTime, modelDescription, modelName, patchReasonLabel, type SegmentSelectionMode } from "../app-view-model";
export function SpeechInsightsPanel({ insights, onLocateEvidence, onLocatePause }: {
    insights: SpeechInsights;
    onLocateEvidence: (evidence: SpeechEvidence) => void;
    onLocatePause: (pause: SpeechPause) => void;
}) {
    return <section className="speech-insights" aria-label={tr("app.s0533")}>
    <header><div><p className="eyebrow">{tr("app.s0534")}</p><h3><Activity size={15}/>{tr("app.s0533")}</h3></div><small>{insights.analyzerVersion}</small></header>
    {insights.status === "insufficient_evidence" ? <p className="speech-empty">{tr("app.s0535")}</p> : <>
      <div className="speech-metrics">
        <span><strong>{insights.tokensPerMinute}</strong><small>{tr("app.s0536")}</small></span>
        <span><strong>{insights.pauseCount}</strong><small>{tr("app.s0537")}</small></span>
        <span><strong>{insights.fillerCount}</strong><small>{tr("app.s0010")}</small></span>
        <span><strong>{insights.lowConfidenceCount}</strong><small>{tr("app.s0538")}</small></span>
      </div>
      {(insights.pauses.length > 0 || insights.evidence.length > 0) && <div className="speech-findings">
        {insights.pauses.slice(0, 2).map((pause) => <button key={`${pause.previousWordId}-${pause.nextWordId}`} onClick={() => onLocatePause(pause)} aria-label={tr("app.s0539", { "0": pause.severity === "long_pause" ? tr("app.s0540") : tr("app.s0537"), "1": formatTime(pause.start), "2": formatTime(pause.end) })}><Clock3 size={12}/><span><strong>{pause.severity === "long_pause" ? tr("app.s0540") : tr("app.s0537")}</strong><small>{pause.duration.toFixed(1)}{tr("app.s0541") + " "}{formatTime(pause.start)}</small></span></button>)}
        {insights.evidence.slice(0, 3).map((evidence, index) => <button key={`${evidence.kind}-${evidence.wordId}-${index}`} onClick={() => onLocateEvidence(evidence)} aria-label={tr("app.s0542", { "0": evidence.kind === "filler" ? tr("app.s0010") : tr("app.s0538"), "1": evidence.text, "2": formatTime(evidence.start) })}><CircleAlert size={12}/><span><strong>{evidence.kind === "filler" ? tr("app.s0010") : tr("app.s0538")} · {evidence.text}</strong><small>{formatTime(evidence.start)}{evidence.confidence == null ? "" : ` · ${Math.round(evidence.confidence * 100)}%`}</small></span></button>)}
      </div>}
      <p className="speech-disclosure">{tr("app.s0543")}</p>
    </>}
  </section>;
}
export function AudioQualityPanel({ job, onStart, onCancel, onResume, onLocate, disabled }: {
    job: AudioAnalysisJob | null;
    onStart: () => void;
    onCancel: () => void;
    onResume: () => void;
    onLocate: (risk: AudioRisk) => void;
    disabled: boolean;
}) {
    const active = job && ["queued", "running"].includes(job.status);
    const resumable = job && ["cancelled", "failed", "interrupted"].includes(job.status);
    return <section className="audio-quality" aria-label={tr("app.s0315")}>
    <header><div><p className="eyebrow">{tr("app.s0544")}</p><h3><Headphones size={15}/>{tr("app.s0315")}</h3></div>{job?.report && <small>{job.report.analyzerVersion}</small>}</header>
    {!job && <><p className="speech-empty">{tr("app.s0545")}</p><button className="audio-analysis-action" disabled={disabled} onClick={onStart}>{tr("app.s0546")}</button></>}
    {active && <div className="audio-analysis-progress"><span><LoaderCircle className="spin" size={13}/>{tr("app.s0547") + " "}{Math.round(job.progress * 100)}%</span><progress max={1} value={job.progress}/><button disabled={disabled || Boolean(job.cancelRequestedAt)} onClick={onCancel}>{job.cancelRequestedAt ? tr("app.s0317") : tr("app.s0511")}</button></div>}
    {resumable && <div className="audio-analysis-error"><JobFailureDetails context="audio" status={job.status} errorCode={job.errorCode} errorMessage={job.errorMessage}/><button disabled={disabled} onClick={onResume}><RefreshCw size={12}/>{tr("app.s0279")}</button></div>}
    {job?.status === "completed" && job.report && <>
      <div className="speech-metrics">
        <span><strong>{job.report.integratedLoudnessLufs ?? "—"}</strong><small>{tr("app.s0550")}</small></span>
        <span><strong>{job.report.truePeakDbfs ?? "—"}</strong><small>{tr("app.s0551")}</small></span>
        <span><strong>{job.report.risks.length}</strong><small>{tr("app.s0552")}</small></span>
        <span><strong>{job.report.silenceDurationSeconds.toFixed(1)}</strong><small>{tr("app.s0553")}</small></span>
      </div>
      {job.report.risks.length > 0 ? <div className="speech-findings">{job.report.risks.slice(0, 3).map((risk, index) => <button key={`${risk.kind}-${risk.start}-${index}`} onClick={() => onLocate(risk)}><CircleAlert size={12}/><span><strong>{audioRiskLabel(risk.kind)}</strong><small>{formatTime(risk.start)} · {risk.measuredValue} {audioUnitLabel(risk.unit)}{tr("app.s0554") + " "}{risk.threshold}</small></span></button>)}</div> : <p className="audio-quality-ok"><Check size={13}/>{tr("app.s0555")}</p>}
      <p className="speech-disclosure" title={job.report.toolVersion}>{tr("app.s0556")}</p>
      <button className="audio-analysis-action quiet" disabled={disabled} onClick={onStart}>{tr("app.s0557")}</button>
    </>}
  </section>;
}
export function SpeakerTrackPanel({ packageStatus, track, job, selectedSegmentId, disabled, onOpenRuntime, onAnalyze, onCancel, onResume, onRename, onMerge, onAssign }: {
    packageStatus: SpeakerPackageStatus | null;
    track: SpeakerTrack | null;
    job: SpeakerJob | null;
    selectedSegmentId: string | null;
    disabled: boolean;
    onOpenRuntime: () => void;
    onAnalyze: () => void;
    onCancel: () => void;
    onResume: () => void;
    onRename: (speakerId: string, name: string) => void;
    onMerge: (fromId: string, intoId: string) => void;
    onAssign: (segmentId: string, speakerId: string) => void;
}) {
    const active = job && ["queued", "running"].includes(job.status);
    const resumable = job && ["cancelled", "failed", "interrupted"].includes(job.status);
    const association = track?.associations.find((item) => item.segmentId === selectedSegmentId);
    return <section className="speaker-track-panel" aria-label={tr("app.s0322")}>
    <header><div><p className="eyebrow">{tr("app.s0558")}</p><h3><Users size={15}/>{tr("app.s0322")}</h3></div>{track?.status === "ready" && <small>{track.speakers.length}{tr("app.s0559") + " "}{track.turns.length}{tr("app.s0560")}</small>}</header>
    {!packageStatus?.installed || packageStatus.verified !== true ? <>
      <p className="speech-empty">{tr("app.s0561")}</p>
      <button className="audio-analysis-action quiet" disabled={disabled} onClick={onOpenRuntime}>{tr("app.s0562")}</button>
    </> : null}
    {packageStatus?.installed && packageStatus.verified === true && !active && !resumable && (!track || track.status === "not_analyzed") && <>
      <p className="speech-empty">{tr("app.s0563")}</p>
      <button className="audio-analysis-action" disabled={disabled} onClick={onAnalyze}>{tr("app.s0546")}</button>
    </>}
    {active && <div className="audio-analysis-progress"><span><LoaderCircle className="spin" size={13}/>{job.stage} · {Math.round(job.progress * 100)}%</span><progress max={1} value={job.progress}/><button disabled={disabled} onClick={onCancel}>{tr("app.s0511")}</button></div>}
    {resumable && <div className="audio-analysis-error"><JobFailureDetails context="speaker" status={job.status} errorCode={job.errorCode} errorMessage={job.errorMessage}/><button disabled={disabled} onClick={onResume}><RefreshCw size={12}/>{tr("app.s0279")}</button></div>}
    {track?.status === "no_speech" && <><p className="speech-empty">{tr("app.s0565")}</p><button className="audio-analysis-action quiet" disabled={disabled} onClick={onAnalyze}>{tr("app.s0557")}</button></>}
    {track?.status === "ready" && <>
      {selectedSegmentId && <label className="speaker-assignment"><span>{tr("app.s0566")}</span><select aria-label={tr("app.s0566")} value={association?.speakerId ?? ""} disabled={disabled} onChange={(event) => event.target.value && onAssign(selectedSegmentId, event.target.value)}><option value="">{tr("app.s0567")}</option>{track.speakers.map((speaker) => <option value={speaker.id} key={speaker.id}>{speaker.label}</option>)}</select>{association?.source === "manual" && <small>{tr("app.s0568")}</small>}</label>}
      <div className="speaker-identities">{track.speakers.map((speaker) => <SpeakerIdentityRow key={speaker.id} speaker={speaker} allSpeakers={track.speakers} disabled={disabled} onRename={onRename} onMerge={onMerge}/>)}</div>
      <p className="speech-disclosure">{track.runtimeVersion}{tr("app.s0569")}</p>
      <button className="audio-analysis-action quiet" disabled={disabled} onClick={onAnalyze}>{tr("app.s0557")}</button>
    </>}
  </section>;
}
export function SpeakerIdentityRow({ speaker, allSpeakers, disabled, onRename, onMerge }: {
    speaker: SpeakerIdentity;
    allSpeakers: SpeakerIdentity[];
    disabled: boolean;
    onRename: (speakerId: string, name: string) => void;
    onMerge: (fromId: string, intoId: string) => void;
}) {
    const [name, setName] = useState(speaker.label);
    const [mergeTarget, setMergeTarget] = useState("");
    useEffect(() => setName(speaker.label), [speaker.label]);
    const save = () => {
        const next = name.trim();
        if (next && next !== speaker.label)
            onRename(speaker.id, next);
        else
            setName(speaker.label);
    };
    return <div className="speaker-identity-row">
    <i className={`speaker-color speaker-${speaker.colorIndex % 6}`}/>
    <input aria-label={tr("app.s0570", { "0": speaker.label })} value={name} maxLength={40} disabled={disabled} onChange={(event) => setName(event.target.value)} onBlur={save} onKeyDown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
        event.preventDefault();
        save();
    } }} title={tr("app.s0571")}/>
    {allSpeakers.length > 1 && <><select aria-label={tr("app.s0572", { "0": speaker.label })} value={mergeTarget} disabled={disabled} onChange={(event) => setMergeTarget(event.target.value)}><option value="">{tr("app.s0573")}</option>{allSpeakers.filter((item) => item.id !== speaker.id).map((item) => <option value={item.id} key={item.id}>{item.label}</option>)}</select><button disabled={disabled || !mergeTarget} onClick={() => mergeTarget && onMerge(speaker.id, mergeTarget)}>{tr("app.s0352")}</button></>}
  </div>;
}
export function PatchReviewCard({ item, onReview, onSelect }: {
    item: Project["patchSets"][number]["items"][number];
    onReview: (action: "apply" | "keep") => void;
    onSelect: () => void;
}) {
    const conflict = item.status === "conflict";
    return <article className={`review-item patch-review ${conflict ? "conflict" : ""}`}>
    <span className={`review-tag ${conflict ? "conflict" : ""}`}>{conflict && <CircleAlert size={12}/>}{conflict ? tr("app.s0574") : tr("app.s0575")}</span>
    <strong>{patchReasonLabel(item.reason)}</strong>
    <div className="patch-diff">
      <p><small>{tr("app.s0576")}</small>{item.beforeText || tr("app.s0577")}</p>
      {conflict && <p className="current"><small>{tr("app.s0578")}</small>{item.currentText || tr("app.s0577")}</p>}
      <p className="proposed"><small>{tr("app.s0579")}</small>{item.target === "cut" && !item.afterText ? tr("app.s0580") : item.afterText}</p>
    </div>
    <p className="patch-meta">{item.confidence == null ? tr("app.s0581") : tr("app.s0582", { "0": Math.round(item.confidence * 100) })}</p>
    <div className="patch-actions"><button onClick={onSelect}>{tr("app.s0303")}</button><span /><button onClick={() => onReview("keep")}>{tr("app.s0583")}</button><button className="apply" onClick={() => onReview("apply")}>{tr("app.s0584")}</button></div>
  </article>;
}
export function RuntimeChecklist({ runtime, modelPath, onChooseModel, compact = false }: {
    runtime: RuntimeInfo | null;
    modelPath: string | null;
    onChooseModel: () => void;
    compact?: boolean;
}) {
    const items = [
        { icon: Database, label: "Core", ok: Boolean(runtime), detail: runtime ? `API ${runtime.coreApiVersion}` : tr("app.s0585") },
        { icon: HardDrive, label: "FFmpeg", ok: runtime?.ffmpegConfigured ?? false, detail: runtime?.ffmpegConfigured ? tr("app.s0586") : tr("app.s0587") },
        { icon: Cpu, label: "whisper.cpp", ok: runtime?.asrConfigured ?? false, detail: runtime?.asrConfigured ? `${runtime.asrBackend.toUpperCase()}${runtime.asrDevice ? ` · ${runtime.asrDevice}` : ""}${runtime.vadConfigured ? " · VAD" : ""}` : tr("app.s0587") },
        { icon: Download, label: tr("app.s0513"), ok: runtime?.ytDlpConfigured ?? false, detail: runtime?.ytDlpConfigured ? "yt-dlp 2026.06.09" : tr("app.s0587") },
    ];
    const modelName = modelPath ? modelPath.split(/[\\/]/).pop() : tr("app.s0468");
    if (compact)
        return <div className="runtime-checklist compact" aria-label={tr("app.s0588")}>
    <div className="runtime-components">
      {items.map(({ icon: Icon, label, ok, detail }) => <div className="runtime-row" key={label}>
        <span className="runtime-component-icon"><Icon size={16}/></span>
        <span><strong>{label}</strong><small>{detail}</small></span>
        <i className={ok ? "ok" : "missing"} aria-label={`${label}: ${ok ? tr("app.s0589") : tr("app.s0590")}`}>{ok ? <Check size={13}/> : <CircleAlert size={13}/>}</i>
      </div>)}
    </div>
    <div className="runtime-model-row">
      <span className="runtime-component-icon"><FileVideo2 size={16}/></span>
      <span><strong>{tr("app.s0591")}</strong><small>{modelName}</small></span>
      <button onClick={onChooseModel}>{modelPath ? tr("app.s0592") : tr("app.s0593")}</button>
    </div>
  </div>;
    return <div className="runtime-checklist">{items.map(({ icon: Icon, label, ok, detail }) => <div className="runtime-row" key={label}><Icon size={16}/><span><strong>{label}</strong><small>{detail}</small></span><i className={ok ? "ok" : "missing"} aria-label={`${label}: ${ok ? tr("app.s0589") : tr("app.s0590")}`}>{ok ? <Check size={13}/> : <CircleAlert size={13}/>}</i></div>)}<div className="runtime-row model"><FileVideo2 size={16}/><span><strong>{tr("app.s0591")}</strong><small title={modelPath ?? ""}>{modelName}</small></span><button onClick={onChooseModel}>{tr("app.s0593")}</button></div></div>;
}
export function UpdatePanel({ policy, update, busy, error, onCheck, onInstall }: {
    policy: UpdatePolicy | null;
    update: UpdateMetadata | null;
    busy: string | null;
    error: string | null;
    onCheck: () => void;
    onInstall: () => void;
}) {
    return <section className="update-panel" aria-label={tr("app.s0594")}>
    <header><span><strong>{tr("app.s0594")}</strong><small>{tr("app.composite.currentVersion", { version: policy?.currentVersion ?? tr("app.s0596") })}</small></span><ShieldCheck size={16}/></header>
    {!policy?.enabled && <p>{policy ? tr("app.update.previewDisabled") : tr("app.s0598")}</p>}
    {update && <div className="update-release">
      <strong>SiaoCut {update.version}</strong>
      <small>{formatBytes(update.sizeBytes)}{update.publishedAt ? ` · ${new Date(update.publishedAt).toLocaleDateString(getUiLocale())}` : ""}</small>
      <p>{update.notes || tr("app.s0599")}</p>
      <button className="button primary full" disabled={Boolean(busy)} onClick={onInstall}>{busy ? <LoaderCircle className="spin" size={14}/> : <Download size={14}/>}{busy ?? tr("app.s0600")}</button>
      <em>{tr("app.s0601")}</em>
    </div>}
    {error && <p className="update-error" role="alert">{error}</p>}
    <button className="button quiet full" disabled={!policy?.enabled || Boolean(busy)} onClick={onCheck}>{busy && !update ? <LoaderCircle className="spin" size={14}/> : <RefreshCw size={14}/>}{busy && !update ? busy : tr("app.s0602")}</button>
  </section>;
}
export function AsrBackendPicker({ runtime, onSelect }: {
    runtime: RuntimeInfo | null;
    onSelect: (backend: "cpu" | "vulkan") => void;
}) {
    if (!runtime || !runtime.availableAsrBackends.includes("vulkan"))
        return null;
    return <section className="backend-picker"><span><strong>{tr("app.s0603")}</strong><small>{tr("app.s0604")}</small></span><div>{(["cpu", "vulkan"] as const).map((backend) => <button key={backend} className={runtime.asrBackend === backend ? "active" : ""} onClick={() => onSelect(backend)} aria-pressed={runtime.asrBackend === backend}>{backend.toUpperCase()}</button>)}</div></section>;
}
export function DiagnosticsPanel({ runtime, onOpen }: {
    runtime: RuntimeInfo | null;
    onOpen: () => void;
}) {
    const available = runtime?.diagnosticsAvailable ?? false;
    return <section className="diagnostics-panel"><span><strong>{tr("app.s0605")}</strong><small title={runtime?.logDirectory ?? undefined}>{available ? tr("app.s0606") : tr("app.s0607")}</small></span><button disabled={!available} onClick={onOpen}><FolderOpen size={14}/>{tr("app.s0608")}</button></section>;
}
export function ModelManager({ models, selectedPath, job, onSelect, onInstall, onCancel, onRemove }: {
    models: ModelStatus[];
    selectedPath: string | null;
    job: ModelDownloadJob | null;
    onSelect: (path: string) => void;
    onInstall: (modelId: string) => void;
    onCancel: () => void;
    onRemove: (modelId: string) => void;
}) {
    const formatSize = (bytes: number) => `${Math.round(bytes / 1024 / 1024)} MB`;
    return <section className="model-manager">
    <div className="model-heading"><span><strong>{tr("app.s0609")}</strong><small>{tr("app.s0610")}</small></span><ShieldCheck size={17}/></div>
    <div className="model-options">{models.map((model) => {
            const currentJob = job?.modelId === model.id ? job : null;
            const downloading = currentJob && ["queued", "running"].includes(currentJob.status);
            const selected = model.installed && model.path === selectedPath;
            return <article className={`model-option ${selected ? "selected" : ""}`} key={model.id}>
        <header><span><strong>{model.recommended ? tr("app.composite.recommendedModel", { name: modelName(model) }) : modelName(model)}</strong><small>{formatSize(model.size)} · {model.license}</small></span>{model.installed && <i className={`verification ${model.verificationStatus}`}>{model.verified === true ? <Check size={12}/> : <ShieldCheck size={12}/>}{{ verified: tr("app.model.verified"), failed: tr("app.model.verificationFailed"), not_checked: tr("app.model.notChecked"), not_installed: "" }[model.verificationStatus]}</i>}{selected && <i><Check size={12}/>{tr("app.s0612")}</i>}</header>
        <p>{modelDescription(model)}</p>
        <small className="model-source" title={model.source}>{tr("app.s0613")}</small>
        {downloading && <div className="model-progress"><span style={{ width: `${Math.max(2, currentJob.progress * 100)}%` }}/><small>{Math.round(currentJob.progress * 100)}% · {formatSize(currentJob.bytesDownloaded)} / {formatSize(currentJob.totalBytes)}</small></div>}
        {currentJob && ["cancelled", "failed", "interrupted"].includes(currentJob.status) && <JobFailureDetails context="model" status={currentJob.status} errorCode={currentJob.errorCode} errorMessage={currentJob.errorMessage}/>}
        <div className="model-actions">
          {downloading ? <button onClick={onCancel}>{tr("app.s0614")}</button> : model.installed ? <><button className="primary" onClick={() => onSelect(model.path)}>{selected ? tr("app.s0615") : tr("app.s0616")}</button><button onClick={() => onRemove(model.id)}>{tr("app.s0617")}</button></> : <button className="primary" onClick={() => onInstall(model.id)}><Download size={13}/>{tr("app.s0618")}</button>}
        </div>
      </article>;
        })}</div>
    <p className="runtime-disclosure">{tr("app.s0619")}</p>
  </section>;
}
export function SpeakerPackageManager({ packageStatus, job, disabled, onInstall, onCancel, onResume }: {
    packageStatus: SpeakerPackageStatus | null;
    job: SpeakerJob | null;
    disabled: boolean;
    onInstall: () => void;
    onCancel: () => void;
    onResume: () => void;
}) {
    const active = job && ["queued", "running"].includes(job.status);
    const resumable = job && ["cancelled", "failed", "interrupted"].includes(job.status);
    return <section className="speaker-package-manager" aria-label={tr("app.s0620")}>
    <div className="model-heading"><span><strong>{tr("app.s0621")}</strong><small>{tr("app.composite.speakerRuntime", { version: packageStatus?.runtimeVersion ?? tr("app.s0622") })}</small></span><Users size={17}/></div>
    <p>{packageStatus ? tr("app.speaker.packageDescription") : tr("app.s0624")}</p>
    {packageStatus && <div className="speaker-package-summary"><span><strong>{formatBytes(packageStatus.downloadSize)}</strong><small>{tr("app.s0625")}</small></span><span><strong>{packageStatus.license}</strong><small>{tr("app.s0626")}</small></span><span className={packageStatus.verified === true ? "verified" : "optional"}>{packageStatus.verified === true ? <Check size={13}/> : <ShieldCheck size={13}/>}{packageStatus.verified === true ? tr("app.s0627") : tr("app.s0628")}</span></div>}
    {packageStatus && <details><summary>{tr("app.s0629") + " "}{packageStatus.assets.length}{tr("app.s0630")}</summary><div className="speaker-asset-list">{packageStatus.assets.map((asset) => <article key={asset.id}><span><strong>{asset.name}</strong><small>{formatBytes(asset.size)} · {asset.license}</small></span><small title={asset.source}>{asset.source.replace(/^https?:\/\//, "")}</small><code title={asset.sha256}>SHA-256 {asset.sha256.slice(0, 12)}…</code></article>)}</div></details>}
    {active && <div className="model-progress"><span style={{ width: `${Math.max(2, job.progress * 100)}%` }}/><small>{job.stage} · {Math.round(job.progress * 100)}% · {formatBytes(job.bytesDownloaded)} / {formatBytes(job.totalBytes)}</small></div>}
    {resumable && <JobFailureDetails className="speaker-package-error" context="speaker" status={job.status} errorCode={job.errorCode} errorMessage={job.errorMessage}/>}
    <div className="model-actions">{active ? <button disabled={disabled} onClick={onCancel}>{tr("app.s0530")}</button> : resumable ? <button disabled={disabled} onClick={onResume}><RefreshCw size={13}/>{tr("app.s0279")}</button> : packageStatus?.installed && packageStatus.verified === true ? <span className="speaker-package-ready"><Check size={13}/>{tr("app.s0631")}</span> : <button className="primary" disabled={disabled || !packageStatus} onClick={onInstall}><Download size={13}/>{tr("app.s0632")}</button>}</div>
    <p className="runtime-disclosure">{tr("app.s0633")}</p>
  </section>;
}
export function SegmentRow({ segment, speaker, speakerManual, selected, active, translation, onSelect, onSave }: {
    segment: Segment;
    speaker?: SpeakerIdentity;
    speakerManual?: boolean;
    selected: boolean;
    active: boolean;
    translation?: Project["translations"][string];
    onSelect: (mode: SegmentSelectionMode) => void;
    onSave: (text: string) => void;
}) {
    const [draft, setDraft] = useState(segment.text);
    useEffect(() => setDraft(segment.text), [segment.text]);
    const translated = translation?.segments.find((item) => item.segmentId === segment.id)?.text;
    return <article className={`segment-row ${selected ? "selected" : ""} ${active ? "active" : ""}`} aria-label={tr("app.s0634", { "0": formatTime(segment.start), "1": formatTime(segment.end) })} onClick={(event) => onSelect(event.shiftKey ? "range" : event.ctrlKey || event.metaKey ? "toggle" : "replace")}>
    <input className="segment-select" type="checkbox" aria-label={tr("app.s0635", { "0": formatTime(segment.start), "1": formatTime(segment.end) })} checked={selected} onClick={(event) => { event.stopPropagation(); onSelect(event.shiftKey ? "range" : "toggle"); }} onChange={() => undefined}/>
    <button className="segment-time" aria-label={tr("app.s0636", { "0": formatTime(segment.start) })}>{formatTime(segment.start)}{speaker && <small><i className={`speaker-color speaker-${speaker.colorIndex % 6}`}/>{speaker.label}{speakerManual ? tr("app.s0637") : ""}</small>}</button>
    <div><textarea value={draft} onChange={(event) => setDraft(event.target.value)} onFocus={() => { if (!active)
        onSelect("replace"); }} onBlur={() => onSave(draft)} onKeyDown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
        event.preventDefault();
        onSave(draft);
    } }} onClick={(event) => event.stopPropagation()} aria-label={tr("app.s0638", { "0": formatTime(segment.start) })} title={tr("app.s0639")}/><p className={translation?.status === "stale" ? "translation stale" : "translation"}>{translated ?? ""}</p></div>
    <span className={segment.confidence != null && segment.confidence < 0.8 ? "confidence low" : "confidence"}>{segment.confidence == null ? "—" : `${Math.round(segment.confidence * 100)}%`}</span>
  </article>;
}
