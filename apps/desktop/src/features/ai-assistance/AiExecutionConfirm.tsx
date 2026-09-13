import { approvalRecoveryMessage, requiresNewApproval } from "./approval-recovery";
import { Bot, CircleAlert, Cloud, Copy, Cpu, ShieldCheck, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { Dialog } from "../../components/ui";
import { useAiServices } from "../environment-settings/use-ai-services";
import type { AiExecutionSelection } from "./types";
import "./ai-assistance.css";
import type { AiSendSpec } from "../../generated/core-contract";
import { useAiSendPreview } from "./use-ai-send-preview";

type Props = {
  scope: Omit<AiSendSpec, "target">;
  returnFocusRef: RefObject<HTMLElement | null>;
  codexReady: boolean;
  taskLabel: string;
  segmentCount: number;
  characterCount: number;
  startTime: number;
  endTime: number;
  contextLabel: string | null;
  onClose: () => void;
  onConfirm: (selection: AiExecutionSelection, approvalId?: string) => void | Promise<void>;
};

const formatTime = (seconds: number) => `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`;

export default function AiExecutionConfirm(props: Props) {
  const ai = useAiServices();
  const services = useMemo(() => ai.environment?.aiServices.services.filter((service) => service.credentialState === "stored") ?? [], [ai.environment]);
  const defaultService = services.find((service) => service.isDefault) ?? null;
  const [mode, setMode] = useState<"api" | "codex" | "copy_prompt">("copy_prompt");
  const [serviceId, setServiceId] = useState("");
  const [modelId, setModelId] = useState("");
  const [confirmation, setConfirmation] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const inFlight = useRef(false);
  const touched = useRef(false);
  const initialized = useRef(false);
  useEffect(() => {
    if (touched.current || initialized.current || !ai.environment) return;
    initialized.current = true;
    if (defaultService) {
      setMode("api");
      setServiceId(defaultService.id);
      setModelId(defaultService.modelId ?? "");
    } else if (props.codexReady) setMode("codex");
  }, [ai.environment, defaultService, props.codexReady]);
  const service = services.find((item) => item.id === serviceId) ?? null;
  const chooseService = (id: string) => {
    touched.current = true;
    const next = services.find((item) => item.id === id);
    setServiceId(id);
    setModelId(next?.modelId ?? "");
  };
  const chooseApi = () => {
    touched.current = true;
    const next = service ?? services[0];
    setMode("api");
    if (next && !serviceId) {
      setServiceId(next.id);
      setModelId(next.modelId ?? "");
    }
  };
  const selection: AiExecutionSelection = mode === "api" && service && ai.environment
    ? { kind: "api", serviceConfigId: service.id, serviceRevision: service.revision, networkRevision: ai.environment.network.revision, modelId: modelId.trim() }
    : { kind: mode === "codex" ? "codex" : "copy_prompt" };
  const spec: AiSendSpec | null = selection.kind === "copy_prompt" || (mode === "api" && !modelId.trim()) ? null : {
    ...props.scope, target: selection.kind === "codex" ? { kind: "codex" } : {
      kind: "api", service_config_id: selection.serviceConfigId, service_revision: selection.serviceRevision,
      network_revision: selection.networkRevision, model_id: selection.modelId,
    },
  };
  const scopeKey = JSON.stringify(props.scope);
  useEffect(() => { setConfirmation(null); setSendError(null); }, [scopeKey, mode, serviceId, modelId, service?.revision, ai.environment?.network.revision]);
  const preflight = useAiSendPreview(spec);
  const preview = preflight.preview;
  const confirmationKey = mode === "copy_prompt" ? JSON.stringify(props.scope) : preview?.approvalId ?? null;
  const confirmed = confirmationKey !== null && confirmation === confirmationKey;
  const canSubmit = confirmed && (mode === "copy_prompt" || Boolean(preview)) && !sending;
  const submit = async () => {
    if (!canSubmit || inFlight.current) return;
    inFlight.current = true;
    setSending(true); setSendError(null);
    try { await props.onConfirm(selection, preview?.approvalId); }
    catch (cause) {
      if (requiresNewApproval(cause)) {
        setConfirmation(null); preflight.retry(); setSendError(approvalRecoveryMessage);
      } else setSendError(cause instanceof Error ? cause.message : String(cause));
    }
    finally { inFlight.current = false; setSending(false); }
  };

  return <Dialog label="确认 AI 辅助" className="runtime-dialog ai-execution-dialog" onClose={() => { if (!inFlight.current) props.onClose(); }} returnFocusRef={props.returnFocusRef}>
    <button autoFocus className="dialog-close" aria-label="关闭 AI 辅助确认" title="关闭" disabled={sending} onClick={props.onClose}><X size={18}/></button>
    <p className="eyebrow">每次发送前确认</p><h2>{props.taskLabel}</h2>
    <p className="dialog-copy">选择本次执行方式。临时修改模型只用于本次运行，不会改写服务默认模型。</p>
    <div className="ai-execution-options" role="radiogroup" aria-label="执行方式">
      <label className={mode === "api" ? "selected" : ""}><input type="radio" name="ai-mode" checked={mode === "api"} disabled={!services.length} onChange={chooseApi}/><Cloud size={17}/><span><strong>AI 服务</strong><small>{services.length ? "使用已配置的 LLM API" : "尚无保存了 API Key 的服务"}</small></span></label>
      <label className={mode === "codex" ? "selected" : ""}><input type="radio" name="ai-mode" checked={mode === "codex"} disabled={!props.codexReady} onChange={() => { touched.current = true; setMode("codex"); }}/><Bot size={17}/><span><strong>本机 Codex</strong><small>{props.codexReady ? "使用本机隔离执行器" : "未安装或未登录"}</small></span></label>
      <label className={mode === "copy_prompt" ? "selected" : ""}><input type="radio" name="ai-mode" checked={mode === "copy_prompt"} onChange={() => { touched.current = true; setMode("copy_prompt"); }}/><Copy size={17}/><span><strong>复制提示词</strong><small>手工交给外部 Agent</small></span></label>
    </div>
    {mode === "api" && service && <div className="ai-execution-target ai-service-fields">
      <label className="ai-service-field">
        <span className="ai-service-field-heading"><span>服务</span><small>选择已保存服务</small></span>
        <span className="ai-service-control">
          <Cloud aria-hidden="true" size={17}/>
          <select aria-label="服务" value={service.id} onChange={(event) => chooseService(event.target.value)}>{services.map((item) => <option key={item.id} value={item.id}>{item.displayName}{item.isDefault ? "（默认）" : ""}</option>)}</select>
        </span>
      </label>
      <label className="ai-service-field">
        <span className="ai-service-field-heading"><span>本次模型</span><small>仅影响本次运行</small></span>
        <span className="ai-service-control ai-service-model-control">
          <Cpu aria-hidden="true" size={17}/>
          <input aria-label="本次模型" value={modelId} spellCheck={false} onChange={(event) => { touched.current = true; setModelId(event.target.value); }}/>
        </span>
      </label>
      {service.connectionState !== "ready" && <p className="ai-service-note"><CircleAlert aria-hidden="true" size={15}/><span>此服务尚未通过最近一次连接测试；仍可继续执行。</span></p>}
    </div>}
    <dl className="ai-execution-scope"><div><dt>接收方</dt><dd>{preview ? `${preview.receiver} / ${preview.model ?? "模型未核实"}${preview.endpoint ? ` / ${preview.endpoint}` : ""}` : mode === "copy_prompt" ? "由手工交接决定" : "等待 Core 预检"}</dd></div><div><dt>文本范围</dt><dd>{preview?.segmentCount ?? props.segmentCount} 段 · {(preview?.characterCount ?? props.characterCount).toLocaleString()} 字符 · {formatTime(preview?.startTime ?? props.startTime)}—{formatTime(preview?.endTime ?? props.endTime)}</dd></div>{props.contextLabel && <div><dt>辅助文本</dt><dd>{props.contextLabel}</dd></div>}<div><dt>费用提示</dt><dd>{mode === "api" ? "可能产生 API 用量；SiaoCut 不估算厂商费用。" : mode === "codex" ? "可能消耗订阅额度或 API 用量；接收方和模型未核实，可能使用远程模型。" : "由手工选择的接收方决定，可能消耗额度或产生费用。"}</dd></div></dl>
    {preview && <details className="ai-payload-preview"><summary>查看实际发送文本、术语和结构约束</summary><pre tabIndex={0}>{preview.payloadJson}</pre></details>}
    {spec && !preview && !preflight.error && <p role="status">正在生成实际发送预检…</p>}
    {(preflight.error || sendError) && <p role="alert">{preflight.error ?? sendError}<button className="button quiet" onClick={() => { setConfirmation(null); setSendError(null); preflight.retry(); }}>重新预检</button></p>}
    <section className="ai-execution-boundary"><ShieldCheck size={17}/><span><strong>只发送文本任务载荷</strong><small>不包含视频、音频、本机媒体路径、数据库或凭据。结果只进入待审核流程，不直接修改项目。</small></span></section>
    <label className="ai-execution-confirm"><input type="checkbox" checked={confirmed} disabled={!confirmationKey || sending} onChange={(event) => setConfirmation(event.target.checked ? confirmationKey : null)}/><span>已核对接收方、模型、文本范围和可能的 API 用量，同意执行本次 AI 辅助。</span></label>
    <div className="confirm-actions"><button className="button quiet" disabled={sending} onClick={props.onClose}>取消</button><button className="button agent" disabled={!canSubmit || Boolean(ai.busy)} onClick={() => void submit()}>确认并执行</button></div>
  </Dialog>;
}
