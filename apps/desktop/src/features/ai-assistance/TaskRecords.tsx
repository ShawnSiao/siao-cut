import { useState } from "react";
import type { Task } from "../../types";
import "./task-records.css";

const failed = (task: Task) => ["failed", "interrupted"].includes(task.status);
const terminal = (task: Task) => ["failed", "interrupted", "completed", "cancelled", "canceled"].includes(task.status);
export const archiveToken = (task: Task) => `${task.status}:${task.attemptCount ?? 0}:${task.lastActivity?.createdAt ?? ""}`;
const keyFor = (id: string) => `siaocut.taskArchives.v1:${id}`;
const read = (id: string): Record<string, string> => {
  try { const value = JSON.parse(localStorage.getItem(keyFor(id)) ?? "{}"); return value && typeof value === "object" && !Array.isArray(value) ? value : {}; }
  catch { return {}; }
};
export function TaskRecords({ projectId, tasks, onRetry, pending, english = false }: {
  projectId: string; tasks: Task[]; onRetry(id: string): void; pending: Record<string, string>; english?: boolean;
}) {
  const [archived, setArchived] = useState(() => read(projectId));
  const [filter, setFilter] = useState("attention");
  const [error, setError] = useState("");
  const text = (zh: string, en: string) => english ? en : zh;
  const isArchived = (task: Task) => terminal(task) && archived[task.id] === archiveToken(task);
  const failures = tasks.filter(task => failed(task) && !isArchived(task));
  const change = (items: Task[], hide: boolean) => {
    const next = { ...read(projectId) };
    for (const task of items) { if (hide && terminal(task)) next[task.id] = archiveToken(task); else delete next[task.id]; }
    try { localStorage.setItem(keyFor(projectId), JSON.stringify(next)); setArchived(next); setError(""); }
    catch { setError(text("归档设置保存失败，记录仍保留。", "Could not save archive preferences. Records are retained.")); }
  };
  const visible = tasks.filter(task => filter === "archived" ? isArchived(task) : filter === "attention" ? failed(task) && !isArchived(task) : !isArchived(task)).slice().reverse();
  const kinds: Record<string, string> = { polish: "润色", translate: "翻译", proofread: "校对", edit: "剪辑", punctuate: "标点", speaker_names: "说话人" };
  const states: Record<string, string> = { failed: "失败", interrupted: "中断", completed: "完成", cancelled: "已取消", queued: "排队", claimed: "已领取", running: "运行中", review: "待审核" };
  return <details className="task-records" onKeyDown={event => { if (event.key === "Escape") { event.currentTarget.open = false; event.currentTarget.querySelector("summary")?.focus(); event.stopPropagation(); } }}>
    <summary>{text("执行记录", "Task records")}{failures.length > 0 && <span>{failures.length} {text("待处理", "need attention")}</span>}</summary>
    <section aria-label={text("执行记录", "Task records")}>
      <p>{text("归档只隐藏本机记录，不删除结果，也不会终止外部 Agent。", "Archiving hides records on this device. It does not delete results or stop an external Agent.")}</p>
      <div className="task-record-filters">
        <select aria-label={text("记录筛选", "Filter records")} value={filter} onChange={e => setFilter(e.target.value)}>
          <option value="attention">{text("待处理失败", "Needs attention")}</option><option value="all">{text("全部未归档", "All unarchived")}</option><option value="archived">{text("已归档", "Archived")}</option>
        </select>
        <button disabled={!failures.length} onClick={() => change(failures, true)}>{text("归档全部旧失败", "Archive all failures")}</button>
      </div>
      {error && <p role="alert">{error}</p>}
      <div className="task-record-list">{visible.length === 0 && <p>{text("暂无记录", "No records")}</p>}
      {visible.map(task => <article key={task.id}>
        <header><strong>{english ? task.kind : kinds[task.kind] ?? task.kind}{task.language ? ` · ${task.language}` : ""}</strong><span>{english ? task.status : states[task.status] ?? task.status}</span></header>
        <small>{text("尝试次数", "Attempts")}: {task.attemptCount ?? 0} · {new Date(task.lastActivity?.createdAt ?? task.createdAt ?? "").toLocaleString(english ? "en" : "zh-CN")}</small>
        <details><summary>{text("详情", "Details")}</summary><p>{task.errorMessage}</p><code>{task.id}</code><p>{text("基线版本", "Base version")}: {task.baseVersionId}</p></details>
        <div className="task-record-filters">{failed(task) && !isArchived(task) && <button disabled={Boolean(pending[task.id])} onClick={() => onRetry(task.id)}>{text("重新排队", "Requeue")}</button>}
        {terminal(task) && <button disabled={Boolean(pending[task.id])} onClick={() => change([task], !isArchived(task))}>{isArchived(task) ? text("恢复显示", "Restore") : text("归档", "Archive")}</button>}</div>
      </article>)}</div>
    </section>
  </details>;
}
