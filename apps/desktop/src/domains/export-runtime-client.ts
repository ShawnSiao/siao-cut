import { runCoreStructured } from "../core";
import type { ExportOperation } from "../generated/core-contract";
import { desktopControl } from "./desktop-control-client";
import { desktopQuery } from "./desktop-query-client";

type TranscriptFormat = "srt" | "vtt" | "ass" | "markdown" | "json";
type SubtitleMode = "source" | "translated" | "bilingual";
type SubtitleDelivery = "burned" | "embedded-mp4" | "embedded-mkv" | "sidecar-srt" | "sidecar-vtt";

export const exportRuntimeClient = {
  listVideoExports: (projectId: string) => desktopQuery({action:"video_exports",projectId}),
  getVideoExport: (jobId: string) => desktopQuery({action:"video_export",jobId}),
  exportStructuredTranscript: (projectId: string, format: TranscriptFormat, output: string, includeSpeakerLabels: boolean, confirmWarnings: boolean, expectedVersionId: string) => sendExport(projectId, expectedVersionId, {kind:"structured",format,output,includeSpeakerLabels,confirmWarnings}),
  exportTranscript: (projectId: string, format: TranscriptFormat, output: string, subtitleMode: SubtitleMode, subtitleLanguage: string | undefined, confirmStaleTranslation: boolean, expectedVersionId: string) => sendExport(projectId, expectedVersionId, {kind:"transcript",format,output,subtitleMode,language:subtitleMode === "source" ? null : subtitleLanguage ?? null,allowStaleTranslation:confirmStaleTranslation}),
  exportVideo: (projectId: string, output: string, subtitleDelivery: SubtitleDelivery, subtitleMode: SubtitleMode, subtitleLanguage: string | undefined, confirmStaleTranslation: boolean, expectedVersionId: string) => sendExport(projectId, expectedVersionId, {kind:"video",output,subtitleDelivery,subtitleMode,language:subtitleMode === "source" ? null : subtitleLanguage ?? null,allowStaleTranslation:confirmStaleTranslation}),
  cancelVideoExport: (jobId: string) => desktopControl({ action: "video_cancel", jobId: jobId }),
  retryVideoExport: (jobId: string) => desktopControl({action:"video_retry",jobId}),
};

const pending = new Map<string, ReturnType<typeof runCoreStructured>>();
function sendExport(projectId: string, expectedVersionId: string, operation: ExportOperation) {
  const key = `siaocut.exportMutation:${JSON.stringify({projectId,expectedVersionId,operation})}`;
  const active = pending.get(key);
  if (active) return active;
  const mutationId = sessionStorage.getItem(key) ?? crypto.randomUUID();
  sessionStorage.setItem(key,mutationId);
  const result = runCoreStructured({kind:"export_command",request:{projectId,expectedVersionId,mutationId,operation}})
    .then(value => {sessionStorage.removeItem(key);return value;}).finally(() => pending.delete(key));
  pending.set(key,result);
  return result;
}
