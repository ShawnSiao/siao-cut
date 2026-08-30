import { expect, test } from "@playwright/test";

test("previews burned subtitles and exposes embedded or sidecar delivery", async ({ page }) => {
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
  const caption = page.locator(".caption-overlay");
  const [captionBox, videoFrameBox] = await Promise.all([
    caption.boundingBox(),
    page.locator(".video-frame").boundingBox(),
  ]);
  expect(captionBox!.width / videoFrameBox!.width).toBeGreaterThan(0.9);
  expect(await caption.evaluate((element) => (element as HTMLElement).style.bottom)).toBe("4%");
  await panel.getByLabel("字幕样式预设").selectOption("emphasis");
  await expect(page.getByText("字幕样式已更新；正文和时间未修改，可撤销。")).toBeVisible();
  await panel.getByLabel("字幕位置").selectOption("center");
  await expect(caption).toHaveAttribute("data-preset", "emphasis");
  await expect(caption).toHaveAttribute("data-position", "center");
  await expect(caption.locator(".caption-secondary")).toContainText("Today I want to explain why we are building a local-first editing workbench.");
  await expect(page.locator(".subtitle-safe-area")).toBeVisible();
  await panel.getByRole("checkbox", { name: "显示字幕安全区" }).uncheck();
  await expect(page.locator(".subtitle-safe-area")).toBeHidden();
  await expect(page.getByLabel("00:13 字幕文本")).toHaveValue(original);
  await panel.getByLabel("字幕交付方式").selectOption("embedded-mkv");
  await expect(panel.getByText(/可开关、可提取的文本字幕轨/)).toBeVisible();
  await expect(panel.getByLabel("字幕样式预设")).toHaveCount(0);
  await panel.getByLabel("字幕交付方式").selectOption("sidecar-vtt");
  await expect(panel.getByText(/同名 UTF-8 字幕文件/)).toBeVisible();
});
