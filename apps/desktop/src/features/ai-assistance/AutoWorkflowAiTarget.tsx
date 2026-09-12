import { Cloud, Cpu } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
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
  const touched = useRef(false);
  const initialized = useRef(false);
  useEffect(() => {
    if (touched.current || initialized.current || !ai.environment) return;
    initialized.current = true;
    if (preferred) {
      setMode("api"); setServiceId(preferred.id); setModelId(preferred.modelId ?? "");
    } else if (codexReady) setMode("codex");
  }, [ai.environment, codexReady, preferred]);
  const service = services.find((item) => item.id === serviceId) ?? null;
  useEffect(() => {
    if (mode === "manual") onChange({ kind: "copy_prompt" });
    else if (mode === "codex") onChange({ kind: "codex" });
    else if (service && ai.environment && modelId.trim()) onChange({ kind: "api", serviceConfigId: service.id, serviceRevision: service.revision, networkRevision: ai.environment.network.revision, modelId: modelId.trim() });
    else onChange(null);
  }, [ai.environment, mode, modelId, onChange, service]);
  const switchMode = (next: typeof mode) => {
    setMode(next);
    touched.current = true;
    if (next === "api" && service) {
      setServiceId(service.id);
      setModelId(service.modelId ?? "");
    }
  };
  return <section className="auto-ai-target" aria-label="字幕翻译执行方式">
    <header><strong>字幕翻译执行方式</strong><small>保存执行偏好。转写完成后先展示实际字幕，等待单次发送授权。</small></header>
    <div><label><input type="radio" name="auto-ai-mode" checked={mode === "api"} disabled={!services.length} onChange={() => switchMode("api")}/><span>AI 服务</span></label><label><input type="radio" name="auto-ai-mode" checked={mode === "codex"} disabled={!codexReady} onChange={() => switchMode("codex")}/><span>本机 Codex</span></label><label><input type="radio" name="auto-ai-mode" checked={mode === "manual"} onChange={() => switchMode("manual")}/><span>复制提示词</span></label></div>
    {mode === "api" && service && <div className="auto-ai-fields ai-service-fields">
      <label className="ai-service-field">
        <span className="ai-service-field-heading"><span>服务</span><small>选择已保存服务</small></span>
        <span className="ai-service-control">
          <Cloud aria-hidden="true" size={17}/>
          <select aria-label="服务" value={service.id} onChange={(event) => { const next = services.find((item) => item.id === event.target.value); setServiceId(event.target.value); setModelId(next?.modelId ?? ""); touched.current = true; }}>{services.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}</select>
        </span>
      </label>
      <label className="ai-service-field">
        <span className="ai-service-field-heading"><span>本次模型</span><small>仅影响本次运行</small></span>
        <span className="ai-service-control ai-service-model-control">
          <Cpu aria-hidden="true" size={17}/>
          <input aria-label="本次模型" value={modelId} spellCheck={false} onChange={(event) => { setModelId(event.target.value); touched.current = true; }}/>
        </span>
      </label>
    </div>}
    {mode !== "manual" && <p className="auto-ai-confirm">本次仅保存执行偏好，不授权发送。转写完成后必须核对实际文本和接收方；Codex 接收方未核实，可能使用远程模型及订阅额度。</p>}
  </section>;
}
