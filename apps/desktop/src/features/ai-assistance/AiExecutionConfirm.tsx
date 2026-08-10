import { Bot, Cloud, Copy, ShieldCheck, X } from "lucide-react";
import { useEffect, useMemo, useState, type RefObject } from "react";
import { Dialog } from "../../components/ui";
import { useAiServices } from "../environment-settings/use-ai-services";
import type { AiExecutionSelection } from "./types";
import "./ai-assistance.css";

type Props = {
  returnFocusRef: RefObject<HTMLElement | null>;
  codexReady: boolean;
  taskLabel: string;
  segmentCount: number;
  characterCount: number;
  startTime: number;
  endTime: number;
  contextLabel: string | null;
  onClose: () => void;
  onConfirm: (selection: AiExecutionSelection) => void;
};

const formatTime = (seconds: number) => `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`;

export default function AiExecutionConfirm(props: Props) {
  const ai = useAiServices();
  const services = useMemo(() => ai.environment?.aiServices.services.filter((service) => service.credentialState === "stored") ?? [], [ai.environment]);
  const defaultService = services.find((service) => service.isDefault) ?? null;
  const [mode, setMode] = useState<"api" | "codex" | "copy_prompt">("copy_prompt");
  const [serviceId, setServiceId] = useState("");
  const [modelId, setModelId] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  useEffect(() => {
    if (defaultService) {
      setMode("api");
      setServiceId(defaultService.id);
      setModelId(defaultService.modelId ?? "");
    } else if (props.codexReady) setMode("codex");
  }, [defaultService, props.codexReady]);
  const service = services.find((item) => item.id === serviceId) ?? defaultService ?? services[0] ?? null;
  const chooseService = (id: string) => {
    const next = services.find((item) => item.id === id);
    setServiceId(id);
    setModelId(next?.modelId ?? "");
  };
  const chooseApi = () => {
    const next = service ?? services[0];
    setMode("api");
    if (next && !serviceId) {
      setServiceId(next.id);
      setModelId(next.modelId ?? "");
    }
  };
  const submit = () => {
    if (mode === "api" && service && ai.environment) {
      props.onConfirm({ kind: "api", serviceConfigId: service.id, serviceRevision: service.revision, networkRevision: ai.environment.network.revision, modelId: modelId.trim() });
    } else if (mode === "codex") props.onConfirm({ kind: "codex" });
    else props.onConfirm({ kind: "copy_prompt" });
  };
  const canSubmit = confirmed && (mode !== "api" || Boolean(service && modelId.trim()));

  return <Dialog label="确认 AI 辅助" className="runtime-dialog ai-execution-dialog" onClose={props.onClose} returnFocusRef={props.returnFocusRef}>
    <button autoFocus className="dialog-close" aria-label="关闭 AI 辅助确认" title="关闭" onClick={props.onClose}><X size={18}/></button>
    <p className="eyebrow">每次发送前确认</p><h2>{props.taskLabel}</h2>
    <p className="dialog-copy">选择本次执行方式。临时修改模型只用于本次运行，不会改写服务默认模型。</p>
    <div className="ai-execution-options" role="radiogroup" aria-label="执行方式">
      <label className={mode === "api" ? "selected" : ""}><input type="radio" name="ai-mode" checked={mode === "api"} disabled={!services.length} onChange={chooseApi}/><Cloud size={17}/><span><strong>AI 服务</strong><small>{services.length ? "使用已配置的 LLM API" : "尚无保存了 API Key 的服务"}</small></span></label>
      <label className={mode === "codex" ? "selected" : ""}><input type="radio" name="ai-mode" checked={mode === "codex"} disabled={!props.codexReady} onChange={() => setMode("codex")}/><Bot size={17}/><span><strong>本机 Codex</strong><small>{props.codexReady ? "使用本机隔离执行器" : "未安装或未登录"}</small></span></label>
      <label className={mode === "copy_prompt" ? "selected" : ""}><input type="radio" name="ai-mode" checked={mode === "copy_prompt"} onChange={() => setMode("copy_prompt")}/><Copy size={17}/><span><strong>复制提示词</strong><small>手工交给外部 Agent</small></span></label>
    </div>
    {mode === "api" && service && <div className="ai-execution-target"><label><span>服务</span><select value={service.id} onChange={(event) => chooseService(event.target.value)}>{services.map((item) => <option key={item.id} value={item.id}>{item.displayName}{item.isDefault ? "（默认）" : ""}</option>)}</select></label><label><span>本次模型</span><input value={modelId} spellCheck={false} onChange={(event) => setModelId(event.target.value)}/></label>{service.connectionState !== "ready" && <p>此服务尚未通过最近一次连接测试；仍可继续执行。</p>}</div>}
    <dl className="ai-execution-scope"><div><dt>接收方</dt><dd>{mode === "api" ? `${service?.displayName ?? "AI 服务"} / ${modelId || "未选模型"}` : mode === "codex" ? "本机 Codex" : "复制提示词"}</dd></div><div><dt>文本范围</dt><dd>{props.segmentCount} 段 · {props.characterCount.toLocaleString()} 字符 · {formatTime(props.startTime)}—{formatTime(props.endTime)}</dd></div>{props.contextLabel && <div><dt>辅助文本</dt><dd>{props.contextLabel}</dd></div>}<div><dt>费用提示</dt><dd>{mode === "api" ? "可能产生 API 用量；SiaoCut 不估算厂商费用。" : "SiaoCut 不产生第三方 API 用量。"}</dd></div></dl>
    <section className="ai-execution-boundary"><ShieldCheck size={17}/><span><strong>只发送文本任务载荷</strong><small>不包含视频、音频、本机媒体路径、数据库或凭据。结果只进入待审核流程，不直接修改项目。</small></span></section>
    <label className="ai-execution-confirm"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)}/><span>已核对接收方、模型、文本范围和可能的 API 用量，同意执行本次 AI 辅助。</span></label>
    <div className="confirm-actions"><button className="button quiet" onClick={props.onClose}>取消</button><button className="button agent" disabled={!canSubmit || Boolean(ai.busy)} onClick={submit}>确认并执行</button></div>
  </Dialog>;
}
