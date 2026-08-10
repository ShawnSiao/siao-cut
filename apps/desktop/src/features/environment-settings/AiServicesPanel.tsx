import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { AiNetworkSettingsPanel } from "./AiNetworkSettings";
import { AiServiceEditor } from "./AiServiceEditor";
import { AiServiceList } from "./AiServiceList";
import "./ai-services.css";
import { useAiServices } from "./use-ai-services";

export default function AiServicesPanel() {
  const ai = useAiServices();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const services = ai.environment?.aiServices.services ?? [];
  useEffect(() => {
    if (creating) return;
    const currentExists = services.some((service) => service.id === selectedId);
    if (!currentExists) setSelectedId(ai.environment?.aiServices.defaultServiceId ?? services[0]?.id ?? null);
  }, [ai.environment?.aiServices.defaultServiceId, creating, selectedId, services]);

  if (!ai.environment) return <div className="ai-settings-state" role="status"><strong>{ai.error ? "无法读取 AI 服务" : "正在读取 AI 服务…"}</strong>{ai.error && <p>{ai.error}</p>}<button type="button" onClick={() => void ai.refresh().catch(() => undefined)}>重试</button></div>;
  const selected = creating ? null : services.find((service) => service.id === selectedId) ?? null;
  const disabled = Boolean(ai.busy);
  return <div className="ai-settings-panel">
    <div className="ai-settings-intro"><div><h3>AI 服务</h3><p>配置自己的 LLM API，即使未安装 Codex 也可使用文本 AI 辅助。API 结果始终进入待审核流程。</p></div><button type="button" className="icon-button" disabled={disabled} aria-label="刷新 AI 服务" title="刷新 AI 服务" onClick={() => void ai.refresh().catch(() => undefined)}><RefreshCw size={16}/></button></div>
    {ai.error && <div className="ai-settings-error" role="alert">{ai.error}</div>}
    {ai.busy && <div className="ai-settings-busy" role="status">{ai.busy}</div>}
    <div className="ai-settings-grid">
      <AiServiceList services={services} selectedId={creating ? null : selectedId} disabled={disabled} onSelect={(id) => { setCreating(false); setSelectedId(id); }} onCreate={() => setCreating(true)}/>
      <AiServiceEditor
        key={selected?.id ?? "new"}
        service={selected}
        providers={ai.environment.aiServices.providerCatalog.providers}
        disabled={disabled}
        onSave={ai.save}
        onModels={ai.listModels}
        onTest={ai.test}
        onDefault={ai.setDefault}
        onDeleteCredential={ai.removeCredential}
        onDelete={ai.remove}
        onSaved={(id) => { setCreating(false); setSelectedId(id); }}
        onDeleted={() => { setCreating(false); setSelectedId(null); }}
      />
    </div>
    <AiNetworkSettingsPanel network={ai.environment.network} disabled={disabled} onSave={ai.setProxy}/>
    <p className="ai-privacy-note">远程请求不包含视频、音频、本机媒体路径、数据库或凭据。切换服务、模型或代理后，旧任务需要重新确认。</p>
  </div>;
}
