import { runCore } from "../core";

type TranscriptFormat = "srt" | "vtt" | "ass" | "markdown" | "json";
type SubtitleMode = "source" | "translated" | "bilingual";

export const exportRuntimeClient = {
  listVideoExports: (projectId: string) => runCore(["video", "list", projectId]),
  getVideoExport: (jobId: string) => runCore(["video", "status", jobId]),
  exportStructuredTranscript: (projectId: string, format: TranscriptFormat, output: string, includeSpeakerLabels: boolean, confirmWarnings: boolean) => runCore([
    "transcription", "export", projectId,
    "--format", format,
    "--output", output,
    ...(includeSpeakerLabels ? ["--include-speaker-labels"] : []),
    ...(confirmWarnings ? ["--confirm-warnings"] : []),
  ]),
  exportTranscript: (projectId: string, format: TranscriptFormat, output: string, subtitleMode: SubtitleMode, subtitleLanguage?: string) => runCore([
    "transcript", "export", projectId,
    "--format", format,
    "--output", output,
    "--subtitle-mode", subtitleMode,
    ...(subtitleMode === "source" ? [] : ["--lang", subtitleLanguage ?? ""]),
  ]),
  exportVideo: (projectId: string, output: string, subtitleMode: SubtitleMode, subtitleLanguage?: string) => runCore([
    "video", "export", projectId,
    "--output", output,
    "--burn-subtitles",
    "--subtitle-mode", subtitleMode,
    ...(subtitleMode === "source" ? [] : ["--lang", subtitleLanguage ?? ""]),
  ]),
  cancelVideoExport: (jobId: string) => runCore(["video", "cancel", jobId]),
  retryVideoExport: (jobId: string) => runCore(["video", "retry", jobId]),
};
