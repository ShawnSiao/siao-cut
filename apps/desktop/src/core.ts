import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { tr } from "./i18n";
import type { CoreEnvelope, Project, RuntimeInfo, UpdateDownloadEvent, UpdateMetadata, UpdatePolicy } from "./types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

function ensureOk(envelope: CoreEnvelope): CoreEnvelope {
  if (envelope.status === "error") {
    throw new Error(envelope.error?.message ?? envelope.message ?? tr("app.core.requestFailed"));
  }
  return envelope;
}

async function runMockCore(args: string[]): Promise<CoreEnvelope> {
  const { mockRun } = await import("./core.mock");
  return mockRun(args);
}

export async function runCore(args: string[]): Promise<CoreEnvelope> {
  return ensureOk(isTauri() ? await invoke<CoreEnvelope>("run_core", { args }) : await runMockCore(args));
}

export type StructuredCoreRequest =
  | { kind: "transcript_offset"; projectId: string; segmentIds: string[]; delta: number }
  | { kind: "transcription_start"; projectId: string; language: "auto" | "en" | "zh"; prompt?: string; hotwords: string[] };

function expandStructuredCoreRequest(request: StructuredCoreRequest): string[] {
  if (request.kind === "transcript_offset") {
    return ["transcript", "offset", request.projectId, ...request.segmentIds.flatMap((segmentId) => ["--segment", segmentId]), "--delta", String(request.delta)];
  }
  return [
    "transcription", "start", request.projectId,
    "--language", request.language,
    ...(request.prompt ? ["--prompt", request.prompt] : []),
    ...request.hotwords.flatMap((hotword) => ["--hotword", hotword]),
  ];
}

export function structuredCoreErrorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  const code = message.split(":", 1)[0];
  return ({
    structured_core_payload_too_large: tr("app.core.structuredTooLarge"),
    structured_core_payload_invalid: tr("app.core.structuredInvalidJson"),
    structured_core_request_invalid: tr("app.core.structuredInvalid"),
    structured_core_request_file_failed: tr("app.core.structuredFileFailed"),
  } as Record<string, string>)[code] ?? message;
}

export async function runCoreStructured(request: StructuredCoreRequest): Promise<CoreEnvelope> {
  try {
    const envelope = isTauri()
      ? await invoke<CoreEnvelope>("run_core_structured", { payload: JSON.stringify(request) })
      : await runMockCore(expandStructuredCoreRequest(request));
    return ensureOk(envelope);
  } catch (error) {
    throw new Error(structuredCoreErrorMessage(error));
  }
}

export async function runAiRequest<T extends object>(request: T): Promise<CoreEnvelope> {
  if (!isTauri()) {
    const { mockAiRequest } = await import("./features/environment-settings/mock-ai-settings");
    return ensureOk(await mockAiRequest(request));
  }
  return ensureOk(await invoke<CoreEnvelope>("run_ai_request", { payload: JSON.stringify(request) }));
}

export async function localFileAvailable(path: string): Promise<boolean> {
  if (!path.trim()) return false;
  if (!isTauri()) return true;
  return invoke<boolean>("local_file_available", { path });
}

export async function runtimeInfo(): Promise<RuntimeInfo> {
  if (!isTauri()) return {
    corePath: tr("app.preview.mode"),
    coreApiVersion: "0.1",
    ffmpegConfigured: true,
    asrConfigured: true,
    vadConfigured: true,
    vadTimelineVerified: true,
    vadStatus: "verified",
    vadReasonCode: null,
    ytDlpConfigured: true,
    asrBackend: "cpu",
    asrDevice: null,
    availableAsrBackends: ["cpu", "vulkan"],
    ffmpegPath: `${tr("app.runtime.external")}\\ffmpeg.exe`,
    whisperPath: `${tr("app.runtime.external")}\\whisper-cli.exe`,
    ytDlpPath: `${tr("app.runtime.external")}\\yt-dlp.exe`,
    runtimeManifestPath: `${tr("app.runtime.external")}\\runtime-manifest.json`,
    defaultModelPath: tr("app.runtime.localModelDirectory"),
    defaultModelAvailable: true,
    logDirectory: tr("app.runtime.localDiagnosticsDirectory"),
    diagnosticsAvailable: true,
  };
  return invoke<RuntimeInfo>("runtime_info");
}

export async function openLogDirectory(): Promise<void> {
  if (!isTauri()) return;
  return invoke<void>("open_log_directory");
}

export async function selectAsrBackend(backend: "cpu" | "vulkan"): Promise<RuntimeInfo> {
  if (!isTauri()) return { ...(await runtimeInfo()), asrBackend: backend, asrDevice: backend === "vulkan" ? "NVIDIA GeForce GTX 1660 SUPER" : null };
  return invoke<RuntimeInfo>("select_asr_backend", { backend });
}

