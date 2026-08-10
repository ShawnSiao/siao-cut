import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import App from "./App";
import { speakerStageLabel, versionReasonLabel } from "./app-view-model";
import { SpeakerPackageManager } from "./components/workbench-panels";
import { changeUiLocale } from "./i18n";
import type { SpeakerJob } from "./types";

afterEach(() => {
  cleanup();
  changeUiLocale("zh-CN");
});

describe("App locale switching", () => {
  it("switches the application chrome to English without changing project content", async () => {
    changeUiLocale("zh-CN");
    render(<App />);

    expect(await screen.findByRole("button", { name: "新建项目" })).toBeInTheDocument();
    const projectHeading = await screen.findByRole("heading", { name: "发布口播 · 草稿" });

    fireEvent.change(screen.getByRole("combobox", { name: "界面语言" }), {
      target: { value: "en-US" },
    });

    await waitFor(() => expect(screen.getByRole("button", { name: "New project" })).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Review suggestions" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start AI assistance" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Export" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument();
    expect(screen.getByText("4 subtitles")).toBeInTheDocument();
    expect(screen.getByText("1 subtitle")).toBeInTheDocument();
    expect(screen.getByText("ZH · 4 segments · 5 words")).toBeInTheDocument();
    expect(projectHeading).toHaveTextContent("发布口播 · 草稿");
    expect(document.documentElement.lang).toBe("en-US");

    fireEvent.click(screen.getByRole("button", { name: "Local resources" }));
    expect(await screen.findByRole("dialog", { name: "Local resources" })).toHaveTextContent("Browser preview is not connected to an update source.");
  });

  it("localizes Core-authored version reasons and speaker stages in English chrome", () => {
    changeUiLocale("en-US");

    expect(versionReasonLabel("生成说话人轨")).toBe("Generated speaker track");
    expect(versionReasonLabel("批量偏移字幕 +0.125 秒")).toBe("Offset subtitles by +0.125 seconds");
    expect(speakerStageLabel("本地识别说话人")).toBe("Identifying speakers locally");
  });

  it("localizes the speaker package install stage instead of rendering Core Chinese", () => {
    changeUiLocale("en-US");
    const job = {
      status: "running",
      stage: "下载组件 1/3",
      progress: 0.25,
      bytesDownloaded: 25,
      totalBytes: 100,
    } as SpeakerJob;

    render(<SpeakerPackageManager packageStatus={null} job={job} disabled={false} onInstall={() => undefined} onCancel={() => undefined} onResume={() => undefined}/>);

    expect(screen.getByText(/Downloading component 1\/3/)).toBeInTheDocument();
    expect(screen.queryByText(/下载组件/)).not.toBeInTheDocument();
  });
});
