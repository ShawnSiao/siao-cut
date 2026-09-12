import { getUiLocale } from "../../i18n";
import WorkbenchActivityCenter, { type WorkbenchActivityAction } from "../../workbench/workbench-activity-center";
import type { WorkbenchActivity } from "../../workbench/workbench-activity";
import type { useTranscriptionTasks } from "./use-transcription-tasks";
import "./transcription-status.css";

export function TranscriptionStatus({ projectId, tasks, titles, actionsFor }: {
  projectId?: string; tasks: ReturnType<typeof useTranscriptionTasks>;
  titles?: Record<string, string>; actionsFor(activity: WorkbenchActivity): WorkbenchActivityAction[];
}) {
  const zh = getUiLocale() === "zh-CN";
  const jobs = tasks.jobs.filter(job => job.projectId === projectId && ["queued", "running", "finalizing"].includes(job.status));
  if (!jobs.length) return null;
  const running = jobs.some(job => ["queued", "running", "finalizing"].includes(job.status));
  return <section className={`transcription-status${tasks.error ? " query-failed" : ""}`} aria-label={zh ? "当前项目转写状态" : "Current project transcription"}>
    <WorkbenchActivityCenter inputs={{ transcriptionJobs: jobs, projectTitles: titles }} actionsFor={actionsFor}/>
    <div className="transcription-status-query">
      {tasks.error ? <span role="alert">{zh ? "暂时无法更新任务状态，当前显示的是上次查询结果。" : "Unable to refresh task status; the last known state may be stale."}</span>
        : <span>{running ? (zh ? "后台任务进行中；当前阶段未提供进度百分比。" : "Background task active; this stage does not report a percentage.") : (zh ? "请处理任务结果。" : "Review the task result.")}</span>}
      <small>{tasks.lastCheckedAt ? `${zh ? "最近查询" : "Last checked"} ${new Date(tasks.lastCheckedAt).toLocaleTimeString()}` : (zh ? "正在查询任务状态…" : "Checking task status…")}</small>
      {tasks.error && <button onClick={tasks.refresh}>{zh ? "重新查询" : "Retry query"}</button>}
    </div>
  </section>;
}
