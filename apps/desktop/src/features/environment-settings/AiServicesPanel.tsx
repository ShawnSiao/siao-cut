import { useEffect, useRef, useState } from "react";
import type { CodexHealth } from "../../types";
import { AiServiceEditor } from "./AiServiceEditor";
import { AiServiceList } from "./AiServiceList";
import {
  initialServiceSelection,
  localCodexSelectionId,
  providerForSelection,
  serviceForSelection,
} from "./ai-service-selection";
import { LocalCodexDetail } from "./LocalCodexDetail";
import "./ai-services.css";
import { useAiServices } from "./use-ai-services";

type Props = {
  codexHealth: CodexHealth | null;
  onRefreshCodex: () => void;
};

export default function AiServicesPanel({ codexHealth, onRefreshCodex }: Props) {
  const ai = useAiServices();
  const initialized = useRef(false);
  const [selectionId, setSelectionId] = useState(localCodexSelectionId);
  const settings = ai.environment?.aiServices;

  useEffect(() => {
    if (!settings) return;
    if (!initialized.current) {
      initialized.current = true;
      setSelectionId((current) => {
        if (current !== localCodexSelectionId) return current;
        return initialServiceSelection(settings);
      });
      return;
    }
    const valid = selectionId === localCodexSelectionId || selectionId.startsWith("provider:") || settings.services.some((service) => service.id === selectionId);
    if (!valid) setSelectionId(initialServiceSelection(settings));
  }, [selectionId, settings]);

  if (!ai.environment || !settings) {
    return <div className="ai-settings-state" role="status"><strong>{ai.error ? "无法读取 AI 服务" : "正在读取 AI 服务…"}</strong>{ai.error && <p>{ai.error}</p>}<button type="button" onClick={() => void ai.refresh().catch(() => undefined)}>重试</button></div>;
  }

  const disabled = Boolean(ai.busy);
  const service = serviceForSelection(settings, selectionId);
  const provider = providerForSelection(settings, selectionId);
  return <div className="ai-settings-panel">
    <AiServiceList settings={settings} selectionId={selectionId} disabled={disabled} onSelect={setSelectionId}/>
    {selectionId === localCodexSelectionId
      ? <LocalCodexDetail health={codexHealth} busy={disabled} onRefresh={() => { onRefreshCodex(); void ai.refresh().catch(() => undefined); }}/>
      : provider
        ? <AiServiceEditor
            key={selectionId}
            service={service}
            provider={provider}
            network={ai.environment.network}
            disabled={disabled}
            busy={ai.busy}
            error={ai.error}
            onSave={ai.save}
            onModels={ai.listModels}
            onTest={ai.test}
            onDefault={ai.setDefault}
            onSetProxy={ai.setProxy}
            onDeleteCredential={ai.removeCredential}
            onDelete={ai.remove}
            onSaved={setSelectionId}
            onDeleted={() => setSelectionId(localCodexSelectionId)}
          />
        : <section className="environment-provider-detail"><div className="ai-settings-state">无法读取所选服务。</div></section>}
  </div>;
}
