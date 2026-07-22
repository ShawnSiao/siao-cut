import { expect, test, type Page } from "@playwright/test";

async function bindMockMedia(page: Page) {
  await page.getByRole("button", { name: "更多命令" }).click();
  await page.getByRole("menuitem", { name: "重新定位原片" }).click();
  await expect(page.getByText("已重新定位原片；内容哈希与项目记录一致。")).toBeVisible();
}

test("switches the application chrome to English without reloading the project", async ({ page }) => {
  await page.goto("/");
  const projectHeading = page.getByRole("heading", { name: "发布口播 · 草稿" });
  await expect(projectHeading).toBeVisible();
  await bindMockMedia(page);

  await page.getByRole("combobox", { name: "界面语言" }).selectOption("en-US");

  await expect(page.getByText("已重新定位原片；内容哈希与项目记录一致。")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "New project" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Review suggestions" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Send to local Codex" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Export" })).toBeVisible();
  await page.getByRole("button", { name: "Runtime" }).click();
  const runtime = page.getByRole("dialog", { name: "Runtime" });
  await runtime.getByRole("combobox", { name: "Source language" }).selectOption("en");
  await expect(runtime.getByRole("combobox", { name: "Source language" })).toHaveValue("en");
  await runtime.getByRole("button", { name: "Close the running environment" }).click();
  await page.getByRole("combobox", { name: "Agent workflow" }).selectOption("edit");
  await page.getByRole("button", { name: "Manual handoff" }).click();
  await page.getByRole("checkbox", { name: "I will continue in an external Agent tool that can access this computer's SiaoCut Core." }).check();
  await page.getByRole("button", { name: "Create handoff task" }).click();
  await expect(page.getByRole("heading", { name: "Task ready to hand off" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Complete instructions to copy to the external Agent" })).toHaveValue(/task claim/);
  await expect(projectHeading).toHaveText("发布口播 · 草稿");
  await expect(page.locator("html")).toHaveAttribute("lang", "en-US");
});

test("uses a compact command bar without horizontal overflow", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "发布口播 · 草稿" })).toBeVisible();

  const topbar = await page.locator(".topbar").boundingBox();
  const heading = await page.locator(".topbar h1").boundingBox();
  const commands = await page.getByLabel("项目命令").boundingBox();
  const primaryAction = await page.getByRole("button", { name: "审阅建议" }).boundingBox();

  expect(topbar).not.toBeNull();
  expect(heading).not.toBeNull();
  expect(commands).not.toBeNull();
  expect(primaryAction).not.toBeNull();
  expect(heading!.height).toBeLessThan(32);
  expect(commands!.x + commands!.width).toBeLessThanOrEqual(1444);
  expect(primaryAction!.x + primaryAction!.width).toBeLessThanOrEqual(1444);
  expect(topbar!.height).toBeLessThan(90);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test("keeps command groups non-overlapping in Chinese and English", async ({ page }) => {
  const viewports = [
    { width: 1280, height: 720 },
    { width: 1440, height: 900 },
    { width: 1920, height: 1080 },
  ];

  for (const locale of ["zh-CN", "en-US"]) {
    for (const viewport of viewports) {
      await page.setViewportSize(viewport);
      await page.goto("/");
      await page.locator(".locale-switch select").selectOption(locale);
      const commandBar = await page.locator(".creator-command-bar").boundingBox();
      expect(commandBar).not.toBeNull();
      expect(commandBar!.x).toBeGreaterThanOrEqual(0);
      expect(commandBar!.x + commandBar!.width).toBeLessThanOrEqual(viewport.width + 1);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      expect(await page.locator(".command-bar button, .command-bar select").evaluateAll((elements) => elements.every((element) => {
        const rect = element.getBoundingClientRect();
        return rect.left >= 0 && rect.right <= window.innerWidth + 1 && rect.width > 0 && rect.height > 0;
      }))).toBe(true);
      await expect(page.locator(".creator-drawer")).toHaveCount(1);
      await expect(page.locator('.creator-drawer [role="tab"][aria-selected="true"]')).toHaveCount(1);
      await page.getByRole("tab", { name: locale === "zh-CN" ? "导出" : "Export" }).click();
      await expect(page.locator(".creator-drawer .export-panel")).toBeVisible();
      await page.getByRole("tab", { name: locale === "zh-CN" ? /^审阅/ : /^Review/ }).click();
      await expect(page.locator(".creator-drawer .export-panel")).toHaveCount(0);
    }
  }
});

test("uses direct transcript keys for time-confirmed split and adjacent merge", async ({ page }) => {
  await page.goto("/");
  const editor = page.getByLabel("00:13 字幕文本");
  await editor.evaluate((element) => {
    const textarea = element as HTMLTextAreaElement;
    textarea.focus();
    textarea.setSelectionRange(7, 7);
  });
  await editor.press("Enter");

  const split = page.getByRole("dialog", { name: "拆分字幕" });
  await expect(split.getByText(/缺少可信的词级时间/)).toBeVisible();
  await split.getByRole("spinbutton", { name: /时间拆分点/ }).fill("15.500");
  await split.getByRole("button", { name: "确认拆分当前段" }).click();
  await expect(page.getByLabel("字幕文稿列表").locator("textarea")).toHaveCount(5);

  const secondHalf = page.getByLabel("00:15 字幕文本");
  await secondHalf.evaluate((element) => {
    const textarea = element as HTMLTextAreaElement;
    textarea.focus();
    textarea.setSelectionRange(0, 0);
  });
  await secondHalf.press("Backspace");
  const merge = page.getByRole("dialog", { name: "合并字幕" });
  await expect(merge.getByText(/作用范围：2 段/)).toBeVisible();
  await merge.getByRole("button", { name: "确认合并 2 段" }).click();
  await expect(page.getByLabel("字幕文稿列表").locator("textarea")).toHaveCount(4);
});

test("runs local Codex and keeps every result pending review", async ({ page }) => {
  await page.goto("/");
  await bindMockMedia(page);
  const editor = page.getByLabel("00:13 字幕文本");
  const original = await editor.inputValue();
  await page.getByRole("button", { name: "交给本机 Codex" }).click();
  await expect(page.getByText("本机 Codex 已启动；完成后仍需逐条审阅。")).toBeVisible();
  await expect(page.getByText("本机 Codex 已完成；建议已进入集中审阅，文稿未自动修改。")).toBeVisible({ timeout: 5000 });
  await expect(editor).toHaveValue(original);
  await expect(page.getByText("本机 Codex 提供的待审建议")).toBeVisible();
});

test("keeps the transcript primary at the minimum supported workspace size", async ({ page }) => {
  await page.setViewportSize({ width: 1080, height: 720 });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "发布口播 · 草稿" })).toBeVisible();

  const transcript = await page.locator(".transcript-panel").boundingBox();
  const context = await page.locator(".creator-drawer").boundingBox();
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
  const transcriptList = page.getByLabel("字幕文稿列表");
  const transcriptListLayout = await transcriptList.evaluate((element) => {
    const style = getComputedStyle(element);
    return { overflowY: style.overflowY, height: element.getBoundingClientRect().height };
  });
  expect(transcriptListLayout.overflowY).toBe("auto");
  expect(transcriptListLayout.height).toBeLessThanOrEqual(560);
  await page.getByRole("checkbox", { name: "选择字幕 00:13 至 00:18" }).click();
  await page.getByRole("tab", { name: "导出" }).click();
  const panel = page.getByLabel("导出设置");
  await panel.getByLabel("字幕模式").selectOption("bilingual");
  await expect(panel.getByLabel("字幕模式")).toHaveValue("bilingual");
  await expect(panel.getByLabel("译文语言")).toHaveValue("en");
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

test("uses the full transcript panel height without leaving an empty footer", async ({ page }) => {
  await page.setViewportSize({ width: 2560, height: 720 });
  await page.goto("/");

  const panel = await page.locator(".transcript-panel").boundingBox();
  const list = await page.getByLabel("字幕文稿列表").boundingBox();
  expect(panel).not.toBeNull();
  expect(list).not.toBeNull();
  expect(Math.abs(panel!.y + panel!.height - (list!.y + list!.height))).toBeLessThanOrEqual(2);
  await expect(page.getByLabel("字幕文稿列表")).toHaveCSS("overflow-y", "auto");
});

test("keeps translated subtitle modes selected before a translation exists", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /^第二个本地项目/ }).click();
  await expect(page.getByRole("heading", { name: "第二个本地项目" })).toBeVisible();
  await page.getByRole("tab", { name: "导出" }).click();

  const panel = page.getByLabel("导出设置");
  const subtitleMode = panel.getByLabel("字幕模式");
  await subtitleMode.selectOption("translated");
  await expect(subtitleMode).toHaveValue("translated");
  await expect(panel.getByRole("alert")).toContainText("项目中没有可用译文");
  await expect(panel.getByRole("button", { name: "导出字幕" })).toBeDisabled();
  await subtitleMode.selectOption("bilingual");
  await expect(subtitleMode).toHaveValue("bilingual");
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

  await page.getByRole("button", { name: "定位到 00:13" }).click();
  await expect(toolbar.getByText(/^1 段/)).toBeVisible();
  const editor = page.getByLabel("00:13 字幕文本");
  await editor.focus();
  await page.keyboard.press("Control+Shift+S");
  await expect(page.getByRole("dialog", { name: "拆分字幕" })).toBeHidden();
  await editor.evaluate((element) => element.blur());
  await toolbar.getByRole("button", { name: "拆分" }).click();
  const splitDialog = page.getByRole("dialog", { name: "拆分字幕" });
  await expect(splitDialog.getByRole("region", { name: "拆分预览" })).toBeVisible();
  await expect(splitDialog.getByText(/缺少可信的词级时间/)).toBeVisible();
  await splitDialog.getByRole("spinbutton", { name: /时间拆分点/ }).fill("15.500");
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
  const workflow = await page.locator(".creator-drawer").boundingBox();
  const drawerBody = page.locator(".creator-drawer-body");
  const transcript = await page.locator(".transcript-panel").boundingBox();

  expect(workbench).not.toBeNull();
  expect(video).not.toBeNull();
  expect(videoFrame).not.toBeNull();
  expect(workflow).not.toBeNull();
  expect(transcript).not.toBeNull();
  expect(workbench!.width).toBeGreaterThan(2200);
  expect(video!.width).toBeGreaterThan(1500);
  expect(videoFrame!.height).toBeGreaterThan(500);
  expect(workflow!.width).toBeGreaterThanOrEqual(340);
  expect(workflow!.height).toBeGreaterThan(500);
  await expect(drawerBody).toHaveCSS("overflow-y", "auto");
  expect(await drawerBody.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
  expect(transcript!.height).toBeGreaterThan(500);
});

