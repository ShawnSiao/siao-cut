import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RuntimeInfo } from "../types";
import { RuntimeChecklist } from "./workbench-panels";

afterEach(cleanup);

function runtimeInfo(overrides: Partial<RuntimeInfo> = {}): RuntimeInfo {
  return {
    corePath: "core.exe",
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
    ffmpegPath: "ffmpeg.exe",
    whisperPath: "whisper-cli.exe",
    ytDlpPath: "yt-dlp.exe",
    runtimeManifestPath: "runtime-manifest.json",
    defaultModelPath: "model.bin",
    defaultModelAvailable: true,
    logDirectory: "logs",
    diagnosticsAvailable: true,
    ...overrides,
  };
}

describe("RuntimeChecklist", () => {
  it("distinguishes a verified VAD timeline from an installed model", () => {
    const { rerender } = render(
      <RuntimeChecklist
        runtime={runtimeInfo()}
        modelPath="model.bin"
        onChooseModel={vi.fn()}
      />,
    );

    expect(screen.getByText(/VAD 时间轴已验证/)).toBeInTheDocument();
    expect(screen.queryByText(/无 VAD 安全回退/)).not.toBeInTheDocument();

    rerender(
      <RuntimeChecklist
        runtime={runtimeInfo({
          vadTimelineVerified: false,
          vadStatus: "safe_fallback",
          vadReasonCode: "vad_metadata_missing",
        })}
        modelPath="model.bin"
        onChooseModel={vi.fn()}
      />,
    );

    expect(screen.getByText(/无 VAD 安全回退/)).toBeInTheDocument();
    expect(screen.queryByText(/VAD 时间轴已验证/)).not.toBeInTheDocument();
  });
});
