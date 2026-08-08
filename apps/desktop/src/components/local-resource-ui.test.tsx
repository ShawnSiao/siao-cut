import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { changeUiLocale } from "../i18n";
import type { LocalResourcePlan, LocalResourceStatus } from "../types";
import { LocalResourcePanel, LocalResourceSetupDialog } from "./local-resource-ui";

const unconfiguredStatus: LocalResourceStatus = {
  configured: false,
  root: null,
  rootAvailable: false,
  writable: false,
  availableBytes: null,
  transcriptionProfile: "standard",
  capabilities: [
    { id: "basic_media", name: "basic_media", state: "not_ready", enabled: false },
    { id: "url_import", name: "url_import", state: "not_ready", enabled: false },
    { id: "local_transcription", name: "local_transcription", state: "not_ready", enabled: false },
    { id: "speaker_identity", name: "speaker_identity", state: "not_ready", enabled: false },
  ],
  needsSetup: true,
};

const plan: LocalResourcePlan = {
  capabilityId: "basic_media",
  capabilityName: "basic_media",
  transcriptionProfile: null,
  downloadBytes: 70_510_962,
  unknownSize: false,
};

afterEach(() => {
  cleanup();
  changeUiLocale("zh-CN");
});

describe("local resource setup", () => {
  it("requires an explicitly selected and confirmed directory before preparation", () => {
    const onChooseLocation = vi.fn();
    const onConfirmLocation = vi.fn();
    const onStart = vi.fn();
    const common = {
      reason: "first_run" as const,
      capability: "basic_media" as const,
      plan,
      job: null,
      busy: false,
      error: null,
      onClose: vi.fn(),
      onChooseLocation,
      onConfirmLocation,
      onStart,
      onCancel: vi.fn(),
      onResume: vi.fn(),
      onDefer: vi.fn(),
    };
    const view = render(<LocalResourceSetupDialog {...common} status={unconfiguredStatus} selectedRoot=""/>);

    expect(screen.getByText(/产品安装包不包含第三方运行时或模型/)).toBeInTheDocument();
    expect(screen.getByText(/不会自动把大文件写入 C 盘/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "准备推荐资源" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "选择文件夹" }));
    expect(onChooseLocation).toHaveBeenCalledOnce();

    view.rerender(<LocalResourceSetupDialog {...common} status={unconfiguredStatus} selectedRoot={"E:\\SiaoCut Resources"}/>);
    expect(screen.getByRole("button", { name: "准备推荐资源" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "确认此位置" }));
    expect(onConfirmLocation).toHaveBeenCalledOnce();

    const configured = { ...unconfiguredStatus, configured: true, root: "E:\\SiaoCut Resources", rootAvailable: true, writable: true, needsSetup: false };
    view.rerender(<LocalResourceSetupDialog {...common} status={configured} selectedRoot={"E:\\SiaoCut Resources"}/>);
    const start = screen.getByRole("button", { name: "准备推荐资源" });
    expect(start).toBeEnabled();
    fireEvent.click(start);
    expect(onStart).toHaveBeenCalledOnce();
  });

  it("shows product capabilities without raw component identities", () => {
    const ready = {
      ...unconfiguredStatus,
      configured: true,
      root: "D:\\SiaoCut Resources",
      rootAvailable: true,
      writable: true,
      needsSetup: false,
      capabilities: unconfiguredStatus.capabilities.map((capability) => ({ ...capability, state: "ready" as const, enabled: true })),
    };
    render(<LocalResourcePanel status={ready} job={null} busy={false} onPrepare={vi.fn()} onChangeLocation={vi.fn()} onRemove={vi.fn()}/>);

    expect(screen.getByText("基础媒体处理")).toBeInTheDocument();
    expect(screen.getByText("URL 导入")).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent(/FFmpeg|FFprobe|yt-dlp|whisper\.cpp|SIAOCUT_|SHA-?256/);
  });
});