test("shows local speech rhythm evidence and locates a finding", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");
  await page.getByRole("tab", { name: "分析" }).click();

  const insights = page.getByRole("region", { name: "语音节奏" });
  await expect(insights).toBeVisible();
  await expect(insights.getByText("83.3")).toBeVisible();
  await expect(insights.getByText("词条/分钟")).toBeVisible();
  await expect(insights).toContainText("不会自动剪辑");

  await insights.getByRole("button", { name: /定位长停顿/ }).click();
  await expect(page.locator(".segment-row").filter({ has: page.getByLabel("00:24 字幕文本") })).toHaveClass(/active/);
  await expect(page.getByText("成片 04:38 · 原片 04:38")).toBeVisible();
});

test("analyzes audio locally and sends measurable risks to the review queue", async ({ page }) => {
  await page.setViewportSize({ width: 1444, height: 972 });
  await page.goto("/");
  await bindMockMedia(page);
  await page.getByRole("tab", { name: "分析" }).click();

  const quality = page.getByRole("region", { name: "音频质量" });
  await expect(quality).toBeVisible();
  await quality.getByRole("button", { name: "开始本地分析" }).click();
  await expect(quality.getByText("综合响度 LUFS")).toBeVisible();
  await expect(quality.getByText("-25.4", { exact: true })).toBeVisible();
  await page.getByRole("tab", { name: /^审阅/ }).click();
  await expect(page.getByText("音频质量 · 等待确认")).toHaveCount(3);
  await expect(page.locator(".audio-risk-strip")).toContainText("3 项音频风险");
  await expect(page.getByText(/媒体不会上传，也不阻断编辑和导出/)).toBeVisible();
});

