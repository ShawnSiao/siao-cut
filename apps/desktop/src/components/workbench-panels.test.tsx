import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RuntimeInfo } from "../types";
import { RuntimeChecklist, SegmentRow } from "./workbench-panels";

afterEach(cleanup);

describe("SegmentRow input safety", () => {
  const segment = { id: "s1", start: 0, end: 2, text: "原始字幕", confidence: null };
  it("does not overwrite a dirty input during refresh", () => {
    const props = { segment, selected: true, active: true, onSelect: vi.fn(), onSave: vi.fn(), onSplitAt: vi.fn(), onMergePrevious: vi.fn() };
    const { rerender } = render(<SegmentRow {...props}/>);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "还在编辑的草稿" } });
    rerender(<SegmentRow {...props} segment={{ ...segment, text: "后台刷新内容" }}/>);
    expect(screen.getByRole("textbox")).toHaveValue("还在编辑的草稿");
  });
  it("ignores save/split/merge keys while an IME composition is active", () => {
    const onSave = vi.fn(), onSplitAt = vi.fn(), onMergePrevious = vi.fn();
    render(<SegmentRow segment={segment} selected active onSelect={vi.fn()} onSave={onSave} onSplitAt={onSplitAt} onMergePrevious={onMergePrevious}/>);
    const input = screen.getByRole("textbox") as HTMLTextAreaElement;
    fireEvent.compositionStart(input); input.setSelectionRange(2, 2);
    fireEvent.keyDown(input, { key: "Enter", isComposing: true });
    fireEvent.keyDown(input, { key: "s", ctrlKey: true }); input.setSelectionRange(0, 0);
    fireEvent.keyDown(input, { key: "Backspace" });
    expect(onSave).not.toHaveBeenCalled(); expect(onSplitAt).not.toHaveBeenCalled(); expect(onMergePrevious).not.toHaveBeenCalled();
  });
});

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
