import { expect, test } from "@playwright/test";

test("uses a compact command bar without horizontal overflow", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "发布口播 · 草稿" })).toBeVisible();

  const topbar = await page.locator(".topbar").boundingBox();
  const heading = await page.locator(".topbar h1").boundingBox();
  const commands = await page.getByLabel("项目命令").boundingBox();
  const exportButton = await page.getByRole("button", { name: "导出视频" }).boundingBox();

  expect(topbar).not.toBeNull();
  expect(heading).not.toBeNull();
  expect(commands).not.toBeNull();
  expect(exportButton).not.toBeNull();
  expect(heading!.height).toBeLessThan(32);
  expect(commands!.x + commands!.width).toBeLessThanOrEqual(1444);
  expect(exportButton!.x + exportButton!.width).toBeLessThanOrEqual(1444);
  expect(topbar!.height).toBeLessThan(90);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test("keeps the transcript primary at the minimum supported workspace size", async ({ page }) => {
  await page.setViewportSize({ width: 1080, height: 720 });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "发布口播 · 草稿" })).toBeVisible();

  const transcript = await page.locator(".transcript-panel").boundingBox();
  const context = await page.locator(".context-panel").boundingBox();
  const commands = await page.getByLabel("项目命令").boundingBox();
  const subtitleTools = await page.getByRole("region", { name: "字幕结构工具栏" }).boundingBox();

  expect(transcript).not.toBeNull();
  expect(context).not.toBeNull();
  expect(commands).not.toBeNull();
  expect(subtitleTools).not.toBeNull();
  expect(transcript!.width).toBeGreaterThanOrEqual(context!.width);
  expect(context!.y).toBeGreaterThanOrEqual(transcript!.y + transcript!.height - 1);
  expect(commands!.x + commands!.width).toBeLessThanOrEqual(1080);
  expect(subtitleTools!.x + subtitleTools!.width).toBeLessThanOrEqual(1080);
  await expect(page.getByRole("button", { name: "拆分" })).toBeAttached();
  await expect(page.getByRole("button", { name: "偏移" })).toBeAttached();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  await page.keyboard.press("Control+Shift+E");
  await expect(page.getByRole("complementary", { name: "导出设置" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("complementary", { name: "导出设置" })).toBeHidden();
});

test("keeps subtitle styling in export settings and previews the saved bilingual hierarchy", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");
  const original = await page.getByLabel("00:13 字幕文本").inputValue();
  await page.getByRole("checkbox", { name: "选择字幕 00:13 至 00:18" }).click();
  await page.getByRole("button", { name: "打开导出设置" }).click();
  const panel = page.getByLabel("导出设置");
  await panel.getByLabel("字幕模式").selectOption("bilingual");
  await panel.getByLabel("字幕样式预设").selectOption("emphasis");
  await expect(page.getByText("字幕样式已更新；正文和时间未修改，可撤销。")).toBeVisible();
  await panel.getByLabel("字幕位置").selectOption("center");

  const caption = page.locator(".caption-overlay");
  await expect(caption).toHaveAttribute("data-preset", "emphasis");
  await expect(caption).toHaveAttribute("data-position", "center");
  await expect(caption.locator(".caption-secondary")).toContainText("Today I want to explain why we are building a local-first editing workbench.");
  await expect(page.locator(".subtitle-safe-area")).toBeVisible();
  await panel.getByRole("checkbox", { name: "显示字幕安全区" }).uncheck();
  await expect(page.locator(".subtitle-safe-area")).toBeHidden();
  await expect(page.getByLabel("00:13 字幕文本")).toHaveValue(original);
  await expect(page.locator(".topbar").getByLabel("字幕样式预设")).toHaveCount(0);
});

