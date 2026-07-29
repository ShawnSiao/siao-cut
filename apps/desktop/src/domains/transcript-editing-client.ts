import { runCore, runCoreStructured } from "../core";
import type { CanvasSettings, Project, TranscriptionLanguage } from "../types";

export const transcriptEditingClient = {
  quickTranscribe: (projectId: string, modelPath: string, language: TranscriptionLanguage, expectedVersionId: string, confirmReplace = false) => runCore(["transcribe", projectId, "--model", modelPath, "--language", language, "--expected-version", expectedVersionId, ...(confirmReplace ? ["--confirm-replace"] : [])]),
  transcriptReplacementPreflight: (projectId: string) => runCore(["transcript", "replacement-preflight", projectId]),
  getSpeakerTrack: (projectId: string) => runCore(["speaker", "track", projectId]),
  renameSpeaker: (projectId: string, speakerId: string, name: string) => runCore(["speaker", "rename", projectId, speakerId, "--name", name]),
  mergeSpeaker: (projectId: string, fromId: string, intoId: string) => runCore(["speaker", "merge", projectId, "--from", fromId, "--into", intoId]),
  assignSpeaker: (projectId: string, segmentId: string, speakerId: string) => runCore(["speaker", "assign", projectId, segmentId, speakerId]),
  editSegment: (projectId: string, segmentId: string, text: string) => runCore(["transcript", "edit", projectId, segmentId, "--text", text]),
  replaceAll: (projectId: string, search: string, replacement: string) => runCore(["transcript", "replace", projectId, "--find", search, "--replace", replacement]),
  splitSegment: (projectId: string, segmentId: string, textOffset: number, at: number) => runCore(["transcript", "split", projectId, segmentId, "--text-offset", String(textOffset), "--at", String(at)]),
  mergeSegments: (projectId: string, firstId: string, secondId: string) => runCore(["transcript", "merge", projectId, firstId, secondId]),
  updateTiming: (projectId: string, segmentId: string, start: number, end: number) => runCore(["transcript", "timing", projectId, segmentId, "--start", String(start), "--end", String(end)]),
  offsetSegments: (projectId: string, segmentIds: string[], delta: number) => runCoreStructured({ kind: "transcript_offset", projectId, segmentIds, delta }),
  inspectSubtitleFile: (projectId: string, path: string) => runCore(["transcript", "inspect-file", projectId, path]),
  importSubtitleFile: (projectId: string, path: string, expectedSha256: string, expectedVersionId: string) => runCore(["transcript", "import-file", projectId, path, "--confirm-replace", "--expected-sha256", expectedSha256, "--expected-version", expectedVersionId]),
  setCanvas: (projectId: string, settings: CanvasSettings) => runCore(["canvas", "set", projectId, "--aspect-ratio", settings.aspectRatio, "--framing", settings.framing]),
  setSubtitleStyle: (projectId: string, preset: Project["subtitleStyle"]["preset"], position: Project["subtitleStyle"]["position"]) => runCore(["transcript", "set-style", projectId, "--preset", preset, "--position", position]),
  prepareMedia: (projectId: string) => runCore(["media", "prepare", projectId]),
  updateCut: (projectId: string, editId: string, action: "apply" | "restore") => runCore(["cut", action, projectId, editId]),
  detectCuts: (projectId: string) => runCore(["cut", "detect", projectId]),
  previewCut: (projectId: string, editId: string) => runCore(["cut", "preview", projectId, editId]),
  createWordCut: (projectId: string, segmentId: string, fromWordId: string, toWordId: string, paddingMs: number) => runCore(["cut", "create", projectId, "--segment", segmentId, "--from-word", fromWordId, "--to-word", toWordId, "--padding-ms", String(paddingMs)]),
};