export async function updaterPolicy(): Promise<UpdatePolicy> {
  if (!isTauri()) return {
    currentVersion: "0.2.0-preview",
    enabled: false,
    automaticCheckIntervalHours: 24,
    disabledReason: tr("app.update.previewDisabled"),
  };
  return invoke<UpdatePolicy>("update_policy");
}

export async function checkForUpdate(): Promise<UpdateMetadata | null> {
  if (!isTauri()) return null;
  return invoke<UpdateMetadata | null>("check_for_update");
}

export async function installUpdate(onEvent: (event: UpdateDownloadEvent) => void): Promise<void> {
  if (!isTauri()) throw new Error(tr("app.preview.installUpdateUnavailable"));
  const channel = new Channel<UpdateDownloadEvent>();
  channel.onmessage = onEvent;
  return invoke<void>("install_update", { onEvent: channel });
}

export async function listProjects(): Promise<Project[]> {
  return (await runCore(["project", "list"])).projects ?? [];
}

export async function loadProject(projectId: string): Promise<Project> {
  const project = (await runCore(["project", "show", projectId])).project;
  if (!project) throw new Error(tr("app.core.projectMissing"));
  return project;
}

export async function pickMedia(): Promise<string | null> {
  if (!isTauri()) return "demo.mp4";
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: tr("app.dialog.mediaFiles"), extensions: ["mp4", "mov", "mkv", "mp3", "m4a", "wav"] }],
  });
}

export async function pickResourceDirectory(): Promise<string | null> {
  if (!isTauri()) return "D:\\SiaoCut Resources";
  return open({
    multiple: false,
    directory: true,
    title: tr("app.resources.chooseLocation"),
  });
}

export async function pickSubtitleFile(): Promise<string | null> {
  if (!isTauri()) return "demo.srt";
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: tr("app.dialog.subtitleFiles"), extensions: ["srt", "vtt", "ass", "ssa"] }],
  });
}

export function sanitizeWindowsFileName(value: string): string {
  const withoutInvalidCharacters = value
    .normalize("NFKC")
    .replace(/[<>:"/\\|?*\u0000-\u001F]/gu, "_")
    .replace(/\s+/gu, " ")
    .replace(/[ .]+$/gu, "")
    .trim();
  const truncated = Array.from(withoutInvalidCharacters).slice(0, 120).join("");
  if (!truncated || /^\.+$/u.test(truncated)) return tr("app.file.untitled");
  return /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/iu.test(truncated) ? `_${truncated}` : truncated;
}

export async function pickTranscriptPath(title: string, format: "srt" | "vtt" | "ass" | "markdown" | "json"): Promise<string | null> {
  const options = {
    srt: { extension: "srt", name: tr("app.dialog.subripSubtitle") },
    vtt: { extension: "vtt", name: tr("app.dialog.webvttSubtitle") },
    ass: { extension: "ass", name: tr("app.dialog.assSubtitle") },
    markdown: { extension: "md", name: tr("app.dialog.markdownTranscript") },
    json: { extension: "json", name: tr("app.dialog.structuredTranscript") },
  }[format];
  const safeTitle = sanitizeWindowsFileName(title);
  if (!isTauri()) return `${safeTitle}.${options.extension}`;
  return save({ defaultPath: `${safeTitle}.${options.extension}`, filters: [{ name: options.name, extensions: [options.extension] }] });
}

export async function pickVideoPath(title: string, delivery: "burned" | "embedded-mp4" | "embedded-mkv" | "sidecar-srt" | "sidecar-vtt" = "burned"): Promise<string | null> {
  const safeTitle = sanitizeWindowsFileName(title);
  const extension = delivery === "embedded-mkv" ? "mkv" : "mp4";
  const name = extension === "mkv" ? tr("app.dialog.mkvVideo") : tr("app.dialog.mp4Video");
  if (!isTauri()) return `${safeTitle}.${extension}`;
  return save({ defaultPath: `${safeTitle}.${extension}`, filters: [{ name, extensions: [extension] }] });
}

export async function pickModel(): Promise<string | null> {
  if (!isTauri()) return "C:\\Models\\ggml-model.bin";
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: tr("app.dialog.whisperModel"), extensions: ["bin", "gguf"] }],
  });
}

export async function authorizeMedia(projectId: string): Promise<string | null> {
  if (!isTauri()) {
    const { mockAuthorizeMedia } = await import("./core.mock");
    return mockAuthorizeMedia();
  }
  const path = await invoke<string>("authorize_media", { projectId });
  return convertFileSrc(path);
}

export async function authorizeArtifact(projectId: string, kind: "preview" | "waveform"): Promise<string | null> {
  if (!isTauri()) return null;
  const path = await invoke<string | null>("authorize_artifact", { projectId, kind });
  return path ? convertFileSrc(path) : null;
}
