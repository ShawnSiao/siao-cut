import { RefreshCw, X } from "lucide-react";
import { useState, type RefObject } from "react";
import type { ComponentStoreComponent } from "../core";
import { tr } from "../i18n";
import type {
  ModelDownloadJob,
  ModelStatus,
  RuntimeInfo,
  SpeakerJob,
  SpeakerPackageStatus,
  TranscriptionProviderConfig,
  TranscriptionProviderHealth,
  UpdateMetadata,
  UpdatePolicy,
} from "../types";
import { Dialog } from "./ui";
import {
  AsrBackendPicker,
  DiagnosticsPanel,
  ModelManager,
  RuntimeChecklist,
  SpeakerPackageManager,
  TranscriptionProviderSettings,
  UpdatePanel,
} from "./workbench-panels";

type RuntimeSettingsDialogProps = {
  returnFocusRef: RefObject<HTMLButtonElement | null>;
  runtime: RuntimeInfo | null;
  componentOperations: Record<string, unknown>[];
  modelPath: string | null;
  modelAvailable: boolean;
  transcriptionConfig: TranscriptionProviderConfig | null;
  transcriptionHealth: TranscriptionProviderHealth | null;
  transcriptionMode: "quick" | "multispeaker";
  transcriptionLanguage: "auto" | "en" | "zh";
  busy: boolean;
  models: ModelStatus[];
  modelJob: ModelDownloadJob | null;
  speakerPackage: SpeakerPackageStatus | null;
  speakerJob: SpeakerJob | null;
  updatePolicy: UpdatePolicy | null;
  availableUpdate: UpdateMetadata | null;
  updateBusy: string | null;
  updateError: string | null;
  onClose: () => void;
  onChooseModel: () => void;
  onSaveTranscriptionProvider: (endpoint: string, modelId: string) => void;
  onCheckTranscriptionProvider: () => void;
  onSelectTranscriptionMode: (mode: "quick" | "multispeaker") => void;
  onSelectTranscriptionLanguage: (language: "auto" | "en" | "zh") => void;
  onSelectAsrBackend: (backend: "cpu" | "vulkan") => void;
  onSelectModel: (path: string) => void;
  onInstallModel: (modelId: string) => void;
  onCancelModel: () => void;
  onRemoveModel: (modelId: string) => void;
  onInstallComponent: (component: ComponentStoreComponent) => void;
  onVerifyComponent: (component: ComponentStoreComponent) => void;
  onRegisterExternalComponent: (component: ComponentStoreComponent, path: string) => void;
  onPauseComponentOperation: (operationId: string) => void;
  onResumeComponentOperation: (operationId: string) => void;
  onCancelComponentOperation: (operationId: string) => void;
  onMigrateComponentRoot: (targetRoot: string) => void;
  onInstallSpeakerPackage: () => void;
  onCancelSpeakerJob: () => void;
  onResumeSpeakerJob: () => void;
  onOpenDiagnostics: () => void;
  onCheckUpdates: () => void;
  onInstallUpdate: () => void;
  onRefresh: () => void;
};

const STORE_COMPONENTS: Array<{ id: ComponentStoreComponent; label: string }> = [
  { id: "ffmpeg", label: "FFmpeg" },
  { id: "yt-dlp", label: "yt-dlp" },
  { id: "whisper-cpu", label: "Whisper CPU" },
  { id: "whisper-vulkan", label: "Whisper Vulkan" },
  { id: "vad", label: "VAD" },
  { id: "tiny", label: "Whisper tiny" },
  { id: "base", label: "Whisper base" },
  { id: "small", label: "Whisper small" },
];

