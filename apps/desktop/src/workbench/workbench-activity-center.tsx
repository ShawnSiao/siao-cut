import { Activity, AudioWaveform, Bot, CircleAlert, FileVideo2, Film, Link2, LoaderCircle, Sparkles } from "lucide-react";
import { JobFailureDetails } from "../components/job-failure";
import { tr } from "../i18n";
import { autoStageLabel, autoStatusLabel } from "../app-view-model";
import { deriveWorkbenchActivities, transcriptionStageLabel, type WorkbenchActivity, type WorkbenchActivityInputs, type WorkbenchActivityKind } from "./workbench-activity";

export type WorkbenchActivityAction = {
  id: string;
  label: string;
  primary?: boolean;
  disabled?: boolean;
  onClick: () => void;
};

function activityTitle(kind: WorkbenchActivityKind) {
  return tr(({
    local: "app.activity.local",
    source: "app.activity.source",
    transcription: "app.activity.transcription",
    agent: "app.activity.agent",
    audio: "app.activity.audio",
    export: "app.activity.export",
    auto: "app.activity.auto",
  } as const)[kind]);
}

function statusLabel(activity: WorkbenchActivity) {
  if (activity.kind === "transcription")
    return transcriptionStageLabel(activity.stage);
  if (activity.kind === "auto" && activity.stage) {
    return `${autoStatusLabel(activity.status)} · ${autoStageLabel(activity.stage)}`;
  }
  if (activity.kind === "local" && activity.detail)
    return activity.detail;
  return tr(({
    action_required: "app.activity.actionRequired",
    failed: activity.status === "interrupted" ? "app.activity.interrupted" : activity.status === "cancelled" || activity.status === "canceled" ? "app.activity.cancelled" : "app.activity.failed",
    running: "app.activity.running",
    ready: "app.activity.ready",
  } as const)[activity.state]);
}

function ActivityIcon({ activity }: { activity: WorkbenchActivity }) {
  if (activity.state === "failed" || activity.state === "action_required") return <CircleAlert size={16}/>;
  if (activity.kind === "source") return <Link2 size={16}/>;
  if (activity.kind === "transcription") return <AudioWaveform size={16}/>;
  if (activity.kind === "agent") return <Bot size={16}/>;
  if (activity.kind === "audio") return <Activity size={16}/>;
  if (activity.kind === "export") return <Film size={16}/>;
  if (activity.kind === "auto") return <Sparkles size={16}/>;
  if (activity.kind === "local") return <LoaderCircle className="spin" size={16}/>;
  return <FileVideo2 size={16}/>;
}

function ActivityRow({ activity, actions }: { activity: WorkbenchActivity; actions: WorkbenchActivityAction[] }) {
  const title = activityTitle(activity.kind);
  return <div className={`workbench-activity-row ${activity.state}`} role="group" aria-label={tr("app.activity.item", { name: title })}>
    <ActivityIcon activity={activity}/>
    <span className="workbench-activity-copy"><strong><span>{title}</span><span aria-hidden="true"> · </span><span>{statusLabel(activity)}</span></strong>{activity.detail && activity.kind !== "local" ? <small title={activity.detail}>{activity.detail}</small> : null}</span>
    {activity.progress != null ? <><progress max={1} value={activity.progress}/><code>{Math.round(activity.progress * 100)}%</code></> : <span className="workbench-activity-stage"/>}
    {activity.errorMessage ? <JobFailureDetails context={activity.kind === "auto" ? "auto" : activity.kind === "export" ? "export" : activity.kind === "transcription" ? "transcription" : activity.kind === "source" ? "source" : activity.kind === "audio" ? "audio" : "agent"} status={activity.status} errorCode={activity.errorCode} errorMessage={activity.errorMessage}/> : null}
    {actions.length ? <div className="workbench-activity-actions">{actions.map((action) => <button type="button" className={action.primary ? "primary" : ""} disabled={action.disabled} key={action.id} onClick={action.onClick}>{action.label}</button>)}</div> : null}
  </div>;
}

export default function WorkbenchActivityCenter({ inputs, actionsFor }: { inputs: WorkbenchActivityInputs; actionsFor: (activity: WorkbenchActivity) => WorkbenchActivityAction[] }) {
  const activities = deriveWorkbenchActivities(inputs);
  if (!activities.length)
    return null;
  const [primary, ...rest] = activities;
  return <section className={`workbench-activity-center ${primary.state}`} aria-label={tr("app.activity.region")} role="region" aria-live="polite">
    <ActivityRow activity={primary} actions={actionsFor(primary)}/>
    {rest.length ? <details><summary>{tr("app.activity.more", { count: rest.length })}</summary><div className="workbench-activity-list">{rest.map((activity) => <ActivityRow key={activity.id} activity={activity} actions={actionsFor(activity)}/>)}</div></details> : null}
  </section>;
}
