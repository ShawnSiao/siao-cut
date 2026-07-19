import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import App, { PatchReviewCard, TRANSCRIPTION_LANGUAGE_STORAGE_KEY, clearTransientCoreError, getProjectCapabilities, isHttpsSourceUrl, parseExportPreferences, parseTranscriptionLanguage, resolveCanvasMedia, resolvePlaybackDuration, shouldCheckForUpdates, startSerialPolling, taskLabel } from "./App";
import { sampleProject } from "./mock";

afterEach(() => {
  cleanup();
  localStorage.removeItem("siaocut.exportPreferences.v1");
  localStorage.removeItem(TRANSCRIPTION_LANGUAGE_STORAGE_KEY);
  vi.useRealTimers();
});

describe("SiaoCut review workbench", () => {
  it("falls back to source media when a stale canvas preview cannot be authorized", async () => {
    const result = await resolveCanvasMedia(
      "p-test",
      async () => { throw new Error("preview stale"); },
      async () => "asset://source.mp4",
    );

    expect(result).toEqual({ mediaUrl: "asset://source.mp4", warning: "preview stale" });
  });

  it("persists source language independently and creates the selected Agent workflow", async () => {
    render(<App />);
    const newProject = await screen.findByRole("button", { name: "新建项目" });
    const agentButton = screen.getByRole("button", { name: "交给 Agent" });
    expect(agentButton).toBeDisabled();
    expect(agentButton).toHaveAttribute("title", "请先导入或重新定位本地媒体。");
    fireEvent.click(newProject);
    await waitFor(() => expect(agentButton).toBeEnabled());

    fireEvent.change(screen.getByRole("combobox", { name: "素材语言" }), { target: { value: "en" } });
    expect(localStorage.getItem(TRANSCRIPTION_LANGUAGE_STORAGE_KEY)).toBe("en");
    expect(parseTranscriptionLanguage("unsupported")).toBe("auto");

    fireEvent.change(screen.getByRole("combobox", { name: "Agent 工作流" }), { target: { value: "edit" } });
    fireEvent.click(screen.getByRole("button", { name: "交给 Agent" }));
    await waitFor(() => expect(screen.getAllByText("edit").length).toBeGreaterThan(0));
    expect(screen.getByRole("region", { name: "等待 Codex Worker" })).toHaveTextContent("不包含媒体文件或路径");

    fireEvent.click(screen.getByRole("button", { name: "一键成片" }));
    expect(screen.getByRole("combobox", { name: "素材语言 · 一键成片" })).toHaveValue("en");
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
    expect(screen.getByRole("menu")).toBeInTheDocument();
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
    await waitFor(() => expect(screen.getByRole("button", { name: "交给 Agent" })).toBeEnabled());
    fireEvent.change(workflow, { target: { value: "translate" } });
    const target = screen.getByRole("combobox", { name: "翻译目标语言" });
    expect(target).toHaveValue("en");
    fireEvent.change(target, { target: { value: "ja" } });
    fireEvent.click(screen.getByRole("button", { name: "交给 Agent" }));
    await waitFor(() => expect(screen.getByText(/翻译工作流已创建，目标语言为 JA/)).toBeInTheDocument());
    expect(screen.getAllByText("JA").length).toBeGreaterThan(0);
  });

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

  it("shows why preview builds cannot install updates", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "运行环境" }));
    const updates = screen.getByRole("region", { name: "应用更新" });
    expect(within(updates).getByText("当前版本 0.2.0-preview · 每 24 小时检查")).toBeInTheDocument();
    expect(within(updates).getByText("浏览器预览不连接更新源。")).toBeInTheDocument();
    expect(within(updates).getByRole("button", { name: "手动检查更新" })).toBeDisabled();
  });

  it("loads the browser preview project and exposes the three-layer workbench", async () => {
    render(<App />);
    expect(await screen.findByRole("heading", { name: "发布口播 · 草稿" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "转录" })).toBeInTheDocument();
    expect(screen.getByText("字幕时间轴")).toBeInTheDocument();
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
    const dialog = screen.getByRole("dialog", { name: "删除项目" });
    expect(within(dialog).getByText(/原始音视频文件不会删除或修改/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "确认删除" }));

    await waitFor(() => expect(screen.queryByRole("button", { name: /第二个本地项目/ })).not.toBeInTheDocument());
    expect(screen.getByText("项目「第二个本地项目」已删除；原始媒体文件未被修改。")).toBeInTheDocument();
  });

  it("keeps an active-task deletion warning inside the confirmation dialog", async () => {
    render(<App />);
    await screen.findByRole("heading", { name: "发布口播 · 草稿" });

    fireEvent.click(screen.getByRole("button", { name: "删除项目 发布口播 · 草稿" }));
    const dialog = screen.getByRole("dialog", { name: "删除项目" });

    expect(within(dialog).getByRole("alert")).toHaveTextContent("该项目有 1 项正在运行或等待 Agent 处理的任务");
    expect(within(dialog).getByRole("button", { name: "确认删除" })).toBeDisabled();
    expect(screen.queryByText(/project_busy/)).not.toBeInTheDocument();
  });

  it("allows choosing an existing translation language while source subtitles are selected", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "打开导出设置" }));
    const language = await screen.findByLabelText("译文语言");

    expect(screen.getByLabelText("字幕模式")).toHaveValue("source");
    expect(language).toBeEnabled();
    expect(language).toHaveValue("en");
  });

  it("marks translation stale after source text changes", async () => {
    render(<App />);
    const editor = await screen.findByLabelText("00:13 字幕文本");
    fireEvent.change(editor, { target: { value: "这是一段人工修订后的原文。" } });
    fireEvent.blur(editor);
    await waitFor(() => expect(screen.getByText("需要更新")).toBeInTheDocument());
  });

  it("exposes word timing evidence for the selected segment", async () => {
    render(<App />);
    const evidence = await screen.findByRole("region", { name: "词级时间" });
    expect(within(evidence).getByRole("button", { name: "嗯" })).toHaveAttribute("title", expect.stringContaining("52%"));
    expect(screen.getByText("ZH · 4 段 · 5 词")).toBeInTheDocument();
  });

  it("supports keyboard navigation between inspector tabs", async () => {
    render(<App />);
    const segmentTab = await screen.findByRole("tab", { name: "当前字幕" });
    segmentTab.focus();
    fireEvent.keyDown(segmentTab, { key: "ArrowRight" });
    const analysisTab = screen.getByRole("tab", { name: "本地分析" });
    expect(analysisTab).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(analysisTab).toHaveFocus());
    expect(screen.getByRole("region", { name: "语音节奏" })).toBeInTheDocument();
  });

  it("shows local speech rhythm evidence without applying edits", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("tab", { name: "本地分析" }));
    const insights = await screen.findByRole("region", { name: "语音节奏" });
    expect(within(insights).getByText("83.3")).toBeInTheDocument();
    expect(within(insights).getByText("词条/分钟")).toBeInTheDocument();
    expect(within(insights).getByText("只提供定位证据，不会自动剪辑。", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("成片 04:38 · 原片 04:38")).toBeInTheDocument();

    fireEvent.click(within(insights).getByRole("button", { name: /定位长停顿/ }));
    fireEvent.click(screen.getByRole("tab", { name: "当前字幕" }));
    expect(screen.getByRole("heading", { name: "你可以，你可以先看建议，再决定是否删除。" })).toBeInTheDocument();
  });

  it("runs local audio quality analysis and exposes measurable risks for review", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "新建项目" }));
    fireEvent.click(await screen.findByRole("tab", { name: "本地分析" }));
    const quality = await screen.findByRole("region", { name: "音频质量" });
    await waitFor(() => expect(within(quality).getByRole("button", { name: "开始本地分析" })).toBeEnabled());
    expect(within(quality).getByText(/综合响度、峰值、静音区间和疑似削波/)).toBeInTheDocument();
    fireEvent.click(within(quality).getByRole("button", { name: "开始本地分析" }));

    await waitFor(() => expect(within(quality).getByText("-25.4")).toBeInTheDocument());
    expect(within(quality).getByText("3")).toBeInTheDocument();
    expect(screen.getAllByText("音频质量 · 等待确认")).toHaveLength(3);
    expect(screen.getByText(/实测 1.3 秒 · 阈值 0.8 秒/)).toBeInTheDocument();
    expect(screen.getByText(/媒体不会上传，也不阻断编辑和导出/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^第二个本地项目/ }));
    await screen.findByRole("heading", { name: "第二个本地项目" });
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
    const evidence = await screen.findByRole("region", { name: "词级时间" });
    fireEvent.click(within(evidence).getByRole("button", { name: "嗯" }));
    expect(within(evidence).getByLabelText("剪切起点")).toHaveValue("0");
    expect(within(evidence).getByLabelText("剪切终点")).toHaveValue("0");
    fireEvent.change(within(evidence).getByLabelText("安全留白"), { target: { value: "200" } });
    fireEvent.click(within(evidence).getByRole("button", { name: "创建并试听" }));
    await waitFor(() => expect(screen.getByText("切点已创建；生成媒体预览后可试听切点前后 1 秒。")).toBeInTheDocument());
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
    fireEvent.keyDown(editor, { key: "Enter", code: "Enter", ctrlKey: true });
    await waitFor(() => expect(screen.getByText("原文已更新；对应译文需要更新。")).toBeInTheDocument());
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

    await waitFor(() => expect(screen.getByText(/相邻字幕已合并.*Ctrl\+Z 撤销/)).toBeInTheDocument());
    expect(screen.getAllByRole("textbox", { name: /字幕文本/ })).toHaveLength(3);
    expect(screen.getByDisplayValue(/今天想和大家聊聊.*它不是替你决定内容/)).toBeInTheDocument();
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

    fireEvent.click(screen.getByRole("button", { name: "打开导出设置" }));
    const exportPanel = screen.getByLabelText("导出设置");
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
    const dialog = screen.getByRole("dialog", { name: "导入字幕" });
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
    const quality = screen.getByRole("region", { name: "字幕质量" });
    expect(within(quality).getByText("1 项质量提醒")).toBeInTheDocument();
    fireEvent.click(within(quality).getByRole("button", { name: "提醒 1" }));
    expect(within(transcript).queryByDisplayValue("导入后的第一条字幕")).not.toBeInTheDocument();
    expect(within(transcript).getByDisplayValue("导入后的第二条字幕")).toBeInTheDocument();
    fireEvent.click(within(quality).getByRole("button", { name: /与上一条字幕时间重叠/ }));
    expect(within(transcript).getByDisplayValue("导入后的第二条字幕").closest("article")).toHaveClass("selected");
  });

  it("updates the vertical canvas and exposes explicit subtitle modes", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "打开导出设置" }));
    const canvas = await screen.findByLabelText("画布比例");
    fireEvent.change(canvas, { target: { value: "9:16" } });
    await waitFor(() => expect(screen.getByText("画布已改为 9:16；请重新生成预览以查看最终构图。")).toBeInTheDocument());
    expect(screen.getByLabelText("竖屏构图")).toBeEnabled();
    fireEvent.change(screen.getByLabelText("字幕模式"), { target: { value: "translated" } });
    expect(screen.getByLabelText("译文语言")).toBeEnabled();
    expect(screen.getByText(/不会隐藏原片已烧录字幕/)).toBeInTheDocument();
  });

  it("previews the saved subtitle style, safe area, and bilingual hierarchy without changing text", async () => {
    render(<App />);
    const transcriptText = await screen.findByLabelText("00:13 字幕文本");
    const originalText = (transcriptText as HTMLTextAreaElement).value;
    fireEvent.click(screen.getByRole("checkbox", { name: "选择字幕 00:13 至 00:18" }));
    fireEvent.click(screen.getByRole("button", { name: "打开导出设置" }));
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
    expect(screen.getByRole("dialog", { name: "URL 导入" })).toBeInTheDocument();
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
    expect(within(preview).getByText("yt-dlp 2026.06.09")).toBeInTheDocument();
    const start = within(preview).getByRole("button", { name: "确认信息并开始下载" });
    expect(start).toBeDisabled();
    fireEvent.click(within(preview).getByRole("checkbox"));
    expect(start).toBeEnabled();
  });

  it("cancels a URL import without a project and only resumes explicitly", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "从 URL 导入" }));
    fireEvent.change(screen.getByLabelText("公开视频 URL"), { target: { value: "https://www.youtube.com/watch?v=HOfdboHvshg" } });
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
    fireEvent.change(screen.getByLabelText("公开视频 URL"), { target: { value: "https://www.youtube.com/watch?v=HOfdboHvshg" } });
    fireEvent.click(screen.getByRole("button", { name: "读取视频信息" }));
    const preview = await screen.findByRole("region", { name: "待确认视频信息" });
    fireEvent.click(within(preview).getByRole("checkbox"));
    fireEvent.click(within(preview).getByRole("button", { name: "确认信息并开始下载" }));
    await waitFor(() => expect(screen.getByRole("heading", { name: "Sintel Trailer, Durian Open Movie Project" })).toBeInTheDocument(), { timeout: 4000 });
    expect(screen.queryByRole("dialog", { name: "URL 导入" })).not.toBeInTheDocument();
    expect(screen.getByText(/原 URL、站点媒体 ID、工具版本和文件哈希已保存/)).toBeInTheDocument();
  });

  it("starts a one-click local workflow and removes its progress panel after cancellation", async () => {
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
    await waitFor(() => expect(screen.queryByRole("region", { name: "一键工作流状态" })).not.toBeInTheDocument());
    expect(screen.getByText("自动工作流已取消；已完成的本地项目和中间证据仍然保留。")).toBeInTheDocument();
  });

  it("allows a workflow waiting for Agent translation to be cancelled", async () => {
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

    await waitFor(() => expect(screen.queryByRole("region", { name: "一键工作流状态" })).not.toBeInTheDocument());
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
    const settings = await screen.findByRole("button", { name: "运行环境" });
    fireEvent.click(settings);
    expect(screen.getByRole("dialog", { name: "运行环境" })).toBeInTheDocument();
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
    const settings = await screen.findByRole("button", { name: "运行环境" });
    fireEvent.click(settings);
    expect(screen.getByRole("button", { name: "关闭运行环境" })).toHaveFocus();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "运行环境" })).not.toBeInTheDocument());
    expect(settings).toHaveFocus();
  });

  it("requires an explicit model download and selects it after verification", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "运行环境" }));
    const option = screen.getByText("平衡 · 推荐").closest("article");
    expect(option).not.toBeNull();
    fireEvent.click(within(option!).getByRole("button", { name: "下载" }));
    await waitFor(() => expect(screen.getByText("模型已下载并通过 SHA-256 校验，可以开始本地转录。")).toBeInTheDocument());
    expect(within(option!).getByText("使用中")).toBeInTheDocument();
  });

  it("installs the optional speaker package explicitly and keeps speaker edits reviewable", async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole("tab", { name: "本地分析" }));
    const speakerPanel = await screen.findByRole("region", { name: "说话人轨" });
    expect(within(speakerPanel).getByText(/可选模型尚未安装/)).toBeInTheDocument();
    expect(screen.getByLabelText("00:12 字幕文本")).toHaveValue("嗯，");

    fireEvent.click(within(speakerPanel).getByRole("button", { name: "查看模型来源与安装" }));
    const dialog = screen.getByRole("dialog", { name: "运行环境" });
    const packagePanel = within(dialog).getByRole("region", { name: "说话人模型包" });
    expect(within(packagePanel).getByText("sherpa-onnx 1.13.2 · CPU 本地运行")).toBeInTheDocument();
    expect(within(packagePanel).getByText("Apache-2.0 / MIT")).toBeInTheDocument();
    expect(within(packagePanel).getByText(/只有点击「明确安装」/)).toBeInTheDocument();
    fireEvent.click(within(packagePanel).getByRole("button", { name: "明确安装" }));
    await waitFor(() => expect(within(packagePanel).getByText("可以开始本地说话人分析")).toBeInTheDocument());
    fireEvent.click(within(dialog).getByRole("button", { name: "关闭运行环境" }));

    fireEvent.click(within(speakerPanel).getByRole("button", { name: "开始本地分析" }));
    await waitFor(() => expect(within(speakerPanel).getByLabelText("当前字幕说话人")).toHaveValue("voice-a"));
    expect(screen.getByLabelText("00:12 字幕文本")).toHaveValue("嗯，");

    const name = within(speakerPanel).getByLabelText("说话人 1名称");
    fireEvent.change(name, { target: { value: "主持人" } });
    fireEvent.blur(name);
    await waitFor(() => expect(screen.getByText(/说话人名称已更新，可撤销或从版本历史恢复/)).toBeInTheDocument());
    expect(within(speakerPanel).getByLabelText("主持人名称")).toBeInTheDocument();
    fireEvent.change(within(speakerPanel).getByLabelText("当前字幕说话人"), { target: { value: "voice-b" } });
    await waitFor(() => expect(screen.getByText(/当前字幕段的说话人已更新，可撤销或从版本历史恢复/)).toBeInTheDocument());
    expect(screen.getByLabelText("00:12 字幕文本")).toHaveValue("嗯，");
  });

  it("maps interrupted tasks to an actionable Agent state", () => {
    const project = structuredClone(sampleProject);
    project.tasks = [{ ...project.tasks[0], status: "interrupted" }];
    project.patchSets = [];
    project.edits = [];
    expect(taskLabel(project)).toBe("需要 Agent 继续");
  });

  it("shows a three-way conflict and requires an explicit review action", () => {
    const apply = vi.fn();
    render(<PatchReviewCard item={{
      id: "pi1", segmentId: "s1", target: "transcript", beforeText: "旧的项目名称",
      afterText: "建议的新名称", currentText: "人工修改", reason: "修正产品名",
      confidence: 0.88, status: "conflict",
    }} onReview={apply} onSelect={() => undefined} />);
    expect(screen.getByText("状态冲突 · 当前文本已变化")).toBeInTheDocument();
    expect(screen.getByText("旧的项目名称")).toBeInTheDocument();
    expect(screen.getByText("人工修改")).toBeInTheDocument();
    expect(screen.getByText("建议的新名称")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "应用建议" }));
    expect(apply).toHaveBeenCalledWith("apply");
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