test("selects subtitle ranges and confirms recoverable structure edits", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");
  const toolbar = page.getByRole("region", { name: "字幕结构工具栏" });
  await expect(toolbar.getByText(/1 段/)).toBeVisible();

  await page.getByRole("checkbox", { name: "选择字幕 00:13 至 00:18" }).click();
  await page.getByRole("checkbox", { name: "选择字幕 00:24 至 00:27" }).click({ modifiers: ["Shift"] });
  await expect(toolbar.getByText(/3 段/)).toBeVisible();
  await toolbar.getByRole("button", { name: "偏移" }).click();

  const offsetDialog = page.getByRole("dialog", { name: "批量偏移字幕" });
  await expect(offsetDialog.getByText(/作用范围：3 段/)).toBeVisible();
  await expect(offsetDialog.getByText(/操作会创建可恢复版本/)).toBeVisible();
  await offsetDialog.getByLabel(/统一偏移/).fill("0.250");
  await offsetDialog.getByRole("button", { name: "确认偏移 3 段" }).click();
  await expect(offsetDialog).toBeHidden();
  await expect(page.getByText(/已将 3 段字幕批量偏移 \+0.250 秒/)).toBeVisible();
  await expect(page.getByLabel("项目命令").getByRole("button", { name: "撤销" })).toBeEnabled();

  await page.getByLabel("字幕段 00:12 至 00:13").click();
  const editor = page.getByLabel("00:12 字幕文本");
  await editor.focus();
  await page.keyboard.press("Control+Shift+S");
  await expect(page.getByRole("dialog", { name: "拆分字幕" })).toBeHidden();
  await editor.evaluate((element) => element.blur());
  await page.keyboard.press("Control+Shift+S");
  const splitDialog = page.getByRole("dialog", { name: "拆分字幕" });
  await expect(splitDialog.getByRole("region", { name: "拆分预览" })).toBeVisible();
  await splitDialog.getByRole("button", { name: "确认拆分当前段" }).click();
  await expect(page.getByText(/字幕已拆分.*Ctrl\+Z 撤销/)).toBeVisible();
  await expect(page.getByLabel("字幕文稿列表").locator("textarea")).toHaveCount(5);
});

test("expands the editing workbench on a maximized 27-inch display", async ({ page }) => {
  await page.setViewportSize({ width: 2560, height: 1410 });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "发布口播 · 草稿" })).toBeVisible();

  const workbench = await page.locator(".workbench").boundingBox();
  const video = await page.locator(".video-panel").boundingBox();
  const videoFrame = await page.locator(".video-frame").boundingBox();
  const workflow = await page.locator(".review-panel").boundingBox();
  const workflowList = page.getByRole("region", { name: "工作流任务列表" });
  const transcript = await page.locator(".transcript-panel").boundingBox();
  const context = await page.locator(".context-panel").boundingBox();

  expect(workbench).not.toBeNull();
  expect(video).not.toBeNull();
  expect(videoFrame).not.toBeNull();
  expect(workflow).not.toBeNull();
  expect(transcript).not.toBeNull();
  expect(context).not.toBeNull();
  expect(workbench!.width).toBeGreaterThan(2200);
  expect(video!.width).toBeGreaterThan(1700);
  expect(videoFrame!.height).toBeGreaterThan(500);
  expect(workflow!.width).toBeGreaterThanOrEqual(400);
  expect(Math.abs(workflow!.height - video!.height)).toBeLessThanOrEqual(2);
  await expect(workflowList).toHaveCSS("overflow-y", "auto");
  expect(await workflowList.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
  expect(transcript!.height).toBeGreaterThan(500);
  expect(context!.height).toBeGreaterThan(500);
});

test("shows local speech rhythm evidence and locates a finding", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");

  const insights = page.getByRole("region", { name: "语音节奏" });
  await expect(insights).toBeVisible();
  await expect(insights.getByText("83.3")).toBeVisible();
  await expect(insights.getByText("词条/分钟")).toBeVisible();
  await expect(insights).toContainText("不会自动剪辑");

  await insights.getByRole("button", { name: /定位长停顿/ }).click();
  await expect(page.locator(".context-panel").getByRole("heading", { name: "你可以，你可以先看建议，再决定是否删除。" })).toBeVisible();
  await expect(page.getByText("成片 04:38 · 原片 04:38")).toBeVisible();
});

test("analyzes audio locally and sends measurable risks to the review queue", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");

  const quality = page.getByRole("region", { name: "音频质量" });
  await expect(quality).toBeVisible();
  await quality.getByRole("button", { name: "开始本地分析" }).click();
  await expect(quality.getByText("综合响度 LUFS")).toBeVisible();
  await expect(quality.getByText("-25.4", { exact: true })).toBeVisible();
  await expect(page.getByText("音频质量 · 等待确认")).toHaveCount(3);
  await expect(page.locator(".audio-risk-strip")).toContainText("3 项音频风险");
  await expect(page.getByText(/媒体不会上传，也不阻断编辑和导出/)).toBeVisible();
});

