import { RefreshCw, X } from "lucide-react";
import type { RefObject } from "react";
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
  modelPath: string | null;
  transcriptionConfig: TranscriptionProviderConfig | null;
  transcriptionHealth: TranscriptionProviderHealth | null;
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
};

export default function RuntimeSettingsDialog(props: RuntimeSettingsDialogProps) {
  return (
    <Dialog label={tr("app.s0245")} className="runtime-dialog runtime-settings-dialog" onClose={props.onClose} returnFocusRef={props.returnFocusRef}>
      <header className="runtime-dialog-header">
        <div><p className="eyebrow">{tr("app.s0505")}</p><h2>{tr("app.s0245")}</h2></div>
        <button autoFocus className="dialog-close" aria-label={tr("app.s0503")} title={tr("app.s0504")} onClick={props.onClose}><X size={18}/></button>
      </header>
      <div className="runtime-dialog-content">
        <p className="dialog-copy">{tr("app.s0506")}</p>
        <RuntimeChecklist runtime={props.runtime} modelPath={props.modelPath} onChooseModel={props.onChooseModel}/>
        <TranscriptionProviderSettings config={props.transcriptionConfig} health={props.transcriptionHealth} busy={props.busy} onSave={props.onSaveTranscriptionProvider} onCheck={props.onCheckTranscriptionProvider}/>
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