test("keeps core controls usable across supported desktop viewports", async ({ page }) => {
  const viewports = [
    { width: 1080, height: 720 },
    { width: 1280, height: 720 },
    { width: 1440, height: 900 },
    { width: 1920, height: 1080 },
    { width: 2209, height: 1290 },
  ];

  for (const viewport of viewports) {
    await page.setViewportSize(viewport);
    await page.goto("/");

    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    const commands = await page.locator(".command-bar").boundingBox();
    const workflow = page.locator(".creator-agent-control select").first();
    const oneClick = page.locator(".new-project.auto");
    expect(commands).not.toBeNull();
    expect(commands!.x).toBeGreaterThanOrEqual(0);
    expect(commands!.x + commands!.width).toBeLessThanOrEqual(viewport.width);
    await expect(workflow).toBeVisible();
    expect(await workflow.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
    expect((await oneClick.boundingBox())!.height).toBeGreaterThanOrEqual(40);

    await page.getByRole("tab", { name: "导出" }).click();
    await page.locator(".creator-drawer").scrollIntoViewIfNeeded();
    const drawer = await page.locator(".creator-drawer").boundingBox();
    const drawerHeader = await page.locator(".export-panel-header").boundingBox();
    expect(drawer).not.toBeNull();
    expect(drawerHeader).not.toBeNull();
    expect(drawer!.y).toBeGreaterThanOrEqual(0);
    expect(drawer!.y + drawer!.height).toBeLessThanOrEqual(viewport.height + 2);
    expect(drawerHeader!.y).toBeGreaterThanOrEqual(0);
    await expect(page.locator(".creator-drawer-body")).toHaveCSS("overflow-y", "auto");
    await page.locator(".export-panel-header .ui-icon-button").click();

    await page.locator(".runtime-link").click();
    const runtime = await page.locator(".runtime-settings-dialog").boundingBox();
    expect(runtime).not.toBeNull();
    expect(runtime!.y).toBeGreaterThanOrEqual(0);
    expect(runtime!.y + runtime!.height).toBeLessThanOrEqual(viewport.height + 1);
    await page.locator(".runtime-dialog-header .dialog-close").click();

    await page.locator(".command-more .ui-icon-button").click();
    await expect(page.locator(".command-menu")).toBeVisible();
    await page.locator(".topbar-heading").click();
    await expect(page.locator(".command-menu")).toBeHidden();
  }
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
  await page.getByRole("tab", { name: /^质量/ }).click();
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
  await bindMockMedia(page);
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
  await page.getByRole("button", { name: "展开时间线" }).click();
  await page.getByText("恢复此处").click();
  await expect(page.getByText("已恢复此处；预览时间线已更新。")).toBeVisible();
  await page.getByRole("tab", { name: "分析" }).click();
  const wordEvidence = page.getByRole("region", { name: "词级时间" });
  await wordEvidence.getByRole("button", { name: "嗯" }).click();
  await wordEvidence.getByLabel("安全留白").selectOption("200");
  await wordEvidence.getByRole("button", { name: "创建并试听" }).click();
  await expect(page.getByText("切点已创建；生成媒体预览后可试听切点前后 1 秒。")).toBeVisible();
  await page.getByRole("tab", { name: /^审阅/ }).click();
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
  await expect(page.getByText("需要更新", { exact: true }).first()).toBeVisible();
  await page.getByRole("tab", { name: "导出" }).click();
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

test("versions glossary terms and requires stale-translation export confirmation", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "发布口播 · 草稿" })).toBeVisible();
  await page.getByRole("combobox", { name: "Agent 工作流" }).selectOption("translate");
  const glossary = page.getByRole("textbox", { name: "项目术语表" });
  await glossary.fill("本地优先=local-first\n工作台=workbench");
  await page.getByRole("button", { name: "保存新版本" }).click();
  await expect(page.getByText(/术语表已保存为版本 1/)).toBeVisible();
  await expect(page.getByText("版本 1", { exact: true })).toBeVisible();

  await page.getByRole("tab", { name: "导出" }).click();
  const exportPanel = page.getByLabel("导出设置");
  await exportPanel.getByLabel("字幕模式").selectOption("translated");
  const exportButton = exportPanel.getByRole("button", { name: "导出字幕" });
  const confirmation = exportPanel.getByRole("checkbox", { name: /确认仍使用当前译文导出/ });
  await expect(exportButton).toBeDisabled();
  await confirmation.check();
  await expect(exportButton).toBeEnabled();
});

