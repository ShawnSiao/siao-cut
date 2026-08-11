import type { CodexHealth } from "../../types";

type Props = {
  health: CodexHealth | null;
  busy: boolean;
  onRefresh: () => void;
};

export function LocalCodexDetail({ health, busy, onRefresh }: Props) {
  const ready = Boolean(health?.available && health.authenticated);
  const title = ready
    ? `Codex ${health?.version ?? "CLI"} 已准备`
    : health
      ? "当前没有检测到可用的本机 Codex"
      : "正在检测本机 Codex…";
  const description = ready
    ? "字幕文本只交给这台电脑上的 Codex 处理。"
    : health?.available
      ? "已检测到 Codex，但尚未登录。仍可使用已配置的 API 服务。"
      : "可以继续使用已配置的 API 服务或复制提示词。";

  return <>
    <section className="environment-provider-detail" aria-label="本机 Codex 状态">
      <div className="environment-detail-scroll">
        <div className="environment-detail-heading">
          <div><h2>本机 Codex</h2><p>检测这台电脑上的 Codex，作为本地文本辅助服务。</p></div>
          <span className={`environment-status-chip ${ready ? "" : "unavailable"}`}>{ready ? "可以使用" : "不可用"}</span>
        </div>
        <div className="environment-codex-status"><strong>{title}</strong><p>{description}</p></div>
        <div className="environment-privacy-note">◇ 本机服务不会把字幕发送给外部 API 服务。</div>
      </div>
    </section>
    <footer className="environment-settings-footer">
      <span>配置只保存在这台电脑，使用时会再次确认发送范围。</span>
      <button className="button quiet" type="button" disabled={busy} onClick={onRefresh}>{busy ? "正在检测…" : "重新检测"}</button>
    </footer>
  </>;
}
