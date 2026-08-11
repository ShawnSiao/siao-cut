import { useState } from "react";
import { AiNetworkSettingsPanel } from "./AiNetworkSettings";
import { draftForSelection } from "./ai-service-selection";
import type {
  AiModelInfo,
  AiNetworkSettings,
  AiProviderCatalogEntry,
  AiServiceDraft,
  AiServiceSettings,
  AiServiceSummary,
  AiServiceTestResult,
} from "./ai-service-types";

type Props = {
  service: AiServiceSummary | null;
  provider: AiProviderCatalogEntry;
  network: AiNetworkSettings;
  disabled: boolean;
  busy: string | null;
  error: string | null;
  onSave: (draft: AiServiceDraft) => Promise<AiServiceSettings>;
  onModels: (draft: AiServiceDraft) => Promise<{ models: AiModelInfo[] }>;
  onTest: (draft: AiServiceDraft) => Promise<AiServiceTestResult>;
  onDefault: (id: string | null) => Promise<unknown>;
  onSetProxy: (url: string | null) => Promise<unknown>;
  onDeleteCredential: (id: string) => Promise<unknown>;
  onDelete: (id: string) => Promise<unknown>;
  onSaved: (id: string) => void;
  onDeleted: () => void;
};

function Toggle({ checked, label, disabled, onChange }: { checked: boolean; label: string; disabled: boolean; onChange: (checked: boolean) => void }) {
  return <button className={`environment-toggle ${checked ? "on" : ""}`} type="button" role="switch" aria-checked={checked} aria-label={label} disabled={disabled} onClick={() => onChange(!checked)}/>;
}