function ComponentStorePanel(props: Pick<RuntimeSettingsDialogProps, "runtime" | "componentOperations" | "onInstallComponent" | "onVerifyComponent" | "onRegisterExternalComponent" | "onPauseComponentOperation" | "onResumeComponentOperation" | "onCancelComponentOperation" | "onMigrateComponentRoot" | "onRefresh">) {
  const [externalComponent, setExternalComponent] = useState<ComponentStoreComponent>("ffmpeg");
  const [externalPath, setExternalPath] = useState("");
  const [targetRoot, setTargetRoot] = useState("");
  const store = props.runtime?.componentStore as { status?: string; canonicalRevision?: string; schemaVersion?: number; catalogId?: string; installations?: Array<Record<string, unknown>> } | null | undefined;
  const installations = store?.installations ?? [];
  const isInstalled = (component: ComponentStoreComponent) => {
    const model = ["tiny", "base", "small"].includes(component) ? component : null;
    const id = model ? "whisper-model" : component === "whisper-cpu" || component === "whisper-vulkan" ? "whisper-runtime" : component === "yt-dlp" ? "yt-dlp" : component === "vad" ? "whisper-vad" : "ffmpeg";
    const backend = component === "whisper-cpu" ? "cpu" : component === "whisper-vulkan" ? "vulkan" : null;
    return installations.some((entry) => entry.componentId === id && entry.verificationStatus === "verified"
      && (!model || (entry.variant as { model?: unknown } | undefined)?.model === model)
      && (!backend || (entry.variant as { backend?: unknown } | undefined)?.backend === backend));
  };
  return <section className="component-store-panel" aria-label="共享组件 Store">
    <div className="model-heading"><span><strong>共享组件 Store</strong><small>canonical {store?.canonicalRevision ?? "unknown"} · catalog {store?.catalogId ?? "unknown"}</small></span><button className="button quiet" aria-label="刷新共享 Store 状态" title="刷新共享 Store 状态" onClick={props.onRefresh}><RefreshCw size={17}/></button></div>
    <p className="runtime-disclosure">正式执行只使用 common v2 的已验证组件；产品不会读取 Store manifest、journal、lease 或 packages。</p>
    <div className="component-store-status"><strong>{store?.status ?? "unavailable"}</strong><small>schema v{store?.schemaVersion ?? "?"}</small></div>
    <div className="component-store-options">{STORE_COMPONENTS.map((component) => <article key={component.id} className="component-store-option"><span><strong>{component.label}</strong><small>{isInstalled(component.id) ? "verified" : "未安装或未校验"}</small></span><div><button className="button quiet" onClick={() => props.onVerifyComponent(component.id)}>校验</button><button className="button primary" disabled={isInstalled(component.id)} onClick={() => props.onInstallComponent(component.id)}>安装</button></div></article>)}</div>
    {props.componentOperations.length > 0 && <div className="component-store-operations"><strong>操作</strong>{props.componentOperations.slice(0, 8).map((operation) => {
      const id = String(operation.operationId ?? "");
      const state = String(operation.state ?? operation.status ?? "unknown");
      const readOnly = ["foreign_or_incompatible", "foreign_or_incompatible_operation"].includes(state);
      return <article key={id}><span><code>{id}</code><small>{readOnly ? "只读：当前 catalog 不兼容" : state}</small></span><div><button disabled={readOnly} onClick={() => props.onPauseComponentOperation(id)}>暂停</button><button disabled={readOnly} onClick={() => props.onResumeComponentOperation(id)}>继续</button><button disabled={readOnly} onClick={() => props.onCancelComponentOperation(id)}>取消</button></div></article>;
    })}</div>}
    <div className="component-store-external"><strong>登记 external</strong><div><select value={externalComponent} onChange={(event) => setExternalComponent(event.target.value as ComponentStoreComponent)}>{STORE_COMPONENTS.map((component) => <option key={component.id} value={component.id}>{component.label}</option>)}</select><input value={externalPath} placeholder="已校验的文件或目录路径" onChange={(event) => setExternalPath(event.target.value)}/><button className="button quiet" disabled={!externalPath.trim()} onClick={() => props.onRegisterExternalComponent(externalComponent, externalPath.trim())}>登记</button></div></div>
    <div className="component-store-migration"><strong>迁移 shared root</strong><div><input value={targetRoot} placeholder="新的绝对路径" onChange={(event) => setTargetRoot(event.target.value)}/><button className="button quiet" disabled={!targetRoot.trim()} onClick={() => props.onMigrateComponentRoot(targetRoot.trim())}>开始迁移</button></div></div>
  </section>;
}

