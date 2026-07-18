import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

const root = await mkdtemp(path.join(os.tmpdir(), "siaocut-test-"));
process.env.SIAOCUT_HOME = root;
const { createProject, loadProject } = await import("../src/core/store.mjs");
const { addSegment, detectCuts, setCutStatus, editSegment } = await import("../src/core/project.mjs");
const { createTask, claimTask, submitTask } = await import("../src/core/tasks.mjs");
const { renderExport } = await import("../src/core/export.mjs");

test.after(async () => rm(root, { recursive: true, force: true }));

test("project retains media evidence and transcript changes create versions", async () => {
  const media = path.join(root, "talk.mp4");
  await writeFile(media, "not a real video");
  const project = await createProject(media, "测试口播");
  const { result: first } = await addSegment(project.id, { start: 0, end: 0.8, text: "嗯" });
  await addSegment(project.id, { start: 0.8, end: 2, text: "今天我们聊 SiaoCut" });
  const updated = await loadProject(project.id);
  assert.equal(updated.media.sha256.length, 64);
  assert.equal(updated.transcript.segments.length, 2);
  assert.ok(updated.versions.length >= 3);
  await editSegment(project.id, first.id, "好的");
  assert.equal((await loadProject(project.id)).transcript.segments[0].text, "好的");
});

test("agent translation lease applies structured output and source edits stale it", async () => {
  const media = path.join(root, "translate.wav");
  await writeFile(media, "audio");
  const project = await createProject(media);
  const { result: segment } = await addSegment(project.id, { start: 0, end: 2, text: "你好，世界" });
  const { result: task } = await createTask(project.id, "translate", "en");
  const claim = await claimTask("test-agent");
  assert.equal(claim.result.task.id, task.id);
  await submitTask(task.id, "test-agent", { segments: [{ segmentId: segment.id, text: "Hello, world." }] });
  assert.equal((await loadProject(project.id)).translations.en.status, "current");
  await editSegment(project.id, segment.id, "你好，SiaoCut");
  assert.equal((await loadProject(project.id)).translations.en.status, "stale");
});

test("soft cut removes full filler cue and retimes later subtitles", async () => {
  const media = path.join(root, "cuts.m4a");
  await writeFile(media, "audio");
  const project = await createProject(media);
  await addSegment(project.id, { start: 0, end: 1, text: "嗯" });
  await addSegment(project.id, { start: 1, end: 3, text: "开始吧" });
  const { result: suggestions } = await detectCuts(project.id);
  assert.equal(suggestions.length, 1);
  await setCutStatus(project.id, suggestions[0].id, "applied");
  const srt = renderExport(await loadProject(project.id));
  assert.match(srt, /00:00:00,000 --> 00:00:02,000/);
  assert.doesNotMatch(srt, /嗯/);
});
