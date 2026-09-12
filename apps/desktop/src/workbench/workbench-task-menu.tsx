import { useEffect, useRef } from "react";
import { ListChecks } from "lucide-react";
import { getUiLocale } from "../i18n";
import WorkbenchActivityCenter, { type WorkbenchActivityAction } from "./workbench-activity-center";
import { deriveWorkbenchActivities, type WorkbenchActivity, type WorkbenchActivityInputs } from "./workbench-activity";

export default function WorkbenchTaskMenu({ inputs, actionsFor }: { inputs: WorkbenchActivityInputs; actionsFor: (activity: WorkbenchActivity) => WorkbenchActivityAction[] }) {
  const ref = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const outside = (event: PointerEvent) => { if (ref.current && !ref.current.contains(event.target as Node)) ref.current.open = false; };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, []);
  const activities = deriveWorkbenchActivities(inputs);
  const zh = getUiLocale() === "zh-CN";
  const attention = activities.some((item) => item.state === "action_required" || item.state === "failed");
  const close = () => { if (ref.current) ref.current.open = false; };
  return <details ref={ref} className={`workspace-tasks${attention ? " needs-attention" : ""}`} onKeyDown={(event) => {
    if (event.key === "Escape") { event.stopPropagation(); close(); ref.current?.querySelector("summary")?.focus(); }
  }}>
    <summary><ListChecks size={16}/>{zh ? "任务" : "Tasks"}<span>{activities.length}</span>{attention && <i aria-label={zh ? "需要处理" : "Needs attention"}>!</i>}</summary>
    <div className="workspace-tasks-popover">
      <header><strong>{zh ? "任务中心" : "Task center"}</strong><button onClick={close} aria-label={zh ? "关闭任务中心" : "Close task center"}>×</button></header>
      {activities.length ? <WorkbenchActivityCenter inputs={inputs} actionsFor={actionsFor}/> : <p>{zh ? "暂无进行中的任务" : "No active tasks"}</p>}
    </div>
  </details>;
}
