import { Cloud, Cpu } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useAiServices } from "../environment-settings/use-ai-services";
import type { AiExecutionSelection } from "./types";
import "./ai-assistance.css";

type Props = { codexReady: boolean; onChange: (selection: AiExecutionSelection | null) => void };

export default function AutoWorkflowAiTarget({ codexReady, onChange }: Props) {
  const ai = useAiServices();
  const services = useMemo(() => ai.environment?.aiServices.services.filter((service) => service.credentialState === "stored") ?? [], [ai.environment]);
  const preferred = services.find((service) => service.isDefault) ?? null;
  const [mode, setMode] = useState<"api" | "codex" | "manual">("manual");
  const [serviceId, setServiceId] = useState("");
  const [modelId, setModelId] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  useEffect(() => {
    if (preferred) {
      setMode("api"); setServiceId(preferred.id); setModelId(preferred.modelId ?? "");
    } else if (codexReady) setMode("codex");
  }, [codexReady, preferred]);
  const service = services.find((item) => item.id === serviceId) ?? preferred ?? services[0] ?? null;
  useEffect(() => {
    if (mode === "manual") onChange({ kind: "copy_prompt" });
    else if (!confirmed) onChange(null);
    else if (mode === "codex") onChange({ kind: "codex" });
    else if (service && ai.environment && modelId.trim()) onChange({ kind: "api", serviceConfigId: service.id, serviceRevision: service.revision, networkRevision: ai.environment.network.revision, modelId: modelId.trim() });
    else onChange(null);
  }, [ai.environment, confirmed, mode, modelId, onChange, service]);
  const switchMode = (next: typeof mode) => {
    setMode(next);
    setConfirmed(false);
    if (next === "api" && service) {
      setServiceId(service.id);
      setModelId(service.modelId ?? "");
    }
  };
  return <section className="auto-ai-target" aria-label="字幕翻译执行方式">
    <header><strong>字幕翻译执行方式</strong><small>在转写完成后按此选择处理；API 失败或配置变化时停在「需要 Agent」。</small></header>
    <div><label><input type="radio" name="auto-ai-mode" checked={mode === "api"} disabled={!services.length} onChange={() => switchMode("api")}/><span>AI 服务</span></label><label><input type="radio" name="auto-ai-mode" checked={mode === "codex"} disabled={!codexReady} onChange={() => switchMode("codex")}/><span>本机 Codex</span></label><label><input type="radio" name="auto-ai-mode" checked={mode === "manual"} onChange={() => switchMode("manual")}/><span>复制提示词</span></label></div>
    {mode === "api" && service && <div className="auto-ai-fields ai-service-fields">
      <label className="ai-service-field">
        <span className="ai-service-field-heading"><span>服务</span><small>选择已保存服务</small></span>
        <span className="ai-service-control">
          <Cloud aria-hidden="true" size={17}/>
          <select aria-label="服务" value={service.id} onChange={(event) => { const next = services.find((item) => item.id === event.target.value); setServiceId(event.target.value); setModelId(next?.modelId ?? ""); setConfirmed(false); }}>{services.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}</select>
        </span>
      </label>
      <label className="ai-service-field">
        <span className="ai-service-field-heading"><span>本次模型</span><small>仅影响本次运行</small></span>
        <span className="ai-service-control ai-service-model-control">
          <Cpu aria-hidden="true" size={17}/>
          <input aria-label="本次模型" value={modelId} spellCheck={false} onChange={(event) => { setModelId(event.target.value); setConfirmed(false); }}/>
        </span>
      </label>
    </div>}
    {mode !== "manual" && <label className="auto-ai-confirm"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)}/><span>确认在翻译阶段向所选执行方发送转写后的字幕文本、时间戳、目标语言和术语表。不会发送媒体、路径、数据库或凭据；结果停在人工审核。</span></label>}
  </section>;
}
