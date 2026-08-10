import { useEffect, useMemo, useState } from "react";
import type { AiModelInfo, AiProviderCatalogEntry, AiServiceDraft, AiServiceSettings, AiServiceSummary, AiServiceTestResult } from "./ai-service-types";

type Props = {
  service: AiServiceSummary | null;
  providers: AiProviderCatalogEntry[];
  disabled: boolean;
  onSave: (draft: AiServiceDraft) => Promise<AiServiceSettings>;
  onModels: (draft: AiServiceDraft) => Promise<{ models: AiModelInfo[] }>;
  onTest: (draft: AiServiceDraft) => Promise<AiServiceTestResult>;
  onDefault: (id: string) => Promise<unknown>;
  onDeleteCredential: (id: string) => Promise<unknown>;
  onDelete: (id: string) => Promise<unknown>;
  onSaved: (id: string) => void;
  onDeleted: () => void;
};

function makeDraft(service: AiServiceSummary | null, provider: AiProviderCatalogEntry): AiServiceDraft {
  return {
    id: service?.id,
    providerId: service?.providerId ?? provider.id,
    displayName: service?.displayName ?? provider.displayName,
    protocol: service?.protocol ?? provider.protocol,
    baseUrl: service?.baseUrl ?? provider.officialBaseUrl ?? "",
    modelId: service?.modelId ?? "",
    apiKey: "",
  };
}

export function AiServiceEditor(props: Props) {
  const firstProvider = props.providers[0];
  const currentProvider = useMemo(() => props.providers.find((item) => item.id === props.service?.providerId) ?? firstProvider, [firstProvider, props.providers, props.service?.providerId]);
  const [draft, setDraft] = useState(() => makeDraft(props.service, currentProvider));
  const [models, setModels] = useState<AiModelInfo[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => { setDraft(makeDraft(props.service, currentProvider)); setModels([]); setNotice(null); }, [currentProvider, props.service]);

  const provider = props.providers.find((item) => item.id === draft.providerId) ?? firstProvider;
  const update = (value: Partial<AiServiceDraft>) => setDraft((current) => ({ ...current, ...value }));
  const chooseProvider = (id: string) => {
    const next = props.providers.find((item) => item.id === id) ?? firstProvider;
    setDraft((current) => ({ ...current, providerId: next.id, protocol: next.protocol, displayName: next.displayName, baseUrl: next.officialBaseUrl ?? "" }));
    setModels([]);
  };
  const canSubmit = Boolean(draft.displayName.trim() && draft.modelId.trim() && (provider.officialBaseUrl || draft.baseUrl.trim()) && (props.service?.credentialState === "stored" || draft.apiKey.trim()));
  const fetchModels = async () => {
    const result = await props.onModels(draft);
    setModels(result.models);
    setNotice(result.models.length ? `已获取 ${result.models.length} 个模型。` : "服务未提供模型列表，可手工输入模型。" );
  };
  const test = async () => {
    const result = await props.onTest(draft);
    setModels(result.models);
    setNotice(result.mayIncurUsage ? "连接正常；本次最小请求可能产生 API 用量。" : "连接正常。" );
  };
  const save = async () => {
    const settings = await props.onSave({ ...draft, displayName: draft.displayName.trim(), baseUrl: draft.baseUrl.trim(), modelId: draft.modelId.trim(), apiKey: draft.apiKey.trim() });
    const saved = draft.id ? settings.services.find((item) => item.id === draft.id) : settings.services.at(-1);
    if (saved) props.onSaved(saved.id);
    setNotice("配置已保存。API Key 不会在界面中回显。");
    setDraft((current) => ({ ...current, id: saved?.id, apiKey: "" }));
  };

  return <section className="ai-service-editor" aria-label={props.service ? `编辑 ${props.service.displayName}` : "添加 AI 服务"}>
    <header><div><strong>{props.service ? props.service.displayName : "添加 AI 服务"}</strong><small>只发送确认页列出的字幕文本与结构约束</small></div>{props.service?.isDefault && <span className="ai-default-chip">默认服务</span>}</header>
    <div className="ai-service-form">
      <label><span>服务类型</span><select value={draft.providerId} disabled={props.disabled || Boolean(props.service)} onChange={(event) => chooseProvider(event.target.value)}>{props.providers.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}</select></label>
      <label><span>显示名称</span><input value={draft.displayName} disabled={props.disabled} maxLength={80} onChange={(event) => update({ displayName: event.target.value })}/></label>
      <label className="wide"><span>服务地址</span><input value={draft.baseUrl} readOnly={Boolean(provider.officialBaseUrl)} disabled={props.disabled} spellCheck={false} placeholder="https://api.example.com/v1" onChange={(event) => update({ baseUrl: event.target.value })}/><small>{provider.officialBaseUrl ? "内置服务固定使用官方端点。" : "公网地址必须使用 HTTPS；HTTP 仅限本机回环地址。"}</small></label>
      <label><span>API Key</span><input type="password" value={draft.apiKey} disabled={props.disabled} autoComplete="off" placeholder={props.service?.credentialState === "stored" ? "已保存；留空则保留" : "输入 API Key"} onChange={(event) => update({ apiKey: event.target.value })}/></label>
      <label><span>模型</span><input list="ai-service-models" value={draft.modelId} disabled={props.disabled} spellCheck={false} placeholder="输入或获取模型" onChange={(event) => update({ modelId: event.target.value })}/><datalist id="ai-service-models">{models.map((model) => <option key={model.id} value={model.id}>{model.displayName}</option>)}</datalist></label>
    </div>
    {notice && <p className="ai-service-notice" role="status">{notice}</p>}
    {props.service?.connectionState === "untested" && <p className="ai-service-warning">尚未测试。仍可执行任务，但发送前会再次提醒。</p>}
    <div className="ai-service-actions"><button type="button" disabled={props.disabled || !draft.apiKey.trim() && props.service?.credentialState !== "stored"} onClick={() => void fetchModels()}>获取模型</button><button type="button" disabled={props.disabled || !canSubmit} onClick={() => void test()}>测试连接</button><button type="button" className="primary" disabled={props.disabled || !canSubmit} onClick={() => void save()}>保存</button></div>
    {props.service && <div className="ai-service-danger-actions">{!props.service.isDefault && <button type="button" disabled={props.disabled} onClick={() => void props.onDefault(props.service!.id)}>设为默认</button>}{props.service.credentialState === "stored" && <button type="button" disabled={props.disabled} onClick={() => window.confirm("确认删除此服务保存的 API Key？服务配置会保留。") && void props.onDeleteCredential(props.service!.id)}>删除 API Key</button>}<button type="button" className="danger" disabled={props.disabled} onClick={() => { if (window.confirm("确认删除此 AI 服务及其 API Key？此操作不会删除项目或媒体。")) void props.onDelete(props.service!.id).then(props.onDeleted); }}>删除服务</button></div>}
  </section>;
}