test("previews and explicitly replaces local subtitle files", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");

  await page.getByRole("button", { name: "导入字幕" }).click();
  const dialog = page.getByRole("dialog", { name: "导入字幕" });
  await expect(dialog.getByText(/确认前不会修改项目/)).toBeVisible();
  await dialog.getByRole("button", { name: "选择文件" }).click();
  const preview = dialog.getByRole("region", { name: "字幕导入预检" });
  await expect(preview.getByText("2 段字幕")).toBeVisible();
  await expect(preview.getByText("1 项质量提醒")).toBeVisible();
  const replace = preview.getByRole("button", { name: "确认替换字幕" });
  await expect(replace).toBeDisabled();
  await preview.getByRole("checkbox").check();
  await replace.click();

  await expect(dialog).toBeHidden();
  await expect(page.getByText(/已导入 2 段字幕并创建可撤销版本/)).toBeVisible();
  const transcript = page.getByLabel("字幕文稿列表");
  await expect(transcript.locator("textarea").first()).toHaveValue("导入后的第一条字幕");
  const quality = page.getByRole("region", { name: "字幕质量" });
  await expect(quality.getByText("1 项质量提醒")).toBeVisible();
  await expect(quality).toHaveClass(/warning/);
  await expect(quality.locator("svg.lucide-circle-alert").first()).toBeVisible();
  await quality.getByRole("button", { name: "提醒 1" }).click();
  await expect(transcript.locator("textarea")).toHaveCount(1);
  await expect(transcript.locator("textarea").first()).toHaveValue("导入后的第二条字幕");
});

test("separates runtime status cards from the transcription model control", async ({ page }) => {
  await page.setViewportSize({ width: 1368, height: 763 });
  await page.addInitScript(() => localStorage.setItem("siaocut.modelPath", "C:\\Models\\ggml-large-v3-turbo-q5_0-multilingual.bin"));
  await page.goto("/");

  await page.getByRole("button", { name: "删除项目 第二个本地项目" }).click();
  await page.getByRole("dialog", { name: "删除项目" }).getByRole("button", { name: "确认删除" }).click();
  await page.getByRole("button", { name: "取消任务" }).click();
  await page.getByRole("button", { name: "删除项目 发布口播 · 草稿" }).click();
  await page.getByRole("dialog", { name: "删除项目" }).getByRole("button", { name: "确认删除" }).click();

  await expect(page.getByRole("heading", { name: "从一段口播开始。" })).toBeVisible();
  const checklist = page.getByLabel("本机运行组件");
  const cards = checklist.locator(".runtime-components .runtime-row");
  const model = checklist.locator(".runtime-model-row");
  await expect(cards).toHaveCount(4);
  await expect(model).toContainText("转录模型");
  await expect(model).toContainText("ggml-large-v3-turbo-q5_0-multilingual.bin");
  await expect(model.getByRole("button", { name: "更换模型" })).toBeVisible();
  const topOffsets = await cards.evaluateAll((items) => items.map((item) => Math.round(item.getBoundingClientRect().top)));
  expect(new Set(topOffsets).size).toBe(1);
  const componentDetails = cards.locator("small");
  for (let index = 0; index < await componentDetails.count(); index += 1) {
    const detail = componentDetails.nth(index);
    expect(await detail.evaluate((element) => element.scrollWidth <= element.clientWidth && element.scrollHeight <= element.clientHeight)).toBe(true);
  }
  const cardsBox = await cards.first().locator("xpath=..").boundingBox();
  const modelBox = await model.boundingBox();
  expect(cardsBox).not.toBeNull();
  expect(modelBox).not.toBeNull();
  expect(modelBox!.y).toBeGreaterThan(cardsBox!.y + cardsBox!.height);
  expect(Math.abs(modelBox!.width - cardsBox!.width)).toBeLessThanOrEqual(1);
  const modelDetail = model.locator("small");
  expect(await modelDetail.evaluate((element) => element.scrollWidth <= element.clientWidth && element.scrollHeight <= element.clientHeight)).toBe(true);
});

