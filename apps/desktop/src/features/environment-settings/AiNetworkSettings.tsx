import { useEffect, useState } from "react";
import type { AiNetworkSettings } from "./ai-service-types";

type Props = { network: AiNetworkSettings; disabled: boolean; onSave: (url: string | null) => Promise<unknown> };

export function AiNetworkSettingsPanel({ network, disabled, onSave }: Props) {
  const [proxy, setProxy] = useState(network.customProxyUrl ?? "");
  useEffect(() => setProxy(network.customProxyUrl ?? ""), [network.customProxyUrl]);
  const changed = proxy.trim() !== (network.customProxyUrl ?? "");
  return <div className="environment-network-fields">
    <label><span>自定义代理</span><input aria-label="自定义代理" value={proxy} disabled={disabled} placeholder="例如 http://127.0.0.1:7890" spellCheck={false} onChange={(event) => setProxy(event.target.value)}/></label>
    <small>顺序：自定义代理、环境变量、Windows 系统代理、直接连接。当前：{network.effectiveSource === "direct" ? "直接连接" : network.effectiveSource}</small>
    <button className="button quiet" type="button" disabled={disabled || !changed} onClick={() => void onSave(proxy.trim() || null)}>保存代理</button>
  </div>;
}
