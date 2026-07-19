import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { mockRun } from "./core.mock";
import type { CoreEnvelope, Project, RuntimeInfo, UpdateDownloadEvent, UpdateMetadata, UpdatePolicy } from "./types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

function ensureOk(envelope: CoreEnvelope): CoreEnvelope {
  if (envelope.status === "error") {
    throw new Error(envelope.error?.message ?? envelope.message ?? "Core 请求失败");
  }
  return envelope;
}
export async function runCore(args: string[]): Promise<CoreEnvelope> {
  return ensureOk(isTauri() ? await invoke<CoreEnvelope>("run_core", { args }) : await mockRun(args));
}

export async function runtimeInfo(): Promise<RuntimeInfo> {
  if (!isTauri()) return {
    corePath: "浏览器预览模式",
    coreApiVersion: "0.1",
    ffmpegConfigured: true,
    asrConfigured: true,
    vadConfigured: true,
    ytDlpConfigured: true,
    asrBackend: "cpu",
    asrDevice: null,
    availableAsrBackends: ["cpu", "vulkan"],
    ffmpegPath: "内置运行时\\ffmpeg.exe",
    whisperPath: "内置运行时\\whisper-cli.exe",
    ytDlpPath: "内置运行时\\yt-dlp.exe",
    runtimeManifestPath: "内置运行时\\runtime-manifest.json",
    defaultModelPath: "本地模型目录",
    defaultModelAvailable: true,
    logDirectory: "本机诊断日志目录",
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
    disabledReason: "浏览器预览不连接更新源。",
  };
  return invoke<UpdatePolicy>("update_policy");
}

export async function checkForUpdate(): Promise<UpdateMetadata | null> {
  if (!isTauri()) return null;
  return invoke<UpdateMetadata | null>("check_for_update");
}

export async function installUpdate(onEvent: (event: UpdateDownloadEvent) => void): Promise<void> {
  if (!isTauri()) throw new Error("浏览器预览不能安装更新。");
  const channel = new Channel<UpdateDownloadEvent>();
  channel.onmessage = onEvent;
  return invoke<void>("install_update", { onEvent: channel });
}

export async function listProjects(): Promise<Project[]> {
  return (await runCore(["project", "list"])).projects ?? [];
}

export async function loadProject(projectId: string): Promise<Project> {
  const project = (await runCore(["project", "show", projectId])).project;
  if (!project) throw new Error("Core 未返回项目数据");
  return project;
}

export async function pickMedia(): Promise<string | null> {
  if (!isTauri()) return "demo.mp4";
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: "音视频", extensions: ["mp4", "mov", "mkv", "mp3", "m4a", "wav"] }],
  });
}

export async function pickSubtitleFile(): Promise<string | null> {
  if (!isTauri()) return "demo.srt";
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: "字幕文件", extensions: ["srt", "vtt", "ass", "ssa"] }],
  });
}

export async function pickTranscriptPath(title: string, format: "srt" | "vtt" | "ass" | "markdown"): Promise<string | null> {
  const options = {
    srt: { extension: "srt", name: "SubRip 字幕" },
    vtt: { extension: "vtt", name: "WebVTT 字幕" },
    ass: { extension: "ass", name: "ASS 字幕" },
    markdown: { extension: "md", name: "Markdown 文稿" },
  }[format];
  if (!isTauri()) return `${title}.${options.extension}`;
  return save({ defaultPath: `${title}.${options.extension}`, filters: [{ name: options.name, extensions: [options.extension] }] });
}

export async function pickVideoPath(title: string): Promise<string | null> {
  if (!isTauri()) return `${title}.mp4`;
  return save({ defaultPath: `${title}.mp4`, filters: [{ name: "MP4 视频", extensions: ["mp4"] }] });
}

export async function pickModel(): Promise<string | null> {
  if (!isTauri()) return "C:\\Models\\ggml-model.bin";
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: "whisper.cpp 模型", extensions: ["bin", "gguf"] }],
  });
}

export async function authorizeMedia(projectId: string): Promise<string | null> {
  if (!isTauri()) return null;
  const path = await invoke<string>("authorize_media", { projectId });
  return convertFileSrc(path);
}

export async function authorizeArtifact(projectId: string, kind: "preview" | "waveform"): Promise<string | null> {
  if (!isTauri()) return null;
  const path = await invoke<string | null>("authorize_artifact", { projectId, kind });
  return path ? convertFileSrc(path) : null;
}
