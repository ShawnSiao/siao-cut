import { expect, test } from "@playwright/test";

for (const viewport of [{ width: 1440, height: 940 }, { width: 1080, height: 720 }]) {
  test(`keeps transcript preview and timeline within ${viewport.width}x${viewport.height}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await page.goto("/");
    const transcript = page.locator(".transcript-panel");
    await expect(transcript).toBeVisible();
    const frame = await transcript.boundingBox(), preview = await page.locator(".creator-player").boundingBox();
    expect(frame!.x + frame!.width).toBeLessThan(preview!.x);
    expect(Math.abs(frame!.y - preview!.y)).toBeLessThan(2);
    expect(frame!.y + frame!.height).toBeLessThan(viewport.height);
    await expect(page.locator(".subtitle-timeline-panel")).toHaveClass(/collapsed/);
    expect(await page.evaluate(() => ({ y: window.scrollY, overflow: document.documentElement.scrollWidth > innerWidth }))).toEqual({ y: 0, overflow: false });
    await page.getByRole("button", { name: "展开时间线" }).click();
    const timeline = await page.locator(".subtitle-timeline-panel").boundingBox();
    expect(timeline!.height).toBeGreaterThanOrEqual(280);
    expect(timeline!.height).toBeLessThanOrEqual(viewport.height * 0.45);
    // Subtitle text must fit before any vertical scrolling in either mode.
    for (const mode of ["精细编辑", "高级审校"]) {
      await page.locator(".subtitle-timeline-modes").getByRole("button", { name: mode }).click();
      const geometry = await page.locator(".subtitle-timeline-scroll").evaluate(scroll => {
        const text = scroll.querySelector(".subtitle-timeline-segment > span:last-child")!;
        const bounds = text.getBoundingClientRect(), viewport = scroll.getBoundingClientRect();
        return { top: bounds.top - viewport.top, bottom: bounds.bottom - viewport.top, height: scroll.clientHeight, fontSize: parseFloat(getComputedStyle(text).fontSize) };
      });
      expect(geometry.top).toBeGreaterThanOrEqual(0);
      expect(geometry.bottom).toBeLessThanOrEqual(geometry.height);
      expect(geometry.fontSize).toBeGreaterThanOrEqual(14);
    }
    const ruler = await page.locator(".subtitle-timeline-ruler").boundingBox();
    const label = await page.locator(".subtitle-timeline-ruler i.major span").first().boundingBox();
    const actions = await page.locator(".subtitle-timeline-actions").boundingBox();
    expect(label!.y).toBeGreaterThanOrEqual(ruler!.y);
    expect(label!.y + label!.height).toBeLessThanOrEqual(ruler!.y + ruler!.height);
    expect(actions!.y + actions!.height).toBeLessThanOrEqual(ruler!.y);
    const player = await page.locator(".creator-player").boundingBox();
    const stage = await page.locator(".stage-grid").boundingBox();
    expect(player!.height).toBeGreaterThanOrEqual(stage!.height * 0.59);
    expect(timeline!.y + timeline!.height).toBeLessThanOrEqual(viewport.height);
    const editor = page.locator(".segment-row").nth(1).getByRole("textbox").first();
    await editor.fill("长字幕和中英文内容 mixed text ".repeat(20));
    await editor.press("Control+s");
    await expect(editor).toBeFocused();
    expect(await page.evaluate(() => window.scrollY)).toBe(0);
    await expect(page.locator(".segment-list")).toHaveCSS("overflow-y", "auto");
  });
}

test("preserves the player across layouts and supports keyboard region navigation", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(async () => { const mock = await import("/src/core.mock.ts"); mock.setMockAuthorizedMediaForTest("mock://local-media"); });
  await page.getByRole("button", { name: "更多命令" }).click();
  await page.getByRole("menuitem", { name: "重新定位原片" }).click();
  const video = page.locator("video");
  await video.evaluate((node) => { node.dataset.identity = "persistent"; (node as HTMLVideoElement).currentTime = 14; });
  await page.getByRole("button", { name: "放大预览" }).click();
  const area = await page.locator(".stage-grid").boundingBox();
  const player = await page.locator(".creator-player").boundingBox();
  expect(Math.abs(player!.width - area!.width)).toBeLessThan(2);
  expect(Math.abs(player!.height - area!.height)).toBeLessThan(2);
  await expect(video).toHaveAttribute("data-identity", "persistent");
  await page.getByRole("button", { name: "还原布局" }).click();
  expect(await video.evaluate((node) => (node as HTMLVideoElement).currentTime)).toBe(14);
  const editor = page.locator(".segment-row").nth(1).getByRole("textbox").first();
  await editor.fill("键盘编辑的字幕");
  await editor.press("Control+s");
  await editor.press("F6");
  await expect(video).toBeFocused();
  await page.keyboard.press("F6");
  await expect(page.getByRole("tab", { name: /^审阅/ })).toBeFocused();
  await page.keyboard.press("Control+Shift+e");
  await expect(page.getByRole("tab", { name: "导出", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(video).toHaveAttribute("data-identity", "persistent");
});

test("allows text review without granting media access", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "审阅建议" }).click();
  await expect(page.getByRole("button", { name: "退出专注审阅" })).toBeVisible();
  await expect(page.locator("video")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.locator(".segment-row").first().getByRole("textbox").first()).toBeEditable();
});

for (const scale of [1.25, 1.5]) {
  test(`keeps English controls and IME input usable at browser pixel scale ${scale}`, async ({ browser, baseURL }) => {
    const context = await browser.newContext({ baseURL, viewport: { width: 1080, height: 720 }, deviceScaleFactor: scale });
    const page = await context.newPage();
    await page.addInitScript(() => localStorage.setItem("siaocut.uiLocale.v1", "en-US"));
    await page.goto("/");
    const editor = page.locator(".segment-row textarea").first();
    await editor.fill("Long subtitle with 中文混排 ".repeat(30));
    await editor.dispatchEvent("compositionstart");
    await editor.press("Enter");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(editor).toBeFocused();
    await editor.dispatchEvent("compositionend");
    await editor.press("Control+s");
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth && window.scrollY === 0)).toBe(true);
    const preview = await page.locator(".creator-player").boundingBox();
    expect(preview!.height).toBeGreaterThan(100);
    expect(preview!.y + preview!.height).toBeLessThan(720);
    await context.close();
  });
}