export default function RuntimeSettingsDialog(props: RuntimeSettingsDialogProps) {
  return (
    <Dialog label={tr("app.s0245")} className="runtime-dialog runtime-settings-dialog" onClose={props.onClose} returnFocusRef={props.returnFocusRef}>
      <header className="runtime-dialog-header">
        <div><p className="eyebrow">{tr("app.s0505")}</p><h2>{tr("app.s0245")}</h2></div>
        <button autoFocus className="dialog-close" aria-label={tr("app.s0503")} title={tr("app.s0504")} onClick={props.onClose}><X size={18}/></button>
      </header>
      <div className="runtime-dialog-content">
        <p className="dialog-copy">{tr("app.s0506")}</p>
        <ComponentStorePanel runtime={props.runtime} componentOperations={props.componentOperations} onInstallComponent={props.onInstallComponent} onVerifyComponent={props.onVerifyComponent} onRegisterExternalComponent={props.onRegisterExternalComponent} onPauseComponentOperation={props.onPauseComponentOperation} onResumeComponentOperation={props.onResumeComponentOperation} onCancelComponentOperation={props.onCancelComponentOperation} onMigrateComponentRoot={props.onMigrateComponentRoot} onRefresh={props.onRefresh}/>
        <RuntimeChecklist runtime={props.runtime} modelPath={props.modelPath} modelAvailable={props.modelAvailable} onChooseModel={props.onChooseModel}/>
        <section className="creator-advanced-transcription" aria-label={tr("app.creator.advancedTranscription")}><header><strong>{tr("app.creator.advancedTranscription")}</strong><small>{tr("app.creator.advancedTranscriptionHelp")}</small></header><div><label><span>{tr("app.moss.mode.label")}</span><select aria-label={tr("app.moss.mode.label")} value={props.transcriptionMode} onChange={(event) => props.onSelectTranscriptionMode(event.target.value as "quick" | "multispeaker")}><option value="quick">{tr("app.moss.mode.quick")}</option><option value="multispeaker">{tr("app.moss.mode.multispeaker")}</option></select></label><label><span>{tr("app.transcription.language")}</span><select aria-label={tr("app.transcription.language")} value={props.transcriptionLanguage} onChange={(event) => props.onSelectTranscriptionLanguage(event.target.value as "auto" | "en" | "zh")}><option value="auto">{tr("app.transcription.auto")}</option><option value="en">{tr("app.transcription.english")}</option><option value="zh">{tr("app.transcription.chinese")}</option></select></label></div></section>
        {props.transcriptionMode === "multispeaker" && <TranscriptionProviderSettings config={props.transcriptionConfig} health={props.transcriptionHealth} busy={props.busy} onSave={props.onSaveTranscriptionProvider} onCheck={props.onCheckTranscriptionProvider}/>}
        <AsrBackendPicker runtime={props.runtime} onSelect={props.onSelectAsrBackend}/>
        <ModelManager models={props.models} selectedPath={props.modelPath} job={props.modelJob} onSelect={props.onSelectModel} onInstall={props.onInstallModel} onCancel={props.onCancelModel} onRemove={props.onRemoveModel}/>
        <SpeakerPackageManager packageStatus={props.speakerPackage} job={props.speakerJob} disabled={props.busy} onInstall={props.onInstallSpeakerPackage} onCancel={props.onCancelSpeakerJob} onResume={props.onResumeSpeakerJob}/>
        <DiagnosticsPanel runtime={props.runtime} onOpen={props.onOpenDiagnostics}/>
        <UpdatePanel policy={props.updatePolicy} update={props.availableUpdate} busy={props.updateBusy} error={props.updateError} onCheck={props.onCheckUpdates} onInstall={props.onInstallUpdate}/>
        <button className="button quiet full" onClick={props.onRefresh}><RefreshCw size={14}/>{tr("app.s0507")}</button>
      </div>
    </Dialog>
  );
}
