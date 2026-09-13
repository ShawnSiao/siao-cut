import { expect, test } from "@playwright/test";

test("keeps transcription running across projects and reviews its actual candidate before replacement", async ({ page }) => {
  await page.goto("/");
  const original = await page.getByLabel("00:13 字幕文本").inputValue();
  await page.evaluate(async () => { const mock = await import("/src/core.mock.ts"); mock.setMockAuthorizedMediaForTest("mock://local-media"); });
  await page.getByRole("button", { name: "更多命令" }).click();
  await page.getByRole("menuitem", { name: "重新定位原片" }).click();
  await page.getByRole("button", { name: "更多命令" }).click();
  await page.getByRole("menuitem", { name: "重新生成快速字幕" }).click();
  const start = page.getByRole("dialog", { name: "确认重新生成快速字幕" });
  await start.getByRole("checkbox").check();
  await start.getByRole("button", { name: "确认并重新转写" }).click();
  await expect(start).toBeHidden();
  await page.getByRole("button", { name: "第二个本地项目", exact: true }).click();
  await expect(page.getByRole("heading", { name: "第二个本地项目" })).toBeVisible();
  await page.locator(".workspace-tasks > summary").click();
  const job = page.getByRole("group", { name: "转写状态", exact: true });
  await expect(job).toContainText("候选结果等待确认", { timeout: 6000 });
  await expect(page.getByRole("heading", { name: "第二个本地项目" })).toBeVisible();
  await job.getByRole("button", { name: "查看候选结果" }).click();
  const review = page.getByRole("dialog", { name: "确认转写候选结果" });
  await expect(review.getByRole("region", { name: "候选字幕文本" })).toContainText("将替换 4 段现有字幕");
  await expect(page.getByLabel("00:13 字幕文本")).toHaveValue(original);
  await review.getByRole("checkbox").check();
  await review.getByRole("button", { name: "应用并替换" }).click();
  await expect(review).toBeHidden();
  await expect(page.getByLabel("00:00 字幕文本")).toHaveValue("这是经过明确确认后应用的多人转写候选结果。");
});