export function AiServiceEditor(props: Props) {
  const [draft, setDraft] = useState(() => draftForSelection(props.service, props.provider));
  const [models, setModels] = useState<AiModelInfo[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [editingKey, setEditingKey] = useState(props.service?.credentialState !== "stored");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [makeDefault, setMakeDefault] = useState(Boolean(props.service?.isDefault));
  const configured = props.service?.credentialState === "stored";
  const custom = props.provider.id === "custom";
  const credentialReady = Boolean(configured || draft.apiKey.trim());
  const canSave = Boolean(draft.displayName.trim() && draft.modelId.trim() && draft.baseUrl.trim() && credentialReady);
  const update = (patch: Partial<AiServiceDraft>) => { setDraft((current) => ({ ...current, ...patch })); setNotice(null); };

  const fetchModels = async () => {
    const result = await props.onModels(draft);
    setModels(result.models);
    if (!draft.modelId && result.models[0]) update({ modelId: result.models[0].id });
    setNotice(result.models.length ? `已获取 ${result.models.length} 个模型，也可以手动填写。` : "服务未返回模型列表，可以手动填写模型名称。");
  };
  const test = async () => {
    const result = await props.onTest(draft);
    setModels(result.models);
    setNotice(result.mayIncurUsage ? "连接成功；本次最小测试可能产生少量 API 用量。" : "连接成功，当前模型可用。");
  };
  const save = async () => {
    const settings = await props.onSave({ ...draft, displayName: draft.displayName.trim(), baseUrl: draft.baseUrl.trim(), modelId: draft.modelId.trim(), apiKey: draft.apiKey.trim() });
    const saved = draft.id
      ? settings.services.find((item) => item.id === draft.id)
      : settings.services.filter((item) => item.providerId === draft.providerId && item.displayName === draft.displayName.trim()).at(-1);
    if (!saved) return;
    if (makeDefault !== saved.isDefault) await props.onDefault(makeDefault ? saved.id : null);
    setDraft((current) => ({ ...current, id: saved.id, apiKey: "" }));
    setEditingKey(false);
    setNotice("配置已保存。API Key 不会在界面、项目文件或日志中回显。");
    props.onSaved(saved.id);
  };
  const remove = () => {
    if (props.service && window.confirm(`删除「${props.service.displayName}」？保存的 API Key 也会同时删除。`)) {
      void props.onDelete(props.service.id).then(props.onDeleted);
    }
  };

  return <>
    <section className="environment-provider-detail" aria-label={`${draft.displayName} 配置`}>
      <div className="environment-detail-scroll">
        <div className="environment-detail-heading">
          <div><h2>{configured ? draft.displayName : custom ? "添加其他兼容服务" : `配置 ${draft.displayName}`}</h2><p>用于字幕润色、校对、翻译和说话人命名</p></div>
          {configured && <span className="environment-status-chip">已保存</span>}
        </div>
        <div className="environment-capabilities"><span>字幕文本</span><span>结构约束</span><span className="disabled">不发送媒体</span></div>
        <form className="environment-form-stack" onSubmit={(event) => event.preventDefault()}>
          {custom && <label className="environment-form-row"><span>服务名称</span><input aria-label="显示名称" value={draft.displayName} maxLength={80} onChange={(event) => update({ displayName: event.target.value })}/><i/></label>}
          <label className="environment-form-row environment-key-row">
            <span>API Key</span>
            {configured && !editingKey
              ? <input aria-label="API Key" value="••••••••••••••••" readOnly tabIndex={-1}/>
              : <input aria-label="API Key" type="password" autoComplete="off" value={draft.apiKey} placeholder={configured ? "输入新的 API Key" : "粘贴服务商提供的 API Key"} onChange={(event) => update({ apiKey: event.target.value })}/>}
            {configured ? <button className="button quiet" type="button" onClick={() => { setEditingKey((value) => !value); update({ apiKey: "" }); }}>{editingKey ? "取消更换" : "更换"}</button> : <i/>}
            <small>{configured && !editingKey ? "密钥已保存在 Windows 凭据管理器中，应用不会反显。" : "留空不会删除已保存凭据；删除使用下方独立操作。"}</small>
          </label>
          <label className="environment-form-row environment-model-row">
            <span>模型</span><input aria-label="模型" list="ai-service-model-options" value={draft.modelId} placeholder="连接后选择或手动填写模型" onChange={(event) => update({ modelId: event.target.value })}/>
            <button className="button quiet" type="button" disabled={props.disabled || !credentialReady} onClick={() => void fetchModels()}>{props.busy === "正在获取模型…" ? "获取中…" : "获取模型"}</button>
            <datalist id="ai-service-model-options">{models.map((model) => <option key={model.id} value={model.id}>{model.displayName}</option>)}</datalist>
            <small>{models.length ? `已获取 ${models.length} 个模型，也可以手动填写。` : "无法获取列表时可以手动填写模型名称。"}</small>
          </label>
          <div className="environment-switch-row"><span><strong>设为默认 AI 服务</strong><small>文本 AI 辅助优先使用此服务。</small></span><Toggle checked={makeDefault} label="设为默认 AI 服务" disabled={props.disabled} onChange={setMakeDefault}/></div>
          <details className="environment-advanced" open={advancedOpen} onToggle={(event) => setAdvancedOpen(event.currentTarget.open)}>
            <summary><span>服务地址与网络设置</span><span>{custom ? "自定义服务" : "使用官方服务"}</span></summary>
            <div><label><span>服务地址</span><input aria-label="服务地址" value={draft.baseUrl} readOnly={!custom} placeholder="https://api.example.com/v1" onChange={(event) => update({ baseUrl: event.target.value })}/></label><AiNetworkSettingsPanel network={props.network} disabled={props.disabled} onSave={props.onSetProxy}/></div>
          </details>
        </form>
        <div className="environment-scope-card"><strong>发送范围</strong><span>✓ 待处理的字幕文本</span><span>✓ 时间戳与结构约束</span><span className="disabled">• 不发送视频、音频或本机路径</span></div>
        <div className="environment-privacy-note">ⓘ 测试连接只发送固定测试内容，可能产生极少量 API 用量，不会发送项目字幕。</div>
        {notice && <div className="environment-connection-result success" role="status">✓ {notice}</div>}
        {props.error && <div className="environment-error" role="alert">{props.error}</div>}
        {props.service && <div className="environment-delete-actions">{configured && <button className="button text" type="button" disabled={props.disabled} onClick={() => window.confirm("确认删除已保存的 API Key？服务配置会保留。") && void props.onDeleteCredential(props.service!.id)}>删除 API Key</button>}<button className="button text" type="button" disabled={props.disabled} onClick={remove}>删除此服务</button></div>}
      </div>
    </section>
    <footer className="environment-settings-footer">
      <span>配置只保存在这台电脑，使用时会再次确认发送范围。</span>
      <button className="button quiet" type="button" disabled={props.disabled || !credentialReady} onClick={() => void test()}>{props.busy === "正在测试连接…" ? "正在测试…" : "测试连接"}</button>
      <button className="button primary" type="button" disabled={props.disabled || !canSave} onClick={() => void save()}>{props.busy === "正在保存 AI 服务…" ? "正在保存…" : configured ? "保存设置" : "保存并使用"}</button>
    </footer>
  </>;
}
