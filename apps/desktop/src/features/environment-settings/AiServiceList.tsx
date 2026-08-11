import anthropicLogo from "../../assets/ai-service-logos/anthropic.svg";
import glmLogo from "../../assets/ai-service-logos/chatglm.svg";
import codexLogo from "../../assets/ai-service-logos/codex.svg";
import deepSeekLogo from "../../assets/ai-service-logos/deepseek.svg";
import geminiLogo from "../../assets/ai-service-logos/gemini.svg";
import kimiLogo from "../../assets/ai-service-logos/kimi.svg";
import openAiLogo from "../../assets/ai-service-logos/openai.svg";
import { localCodexSelectionId, providerSelectionId } from "./ai-service-selection";
import type { AiProviderId, AiServiceSettings, AiServiceSummary } from "./ai-service-types";

const logos: Partial<Record<AiProviderId, string>> = {
  openai: openAiLogo,
  anthropic: anthropicLogo,
  gemini: geminiLogo,
  deepseek: deepSeekLogo,
  kimi: kimiLogo,
  glm: glmLogo,
};

type ServiceStatus = { label: string; tone: "ready" | "untested" | "error" | "neutral" };

function serviceStatus(service: AiServiceSummary | undefined): ServiceStatus {
  if (!service) return { label: "未配置", tone: "neutral" };
  if (service.credentialState !== "stored") return { label: "需要 API Key", tone: "neutral" };
  if (service.connectionState === "ready") {
    return { label: "已连接", tone: "ready" };
  }
  if (service.connectionState === "error") return { label: "连接异常", tone: "error" };
  return { label: "已配置，未测试", tone: "untested" };
}

type RowProps = {
  id: string;
  logo?: string;
  mark?: string;
  name: string;
  status: ServiceStatus;
  selected: boolean;
  disabled: boolean;
  onSelect: () => void;
};

function ServiceRow({ id, logo, mark, name, status, selected, disabled, onSelect }: RowProps) {
  return <button className={`environment-provider-row ${selected ? "selected" : ""}`} data-service-id={id} type="button" disabled={disabled} aria-pressed={selected} onClick={onSelect}>
    <span className="environment-provider-logo" aria-hidden="true">{logo ? <img src={logo} alt=""/> : mark}</span>
    <span className="environment-provider-copy"><strong>{name}</strong></span>
    <span className={`environment-provider-status ${status.tone}`}>{status.label}</span>
  </button>;
}

type Props = {
  settings: AiServiceSettings;
  selectionId: string;
  disabled: boolean;
  onSelect: (id: string) => void;
};

export function AiServiceList({ settings, selectionId, disabled, onSelect }: Props) {
  const builtIns = settings.providerCatalog.providers.filter((provider) => provider.id !== "custom");
  const customServices = settings.services.filter((service) => service.providerId === "custom");
  return <aside className="environment-provider-panel" aria-label="AI 服务列表">
    <h2>可用服务</h2>
    <div className="environment-provider-list">
      <ServiceRow id={localCodexSelectionId} logo={codexLogo} name="本机 Codex" status={{ label: "本机检测", tone: "neutral" }} selected={selectionId === localCodexSelectionId} disabled={disabled} onSelect={() => onSelect(localCodexSelectionId)}/>
      {builtIns.map((provider) => {
        const service = settings.services.find((item) => item.providerId === provider.id);
        const id = service?.id ?? providerSelectionId(provider.id);
        return <ServiceRow key={provider.id} id={id} logo={logos[provider.id]} name={provider.displayName} status={serviceStatus(service)} selected={selectionId === id} disabled={disabled} onSelect={() => onSelect(id)}/>;
      })}
      {customServices.map((service) => <ServiceRow key={service.id} id={service.id} mark="+" name={service.displayName} status={serviceStatus(service)} selected={selectionId === service.id} disabled={disabled} onSelect={() => onSelect(service.id)}/>)}
    </div>
    <button className="button quiet environment-add-provider" type="button" disabled={disabled} onClick={() => onSelect(providerSelectionId("custom"))}>＋ 添加其他服务</button>
  </aside>;
}