test("confirms and controls an audited URL import", async ({ page }) => {
  await page.goto("/");
  await page.getByText("更多导入方式").click();
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
  await page.getByText("更多导入方式").click();
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
  await expect(status.getByText("一键工作流未完成，可以继续或重试。")).toBeVisible();
  await expect(status.getByText("技术详情")).toBeVisible();
  await expect(status.getByText("仍有 Agent 修改或粗剪建议等待人工处理")).toBeHidden();
  await page.getByRole("button", { name: "应用软剪辑" }).click();
  await expect(page.getByText(/已应用软剪辑/)).toBeVisible();
  await status.getByRole("button", { name: "确认完成并继续" }).click();
  await expect(status.getByText(/已完成 · 流程完成/)).toBeVisible({ timeout: 3000 });
  await expect(page.getByText(/一键工作流已完成，视频已导出到/)).toBeVisible();
});

test("uses MOSS as an explicit multispeaker mode with loopback settings and review", async ({ page }) => {
  await page.goto("/");
  await bindMockMedia(page);
  await page.getByRole("button", { name: "运行环境" }).click();
  const runtime = page.getByRole("dialog", { name: "运行环境" });
  await runtime.getByRole("combobox", { name: "转写模式" }).selectOption("multispeaker");
  await runtime.getByRole("button", { name: "关闭运行环境" }).click();
  const start = page.getByRole("button", { name: "开始多人转写" });
  await expect(start).toBeEnabled();
  await start.click();
  await expect(page.getByText(/字幕和说话人轨已作为一个版本写入/)).toBeVisible();
  const review = page.getByRole("region", { name: "多人转写复核" });
  await expect(review.getByText("快速人物切换")).toBeVisible();
  await expect(page.getByText("当前结果没有词级时间戳")).toBeVisible();

  await page.getByRole("combobox", { name: "Agent 工作流" }).selectOption("speaker_names");
  await expect(page.getByRole("button", { name: "交给本机 Codex" })).toBeEnabled();
  await page.getByRole("tab", { name: "导出" }).click();
  const exportPanel = page.getByLabel("导出设置");
  await exportPanel.getByLabel("导出格式").selectOption("json");
  await expect(exportPanel.getByText(/始终保留模型、人物轨、段落关联和复核状态/)).toBeVisible();
  const transcriptExport = exportPanel.getByRole("button", { name: "导出字幕" });
  await expect(transcriptExport).toBeDisabled();
  await exportPanel.getByRole("checkbox", { name: /确认带着 1 个未处理警告/ }).check();
  await expect(transcriptExport).toBeEnabled();
  await exportPanel.getByRole("button", { name: "关闭导出设置" }).click();

  await page.getByRole("button", { name: "运行环境" }).click();
  const settings = page.getByRole("dialog", { name: "运行环境" });
  const provider = settings.getByRole("region", { name: "MOSS 多人长音频服务" });
  await expect(provider.locator("input").first()).toHaveValue("http://127.0.0.1:8000");
  const endpointInput = await provider.locator("input").nth(0).boundingBox();
  const modelInput = await provider.locator("input").nth(1).boundingBox();
  expect(endpointInput).not.toBeNull();
  expect(modelInput).not.toBeNull();
  expect(Math.abs(endpointInput!.y - modelInput!.y)).toBeLessThanOrEqual(1);
  await expect(provider.getByText("服务可用", { exact: true })).toBeVisible();
});

