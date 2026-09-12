import { mockRun } from "./core.mock";
import type { DesktopControl } from "./generated/core-contract";
/** Browser simulation only. Production dispatch never expands command arguments. */
export function mockDesktopControl(request: DesktopControl) {
  switch (request.action) {
    case "auto_start": {
      const o = request.options;
      return mockRun(["auto", "start", ...(o.media ? ["--media", o.media, "--title", o.title ?? ""] : ["--url", o.url ?? "", "--confirm-media-id", o.confirmMediaId ?? ""]), "--model", o.model, "--language", o.language ?? "auto", "--locale", o.locale, "--output", o.output, "--subtitle-mode", o.subtitleMode, "--profile", o.profile, ...(o.translate ? ["--translate", o.translate] : []), ...(o.aiExecution !== "manual" ? ["--ai-execution", o.aiExecution, ...(o.aiExecution === "api" ? ["--ai-service-config-id", o.aiServiceConfigId ?? "", "--ai-service-revision", String(o.aiServiceRevision), "--ai-network-revision", String(o.aiNetworkRevision), "--ai-model-id", o.aiModelId ?? ""] : []), "--confirm-ai-text-send"] : []), ...(o.burnSubtitles ? ["--burn-subtitles"] : [])]);
    }
    case "media_prepare": return mockRun(["media", "prepare", request.projectId]);
    case "agent_resume": return mockRun(["agent", "resume", request.runId]);
    case "video_retry": return mockRun(["video", "retry", request.jobId]);
    case "auto_cancel": return mockRun(["auto", "cancel", request.workflowId]);
    case "auto_continue": return mockRun(["auto", "continue", request.workflowId]);
    case "model_install": return mockRun(["model", "install", request.modelId]);
    case "model_cancel": return mockRun(["model", "cancel", request.jobId]);
    case "model_remove": return mockRun(["model", "remove", request.modelId]);
    case "source_inspect": return mockRun(["source", "inspect", request.url, ...(request.browser ? ["--browser", request.browser] : [])]);
    case "source_start": return mockRun(["source", "start", request.url, "--confirm-media-id", request.confirmMediaId, ...(request.startDelayMs == null ? [] : ["--start-delay-ms", String(request.startDelayMs)]), ...(request.browser ? ["--browser", request.browser] : [])]);
    case "source_cancel": return mockRun(["source", "cancel", request.jobId]);
    case "source_resume": return mockRun(["source", "resume", request.jobId]);
    case "audio_start": return mockRun(["speech", "audio-start", request.projectId, ...(request.startDelayMs == null ? [] : ["--start-delay-ms", String(request.startDelayMs)])]);
    case "audio_cancel": return mockRun(["speech", "audio-cancel", request.jobId]);
    case "audio_resume": return mockRun(["speech", "audio-resume", request.jobId, ...(request.startDelayMs == null ? [] : ["--start-delay-ms", String(request.startDelayMs)])]);
    case "speaker_install": return mockRun(["speaker", "install"]);
    case "speaker_cancel": return mockRun(["speaker", "cancel", request.jobId]);
    case "speaker_resume": return mockRun(["speaker", "resume", request.jobId]);
    case "speaker_analyze": return mockRun(["speaker", "analyze", request.projectId]);
    case "resource_check_updates": return mockRun(["resources", "check-updates", ...(request.capability ? [request.capability] : [])]);
    case "resource_configure": return mockRun(["resources", "configure", "--root", request.root]);
    case "resource_migrate": return mockRun(["resources", "migrate", "--root", request.root]);
    case "resource_install": return mockRun(["resources", "install", request.capability, ...(request.profile ? ["--profile", request.profile] : [])]);
    case "resource_update": return mockRun(["resources", "update", request.capability, ...(request.profile ? ["--profile", request.profile] : [])]);
    case "resource_cancel": return mockRun(["resources", "cancel", request.jobId]);
    case "resource_resume": return mockRun(["resources", "resume", request.jobId]);
    case "resource_repair": return mockRun(["resources", "repair", request.capability]);
    case "resource_rollback": return mockRun(["resources", "rollback", request.capability]);
    case "resource_remove": return mockRun(["resources", "remove", request.capability]);
    case "resource_cleanup": return mockRun(["resources", "cleanup"]);
    case "agent_cancel": return mockRun(["agent", "cancel", request.runId]);
    case "task_retry": return mockRun(["task", "retry", request.taskId]);
    case "task_cancel": return mockRun(["task", "cancel", request.taskId]);
    case "video_cancel": return mockRun(["video", "cancel", request.jobId]);
    case "transcription_configure": return mockRun(["transcription", "configure", "--endpoint", request.endpoint, "--model", request.model]);
  }
}
