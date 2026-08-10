import { CheckCircle2, CircleAlert, CircleDashed, Plus } from "lucide-react";
import type { AiServiceSummary } from "./ai-service-types";

type Props = {
  services: AiServiceSummary[];
  selectedId: string | null;
  disabled: boolean;
  onSelect: (id: string) => void;
  onCreate: () => void;
};

const stateMeta = {
  ready: { label: "连接正常", Icon: CheckCircle2 },
  error: { label: "最近测试失败", Icon: CircleAlert },
  untested: { label: "尚未测试", Icon: CircleDashed },
};

export function AiServiceList({ services, selectedId, disabled, onSelect, onCreate }: Props) {
  return <aside className="ai-service-list" aria-label="AI 服务列表">
    <header><div><strong>服务</strong><small>{services.length} 项配置</small></div><button type="button" className="icon-button" aria-label="添加 AI 服务" title="添加 AI 服务" disabled={disabled} onClick={onCreate}><Plus size={16}/></button></header>
    <div className="ai-service-list-items">
      {services.length === 0 && <p className="ai-empty">尚未配置 API 服务。仍可继续使用本机 Codex 或复制提示词。</p>}
      {services.map((service) => {
        const { label, Icon } = stateMeta[service.connectionState];
        return <button type="button" key={service.id} disabled={disabled} aria-pressed={selectedId === service.id} className={selectedId === service.id ? "selected" : ""} onClick={() => onSelect(service.id)}>
          <span className="ai-service-list-title"><strong>{service.displayName}</strong>{service.isDefault && <em>默认</em>}</span>
          <span className="ai-service-list-meta"><Icon size={13}/>{label} · {service.credentialState === "stored" ? "已保存 Key" : "未配置 Key"}</span>
          <small>{service.modelId || "未选择模型"}</small>
        </button>;
      })}
    </div>
  </aside>;
}
