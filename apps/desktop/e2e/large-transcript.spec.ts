import {test,expect} from "@playwright/test";

test("windows ten thousand rows and keeps an IME editor mounted while scrolling",async({page})=>{
  await page.goto("/");
  await expect(page.locator(".segment-row").first()).toBeVisible();
  await page.evaluate(async()=>{
    const moduleUrl="/src/core.mock.ts";const mock=await import(moduleUrl);
    const project=(await mock.mockRun(["project","show","preview"])).project;
    project.transcript.segments=Array.from({length:10000},(_,index)=>({id:`large-${index}`,start:index*3,end:index*3+2,text:`字幕 ${index} Long transcript line`,confidence:0.98}));
    project.transcript.words=[];project.translations={};project.edits=[];project.tasks=[];project.patchSets=[];project.workflows=[];
    project.subtitleQuality.issues=[];project.speechInsights.evidence=[];project.speechInsights.pauses=[];project.media.durationSeconds=30000;
    mock.setMockProjectForTest(project);await mock.mockRun(["project","list"]);
  });
  await page.getByRole("button",{name:"第二个本地项目",exact:true}).click();
  await page.getByRole("button",{name:"发布口播 · 草稿",exact:true}).click();
  await expect(page.locator("[data-virtual-total='10000']")).toBeVisible();
  expect(await page.locator(".segment-row").count()).toBeLessThan(35);
  const editor=page.locator("[data-segment-id='large-0'] textarea").first();
  await editor.focus();
  await editor.dispatchEvent("compositionstart");
  await editor.fill("中文输入组合中");
  await page.evaluate(()=>{
    const node=document.querySelector<HTMLTextAreaElement>("[data-segment-id='large-0'] textarea")!;
    (window as unknown as {pinnedEditor:HTMLTextAreaElement}).pinnedEditor=node;
    const list=document.querySelector<HTMLElement>(".segment-list")!;list.scrollTop=500000;list.dispatchEvent(new Event("scroll"));
  });
  await expect(page.locator("[data-segment-id='large-0']")).toHaveCount(1);
  expect(await page.evaluate(()=>{const pinned=(window as unknown as {pinnedEditor:HTMLTextAreaElement}).pinnedEditor;return pinned.isConnected&&document.activeElement===pinned;})).toBe(true);
  expect(await page.locator(".segment-row").count()).toBeLessThan(35);
  await editor.dispatchEvent("compositionend");
  await expect(page.locator("[data-segment-id='large-0'] .field-save-status").first()).toHaveText("已保存");
  expect(await page.evaluate(()=>{const pinned=(window as unknown as {pinnedEditor:HTMLTextAreaElement}).pinnedEditor;return pinned.isConnected&&pinned.value==="中文输入组合中";})).toBe(true);
  await page.locator(".segment-list").evaluate((node)=>node.dispatchEvent(new CustomEvent("transcript-locate",{detail:"large-9999"})));
  await expect(page.locator("[data-segment-id='large-9999']")).toBeVisible();
});
