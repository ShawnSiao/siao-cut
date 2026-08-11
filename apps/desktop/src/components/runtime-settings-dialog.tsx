import { RefreshCw, X } from "lucide-react";
import { useState, type RefObject } from "react";
import { tr } from "../i18n";
import type {
  CodexHealth,
  ModelDownloadJob,
  ModelStatus,
  LocalCapabilityId,
  LocalResourceJob,
  LocalResourceStatus,
  RuntimeInfo,
  SpeakerJob,
  SpeakerPackageStatus,
  TranscriptionProviderConfig,
  TranscriptionProviderHealth,
  UpdateMetadata,
  UpdatePolicy,
} from "../types";
import { Dialog } from "./ui";
import { LocalResourcePanel } from "./local-resource-ui";
import AiServicesPanel from "../features/environment-settings/AiServicesPanel";
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
  codexHealth: CodexHealth | null;
  localResources: LocalResourceStatus | null;
  resourceJob: LocalResourceJob | null;
  resourceBusy: boolean;
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
  onInstallSpeakerPackage: () => void;
  onCancelSpeakerJob: () => void;
  onResumeSpeakerJob: () => void;
  onOpenDiagnostics: () => void;
  onCheckUpdates: () => void;
  onInstallUpdate: () => void;
  onRefresh: () => void;
  onPrepareResource: (capability: LocalCapabilityId) => void;
  onChangeResourceLocation: () => void;
  onRemoveResource: (capability: LocalCapabilityId) => void;
  onRollbackResource: (capability: LocalCapabilityId) => void;
  onCleanupResources: () => void;
};

export default function RuntimeSettingsDialog(props: RuntimeSettingsDialogProps) {
  const [tab, setTab] = useState<"local" | "ai">("local");
  return (
    <Dialog label={tr("app.environment.title")} className="runtime-dialog runtime-settings-dialog" onClose={props.onClose} returnFocusRef={props.returnFocusRef}>
      <header className="environment-settings-header">
        <div className="environment-title-group"><span>{tr("app.resources.management")}</span><h2>{tr("app.environment.title")}</h2><p>{tr("app.environment.description")}</p></div>
        <nav className="environment-settings-tabs" role="tablist" aria-label={tr("app.environment.title")}><button className={tab === "local" ? "active" : ""} type="button" role="tab" aria-selected={tab === "local"} onClick={() => setTab("local")}>{tr("app.environment.localTab")}</button><button className={tab === "ai" ? "active" : ""} type="button" role="tab" aria-selected={tab === "ai"} onClick={() => setTab("ai")}>{tr("app.environment.aiTab")}</button></nav>
        <button autoFocus className="environment-settings-close" aria-label={tr("app.environment.close")} title={tr("app.environment.close")} onClick={props.onClose}><X size={17}/></button>
      </header>
      {tab === "local" ? <div className="runtime-dialog-content environment-local-content" role="tabpanel" aria-label="本地功能">
        <p className="dialog-copy">{tr("app.resources.panelDescription")}</p>
        <LocalResourcePanel status={props.localResources} job={props.resourceJob} busy={props.resourceBusy} onPrepare={props.onPrepareResource} onChangeLocation={props.onChangeResourceLocation} onRemove={props.onRemoveResource} onRollback={props.onRollbackResource} onCleanup={props.onCleanupResources}/>
        <details className="resource-diagnostics"><summary><span><strong>{tr("app.resources.diagnostics")}</strong><small>{tr("app.resources.diagnosticsDescription")}</small></span></summary><div>
          <RuntimeChecklist runtime={props.runtime} modelPath={props.modelPath} modelAvailable={props.modelAvailable} onChooseModel={props.onChooseModel}/>
          <section className="creator-advanced-transcription" aria-label={tr("app.creator.advancedTranscription")}><header><strong>{tr("app.creator.advancedTranscription")}</strong><small>{tr("app.creator.advancedTranscriptionHelp")}</small></header><div><label><span>{tr("app.moss.mode.label")}</span><select aria-label={tr("app.moss.mode.label")} value={props.transcriptionMode} onChange={(event) => props.onSelectTranscriptionMode(event.target.value as "quick" | "multispeaker")}><option value="quick">{tr("app.moss.mode.quick")}</option><option value="multispeaker">{tr("app.moss.mode.multispeaker")}</option></select></label><label><span>{tr("app.transcription.language")}</span><select aria-label={tr("app.transcription.language")} value={props.transcriptionLanguage} onChange={(event) => props.onSelectTranscriptionLanguage(event.target.value as "auto" | "en" | "zh")}><option value="auto">{tr("app.transcription.auto")}</option><option value="en">{tr("app.transcription.english")}</option><option value="zh">{tr("app.transcription.chinese")}</option></select></label></div></section>
          {props.transcriptionMode === "multispeaker" && <TranscriptionProviderSettings config={props.transcriptionConfig} health={props.transcriptionHealth} busy={props.busy} onSave={props.onSaveTranscriptionProvider} onCheck={props.onCheckTranscriptionProvider}/>}
          <AsrBackendPicker runtime={props.runtime} onSelect={props.onSelectAsrBackend}/>
          <ModelManager models={props.models} selectedPath={props.modelPath} job={props.modelJob} onSelect={props.onSelectModel} onInstall={props.onInstallModel} onCancel={props.onCancelModel} onRemove={props.onRemoveModel}/>
          <SpeakerPackageManager packageStatus={props.speakerPackage} job={props.speakerJob} disabled={props.busy} onInstall={props.onInstallSpeakerPackage} onCancel={props.onCancelSpeakerJob} onResume={props.onResumeSpeakerJob}/>
          <DiagnosticsPanel runtime={props.runtime} onOpen={props.onOpenDiagnostics}/>
        </div></details>
        <UpdatePanel policy={props.updatePolicy} update={props.availableUpdate} busy={props.updateBusy} error={props.updateError} onCheck={props.onCheckUpdates} onInstall={props.onInstallUpdate}/>
        <button className="button quiet full" onClick={props.onRefresh}><RefreshCw size={14}/>{tr("app.s0507")}</button>
      </div> : <div className="runtime-dialog-content environment-ai-content" role="tabpanel" aria-label="AI 服务"><AiServicesPanel codexHealth={props.codexHealth} onRefreshCodex={props.onRefresh}/></div>}
    </Dialog>
  );
}
