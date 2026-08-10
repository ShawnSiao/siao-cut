import { useEffect, useState } from "react";
import type { AiNetworkSettings } from "./ai-service-types";

type Props = { network: AiNetworkSettings; disabled: boolean; onSave: (url: string | null) => Promise<unknown> };

export function AiNetworkSettingsPanel({ network, disabled, onSave }: Props) {
  const [proxy, setProxy] = useState(network.customProxyUrl ?? "");
  useEffect(() => setProxy(network.customProxyUrl ?? ""), [network.customProxyUrl]);
  const changed = proxy.trim() !== (network.customProxyUrl ?? "");
  return <details className="ai-network-settings">
    <summary><span><strong>网络与代理</strong><small>当前：{network.effectiveSource === "direct" ? "直接连接" : network.effectiveSource}</small></span></summary>
    <div><label><span>自定义代理</span><input value={proxy} disabled={disabled} placeholder="例如 http://127.0.0.1:7890" spellCheck={false} onChange={(event) => setProxy(event.target.value)}/></label>
      <p>连接顺序：自定义代理、环境变量、Windows 系统代理、直接连接。只显示代理来源，不显示认证信息。</p>
      <button type="button" disabled={disabled || !changed} onClick={() => void onSave(proxy.trim() || null)}>保存代理</button>
    </div>
  </details>;
}