test("reviews and edits a transcript from the workbench", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "发布口播 · 草稿" })).toBeVisible();
  await page.getByRole("button", { name: "运行环境" }).click();
  await expect(page.getByRole("dialog", { name: "运行环境" })).toBeVisible();
  await expect(page.getByText("API 0.1")).toBeVisible();
  await expect(page.getByText("平衡 · 推荐")).toBeVisible();
  await expect(page.getByText("下载前显示来源、体积与许可证")).toBeVisible();
  await page.getByRole("button", { name: "关闭运行环境" }).click();
  await expect(page.getByText("删除不影响含义的口语冗余")).toBeVisible();
  await expect(page.getByText("任务原文")).toBeVisible();
  await expect(page.getByText("Agent 建议")).toBeVisible();
  await page.getByRole("button", { name: "应用建议" }).click();
  await expect(page.getByText("已应用此条建议，并创建可恢复版本。")).toBeVisible();
  await page.getByRole("button", { name: "应用软剪辑" }).click();
  await expect(page.getByText("已应用软剪辑；预览时间线已更新，原片未修改。")).toBeVisible();
  await expect(page.getByText("成片 04:37 · 原片 04:38")).toBeVisible();
  await page.getByText("恢复此处").click();
  await expect(page.getByText("已恢复此处；预览时间线已更新。")).toBeVisible();
  const wordEvidence = page.getByRole("region", { name: "词级时间" });
  await wordEvidence.getByRole("button", { name: "嗯" }).click();
  await wordEvidence.getByLabel("安全留白").selectOption("200");
  await wordEvidence.getByRole("button", { name: "创建并试听" }).click();
  await expect(page.getByText("切点已创建；生成媒体预览后可试听切点前后 1 秒。")).toBeVisible();
  const wordCut = page.locator("article.review-item").filter({ hasText: "词范围：嗯" });
  await expect(wordCut.getByRole("button", { name: "试听切点" })).toBeVisible();
  await wordCut.getByRole("button", { name: "应用软剪辑" }).click();
  const commands = page.getByLabel("项目命令");
  await commands.getByRole("button", { name: "撤销" }).click();
  await expect(page.getByText("已撤销上一步项目修改。")).toBeVisible();
  await commands.getByRole("button", { name: "重做" }).click();
  await expect(page.getByText("已重做项目修改。")).toBeVisible();
  await page.getByRole("button", { name: "检测粗剪建议" }).click();
  await expect(page.getByText("发现 1 条粗剪建议；试听并确认后才会应用。")).toBeVisible();
  await expect(page.getByText("说话重启：你可以")).toBeVisible();
  await expect(page.getByText("需要人工确认 · 说话重启")).toBeVisible();
  const editor = page.getByLabel("00:13 字幕文本");
  await editor.fill("人工修订后的原文。");
  await editor.blur();
  await expect(page.getByText("原文已更新；对应译文需要更新。")).toBeVisible();
  await expect(page.getByText("需要更新", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "打开导出设置" }).click();
  const exportPanel = page.getByLabel("导出设置");
  await exportPanel.getByLabel("画布比例").selectOption("9:16");
  await expect(page.getByText("画布已改为 9:16；请重新生成预览以查看最终构图。")).toBeVisible();
  await expect(exportPanel.getByLabel("竖屏构图")).toBeEnabled();
  await exportPanel.getByLabel("字幕模式").selectOption("source");
  await exportPanel.getByRole("button", { name: "导出字幕" }).click();
  await expect(page.getByText(/字幕已导出到/)).toBeVisible();
  await exportPanel.getByRole("button", { name: "导出视频" }).click();
  await expect(page.getByText(/视频已导出到/)).toBeVisible();
});

test("confirms and controls an audited URL import", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "从 URL 导入" }).click();
  const dialog = page.getByRole("dialog", { name: "URL 导入" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("公开视频 URL").fill("https://www.youtube.com/watch?v=HOfdboHvshg");
  await dialog.getByRole("button", { name: "读取视频信息" }).click();
  const preview = dialog.getByRole("region", { name: "待确认视频信息" });
  await expect(preview.getByText("Sintel Trailer, Durian Open Movie Project")).toBeVisible();
  await expect(preview.getByText("HOfdboHvshg", { exact: true })).toBeVisible();
  const start = preview.getByRole("button", { name: "确认信息并开始下载" });
  await expect(start).toBeDisabled();
  await preview.getByRole("checkbox").check();
  await start.click();
  const job = dialog.getByRole("region", { name: "URL 导入任务" });
  await expect(job.getByText("媒体校验通过后创建")).toBeVisible();
  await job.getByRole("button", { name: "取消并保留分片" }).click();
  await expect(job.getByText("已取消")).toBeVisible();
  await expect(job.getByRole("button", { name: "显式继续" })).toBeVisible();
});

test("runs a resumable one-click workflow through the human review gate", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "一键成片", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "一键工作流" });
  await expect(dialog.getByText(/粗剪和 Agent 结果不会自动应用/)).toBeVisible();
  const start = dialog.getByRole("button", { name: "启动一键工作流" });
  await expect(start).toBeDisabled();
  await dialog.getByRole("button", { name: "选择文件" }).click();
  await expect(dialog.getByText("demo.mp4")).toBeVisible();
  await start.click();
  const status = page.getByRole("region", { name: "一键工作流状态" });
  await expect(status.getByText(/需要你确认 · 等待人工确认/)).toBeVisible({ timeout: 5000 });
  await status.getByRole("button", { name: "确认完成并继续" }).click();
  await expect(status.getByText("仍有 Agent 修改或粗剪建议等待人工处理")).toBeVisible();
  await page.getByRole("button", { name: "应用软剪辑" }).click();
  await expect(page.getByText(/已应用软剪辑/)).toBeVisible();
  await status.getByRole("button", { name: "确认完成并继续" }).click();
  await expect(status.getByText(/已完成 · 流程完成/)).toBeVisible({ timeout: 3000 });
  await expect(page.getByText(/一键工作流已完成，视频已导出到/)).toBeVisible();
});
