import { expect, test } from "@playwright/test";

test("autosaves consecutive transcript rows without a global busy lock", async ({ page }) => {
  await page.goto("/");
  const rows = page.locator(".segment-row");
  await expect(rows.first()).toBeVisible();
  await rows.nth(0).getByRole("textbox").first().fill("第一段自动保存");
  await rows.nth(1).getByRole("textbox").first().fill("第二段自动保存");
  await expect(rows.nth(0).locator(".field-save-status").first()).toHaveText("已保存");
  await expect(rows.nth(1).locator(".field-save-status").first()).toHaveText("已保存");
  await expect(rows.nth(0).getByRole("textbox").first()).toHaveValue("第一段自动保存");
  await expect(rows.nth(1).getByRole("textbox").first()).toHaveValue("第二段自动保存");
});

test("keeps the local draft when another client edits the same segment", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".segment-row").first()).toBeVisible();
  const segmentId = await page.locator(".segment-row").first().getAttribute("data-segment-id");
  const row = page.locator(`.segment-row[data-segment-id="${segmentId}"]`);
  // Simulate a second client committing between the read and the user's autosave.
  await page.evaluate(async (id) => {
    const moduleUrl = "/src/core.mock.ts";
    const mock = await import(moduleUrl);
    const project = (await mock.mockRun(["project", "show", "preview"])).project;
    await mock.mockRun(["transcript", "edit", project.id, id, "--text", "另一个客户端的内容"]);
  }, segmentId);
  await row.getByRole("textbox").first().fill("本地尚未确认的修改");
  const conflict = page.locator(".editing-conflict");
  await expect(conflict).toBeVisible();
  await expect(conflict.getByRole("textbox")).toHaveValue("本地尚未确认的修改");
  await expect(conflict.locator("pre")).toContainText("另一个客户端的内容");
  await conflict.getByRole("button", { name: "以此草稿保存" }).click();
  await expect(conflict).toHaveCount(0);
  await expect(row.getByRole("textbox").first()).toHaveValue("本地尚未确认的修改");
});

test("restores a persisted draft after reopening without silently applying it", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".segment-row").first()).toBeVisible();
  await page.evaluate(async () => {
    const moduleUrl = "/src/core.mock.ts";
    const mock = await import(moduleUrl);
    const project = (await mock.mockRun(["project", "list"])).projects[0];
    await mock.mockEditingRequest({ action: "journal", draft: { projectId: project.id, sessionId: "previous-window", segmentId: project.transcript.segments[0].id, field: "source", baseVersionId: project.history.currentVersionId, baseText: project.transcript.segments[0].text, text: "上次退出前的草稿", revision: 1 } });
  });
  await page.reload();
  const conflict = page.locator(".editing-conflict");
  await expect(conflict).toBeVisible();
  await expect(conflict.getByRole("textbox")).toHaveValue("上次退出前的草稿");
  await conflict.getByRole("button", { name: "使用当前内容" }).click();
  await expect(conflict).toHaveCount(0);
});


for (const failure of ["save", "journal"] as const) {
  test(`explains database contention when ${failure} is blocked and keeps an explicit retry`, async ({page}) => {
    await page.goto("/");
    const row=page.locator(".segment-row").first();await expect(row).toBeVisible();
    await page.evaluate(async action => {
      const url="/src/domains/editing-client.ts";const {editingClient}=await import(url);
      const original=editingClient[action];let blocked=true;
      editingClient[action]=(...args: unknown[])=>{if(blocked) return Promise.reject(Object.assign(new Error("Database busy"),{code:"database_busy"}));return original(...args);};
      (window as any).releaseEditingLock=()=>{blocked=false;};
    },failure);
    await row.getByRole("textbox").first().fill("数据库占用时保留的修改");
    const problem=page.locator(".editing-conflict");await expect(problem).toBeVisible();
    await expect(problem).toContainText(failure==="save"?"本地草稿已落盘":"当前草稿尚未确认落盘");
    await expect(problem.getByRole("textbox")).toHaveValue("数据库占用时保留的修改");
    await page.evaluate(()=>(window as any).releaseEditingLock());
    await problem.getByRole("button",{name:"重试保存"}).click();
    await expect(problem).toHaveCount(0);await expect(row.locator(".field-save-status").first()).toHaveText("已保存");
  });
}