test("keeps a conflicting MOSS candidate isolated until explicit replacement", async ({ page }) => {
  await page.goto("/");
  await bindMockMedia(page);
  await page.getByRole("button", { name: "运行环境" }).click();
  const runtime = page.getByRole("dialog", { name: "运行环境" });
  await runtime.getByRole("combobox", { name: "转写模式" }).selectOption("multispeaker");
  await runtime.getByRole("button", { name: "关闭运行环境" }).click();
  await page.getByText("高级实验项：Prompt 与热词").click();
  await page.getByRole("textbox", { name: "自定义 Prompt" }).fill("simulate-conflict");
  await page.getByRole("button", { name: "开始多人转写" }).click();

  await expect(page.getByText("候选结果等待确认")).toBeVisible();
  await expect(page.getByText("18 段 · 3 位说话人 · 2 项提醒")).toBeVisible();
  await page.getByRole("button", { name: "删除项目 发布口播 · 草稿" }).click();
  const deleteDialog = page.getByRole("dialog", { name: "删除项目" });
  await expect(deleteDialog.getByText("仍有多人转写候选结果等待应用或丢弃。")).toBeVisible();
  await expect(deleteDialog.getByRole("button", { name: "确认删除" })).toBeDisabled();
  await deleteDialog.getByRole("button", { name: "取消" }).click();

  await page.getByRole("button", { name: "查看影响" }).click();
  const candidate = page.getByRole("dialog", { name: "确认多人转写候选结果" });
  const apply = candidate.getByRole("button", { name: "应用并替换" });
  await expect(apply).toBeDisabled();
  await candidate.getByRole("checkbox", { name: /确认用候选结果替换/ }).check();
  await apply.click();

  await expect(page.getByText("候选结果已应用为可撤销的新版本。")).toBeVisible();
  await expect(page.getByLabel("00:00 字幕文本")).toHaveValue("这是经过明确确认后应用的多人转写候选结果。");
  await expect(page.getByRole("button", { name: "撤销" })).toBeEnabled();
});
