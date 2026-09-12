import { runCore } from "../core";
import { desktopControl } from "./desktop-control-client";
import { desktopQuery } from "./desktop-query-client";

type TranscriptFormat = "srt" | "vtt" | "ass" | "markdown" | "json";
type SubtitleMode = "source" | "translated" | "bilingual";
type SubtitleDelivery = "burned" | "embedded-mp4" | "embedded-mkv" | "sidecar-srt" | "sidecar-vtt";

export const exportRuntimeClient = {
  listVideoExports: (projectId: string) => desktopQuery({action:"video_exports",projectId}),
  getVideoExport: (jobId: string) => desktopQuery({action:"video_export",jobId}),
  exportStructuredTranscript: (projectId: string, format: TranscriptFormat, output: string, includeSpeakerLabels: boolean, confirmWarnings: boolean) => runCore([
    "transcription", "export", projectId,
    "--format", format,
    "--output", output,
    ...(includeSpeakerLabels ? ["--include-speaker-labels"] : []),
    ...(confirmWarnings ? ["--confirm-warnings"] : []),
  ]),
  exportTranscript: (projectId: string, format: TranscriptFormat, output: string, subtitleMode: SubtitleMode, subtitleLanguage?: string, confirmStaleTranslation = false) => runCore([
    "transcript", "export", projectId,
    "--format", format,
    "--output", output,
    "--subtitle-mode", subtitleMode,
    ...(subtitleMode === "source" ? [] : ["--lang", subtitleLanguage ?? ""]),
    ...(confirmStaleTranslation ? ["--confirm-stale-translation"] : []),
  ]),
  exportVideo: (projectId: string, output: string, subtitleDelivery: SubtitleDelivery, subtitleMode: SubtitleMode, subtitleLanguage?: string, confirmStaleTranslation = false) => runCore([
    "video", "export", projectId,
    "--output", output,
    "--subtitle-delivery", subtitleDelivery,
    "--subtitle-mode", subtitleMode,
    ...(subtitleMode === "source" ? [] : ["--lang", subtitleLanguage ?? ""]),
    ...(confirmStaleTranslation ? ["--confirm-stale-translation"] : []),
  ]),
  cancelVideoExport: (jobId: string) => desktopControl({ action: "video_cancel", jobId: jobId }),
  retryVideoExport: (jobId: string) => desktopControl({action:"video_retry",jobId}),
};
