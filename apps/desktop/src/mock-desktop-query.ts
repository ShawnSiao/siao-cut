import type { DesktopQuery } from "./generated/core-contract";
import { mockRun } from "./core.mock";

/** Browser preview only; production queries dispatch directly to Rust application services. */
export function mockDesktopQuery(query: DesktopQuery) {
  switch (query.action) {
    case "models": return mockRun(["model","list",...(query.verify?["--verify"]:[])]);
    case "model_jobs": return mockRun(["model","jobs"]);
    case "model_job": return mockRun(["model","status",query.jobId]);
    case "source_jobs": return mockRun(["source","jobs"]);
    case "source_job": return mockRun(["source","status",query.jobId]);
    case "auto_workflows": return mockRun(["auto","list"]);
    case "auto_workflow": return mockRun(["auto","status",query.workflowId]);
    case "audio_latest": return mockRun(["speech","audio-latest",query.projectId]);
    case "audio_job": return mockRun(["speech","audio-status",query.jobId]);
    case "speaker_package": return mockRun(["speaker","package",...(query.verify?["--verify"]:[])]);
    case "speaker_jobs": return mockRun(["speaker","jobs"]);
    case "speaker_job": return mockRun(["speaker","job-status",query.jobId]);
    case "speaker_track": return mockRun(["speaker","track",query.projectId]);
    case "transcription_health": return mockRun(["transcription","health"]);
    case "latest_transcription": return mockRun(["transcription","latest",query.projectId]);
    case "transcription_reviews": return mockRun(["transcription","review",query.projectId]);
    case "video_exports": return mockRun(["video","list",query.projectId]);
    case "video_export": return mockRun(["video","status",query.jobId]);
    case "agent_health": return mockRun(["agent","health"]);
    case "agent_runs": return mockRun(["agent","list",...(query.projectId?[query.projectId]:[])]);
    case "agent_run": return mockRun(["agent","status",query.runId]);
    case "resource_status": return mockRun(["resources","status"]);
    case "resource_plan": return mockRun(["resources","plan",query.capability,...(query.profile?["--profile",query.profile]:[])]);
    case "resource_job": return mockRun(["resources","job",query.jobId]);
    case "resource_jobs": return mockRun(["resources","jobs"]);
    case "runtime": return mockRun(["runtime","status"]);
    case "delete_preflight": return mockRun(["project","delete-preflight",query.projectId]);
    case "transcript_replacement_preflight": return mockRun(["transcript","replacement-preflight",query.projectId]);
    case "inspect_subtitle": return mockRun(["transcript","inspect-file",query.projectId,query.path]);
    case "preview_cut": return mockRun(["cut","preview",query.projectId,query.editId]);
  }
}
