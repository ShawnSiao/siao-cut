import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import App, { AUTO_WORKFLOW_DISMISSED_STORAGE_KEY, PatchReviewCard, TRANSCRIPTION_LANGUAGE_STORAGE_KEY, agentTaskStatusLabel, clearTransientCoreError, getProjectCapabilities, isHttpsSourceUrl, parseDismissedAutoWorkflowIds, parseExportPreferences, parseTranscriptionLanguage, resolveCanvasMedia, resolveCaptionKaraokeStyle, resolveCaptionSegment, resolveImportedProjectMedia, resolvePlaybackDuration, shouldCheckForUpdates, startSerialPolling, taskLabel, upsertAutoWorkflowSnapshot } from "./App";
import { mockRun, resetMockLocalResourcesForTest, resetMockProjectForTest, setMockAuthorizedMediaForTest, setMockLocalResourcesForTest, setMockProjectForTest } from "./core.mock";
import { agentReviewClient } from "./domains/agent-review-client";
import { projectSessionClient } from "./domains/project-session-client";
import { transcriptEditingClient } from "./domains/transcript-editing-client";
import { sampleProject } from "./mock";
import type { AutoWorkflow } from "./types";
import { TIMELINE_PREFERENCES_STORAGE_KEY } from "./workbench/subtitle-timeline-panel";

