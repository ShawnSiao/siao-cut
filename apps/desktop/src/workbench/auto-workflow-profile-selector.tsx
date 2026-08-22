import { Check, FileText, ListChecks, ShieldCheck } from "lucide-react";
import { tr } from "../i18n";
import type { WorkflowProfile } from "../types";

const profiles: Array<{ id: WorkflowProfile; icon: typeof FileText }> = [
  { id: "draft", icon: FileText },
  { id: "balanced", icon: ListChecks },
  { id: "delivery", icon: ShieldCheck },
];

export default function AutoWorkflowProfileSelector({ value, onChange }: { value: WorkflowProfile; onChange: (profile: WorkflowProfile) => void }) {
  return <fieldset className="auto-workflow-profiles">
    <legend>{tr("app.workflowProfile.label")}</legend>
    {profiles.map(({ id, icon: Icon }) => <label key={id} className={value === id ? "selected" : ""}>
      <input type="radio" name="auto-workflow-profile" value={id} checked={value === id} onChange={() => onChange(id)}/>
      <Icon size={17}/>
      <span><strong>{tr(`app.workflowProfile.${id}`)}</strong><small>{tr(`app.workflowProfile.${id}.description`)}</small></span>
      {value === id && <Check size={15}/>}
    </label>)}
    {value !== "balanced" && <p role="note">{tr(`app.workflowProfile.${value}.notice`)}</p>}
  </fieldset>;
}