afterEach(() => {
  cleanup();
  localStorage.removeItem("siaocut.exportPreferences.v1");
  localStorage.removeItem(TRANSCRIPTION_LANGUAGE_STORAGE_KEY);
  localStorage.removeItem("siaocut.transcriptionMode");
  localStorage.removeItem(AUTO_WORKFLOW_DISMISSED_STORAGE_KEY);
  localStorage.removeItem(TIMELINE_PREFERENCES_STORAGE_KEY);
  localStorage.removeItem("siaocut.localResourcesSetupDeferred.v1");
  resetMockLocalResourcesForTest();
  resetMockProjectForTest();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

async function openDrawerTab(name: "审阅" | "质量" | "分析" | "历史" | "导出") {
  const tab = await screen.findByRole("tab", { name: new RegExp(`^${name}`) });
  fireEvent.click(tab);
  return tab;
}

async function selectAdvancedTranscriptionMode(mode: "quick" | "multispeaker") {
  fireEvent.click(await screen.findByRole("button", { name: "本地资源" }));
  const dialog = await screen.findByRole("dialog", { name: "本地资源" });
  openResourceDiagnostics(dialog);
  fireEvent.change(within(dialog).getByRole("combobox", { name: "转写模式" }), { target: { value: mode } });
  return dialog;
}

function openResourceDiagnostics(dialog: HTMLElement) {
  const summary = within(dialog).getByText("兼容与诊断").closest("summary");
  if (!summary) throw new Error("local resource diagnostics summary is missing");
  fireEvent.click(summary);
}

function autoWorkflowFixture(overrides: Partial<AutoWorkflow> = {}): AutoWorkflow {
  return {
    id: "auto-regression",
    inputKind: "local",
    inputValue: "demo.mp4",
    title: "回归测试",
    confirmedMediaId: null,
    projectId: null,
    sourceImportId: null,
    modelPath: "model.bin",
    transcribeLanguage: "zh",
    translationLanguage: null,
    outputPath: "output.mp4",
    burnSubtitles: true,
    subtitleMode: "source",
    status: "running",
    currentStage: "transcribe",
    progress: 0.5,
    transcriptVersionId: null,
    agentTaskId: null,
    exportJobId: null,
    audit: null,
    cancelRequestedAt: null,
    errorMessage: null,
    createdAt: "2026-07-25T10:00:00.000Z",
    updatedAt: "2026-07-25T10:01:00.000Z",
    completedAt: null,
    attemptCount: 1,
    instructionLocale: "zh-CN",
    ...overrides,
  };
}

describe("SiaoCut review workbench", () => {
  it("falls back to source media when a stale canvas preview cannot be authorized", async () => {
    const result = await resolveCanvasMedia(
      "p-test",
      async () => { throw new Error("preview stale"); },
      async () => "asset://source.mp4",
    );

    expect(result).toEqual({ mediaUrl: "asset://source.mp4", warning: "preview stale" });
  });

  it("keeps a completed URL import successful when optional preview assets are unavailable", async () => {
    const result = await resolveImportedProjectMedia(
      "p-test",
      async (_projectId, kind) => {
        throw new Error(`${kind} unavailable`);
      },
      async () => "asset://source.mp4",
    );

    expect(result).toEqual({
      mediaUrl: "asset://source.mp4",
      waveformUrl: null,
      warning: "preview unavailable; waveform unavailable",
    });
  });

  it("keeps playback captions visibly filled while applying karaoke progress", () => {
    const style = resolveCaptionKaraokeStyle(true, 0.25, "#F2F4F5", "#B5BEC6") as Record<string, string>;

    expect(style).toMatchObject({
      color: "#B5BEC6",
      "--caption-progress": "25%",
      "--caption-primary-color": "#F2F4F5",
    });
    expect(style.color).not.toBe("transparent");
    expect(style).not.toHaveProperty("backgroundImage");
    expect(resolveCaptionKaraokeStyle(false, 0.25, "#F2F4F5", "#B5BEC6")).toBeUndefined();
  });

  it("keeps the subtitle at the paused playhead instead of reverting to the first selection", () => {
    const first = { id: "first", start: 0, end: 2, text: "First", confidence: null };
    const current = { id: "current", start: 4, end: 7, text: "Current", confidence: null };

    expect(resolveCaptionSegment([first, current], first, 5, false)).toBe(current);
    expect(resolveCaptionSegment([first, current], first, 3, false)).toBe(first);
    expect(resolveCaptionSegment([first, current], first, 3, true)).toBeNull();
  });

  it("persists source language independently and creates the selected Agent workflow", async () => {
    render(<App />);
    const newProject = await screen.findByRole("button", { name: "新建项目" });
    const agentButton = screen.getByRole("button", { name: "交给本机 Codex" });
    expect(agentButton).toBeDisabled();
    expect(agentButton).toHaveAttribute("title", "请先导入或重新定位本地媒体。");
    fireEvent.click(newProject);
    await waitFor(() => expect(agentButton).toBeEnabled());

    const runtime = await selectAdvancedTranscriptionMode("quick");
    fireEvent.change(within(runtime).getByRole("combobox", { name: "素材语言" }), { target: { value: "en" } });
    expect(localStorage.getItem(TRANSCRIPTION_LANGUAGE_STORAGE_KEY)).toBe("en");
    expect(parseTranscriptionLanguage("unsupported")).toBe("auto");
    fireEvent.click(within(runtime).getByRole("button", { name: "关闭本地资源" }));

    fireEvent.change(screen.getByRole("combobox", { name: "Agent 工作流" }), { target: { value: "edit" } });
    fireEvent.click(screen.getByRole("button", { name: "手工交接" }));
    expect(await screen.findByRole("dialog", { name: "交给外部 Agent" })).toHaveTextContent("普通网页聊天不会自动执行任务");
    fireEvent.click(screen.getByRole("checkbox", { name: "我会在可访问本机 SiaoCut Core 的外部 Agent 工具中继续执行。" }));
    fireEvent.click(screen.getByRole("button", { name: "创建交接任务" }));
    await waitFor(() => expect(screen.getByRole("heading", { name: "任务已准备好交接" })).toBeInTheDocument());
    expect(screen.getByText("精简工作流已创建，需要 Agent 继续。媒体文件不会交给 Agent。")).toBeInTheDocument();
    const handoff = (screen.getByRole("textbox", { name: "复制给外部 Agent 的完整说明" }) as HTMLTextAreaElement).value;
    expect(handoff).toContain("task claim");
    expect(handoff).toContain("--payload-output $payloadPath");
    expect(handoff).toContain("Get-FileHash -LiteralPath $payloadPath -Algorithm SHA256");
    expect(handoff).toContain("Get-Content -Raw -Encoding UTF8 -LiteralPath $payloadPath");
    expect(handoff).toContain("$leaseId = [string]$payload.leaseId");
    expect(handoff).toContain("$heartbeatProgress = [Math]::Max(0.05, [double]$claim.task.progress)");
    expect(handoff).toContain("task heartbeat");
    expect(handoff).toContain("--lease-id $leaseId");
    expect(handoff).not.toContain("payload.segments");

    fireEvent.click(screen.getByRole("button", { name: "一键成片" }));
    expect(screen.getByRole("combobox", { name: "素材语言 · 一键成片" })).toHaveValue("en");
  });

  it("removes a manually handed-off task immediately after cancellation", async () => {
    render(<App />);
    const cancel = await screen.findByRole("button", { name: "取消任务" });

    fireEvent.click(cancel);

    await waitFor(() => expect(screen.getByText("任务已取消。")).toBeInTheDocument());
    expect(screen.queryByRole("button", { name: "取消任务" })).not.toBeInTheDocument();
  });

  it("requeues a failed Agent task once and shows when the same Agent claims it again", async () => {
    const failedProject = structuredClone(sampleProject);
    const failedTask = {
      ...failedProject.tasks[0],
      status: "failed" as const,
      progress: 0.05,
      attemptCount: 1,
      errorMessage: "完整任务文本未保存",
      lastActivity: {
        kind: "failed",
        progress: null,
        message: "完整任务文本未保存",
        createdAt: "2026-07-25T15:48:31.000Z",
      },
    };
    failedProject.tasks = [failedTask];
    failedProject.patchSets = [];
    failedProject.edits = [];
    const queuedTask = {
      ...failedTask,
      status: "queued" as const,
      progress: 0,
      errorMessage: null,
      lease: null,
      lastActivity: {
        kind: "queued",
        progress: 0,
        message: "任务已重新排队",
        createdAt: "2026-07-25T15:48:34.000Z",
      },
    };
    const claimedProject = structuredClone(failedProject);
    claimedProject.tasks = [{
      ...queuedTask,
      status: "claimed",
      attemptCount: 2,
      lease: {
        worker: "named-agent",
        id: "lease-2",
        expiresAt: "2026-07-25T15:53:41.000Z",
      },
      lastActivity: {
        kind: "claimed",
        progress: null,
        message: "Agent 已领取任务",
        createdAt: "2026-07-25T15:48:41.000Z",
      },
    }];
    const staleInterruptedProject = structuredClone(failedProject);
    staleInterruptedProject.tasks = [{
      ...failedTask,
      status: "interrupted",
      errorMessage: "旧轮询快照",
      lastActivity: {
        kind: "interrupted",
        progress: null,
        message: "旧轮询快照",
        createdAt: "2026-07-25T15:48:32.000Z",
      },
    }];
    let resolveStaleReload!: (value: typeof staleInterruptedProject) => void;
    const staleReload = new Promise<typeof staleInterruptedProject>((resolve) => {
      resolveStaleReload = resolve;
    });
    vi.spyOn(projectSessionClient, "listProjects").mockResolvedValue([failedProject]);
    const load = vi.spyOn(projectSessionClient, "loadProject")
      .mockImplementationOnce(() => staleReload)
      .mockResolvedValue(claimedProject);
    vi.spyOn(agentReviewClient, "listAgentRuns").mockResolvedValue({
      apiVersion: "0.1",
      status: "ok",
      agentRuns: [],
    });
    const update = vi.spyOn(agentReviewClient, "updateTask").mockResolvedValue({
      apiVersion: "0.1",
      status: "ok",
      task: queuedTask,
    });

    render(<App />);
    const retry = await screen.findByRole("button", { name: "重新排队" });
    fireEvent.click(retry);
    fireEvent.click(retry);

    await waitFor(() => expect(update).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/正在等待第 2 次领取/)).toBeInTheDocument();
    expect(screen.getByText("任务已重新排队")).toBeInTheDocument();

    const handoffTrigger = screen.getByRole("button", { name: "复制交接说明" });
    fireEvent.click(handoffTrigger);
    const handoffDialog = await screen.findByRole("dialog", { name: "交给外部 Agent" });
    expect(within(handoffDialog).getByRole("textbox", { name: /本次 Agent 标识/ })).toBeEnabled();

    await waitFor(() => expect(load).toHaveBeenCalledTimes(2), { timeout: 4_000 });
    expect(await screen.findByText(/named-agent 已领取任务/)).toBeInTheDocument();
    expect(screen.getByText(/第 2 次尝试/)).toBeInTheDocument();

    const handoff = (within(handoffDialog).getByRole("textbox", { name: "复制给外部 Agent 的完整说明" }) as HTMLTextAreaElement).value;
    const lockedIdentity = within(handoffDialog).getByRole("textbox", { name: /本次 Agent 标识/ });
    expect(lockedIdentity).toBeDisabled();
    expect(lockedIdentity).toHaveValue("named-agent");
    expect(handoff).toContain("task claim t1 --worker named-agent --lease-id lease-2 --payload-output $payloadPath");
    expect(handoff).toContain("task heartbeat t1 --worker named-agent --lease-id $leaseId");
    expect(handoff).toContain("task submit t1 --worker named-agent --lease-id $leaseId");
    fireEvent.click(within(handoffDialog).getByRole("button", { name: "关闭 Agent 交接" }));
    await waitFor(() => expect(handoffTrigger).toHaveFocus());

    resolveStaleReload(staleInterruptedProject);
    await waitFor(() => expect(screen.getByText(/named-agent 已领取任务/)).toBeInTheDocument());
    expect(screen.queryByText("旧轮询快照")).not.toBeInTheDocument();
  });

  it("keeps polling a failed external task until a CLI retry and claim become visible", async () => {
    const failedProject = structuredClone(sampleProject);
    const failedTask = {
      ...failedProject.tasks[0],
      status: "failed" as const,
      progress: 0.05,
      attemptCount: 1,
      errorMessage: "外部 Agent 暂时报告失败",
      lastActivity: {
        kind: "failed",
        progress: null,
        message: "外部 Agent 暂时报告失败",
        createdAt: "2026-07-25T15:48:31.000Z",
      },
    };
    failedProject.tasks = [failedTask];
    failedProject.patchSets = [];
    failedProject.edits = [];
    const claimedProject = structuredClone(failedProject);
    claimedProject.tasks = [{
      ...failedTask,
      status: "claimed",
      progress: 0,
      errorMessage: null,
      attemptCount: 2,
      lease: {
        worker: "cli-agent",
        id: "lease-cli-2",
        expiresAt: "2026-07-25T15:53:41.000Z",
      },
      lastActivity: {
        kind: "claimed",
        progress: null,
        message: "Agent 已领取任务",
        createdAt: "2026-07-25T15:48:41.000Z",
      },
    }];
    vi.spyOn(projectSessionClient, "listProjects").mockResolvedValue([failedProject]);
    const load = vi.spyOn(projectSessionClient, "loadProject")
      .mockResolvedValue(claimedProject);
    vi.spyOn(agentReviewClient, "listAgentRuns").mockResolvedValue({
      apiVersion: "0.1",
      status: "ok",
      agentRuns: [],
    });

    render(<App />);
    expect(await screen.findByRole("button", { name: "重新排队" })).toBeInTheDocument();

    await waitFor(() => expect(load).toHaveBeenCalledTimes(1), { timeout: 7_000 });
    expect(await screen.findByText(/cli-agent 已领取任务/)).toBeInTheDocument();
    expect(screen.getByText(/第 2 次尝试/)).toBeInTheDocument();
    expect(screen.queryByText("外部 Agent 暂时报告失败")).not.toBeInTheDocument();
  }, 9_000);

  it("accepts a successful retry that an Agent reclaimed before Core returned", async () => {
    const failedProject = structuredClone(sampleProject);
    const failedTask = {
      ...failedProject.tasks[0],
      status: "failed" as const,
      attemptCount: 1,
      errorMessage: "上一轮失败",
      lastActivity: {
        kind: "failed",
        progress: null,
        message: "上一轮失败",
        createdAt: "2026-07-25T15:48:31.000Z",
      },
    };
    const claimedTask = {
      ...failedTask,
      status: "claimed" as const,
      progress: 0,
      errorMessage: null,
      attemptCount: 2,
      lease: {
        worker: "fast-agent",
        id: "lease-fast",
        expiresAt: "2026-07-25T15:53:41.000Z",
      },
      lastActivity: {
        kind: "claimed",
        progress: null,
        message: "Agent 已领取任务",
        createdAt: "2026-07-25T15:48:41.000Z",
      },
    };
    failedProject.tasks = [failedTask];
    failedProject.patchSets = [];
    failedProject.edits = [];
    const claimedProject = structuredClone(failedProject);
    claimedProject.tasks = [claimedTask];
    vi.spyOn(projectSessionClient, "listProjects").mockResolvedValue([failedProject]);
    vi.spyOn(projectSessionClient, "loadProject").mockResolvedValue(claimedProject);
    vi.spyOn(agentReviewClient, "listAgentRuns").mockResolvedValue({
      apiVersion: "0.1",
      status: "ok",
      agentRuns: [],
    });
    vi.spyOn(agentReviewClient, "updateTask").mockResolvedValue({
      apiVersion: "0.1",
      status: "ok",
      task: claimedTask,
    });

    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "重新排队" }));

    expect(await screen.findByText(/已由 fast-agent 领取；当前是第 2 次尝试/)).toBeInTheDocument();
    expect(screen.queryByText(/Core 未返回预期的任务状态/)).not.toBeInTheDocument();
    expect(screen.getByText(/fast-agent 已领取任务/)).toBeInTheDocument();
  });

  it("derives media, transcript, model, preview, and Agent capabilities from project state", () => {
    const withoutMedia = getProjectCapabilities(sampleProject, { agentWorkflowKind: "polish" });
    expect(withoutMedia).toMatchObject({
      hasProject: true,
      hasBoundMedia: false,
      hasTranscript: true,
      hasWordTiming: true,
      canRelinkMedia: true,
      canAnalyzeAudio: false,
      canPreparePreview: false,
      canExportVideo: false,
      canCreateAgentTask: false,
    });

    const withMedia = structuredClone(sampleProject);
    withMedia.media.sourcePath = "D:\\media\\clip.mp4";
    expect(getProjectCapabilities(withMedia, {
      mediaUrl: "blob:preview",
      modelPath: "D:\\models\\base.bin",
      translationTarget: "en",
      agentWorkflowKind: "translate",
    })).toMatchObject({
      hasBoundMedia: true,
      hasAuthorizedPreview: true,
      hasModel: true,
      hasTranslationTarget: true,
      canTranscribe: true,
      canAnalyzeAudio: true,
      canPreparePreview: true,
      canExportVideo: true,
      canCreateAgentTask: true,
    });
    expect(getProjectCapabilities(withMedia, {
      modelPath: "D:\\models\\missing.bin",
      modelAvailable: false,
    })).toMatchObject({
      hasModel: false,
      canTranscribe: false,
    });
  });

  it("accepts only HTTPS source URLs before inspection", () => {
    expect(isHttpsSourceUrl("https://example.com/video")).toBe(true);
    expect(isHttpsSourceUrl("http://example.com/video")).toBe(false);
    expect(isHttpsSourceUrl("not a url")).toBe(false);
  });

  it("closes the more-command menu when focus moves to another area", async () => {
    render(<App />);
    const more = await screen.findByRole("button", { name: "更多命令" });
    fireEvent.click(more);
    expect(await screen.findByRole("menu")).toBeInTheDocument();
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    fireEvent.click(more);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(more).toHaveFocus();
  });

  it("shows an explicit Agent translation target and reports the selected workflow", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "新建项目" }));
    const workflow = screen.getByRole("combobox", { name: "Agent 工作流" });
    await waitFor(() => expect(screen.getByRole("button", { name: "交给本机 Codex" })).toBeEnabled());
    fireEvent.change(workflow, { target: { value: "translate" } });
    const target = screen.getByRole("combobox", { name: "翻译目标语言" });
    expect(target).toHaveValue("en");
    fireEvent.change(target, { target: { value: "ja" } });
    fireEvent.click(screen.getByRole("button", { name: "手工交接" }));
    fireEvent.click(await screen.findByRole("checkbox", { name: "我会在可访问本机 SiaoCut Core 的外部 Agent 工具中继续执行。" }));
    fireEvent.click(screen.getByRole("button", { name: "创建交接任务" }));
    await waitFor(() => expect(screen.getByText(/翻译工作流已创建，目标语言为 JA/)).toBeInTheDocument());
    expect(screen.getAllByText("JA").length).toBeGreaterThan(0);
  });

  it("saves a versioned project glossary for the selected translation language", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "发布口播 · 草稿" });
    fireEvent.change(screen.getByRole("combobox", { name: "Agent 工作流" }), { target: { value: "translate" } });
    const glossary = screen.getByRole("textbox", { name: "项目术语表" });
    fireEvent.change(glossary, { target: { value: "本地优先=local-first\n工作台=workbench" } });
    fireEvent.click(screen.getByRole("button", { name: "保存新版本" }));
    expect(await screen.findByText(/术语表已保存为版本 1/)).toBeInTheDocument();
    expect(screen.getByText("版本 1")).toBeInTheDocument();
  });

  it("runs local Codex automatically and leaves the transcript unchanged until review", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "新建项目" }));
    const editor = await screen.findByLabelText("00:13 字幕文本");
    const original = (editor as HTMLTextAreaElement).value;
    const start = screen.getByRole("button", { name: "交给本机 Codex" });
    await waitFor(() => expect(start).toBeEnabled());

    fireEvent.click(start);
    expect(await screen.findByText("本机 Codex 已启动；完成后仍需逐条审阅。")).toBeInTheDocument();
    expect(screen.getByText(/本机 Agent 处理中|等待本机 Agent/)).toBeInTheDocument();

    await waitFor(() => expect(screen.getByText("本机 Codex 已完成；建议已进入集中审阅，文稿未自动修改。")).toBeInTheDocument(), { timeout: 5000 });
    expect(screen.getByLabelText("00:13 字幕文本")).toHaveValue(original);
    expect(screen.getByText(/本机 Codex 提供的待审建议/)).toBeInTheDocument();
  }, 7000);

  it("loads versioned export preferences and rejects invalid local data", () => {
    expect(parseExportPreferences('{"version":1,"subtitleMode":"bilingual","subtitleLanguage":"ja","transcriptFormat":"vtt"}')).toEqual({
      version: 1,
      subtitleMode: "bilingual",
      subtitleLanguage: "ja",
      transcriptFormat: "vtt",
    });
    expect(parseExportPreferences("not-json").subtitleMode).toBe("source");
    expect(parseExportPreferences('{"version":2,"subtitleMode":"translated","transcriptFormat":"srt"}').subtitleMode).toBe("source");
  });

  it("checks signed releases at most once per 24 hours", () => {
    const now = Date.parse("2026-07-17T12:00:00Z");
    expect(shouldCheckForUpdates(null, now, true)).toBe(true);
    expect(shouldCheckForUpdates("2026-07-16T11:59:59Z", now, true)).toBe(true);
    expect(shouldCheckForUpdates("2026-07-16T12:00:01Z", now, true)).toBe(false);
    expect(shouldCheckForUpdates(null, now, false)).toBe(false);
  });

  it("waits for each status poll to finish before scheduling the next one", async () => {
    vi.useFakeTimers();
    let active = 0;
    let maxActive = 0;
    let finishFirst = () => undefined;
    const poll = vi.fn(() => new Promise<void>((resolve) => {
      active += 1;
      maxActive = Math.max(maxActive, active);
      finishFirst = () => {
        active -= 1;
        resolve();
      };
    }));
    const stop = startSerialPolling(poll, 800);

    await vi.advanceTimersByTimeAsync(800);
    expect(poll).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(2400);
    expect(poll).toHaveBeenCalledTimes(1);
    finishFirst();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(799);
    expect(poll).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(poll).toHaveBeenCalledTimes(2);
    expect(maxActive).toBe(1);
    stop();
  });

  it("clears recovered Core connection errors without hiding other failures", () => {
    expect(clearTransientCoreError("core_service_unavailable: 无法连接 SiaoCut Core 服务")).toBeNull();
    expect(clearTransientCoreError("core_service_no_response: Core 服务未返回结果")).toBeNull();
    expect(clearTransientCoreError("auto_workflow_audit_failed: 导出前审计未通过")).toBe("auto_workflow_audit_failed: 导出前审计未通过");
    expect(clearTransientCoreError(null)).toBeNull();
  });

  it("normalizes persisted one-click dismissals and ignores malformed storage", () => {
    expect(parseDismissedAutoWorkflowIds("not-json")).toEqual([]);
    expect(parseDismissedAutoWorkflowIds(JSON.stringify([" auto-1 ", "", 42, "auto-1", "auto-2"]))).toEqual(["auto-1", "auto-2"]);
  });

  it("does not let an older one-click poll revive a newer cancelled snapshot", () => {
    const cancelled = autoWorkflowFixture({
      status: "cancelled",
      cancelRequestedAt: "2026-07-25T10:02:00.000Z",
      updatedAt: "2026-07-25T10:02:00.000Z",
    });
    const staleRunning = autoWorkflowFixture({
      status: "running",
      updatedAt: "2026-07-25T10:01:59.000Z",
    });

    expect(upsertAutoWorkflowSnapshot([cancelled], staleRunning)).toEqual([cancelled]);
  });

  it("shows why preview builds cannot install updates", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "本地资源" }));
    const updates = await screen.findByRole("region", { name: "应用更新" });
    expect(within(updates).getByText("当前版本 0.2.0-preview · 每 24 小时检查")).toBeInTheDocument();
    expect(within(updates).getByText("浏览器预览不连接更新源。")).toBeInTheDocument();
    expect(within(updates).getByRole("button", { name: "手动检查更新" })).toBeDisabled();
  });

  it("loads the browser preview project and exposes the three-layer workbench", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "发布口播 · 草稿" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "转录" })).toBeInTheDocument();
    expect(screen.getByText("字幕时间轴")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "精细编辑" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("slider", { name: "缩放比例" })).toHaveValue("160");
    expect(screen.getAllByText("需要你确认").length).toBeGreaterThan(0);
  });

  it("switches the project and transcript as one context", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "发布口播 · 草稿" });

    fireEvent.click(screen.getByRole("button", { name: /^第二个本地项目/ }));

    expect(await screen.findByRole("heading", { name: "第二个本地项目" })).toBeInTheDocument();
    expect(screen.getByLabelText("00:01 字幕文本")).toHaveValue("Second project subtitle");
    expect(screen.queryByText("今天想和大家聊聊，为什么要做一套本地优先的剪辑工作台。")).not.toBeInTheDocument();
  });

  it("requires confirmation before deleting a project and preserves the source-media promise", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "发布口播 · 草稿" });

    fireEvent.click(screen.getByRole("button", { name: "删除项目 第二个本地项目" }));
    const dialog = await screen.findByRole("dialog", { name: "删除项目" });
    expect(within(dialog).getByText(/原始音视频文件不会删除或修改/)).toBeInTheDocument();
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "确认删除" })).toBeEnabled());
    fireEvent.click(within(dialog).getByRole("button", { name: "确认删除" }));

    await waitFor(() => expect(screen.queryByRole("button", { name: /第二个本地项目/ })).not.toBeInTheDocument());
    expect(screen.getByText("项目「第二个本地项目」已删除；原始媒体文件未被修改。")).toBeInTheDocument();
  });

  it("does not silently re-authorize deletion after the displayed project version changes", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "发布口播 · 草稿" });

    fireEvent.click(screen.getByRole("button", { name: "删除项目 第二个本地项目" }));
    const dialog = await screen.findByRole("dialog", { name: "删除项目" });
    const confirm = within(dialog).getByRole("button", { name: "确认删除" });
    await waitFor(() => expect(confirm).toBeEnabled());

    await mockRun(["project", "show", "p_secondary"]);
    await mockRun(["transcript", "offset", "p_secondary", "--segment", "s-secondary", "--delta", "0.1"]);
    fireEvent.click(confirm);

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("项目在删除确认后发生了变化");
    expect(screen.getByRole("button", { name: /^第二个本地项目/ })).toBeInTheDocument();
    expect(confirm).toBeDisabled();
  });

  it("keeps an active-task deletion warning inside the confirmation dialog", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "发布口播 · 草稿" });

    fireEvent.click(screen.getByRole("button", { name: "删除项目 发布口播 · 草稿" }));
    const dialog = await screen.findByRole("dialog", { name: "删除项目" });

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("仍有 Agent 任务正在运行或等待处理");
    expect(within(dialog).getByRole("button", { name: "确认删除" })).toBeDisabled();
    expect(screen.queryByText(/project_busy/)).not.toBeInTheDocument();
  });

  it("allows choosing an existing translation language while source subtitles are selected", async () => {
    render(<App />);
    await openDrawerTab("导出");
    const language = await screen.findByLabelText("译文语言");

    expect(screen.getByLabelText("字幕模式")).toHaveValue("source");
    expect(language).toBeEnabled();
    expect(language).toHaveValue("en");
  });

  it("keeps translated subtitle modes selected when the project has no translation yet", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "发布口播 · 草稿" });
    fireEvent.click(screen.getByRole("button", { name: /^第二个本地项目/ }));
    await screen.findByRole("heading", { name: "第二个本地项目" });
    await openDrawerTab("导出");

    const panel = screen.getByLabelText("导出设置");
    const subtitleMode = within(panel).getByLabelText("字幕模式");
    fireEvent.change(subtitleMode, { target: { value: "translated" } });
    await waitFor(() => expect(subtitleMode).toHaveValue("translated"));
    expect(within(panel).getByRole("alert")).toHaveTextContent("项目中没有可用译文");
    expect(within(panel).getByLabelText("译文语言")).toBeDisabled();
    expect(within(panel).getByRole("button", { name: "导出字幕" })).toBeDisabled();

    fireEvent.change(subtitleMode, { target: { value: "bilingual" } });
    await waitFor(() => expect(subtitleMode).toHaveValue("bilingual"));
  });

  it("marks translation stale after source text changes", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /^发布口播/ }));
    const editor = await screen.findByLabelText("00:13 字幕文本");
    fireEvent.change(editor, { target: { value: "这是一段人工修订后的原文。" } });
    fireEvent.blur(editor);
    await waitFor(() => expect(screen.getAllByText("需要更新").length).toBeGreaterThan(0));
  });

  it("requires explicit confirmation before exporting stale translation segments", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /^发布口播/ }));
    const editor = await screen.findByLabelText("00:13 字幕文本");
    fireEvent.change(editor, { target: { value: "人工修订后的原文。" } });
    fireEvent.blur(editor);
    await waitFor(() => expect(screen.getAllByText("需要更新").length).toBeGreaterThan(0));

    await openDrawerTab("导出");
    const panel = await screen.findByLabelText("导出设置");
    fireEvent.change(within(panel).getByLabelText("字幕模式"), { target: { value: "translated" } });
    const exportButton = within(panel).getByRole("button", { name: "导出字幕" });
    const confirmation = within(panel).getByRole("checkbox", { name: /确认仍使用当前译文导出/ });
    expect(exportButton).toBeDisabled();
    fireEvent.click(confirmation);
    expect(exportButton).toBeEnabled();
  });

  it("exposes word timing evidence for the selected segment", async () => {
    render(<App />);
    await openDrawerTab("分析");
    const evidence = await screen.findByRole("region", { name: "词级时间" });
    expect(within(evidence).getByRole("button", { name: "嗯" })).toHaveAttribute("title", expect.stringContaining("52%"));
    expect(screen.getByText("ZH · 4 段 · 5 词")).toBeInTheDocument();
  });

  it("supports keyboard navigation between inspector tabs", async () => {
    render(<App />);
    const reviewTab = await screen.findByRole("tab", { name: /^审阅/ });
    reviewTab.focus();
    fireEvent.keyDown(reviewTab, { key: "ArrowRight" });
    const qualityTab = screen.getByRole("tab", { name: /^质量/ });
    expect(qualityTab).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(qualityTab, { key: "ArrowRight" });
    const analysisTab = screen.getByRole("tab", { name: "分析" });
    expect(analysisTab).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(analysisTab).toHaveFocus());
    expect(screen.getByRole("region", { name: "语音节奏" })).toBeInTheDocument();
  });

  it("shows local speech rhythm evidence without applying edits", async () => {
    render(<App />);
    await openDrawerTab("分析");
    const insights = await screen.findByRole("region", { name: "语音节奏" });
    expect(within(insights).getByText("83.3")).toBeInTheDocument();
    expect(within(insights).getByText("词条/分钟")).toBeInTheDocument();
    expect(within(insights).getByText("只提供定位证据，不会自动剪辑。", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("成片 04:38 · 原片 04:38")).toBeInTheDocument();

    fireEvent.click(within(insights).getByRole("button", { name: /定位长停顿/ }));
    expect(screen.getByDisplayValue("你可以，你可以先看建议，再决定是否删除。").closest("article")).toHaveClass("active");
  });

  it("runs local audio quality analysis and exposes measurable risks for review", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "新建项目" }));
    await openDrawerTab("分析");
    const quality = await screen.findByRole("region", { name: "音频质量" });
    await waitFor(() => expect(within(quality).getByRole("button", { name: "开始本地分析" })).toBeEnabled());
    expect(within(quality).getByText(/综合响度、峰值、静音区间和疑似削波/)).toBeInTheDocument();
    fireEvent.click(within(quality).getByRole("button", { name: "开始本地分析" }));

    await waitFor(() => expect(within(quality).getByText("-25.4")).toBeInTheDocument());
    expect(within(quality).getByText("3")).toBeInTheDocument();
    await openDrawerTab("审阅");
    expect(screen.getAllByText("音频质量 · 等待确认")).toHaveLength(3);
    expect(screen.getByText(/实测 1.3 秒 · 阈值 0.8 秒/)).toBeInTheDocument();
    expect(screen.getByText(/媒体不会上传，也不阻断编辑和导出/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^第二个本地项目/ }));
    await screen.findByRole("heading", { name: "第二个本地项目" });
    await openDrawerTab("分析");
    expect(within(screen.getByRole("region", { name: "音频质量" })).getByRole("button", { name: "开始本地分析" })).toBeInTheDocument();
    expect(screen.queryByText("音频质量 · 等待确认")).not.toBeInTheDocument();
  });

  it("shows typed, confidence-scored suggestions without applying them", async () => {
    render(<App />);
    expect(await screen.findByText("需要人工确认 · 口头语")).toBeInTheDocument();
    expect(screen.getByText(/置信度 99%/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "检测粗剪建议" }));
    await waitFor(() => expect(screen.getByText("发现 1 条粗剪建议；试听并确认后才会应用。")).toBeInTheDocument());
    expect(screen.getByText("说话重启：你可以")).toBeInTheDocument();
    expect(screen.getByText("需要人工确认 · 说话重启")).toBeInTheDocument();
    expect(screen.getByText(/置信度 96%/)).toBeInTheDocument();
    expect(screen.getByText("成片 04:38 · 原片 04:38")).toBeInTheDocument();
  });

  it("creates a snapped word-range cut with explicit safety padding", async () => {
    render(<App />);
    await openDrawerTab("分析");
    const evidence = await screen.findByRole("region", { name: "词级时间" });
    fireEvent.click(within(evidence).getByRole("button", { name: "嗯" }));
    expect(within(evidence).getByLabelText("剪切起点")).toHaveValue("0");
    expect(within(evidence).getByLabelText("剪切终点")).toHaveValue("0");
    fireEvent.change(within(evidence).getByLabelText("安全留白"), { target: { value: "200" } });
    fireEvent.click(within(evidence).getByRole("button", { name: "创建并试听" }));
    await waitFor(() => expect(screen.getByText("切点已创建；生成媒体预览后可试听切点前后 1 秒。")).toBeInTheDocument());
    await openDrawerTab("审阅");
    expect(await screen.findByText("词范围：嗯")).toBeInTheDocument();
  });

  it("exposes persistent undo and redo controls", async () => {
    render(<App />);
    const commands = await screen.findByLabelText("项目命令");
    const undo = within(commands).getByRole("button", { name: "撤销" });
    fireEvent.click(undo);
    await waitFor(() => expect(screen.getByText("已撤销上一步项目修改。")).toBeInTheDocument());
    const redo = within(commands).getByRole("button", { name: "重做" });
    expect(redo).toBeEnabled();
    fireEvent.click(redo);
    await waitFor(() => expect(screen.getByText("已重做项目修改。")).toBeInTheDocument());
  });

  it("uses history shortcuts outside editors and leaves text input shortcuts alone", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "发布口播 · 草稿" });
    fireEvent.keyDown(window, { key: "z", code: "KeyZ", ctrlKey: true });
    await waitFor(() => expect(screen.getByText("已撤销上一步项目修改。")).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText("关闭提示"));
    const search = screen.getByPlaceholderText("查找文字");
    fireEvent.keyDown(search, { key: "z", code: "KeyZ", ctrlKey: true });
    expect(screen.queryByText("已撤销上一步项目修改。")).not.toBeInTheDocument();
  });

  it("supports transcript, export, and explicit-save keyboard paths", async () => {
    render(<App />);
    const editor = await screen.findByLabelText("00:13 字幕文本");

    fireEvent.keyDown(window, { key: "f", code: "KeyF", ctrlKey: true });
    expect(screen.getByPlaceholderText("查找文字")).toHaveFocus();
    fireEvent.keyDown(window, { key: "h", code: "KeyH", ctrlKey: true });
    expect(screen.getByLabelText("替换为")).toHaveFocus();

    fireEvent.keyDown(window, { key: "e", code: "KeyE", ctrlKey: true, shiftKey: true });
    expect(await screen.findByLabelText("导出设置")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByLabelText("导出设置")).not.toBeInTheDocument());

    fireEvent.change(editor, { target: { value: "通过快捷键保存的人工修订。" } });
    fireEvent.keyDown(editor, { key: "s", code: "KeyS", ctrlKey: true });
    await waitFor(() => expect(screen.getByText("原文已更新；对应译文需要更新。")).toBeInTheDocument());
  });

  it("opens a time-confirmed split directly from Enter in the transcript", async () => {
    render(<App />);
    const editor = await screen.findByLabelText("00:13 字幕文本") as HTMLTextAreaElement;
    fireEvent.change(editor, { target: { value: "人工改写后直接拆分。" } });
    editor.focus();
    editor.setSelectionRange(6, 6);
    fireEvent.keyDown(editor, { key: "Enter", code: "Enter" });

    const dialog = await screen.findByRole("dialog", { name: "拆分字幕" });
    const preview = within(dialog).getByRole("region", { name: "拆分预览" });
    expect(within(preview).getByText("人工改写后直")).toBeInTheDocument();
    expect(within(preview).getByText("接拆分。")).toBeInTheDocument();
    expect(within(dialog).getByText(/缺少可信的词级时间/)).toBeInTheDocument();
    const submit = within(dialog).getByRole("button", { name: "确认拆分当前段" });
    expect(submit).toBeDisabled();
    fireEvent.change(within(dialog).getByRole("spinbutton", { name: /时间拆分点/ }), { target: { value: "15.500" } });
    fireEvent.click(submit);

    await waitFor(() => expect(screen.getAllByRole("textbox", { name: /字幕文本/ })).toHaveLength(5));
  });

  it("opens an adjacent merge directly from Backspace at the start of a segment", async () => {
    render(<App />);
    const editor = await screen.findByLabelText("00:13 字幕文本") as HTMLTextAreaElement;
    editor.focus();
    editor.setSelectionRange(0, 0);
    fireEvent.keyDown(editor, { key: "Backspace", code: "Backspace" });

    const dialog = await screen.findByRole("dialog", { name: "合并字幕" });
    expect(within(dialog).getByText(/作用范围：2 段/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "确认合并 2 段" }));
    await waitFor(() => expect(screen.getAllByRole("textbox", { name: /字幕文本/ })).toHaveLength(3));
  });

  it("selects continuous subtitle ranges and confirms batch offsets before applying", async () => {
    render(<App />);
    const toolbar = await screen.findByRole("region", { name: "字幕结构工具栏" });
    expect(within(toolbar).getByText(/1 段/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("checkbox", { name: "选择字幕 00:13 至 00:18" }));
    expect(within(toolbar).getByText(/2 段/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "选择字幕 00:24 至 00:27" }), { shiftKey: true });
    expect(within(toolbar).getByText(/3 段/)).toBeInTheDocument();

    fireEvent.click(within(toolbar).getByRole("button", { name: "偏移" }));
    const dialog = screen.getByRole("dialog", { name: "批量偏移字幕" });
    expect(within(dialog).getByText(/作用范围：3 段/)).toBeInTheDocument();
    expect(within(dialog).getByText(/操作会创建可恢复版本/)).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText(/统一偏移/), { target: { value: "0.250" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "确认偏移 3 段" }));

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "批量偏移字幕" })).not.toBeInTheDocument());
    expect(screen.getByText(/已将 3 段字幕批量偏移 \+0.250 秒/)).toBeInTheDocument();
    expect(within(await screen.findByLabelText("项目命令")).getByRole("button", { name: "撤销" })).toBeEnabled();
  });

  it("opens structure shortcuts outside editors without overriding text editing", async () => {
    render(<App />);
    const editor = await screen.findByLabelText("00:12 字幕文本");
    fireEvent.keyDown(editor, { key: "s", code: "KeyS", ctrlKey: true, shiftKey: true });
    expect(screen.queryByRole("dialog", { name: "拆分字幕" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("字幕段 00:13 至 00:18"));
    fireEvent.keyDown(window, { key: "s", code: "KeyS", ctrlKey: true, shiftKey: true });
    const dialog = screen.getByRole("dialog", { name: "拆分字幕" });
    expect(within(dialog).getByRole("region", { name: "字幕操作范围" })).toBeInTheDocument();
    expect(within(dialog).getByRole("region", { name: "拆分预览" })).toBeInTheDocument();
    expect(within(dialog).getByText(/缺少可信的词级时间/)).toBeInTheDocument();
    fireEvent.change(within(dialog).getByRole("spinbutton", { name: /时间拆分点/ }), { target: { value: "15.500" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "确认拆分当前段" }));

    await waitFor(() => expect(screen.getByText(/字幕已拆分.*Ctrl\+Z 撤销/)).toBeInTheDocument());
    expect(screen.getAllByRole("textbox", { name: /字幕文本/ })).toHaveLength(5);
    fireEvent.keyDown(window, { key: "ArrowDown", code: "ArrowDown", altKey: true });
    expect(screen.getByLabelText("字幕段 00:15 至 00:18")).toHaveClass("active");
  });

  it("adjusts compact timing and merges only adjacent selected subtitles", async () => {
    render(<App />);
    const toolbar = await screen.findByRole("region", { name: "字幕结构工具栏" });
    fireEvent.click(screen.getByLabelText("字幕段 00:13 至 00:18"));
    fireEvent.click(within(toolbar).getByRole("button", { name: "时间" }));

    const timingDialog = screen.getByRole("dialog", { name: "调整字幕时间" });
    expect(within(timingDialog).getByRole("button", { name: "确认更新时间" })).toBeDisabled();
    expect(within(timingDialog).getByText("开始和结束时间没有变化。")).toBeInTheDocument();
    fireEvent.change(within(timingDialog).getByLabelText(/开始时间/), { target: { value: "13.400" } });
    fireEvent.change(within(timingDialog).getByLabelText(/结束时间/), { target: { value: "18.500" } });
    fireEvent.click(within(timingDialog).getByRole("button", { name: "确认更新时间" }));
    await waitFor(() => expect(screen.getByText(/字幕时间已更新.*Ctrl\+Z 撤销/)).toBeInTheDocument());

    fireEvent.click(screen.getByRole("checkbox", { name: "选择字幕 00:18 至 00:24" }));
    const merge = within(toolbar).getByRole("button", { name: "合并" });
    expect(merge).toBeEnabled();
    fireEvent.click(merge);
    const mergeDialog = screen.getByRole("dialog", { name: "合并字幕" });
    expect(within(mergeDialog).getByText(/作用范围：2 段/)).toBeInTheDocument();
    expect(within(mergeDialog).getByRole("region", { name: "字幕操作范围" })).toBeInTheDocument();
    fireEvent.click(within(mergeDialog).getByRole("button", { name: "确认合并 2 段" }));

    await waitFor(() => {
      expect(screen.getByText(/相邻字幕已合并.*Ctrl\+Z 撤销/)).toBeInTheDocument();
      expect(screen.getAllByRole("textbox", { name: /字幕文本/ })).toHaveLength(3);
      expect(screen.getByDisplayValue(/今天想和大家聊聊.*它不是替你决定内容/)).toBeInTheDocument();
    });
  });

  it("nudges the whole selected subtitle from the precision timeline", async () => {
    render(<App />);
    const timeline = await screen.findByRole("region", { name: "字幕时间轴" });
    fireEvent.click(within(timeline).getByRole("button", { name: /字幕 2，00:13\.2 至 00:18\.6/ }));
    fireEvent.click(within(timeline).getByRole("button", { name: "后移 0.1 秒" }));

    await waitFor(() => expect(screen.getByText(/字幕已后移 0\.1 秒/)).toBeInTheDocument());
    expect(within(timeline).getByText("00:13.3 — 00:18.7")).toBeInTheDocument();
    expect(within(timeline).getByText(/整段移动，字幕时长保持不变/)).toBeInTheDocument();
  });

  it("keeps subtitle timing unchanged when a timeline nudge fails", async () => {
    vi.spyOn(transcriptEditingClient, "offsetSegments").mockRejectedValueOnce(new Error("时间微调失败"));
    render(<App />);
    const timeline = await screen.findByRole("region", { name: "字幕时间轴" });
    fireEvent.click(within(timeline).getByRole("button", { name: /字幕 2，00:13\.2 至 00:18\.6/ }));
    expect(within(timeline).getByText("00:13.2 — 00:18.6")).toBeInTheDocument();

    fireEvent.click(within(timeline).getByRole("button", { name: "后移 0.1 秒" }));

    expect(await screen.findByText("时间微调失败")).toBeInTheDocument();
    expect(within(timeline).getByText("00:13.2 — 00:18.6")).toBeInTheDocument();
  });

  it("batch replaces transcript text and exposes all export formats", async () => {
    render(<App />);
    fireEvent.change(await screen.findByPlaceholderText("查找文字"), { target: { value: "决定" } });
    expect(screen.getByText("3 处匹配")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("替换为"), { target: { value: "判断" } });
    fireEvent.click(screen.getByRole("button", { name: "全部替换" }));
    await waitFor(() => expect(screen.getByText(/已替换 2 个字幕段/)).toBeInTheDocument());
    fireEvent.change(screen.getByPlaceholderText("查找文字"), { target: { value: "" } });
    await waitFor(() => expect(screen.getAllByRole("textbox", { name: /字幕文本/ }).some((input) => (input as HTMLTextAreaElement).value.includes("判断"))).toBe(true));

    fireEvent.click(within(screen.getByLabelText("项目命令")).getByRole("button", { name: "撤销" }));
    await waitFor(() => expect(screen.getAllByRole("textbox", { name: /字幕文本/ }).some((input) => (input as HTMLTextAreaElement).value.includes("决定"))).toBe(true));

    await openDrawerTab("导出");
    const exportPanel = await screen.findByLabelText("导出设置");
    fireEvent.change(within(exportPanel).getByLabelText("导出格式"), { target: { value: "vtt" } });
    fireEvent.click(within(exportPanel).getByRole("button", { name: "导出字幕" }));
    await waitFor(() => expect(screen.getByText(/\.vtt/)).toBeInTheDocument());
  });

  it("exposes playback state and current time as a live status", async () => {
    render(<App />);

    const playback = await screen.findByRole("status", { name: "播放器状态" });
    expect(playback).toHaveTextContent("已暂停 · 00:00 /");
  });

  it("captures loaded media duration without retaining a React synthetic event", () => {
    expect(resolvePlaybackDuration(278.4, 120)).toBe(278.4);
    expect(resolvePlaybackDuration(Number.NaN, 120)).toBe(120);
    expect(resolvePlaybackDuration(0, null)).toBe(0);
  });

  it("requires explicit confirmation before an empty replacement deletes text", async () => {
    render(<App />);
    fireEvent.change(await screen.findByPlaceholderText("查找文字"), { target: { value: "决定" } });
    const replace = screen.getByRole("button", { name: "全部替换" });
    expect(replace).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox", { name: "确认删除全部匹配文字" }));
    expect(replace).toBeEnabled();
  });

  it("preflights and explicitly confirms safe quick subtitle regeneration", async () => {
    render(<App />);
    const commands = await screen.findByLabelText("项目命令");
    fireEvent.click(within(commands).getByRole("button", { name: "更多命令" }));
    let menu = await screen.findByRole("menu");
    fireEvent.click(within(menu).getByRole("menuitem", { name: "重新定位原片" }));
    await waitFor(() => expect(screen.getByText("已重新定位原片；内容哈希与项目记录一致。")).toBeInTheDocument());
    fireEvent.click(within(commands).getByRole("button", { name: "更多命令" }));
    menu = await screen.findByRole("menu");
    const regenerate = within(menu).getByRole("menuitem", { name: "重新生成快速字幕" });
    expect(regenerate).toBeEnabled();
    fireEvent.click(regenerate);

    const dialog = await screen.findByRole("dialog", { name: "确认重新生成快速字幕" });
    expect(within(dialog).getByText(/校验未通过时，当前项目保持不变/)).toBeInTheDocument();
    const confirm = within(dialog).getByRole("button", { name: "确认并重新转写" });
    expect(confirm).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("checkbox", { name: /确认替换当前字幕/ }));
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "确认重新生成快速字幕" })).not.toBeInTheDocument());
    expect(screen.getByText(/快速字幕已重新生成并通过时间校验/)).toBeInTheDocument();
  });

  it("rejects a split that would create a punctuation-only subtitle", async () => {
    render(<App />);
    const toolbar = await screen.findByRole("region", { name: "字幕结构工具栏" });
    fireEvent.click(within(toolbar).getByRole("button", { name: "拆分" }));
    const dialog = screen.getByRole("dialog", { name: "拆分字幕" });
    expect(within(dialog).getByText("拆分后的两段字幕都必须包含文字或数字。")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "确认拆分当前段" })).toBeDisabled();
  });

  it("previews subtitle files before explicit replacement and filters located issues", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "导入字幕" }));
    const dialog = await screen.findByRole("dialog", { name: "导入字幕" });
    expect(within(dialog).getByText(/确认前不会修改项目/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "选择文件" }));

    const preview = await within(dialog).findByRole("region", { name: "字幕导入预检" });
    expect(within(preview).getByText("2 段字幕")).toBeInTheDocument();
    expect(within(preview).getByText("1 项质量提醒")).toBeInTheDocument();
    const replace = within(preview).getByRole("button", { name: "确认替换字幕" });
    expect(replace).toBeDisabled();
    fireEvent.click(within(preview).getByRole("checkbox", { name: /确认用这份文件替换当前字幕/ }));
    expect(replace).toBeEnabled();
    fireEvent.click(replace);

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "导入字幕" })).not.toBeInTheDocument());
    const transcript = screen.getByLabelText("字幕文稿列表");
    await waitFor(() => expect(within(transcript).getByDisplayValue("导入后的第一条字幕")).toBeInTheDocument());
    await openDrawerTab("质量");
    const quality = screen.getByRole("region", { name: "字幕质量" });
    expect(within(quality).getByText("1 项质量提醒")).toBeInTheDocument();
    fireEvent.click(within(quality).getByRole("button", { name: "提醒 1" }));
    expect(within(transcript).queryByDisplayValue("导入后的第一条字幕")).not.toBeInTheDocument();
    expect(within(transcript).getByDisplayValue("导入后的第二条字幕")).toBeInTheDocument();
    fireEvent.click(within(quality).getByRole("button", { name: /与上一条字幕时间重叠/ }));
    expect(within(transcript).getByDisplayValue("导入后的第二条字幕").closest("article")).toHaveClass("selected");
  });

  it("invalidates subtitle replacement approval when the project changes after preflight", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "导入字幕" }));
    const dialog = await screen.findByRole("dialog", { name: "导入字幕" });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择文件" }));
    const preview = await within(dialog).findByRole("region", { name: "字幕导入预检" });
    fireEvent.click(within(preview).getByRole("checkbox", { name: /确认用这份文件替换当前字幕/ }));

    await mockRun(["transcript", "offset", "p_demo", "--segment", "s1", "--delta", "0.100"]);
    fireEvent.click(within(preview).getByRole("button", { name: "确认替换字幕" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("旧确认已失效，请重新选择文件并预检");
    expect(within(dialog).queryByRole("region", { name: "字幕导入预检" })).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "导入字幕" })).toBeInTheDocument();
  });

  it("updates the vertical canvas and exposes explicit subtitle modes", async () => {
    render(<App />);
    await openDrawerTab("导出");
    const canvas = await screen.findByLabelText("画布比例");
    fireEvent.change(canvas, { target: { value: "9:16" } });
    expect(canvas).toHaveValue("9:16");
    await waitFor(() => expect(screen.getByText("画布已改为 9:16；请重新生成预览以查看最终构图。")).toBeInTheDocument());
    expect(screen.getByLabelText("竖屏构图")).toBeEnabled();
    const subtitleMode = screen.getByLabelText("字幕模式");
    fireEvent.change(subtitleMode, { target: { value: "translated" } });
    await waitFor(() => expect(subtitleMode).toHaveValue("translated"));
    expect(screen.getByLabelText("译文语言")).toBeEnabled();
    expect(screen.getByLabelText("译文语言")).toHaveValue("en");
    fireEvent.change(subtitleMode, { target: { value: "bilingual" } });
    await waitFor(() => expect(subtitleMode).toHaveValue("bilingual"));
    expect(screen.getByText(/不会隐藏原片已烧录字幕/)).toBeInTheDocument();
    expect(screen.getByText(/按词级时间戳随说话进度/)).toBeInTheDocument();
    expect(screen.getByText("当前项目未应用任何剪辑；导出视频将与原片等长。")).toBeInTheDocument();
  });

  it("previews the saved subtitle style, safe area, and bilingual hierarchy without changing text", async () => {
    render(<App />);
    const transcriptText = await screen.findByLabelText("00:13 字幕文本");
    const originalText = (transcriptText as HTMLTextAreaElement).value;
    fireEvent.click(screen.getByRole("checkbox", { name: "选择字幕 00:13 至 00:18" }));
    await openDrawerTab("导出");
    const panel = screen.getByLabelText("导出设置");
    fireEvent.change(within(panel).getByLabelText("字幕模式"), { target: { value: "bilingual" } });
    fireEvent.change(within(panel).getByLabelText("字幕样式预设"), { target: { value: "emphasis" } });
    await waitFor(() => expect(screen.getByText("字幕样式已更新；正文和时间未修改，可撤销。")).toBeInTheDocument());
    fireEvent.change(within(panel).getByLabelText("字幕位置"), { target: { value: "center" } });

    await waitFor(() => {
      const caption = document.querySelector(".caption-overlay");
      expect(caption).toHaveAttribute("data-preset", "emphasis");
      expect(caption).toHaveAttribute("data-position", "center");
      expect(caption).toHaveAttribute("data-outline-width", "4");
      expect(caption).toHaveTextContent("Today I want to explain why we are building a local-first editing workbench.");
    });
    expect(screen.getByLabelText("字幕安全区")).toBeInTheDocument();
    expect(within(panel).getByText("60 px")).toBeInTheDocument();
    expect(within(panel).getByText("46 px")).toBeInTheDocument();
    expect(screen.getByLabelText("00:13 字幕文本")).toHaveValue(originalText);
  });

  it("requires metadata and rights confirmation before a URL download can start", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "从 URL 导入" }));
    expect(await screen.findByRole("dialog", { name: "URL 导入" })).toBeInTheDocument();
    const inspect = screen.getByRole("button", { name: "读取视频信息" });
    fireEvent.change(screen.getByLabelText("公开视频 URL"), { target: { value: "http://example.com/video" } });
    expect(inspect).toBeDisabled();
    fireEvent.change(screen.getByLabelText("公开视频 URL"), { target: { value: "https://www.youtube.com/watch?v=HOfdboHvshg" } });
    expect(inspect).toBeEnabled();
    fireEvent.click(inspect);
    const preview = await screen.findByRole("region", { name: "待确认视频信息" });
    expect(within(preview).getByText("Sintel Trailer, Durian Open Movie Project")).toBeInTheDocument();
    expect(within(preview).getByText("00:52")).toBeInTheDocument();
    expect(within(preview).getByText("HOfdboHvshg")).toBeInTheDocument();
    expect(within(preview).getByText("URL 导入能力")).toBeInTheDocument();
    const start = within(preview).getByRole("button", { name: "确认信息并开始下载" });
    expect(start).toBeDisabled();
    fireEvent.click(within(preview).getByRole("checkbox"));
    expect(start).toBeEnabled();
  });

  it("opens first-run preparation without a default root and supports deferral", async () => {
    setMockLocalResourcesForTest({
      configured: false,
      root: null,
      rootAvailable: false,
      writable: false,
      availableBytes: null,
      transcriptionProfile: "standard",
      capabilities: [
        { id: "basic_media", name: "基础媒体处理", state: "not_ready", enabled: false },
        { id: "url_import", name: "URL 导入", state: "not_ready", enabled: false },
        { id: "local_transcription", name: "本地转录", state: "not_ready", enabled: false },
        { id: "speaker_identity", name: "说话人识别", state: "not_ready", enabled: false },
      ],
      needsSetup: true,
    });
    render(<App />);

    const setup = await screen.findByRole("dialog", { name: "准备 SiaoCut" });
    expect(within(setup).getByText("尚未选择")).toBeInTheDocument();
    expect(within(setup).getByRole("button", { name: "准备推荐资源" })).toBeDisabled();
    fireEvent.click(within(setup).getByRole("button", { name: "稍后设置" }));

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "准备 SiaoCut" })).not.toBeInTheDocument());
    expect(localStorage.getItem("siaocut.localResourcesSetupDeferred.v1")).toBe("1");
  });

  it("moves local resources after explicit location confirmation and cleans only on confirmation", async () => {
    setMockLocalResourcesForTest({
      configured: true,
      root: "E:\\SiaoCut Resources",
      rootAvailable: true,
      writable: true,
      availableBytes: 128 * 1024 * 1024 * 1024,
      transcriptionProfile: "standard",
      capabilities: [
        { id: "basic_media", name: "基础媒体处理", state: "ready", enabled: true },
        { id: "url_import", name: "URL 导入", state: "ready", enabled: true },
        { id: "local_transcription", name: "本地转录", state: "ready", enabled: true },
        { id: "speaker_identity", name: "说话人识别", state: "ready", enabled: true },
      ],
      needsSetup: false,
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: "本地资源" }));
    const settings = await screen.findByRole("dialog", { name: "本地资源" });
    fireEvent.click(within(settings).getByRole("button", { name: "更改位置" }));
    const setup = await screen.findByRole("dialog", { name: "准备 SiaoCut" });
    fireEvent.click(within(setup).getByRole("button", { name: "更改位置" }));
    expect(await within(setup).findByText("D:\\SiaoCut Resources")).toBeInTheDocument();
    fireEvent.click(within(setup).getByRole("button", { name: "确认此位置" }));

    expect(await screen.findByText("本地资源已安全移到新的保存位置。")).toBeInTheDocument();
    const reopened = await screen.findByRole("dialog", { name: "本地资源" });
    expect(within(reopened).getByText("D:\\SiaoCut Resources")).toBeInTheDocument();
    fireEvent.click(within(reopened).getByRole("button", { name: "释放无用空间" }));

    expect(confirm).toHaveBeenCalledWith(expect.stringMatching(/当前版本、可恢复版本与可续传下载会保留/));
    expect(await screen.findByText("无用的本地资源文件已清理。")).toBeInTheDocument();
  });

  it("prepares a missing URL-import capability and resumes the original inspection", async () => {
    setMockLocalResourcesForTest({
      configured: true,
      root: "E:\\SiaoCut Resources",
      rootAvailable: true,
      writable: true,
      availableBytes: 128 * 1024 * 1024 * 1024,
      transcriptionProfile: "standard",
      capabilities: [
        { id: "basic_media", name: "基础媒体处理", state: "ready", enabled: true },
        { id: "url_import", name: "URL 导入", state: "not_ready", enabled: false },
        { id: "local_transcription", name: "本地转录", state: "not_ready", enabled: false },
        { id: "speaker_identity", name: "说话人识别", state: "not_ready", enabled: false },
      ],
      needsSetup: false,
    });
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "从 URL 导入" }));
    fireEvent.change(await screen.findByLabelText("公开视频 URL"), { target: { value: "https://www.youtube.com/watch?v=HOfdboHvshg" } });
    fireEvent.click(screen.getByRole("button", { name: "读取视频信息" }));

    const setup = await screen.findByRole("dialog", { name: "准备 SiaoCut" });
    expect(within(setup).getByText(/产品安装包不包含第三方运行时或模型/)).toBeInTheDocument();
    expect(within(setup).getByText("E:\\SiaoCut Resources")).toBeInTheDocument();
    fireEvent.click(within(setup).getByRole("button", { name: "准备并继续" }));

    const preview = await screen.findByRole("region", { name: "待确认视频信息" });
    expect(within(preview).getByText("Sintel Trailer, Durian Open Movie Project")).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "准备 SiaoCut" })).not.toBeInTheDocument();
  });

  it("prepares the selected transcription profile and resumes the original transcription", async () => {
    const emptyProject = structuredClone(sampleProject);
    emptyProject.media.sourcePath = "D:\\Media\\local-resource-test.mp4";
    emptyProject.transcript = { sourceLanguage: "auto", segments: [], words: [] };
    emptyProject.translations = {};
    emptyProject.edits = [];
    emptyProject.tasks = [];
    emptyProject.patchSets = [];
    setMockProjectForTest(emptyProject);
    setMockLocalResourcesForTest({
      configured: true,
      root: "E:\\SiaoCut Resources",
      rootAvailable: true,
      writable: true,
      availableBytes: 128 * 1024 * 1024 * 1024,
      transcriptionProfile: "standard",
      capabilities: [
        { id: "basic_media", name: "基础媒体处理", state: "ready", enabled: true },
        { id: "url_import", name: "URL 导入", state: "ready", enabled: true },
        { id: "local_transcription", name: "本地转录", state: "not_ready", enabled: false },
        { id: "speaker_identity", name: "说话人识别", state: "not_ready", enabled: false },
      ],
      needsSetup: false,
    });
    setMockAuthorizedMediaForTest("mock://local-media");
    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: new RegExp(`^${emptyProject.title}`) }));
    const start = await screen.findByRole("button", { name: "开始转写" });
    await waitFor(() => expect(start).toBeEnabled());
    fireEvent.click(start);
    const setup = await screen.findByRole("dialog", { name: "准备 SiaoCut" });
    expect(within(setup).getByRole("radio", { name: /标准/ })).toBeChecked();
    fireEvent.click(within(setup).getByRole("radio", { name: /高质量/ }));
    await waitFor(() => expect(within(setup).getByRole("radio", { name: /高质量/ })).toBeChecked());
    fireEvent.click(within(setup).getByRole("button", { name: "准备并继续" }));

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "准备 SiaoCut" })).not.toBeInTheDocument());
    expect(await screen.findByText(/未检测到清晰人声/)).toBeInTheDocument();
  });

  it("cancels a URL import without a project and only resumes explicitly", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "从 URL 导入" }));
    fireEvent.change(await screen.findByLabelText("公开视频 URL"), { target: { value: "https://www.youtube.com/watch?v=HOfdboHvshg" } });
    fireEvent.click(screen.getByRole("button", { name: "读取视频信息" }));
    const preview = await screen.findByRole("region", { name: "待确认视频信息" });
    fireEvent.click(within(preview).getByRole("checkbox"));
    fireEvent.click(within(preview).getByRole("button", { name: "确认信息并开始下载" }));
    const job = await screen.findByRole("region", { name: "URL 导入任务" });
    expect(within(job).getByText("媒体校验通过后创建")).toBeInTheDocument();
    fireEvent.click(within(job).getByRole("button", { name: "取消并保留分片" }));
    await waitFor(() => expect(within(job).getByText("已取消")).toBeInTheDocument());
    expect(within(job).getByText("媒体校验通过后创建")).toBeInTheDocument();
    fireEvent.click(within(job).getByRole("button", { name: "显式继续" }));
    await waitFor(() => expect(screen.getByText("URL 导入已显式继续；这是第 2 次尝试。")).toBeInTheDocument());
    expect(within(job).getByText("第 2 次尝试 · HOfdboHvshg")).toBeInTheDocument();
  });

  it("opens the validated project after a background URL import completes", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "从 URL 导入" }));
    fireEvent.change(await screen.findByLabelText("公开视频 URL"), { target: { value: "https://www.youtube.com/watch?v=HOfdboHvshg" } });
    fireEvent.click(screen.getByRole("button", { name: "读取视频信息" }));
    const preview = await screen.findByRole("region", { name: "待确认视频信息" });
    fireEvent.click(within(preview).getByRole("checkbox"));
    fireEvent.click(within(preview).getByRole("button", { name: "确认信息并开始下载" }));
    await waitFor(() => expect(screen.getByRole("heading", { name: "Sintel Trailer, Durian Open Movie Project" })).toBeInTheDocument(), { timeout: 4000 });
    expect(screen.queryByRole("dialog", { name: "URL 导入" })).not.toBeInTheDocument();
    expect(screen.getByText(/原 URL、站点媒体 ID、工具版本和文件哈希已保存/)).toBeInTheDocument();
  });

  it("does not let a completed URL import steal a project selected after the job started", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "从 URL 导入" }));
    fireEvent.change(await screen.findByLabelText("公开视频 URL"), { target: { value: "https://www.youtube.com/watch?v=HOfdboHvshg" } });
    fireEvent.click(screen.getByRole("button", { name: "读取视频信息" }));
    const preview = await screen.findByRole("region", { name: "待确认视频信息" });
    fireEvent.click(within(preview).getByRole("checkbox"));
    fireEvent.click(within(preview).getByRole("button", { name: "确认信息并开始下载" }));

    const secondProject = screen.getByRole("button", { name: /^第二个本地项目/ });
    await waitFor(() => expect(secondProject).toBeEnabled());
    fireEvent.click(secondProject);
    await screen.findByRole("heading", { name: "第二个本地项目" });

    await waitFor(() => expect(screen.getByText(/原 URL、站点媒体 ID、工具版本和文件哈希已保存/)).toBeInTheDocument(), { timeout: 4000 });
    expect(screen.getByRole("heading", { name: "第二个本地项目" })).toBeInTheDocument();
  });

  it("keeps concurrent one-click workflows visible and does not steal a later project selection", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "一键成片" }));
    let dialog = screen.getByRole("dialog", { name: "一键工作流" });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择文件" }));
    await waitFor(() => expect(within(dialog).getByText("demo.mp4")).toBeInTheDocument());
    fireEvent.click(within(dialog).getByRole("button", { name: "启动一键工作流" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "一键工作流" })).not.toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "一键成片" }));
    dialog = screen.getByRole("dialog", { name: "一键工作流" });
    const secondStart = within(dialog).getByRole("button", { name: "启动一键工作流" });
    await waitFor(() => expect(secondStart).toBeEnabled());
    fireEvent.click(secondStart);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "一键工作流" })).not.toBeInTheDocument());
    await waitFor(() => expect(screen.getAllByRole("region", { name: "一键工作流状态" })).toHaveLength(2));

    const secondProject = screen.getByRole("button", { name: /^第二个本地项目/ });
    await waitFor(() => expect(secondProject).toBeEnabled());
    fireEvent.click(secondProject);
    await screen.findByRole("heading", { name: "第二个本地项目" });
    await waitFor(() => {
      const workflows = screen.getAllByRole("region", { name: "一键工作流状态" });
      expect(workflows).toHaveLength(2);
      expect(workflows.some((workflow) => {
        const progress = within(workflow).getByRole("progressbar") as HTMLProgressElement;
        return progress.value > 0.02;
      })).toBe(true);
    }, { timeout: 4000 });
    expect(screen.getByRole("heading", { name: "第二个本地项目" })).toBeInTheDocument();
  }, 10_000);

  it("dismisses a cancelled one-click status without deleting its recovery path", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "一键成片" }));
    const dialog = screen.getByRole("dialog", { name: "一键工作流" });
    expect(within(dialog).getByText(/粗剪和 Agent 结果不会自动应用/)).toBeInTheDocument();
    const start = within(dialog).getByRole("button", { name: "启动一键工作流" });
    expect(start).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("button", { name: "选择文件" }));
    await waitFor(() => expect(within(dialog).getByText("demo.mp4")).toBeInTheDocument());
    expect(start).toBeEnabled();
    fireEvent.click(start);
    const status = await screen.findByRole("region", { name: "一键工作流状态" });
    expect(within(status).getByText(/正在处理 · 导入素材/)).toBeInTheDocument();
    fireEvent.click(within(status).getByRole("button", { name: "取消流程" }));
    await waitFor(() => expect(within(status).getByText(/已取消 · 导入素材/)).toBeInTheDocument());
    expect(screen.getByText("自动工作流已取消；已完成的本地项目和中间证据仍然保留。")).toBeInTheDocument();
    fireEvent.click(within(status).getByRole("button", { name: "关闭此流程状态" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "一键工作流状态" })).not.toBeInTheDocument());
    expect(parseDismissedAutoWorkflowIds(localStorage.getItem(AUTO_WORKFLOW_DISMISSED_STORAGE_KEY))).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "一键成片" }));
    const reopened = screen.getByRole("dialog", { name: "一键工作流" });
    const history = within(reopened).getByRole("region", { name: "最近的一键流程" });
    expect(within(history).getByText(/已取消 · 导入素材/)).toBeInTheDocument();
    fireEvent.click(within(history).getByRole("button", { name: "显式继续" }));
    await waitFor(() => expect(screen.getByText("自动工作流已显式继续；这是第 2 次尝试。")).toBeInTheDocument());
    expect(await screen.findByRole("region", { name: "一键工作流状态" })).toBeInTheDocument();
  });

  it("keeps a cancelled Agent translation workflow available for recovery", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "一键成片" }));
    const dialog = screen.getByRole("dialog", { name: "一键工作流" });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择文件" }));
    await waitFor(() => expect(within(dialog).getByText("demo.mp4")).toBeInTheDocument());
    fireEvent.click(within(dialog).getByRole("checkbox", { name: "创建 Agent 翻译任务" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "启动一键工作流" }));

    const status = await screen.findByRole("region", { name: "一键工作流状态" });
    await waitFor(() => expect(within(status).getByText(/需要 Agent 继续 · 等待 Agent 翻译/)).toBeInTheDocument(), { timeout: 5000 });
    fireEvent.click(within(status).getByRole("button", { name: "取消流程" }));

    await waitFor(() => expect(within(status).getByText(/已取消 · 等待 Agent 翻译/)).toBeInTheDocument());
    expect(within(status).getByRole("button", { name: "显式继续" })).toBeInTheDocument();
    expect(within(status).getByRole("button", { name: "打开待审项目" })).toBeInTheDocument();
  });

  it("requires audited URL metadata and rights confirmation in the one-click flow", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "一键成片" }));
    const dialog = screen.getByRole("dialog", { name: "一键工作流" });
    fireEvent.change(within(dialog).getByLabelText("一键素材来源"), { target: { value: "url" } });
    fireEvent.change(within(dialog).getByLabelText("一键公开视频 URL"), { target: { value: "https://www.youtube.com/watch?v=HOfdboHvshg" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "读取视频信息" }));
    const preview = await within(dialog).findByRole("region", { name: "一键待确认视频信息" });
    expect(within(preview).getByText("HOfdboHvshg")).toBeInTheDocument();
    const start = within(dialog).getByRole("button", { name: "启动一键工作流" });
    expect(start).toBeDisabled();
    fireEvent.click(within(preview).getByRole("checkbox"));
    expect(start).toBeEnabled();
  });

  it("pauses a one-click workflow for review before explicit continuation", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "一键成片" }));
    const dialog = screen.getByRole("dialog", { name: "一键工作流" });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择文件" }));
    await waitFor(() => expect(within(dialog).getByText("demo.mp4")).toBeInTheDocument());
    fireEvent.click(within(dialog).getByRole("button", { name: "启动一键工作流" }));
    const status = await screen.findByRole("region", { name: "一键工作流状态" });
    await waitFor(() => expect(within(status).getByText(/需要你确认 · 等待人工确认/)).toBeInTheDocument(), { timeout: 5000 });
    fireEvent.click(within(status).getByRole("button", { name: "确认完成并继续" }));
    await waitFor(() => expect(within(status).getByText("仍有 Agent 修改或粗剪建议等待人工处理")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "应用软剪辑" }));
    await waitFor(() => expect(screen.getByText(/已应用软剪辑/)).toBeInTheDocument());
    fireEvent.click(within(status).getByRole("button", { name: "确认完成并继续" }));
    await waitFor(() => expect(screen.getByText(/一键工作流已完成，视频已导出到/)).toBeInTheDocument(), { timeout: 3000 });
  }, 10000);

  it("shows local runtime status without exposing filesystem scope", async () => {
    render(<App />);
    const settings = await screen.findByRole("button", { name: "本地资源" });
    fireEvent.click(settings);
    const dialog = await screen.findByRole("dialog", { name: "本地资源" });
    expect(within(dialog).getByText("whisper.cpp")).not.toBeVisible();
    openResourceDiagnostics(dialog);
    expect(screen.getByText("whisper.cpp")).toBeInTheDocument();
    expect(screen.getByText("API 0.1")).toBeInTheDocument();
    expect(screen.getByLabelText("Core: 可用")).toBeInTheDocument();
    expect(screen.queryByLabelText("Core可用")).not.toBeInTheDocument();
    expect(screen.getByText("平衡 · 推荐")).toBeInTheDocument();
    expect(screen.getAllByText("来源：ggerganov/whisper.cpp")).toHaveLength(3);
    expect(screen.getByText("诊断日志")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开日志目录" })).toBeEnabled();
  });

  it("closes the runtime dialog with Escape and restores keyboard focus", async () => {
    render(<App />);
    const settings = await screen.findByRole("button", { name: "本地资源" });
    fireEvent.click(settings);
    expect(await screen.findByRole("button", { name: "关闭本地资源" })).toHaveFocus();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "本地资源" })).not.toBeInTheDocument());
    expect(settings).toHaveFocus();
  });

  it("requires explicit download verification and confirmation before removing a model", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "本地资源" }));
    openResourceDiagnostics(await screen.findByRole("dialog", { name: "本地资源" }));
    const option = (await screen.findByText("平衡 · 推荐")).closest("article");
    expect(option).not.toBeNull();
    fireEvent.click(within(option!).getByRole("button", { name: "下载" }));
    await waitFor(() => expect(screen.getByText("模型已下载并通过 SHA-256 校验，可以开始本地转录。")).toBeInTheDocument());
    expect(within(option!).getByText("使用中")).toBeInTheDocument();

    fireEvent.click(within(option!).getByRole("button", { name: "移除" }));
    const confirmation = screen.getByRole("alertdialog", { name: "确认移除" });
    expect(within(confirmation).getByText(/文件删除后需要重新下载/)).toBeInTheDocument();
    fireEvent.click(within(confirmation).getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("alertdialog", { name: "确认移除" })).not.toBeInTheDocument();
    expect(within(option!).getByText("使用中")).toBeInTheDocument();

    fireEvent.click(within(option!).getByRole("button", { name: "移除" }));
    fireEvent.click(within(screen.getByRole("alertdialog", { name: "确认移除" })).getByRole("button", { name: "确认移除" }));
    await waitFor(() => expect(within(option!).getByRole("button", { name: "下载" })).toBeInTheDocument());
  });

  it("installs the optional speaker package explicitly and keeps speaker edits reviewable", async () => {
    render(<App />);
    await openDrawerTab("分析");
    const speakerPanel = await screen.findByRole("region", { name: "说话人轨" });
    expect(within(speakerPanel).getByText(/可选模型尚未安装/)).toBeInTheDocument();
    expect(screen.getByLabelText("00:12 字幕文本")).toHaveValue("嗯，");

    fireEvent.click(within(speakerPanel).getByRole("button", { name: "准备说话人识别" }));
    const dialog = await screen.findByRole("dialog", { name: "准备 SiaoCut" });
    expect(within(dialog).getByText("说话人识别")).toBeInTheDocument();
    expect(dialog).not.toHaveTextContent(/sherpa|onnx|pyannote|SHA-?256/);
    fireEvent.click(within(dialog).getByRole("button", { name: "准备并继续" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "准备 SiaoCut" })).not.toBeInTheDocument());

    fireEvent.click(within(await screen.findByRole("region", { name: "说话人轨" })).getByRole("button", { name: "开始本地分析" }));
    await waitFor(() => expect(within(speakerPanel).getByLabelText("当前字幕说话人")).toHaveValue("voice-a"));
    expect(screen.getByLabelText("00:12 字幕文本")).toHaveValue("嗯，");

    const name = within(speakerPanel).getByLabelText("说话人 1名称");
    fireEvent.change(name, { target: { value: "主持人" } });
    fireEvent.blur(name);
    await waitFor(
      () => expect(within(speakerPanel).getByLabelText("主持人名称")).toBeInTheDocument(),
      { timeout: 10_000 },
    );
    await waitFor(
      () => expect(screen.getByText(/说话人名称已更新，可撤销或从版本历史恢复/)).toBeInTheDocument(),
      { timeout: 10_000 },
    );
    fireEvent.change(within(speakerPanel).getByLabelText("当前字幕说话人"), { target: { value: "voice-b" } });
    await waitFor(
      () => expect(screen.getByText(/当前字幕段的说话人已更新，可撤销或从版本历史恢复/)).toBeInTheDocument(),
      { timeout: 10_000 },
    );
    expect(screen.getByLabelText("00:12 字幕文本")).toHaveValue("嗯，");
  });

  it("maps interrupted tasks to an actionable Agent state", () => {
    const project = structuredClone(sampleProject);
    project.tasks = [{ ...project.tasks[0], status: "interrupted" }];
    project.patchSets = [];
    project.edits = [];
    expect(taskLabel(project)).toBe("需要 Agent 继续");
  });

  it("runs the explicit MOSS mode and exposes speaker review without word-cut claims", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "新建项目" }));
    const runtime = await selectAdvancedTranscriptionMode("multispeaker");
    expect(localStorage.getItem("siaocut.transcriptionMode")).toBe("multispeaker");
    fireEvent.click(within(runtime).getByRole("button", { name: "关闭本地资源" }));
    const start = screen.getByRole("button", { name: "开始多人转写" });
    await waitFor(() => expect(start).toBeEnabled());
    fireEvent.click(start);
    await waitFor(() => expect(screen.getByText(/字幕和说话人轨已作为一个版本写入/)).toBeInTheDocument());
    expect(await screen.findByRole("region", { name: "多人转写复核" })).toHaveTextContent("快速人物切换");
    expect(screen.getByText("当前结果没有词级时间戳")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "创建词范围软剪辑" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "Agent 工作流" }), { target: { value: "speaker_names" } });
    expect(screen.getByRole("button", { name: "交给本机 Codex" })).toBeEnabled();
    await openDrawerTab("导出");
    const exportPanel = await screen.findByLabelText("导出设置");
    fireEvent.change(within(exportPanel).getByLabelText("导出格式"), { target: { value: "json" } });
    expect(within(exportPanel).getByText(/始终保留模型、人物轨、段落关联和复核状态/)).toBeInTheDocument();
    const exportButton = within(exportPanel).getByRole("button", { name: "导出字幕" });
    expect(exportButton).toBeDisabled();
    fireEvent.click(within(exportPanel).getByRole("checkbox", { name: /确认带着 1 个未处理警告/ }));
    expect(exportButton).toBeEnabled();
  });

  it("preserves a conflicting MOSS result until replacement is explicitly confirmed", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "新建项目" }));
    const runtime = await selectAdvancedTranscriptionMode("multispeaker");
    fireEvent.click(within(runtime).getByRole("button", { name: "关闭本地资源" }));
    fireEvent.click(screen.getByText("高级实验项：Prompt 与热词"));
    fireEvent.change(screen.getByRole("textbox", { name: "自定义 Prompt" }), { target: { value: "simulate-conflict" } });

    const start = screen.getByRole("button", { name: "开始多人转写" });
    await waitFor(() => expect(start).toBeEnabled());
    fireEvent.click(start);

    expect(await screen.findByText("候选结果等待确认")).toBeInTheDocument();
    expect(screen.getByText("18 段 · 3 位说话人 · 2 项提醒")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看影响" }));
    const dialog = await screen.findByRole("dialog", { name: "确认多人转写候选结果" });
    const apply = within(dialog).getByRole("button", { name: "应用并替换" });
    expect(apply).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("checkbox", { name: /确认用候选结果替换/ }));
    expect(apply).toBeEnabled();
    fireEvent.click(apply);

    expect(await screen.findByText("候选结果已应用为可撤销的新版本。")).toBeInTheDocument();
    expect(screen.getByDisplayValue("这是经过明确确认后应用的多人转写候选结果。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "撤销" })).toBeEnabled();
  });

  it("shows MOSS loopback configuration and health in runtime settings", async () => {
    render(<App />);
    const dialog = await selectAdvancedTranscriptionMode("multispeaker");
    const provider = within(dialog).getByRole("region", { name: "MOSS 多人长音频服务" });
    const endpoint = within(provider).getByDisplayValue("http://127.0.0.1:8000");
    expect(endpoint).toBeInTheDocument();
    expect(endpoint.closest("label")?.querySelector("span > small")).toHaveTextContent("仅支持 127.0.0.1、localhost 或 ::1，不保存 API 密钥");
    expect(within(provider).getByText("服务可用")).toBeInTheDocument();
    expect(within(provider).getByText(/不会连接远程地址，也不会静默回退到 Whisper/)).toBeInTheDocument();
  });

  it("does not present a claimed Agent task as active processing before its first heartbeat", () => {
    const project = structuredClone(sampleProject);
    project.tasks = [{ ...project.tasks[0], status: "claimed", progress: 0, lastActivity: { kind: "claimed", progress: null, message: "Agent 已领取任务", createdAt: new Date().toISOString() } }];
    project.patchSets = [];
    project.edits = [];
    expect(taskLabel(project)).toBe("需要 Agent 继续");
  });

  it("distinguishes claimed and stale Agent tasks from active processing", () => {
    const now = Date.parse("2026-07-25T12:00:00.000Z");
    const task = structuredClone(sampleProject.tasks[0]);
    task.status = "claimed";
    task.lastActivity = {
      kind: "claimed",
      progress: null,
      message: "Agent 已领取任务",
      createdAt: "2026-07-25T11:59:00.000Z",
    };
    expect(agentTaskStatusLabel(task, now)).toBe("Agent 已领取，尚未报告处理活动");

    task.status = "running";
    task.lastActivity = {
      kind: "heartbeat",
      progress: 25,
      message: "处理中",
      createdAt: "2026-07-25T11:00:00.000Z",
    };
    expect(agentTaskStatusLabel(task, now)).toBe("暂未收到新的进度");
  });

  it("shows a three-way conflict and prevents applying a stale suggestion", () => {
    const review = vi.fn();
    render(<PatchReviewCard item={{
      id: "pi1", segmentId: "s1", target: "transcript", beforeText: "旧的项目名称",
      afterText: "建议的新名称", currentText: "人工修改", reason: "修正产品名",
      confidence: 0.88, status: "conflict",
    }} onReview={review} onSelect={() => undefined} />);
    expect(screen.getByText("状态冲突 · 当前文本已变化")).toBeInTheDocument();
    expect(screen.getByText("旧的项目名称")).toBeInTheDocument();
    expect(screen.getByText("人工修改")).toBeInTheDocument();
    expect(screen.getByText("建议的新名称")).toBeInTheDocument();
    expect(screen.getByText(/不能应用这条旧建议/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "应用建议" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "保留原文" }));
    expect(review).toHaveBeenCalledWith("keep");
  });

  it("prioritizes pending Agent patches as a confirmation state", () => {
    const project = structuredClone(sampleProject);
    project.tasks = [];
    project.patchSets = [{ id: "p1", taskId: "t1", kind: "polish", language: null, status: "pending_review", baseVersionId: "v1", createdAt: new Date().toISOString(), items: [{ id: "pi1", segmentId: "s1", target: "transcript", beforeText: "原文", afterText: "建议", currentText: "原文", reason: "校对", confidence: null, status: "pending" }] }];
    expect(taskLabel(project)).toBe("需要你确认");
  });

  it("does not let background Agent processing hide a confirmation", () => {
    const project = structuredClone(sampleProject);
    project.tasks = [{ ...project.tasks[0], status: "running" }];
    expect(taskLabel(project)).toBe("需要你确认");
  });

  it("treats a proposed soft cut as an explicit confirmation", () => {
    const project = structuredClone(sampleProject);
    project.tasks = [];
    project.patchSets = [];
    expect(taskLabel(project)).toBe("需要你确认");
  });
});
