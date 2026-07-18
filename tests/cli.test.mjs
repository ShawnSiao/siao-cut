import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

const root = await mkdtemp(path.join(os.tmpdir(), "siaocut-cli-"));
const cli = path.resolve("src/cli.mjs");

function run(args) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [cli, "--json", ...args], { env: { ...process.env, SIAOCUT_HOME: root } });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("close", (code) => code === 0 ? resolve(JSON.parse(stdout)) : reject(new Error(stderr)));
  });
}

test.after(async () => rm(root, { recursive: true, force: true }));

test("JSON CLI creates a project, drives an Agent lease, and exports SRT", async () => {
  const media = path.join(root, "sample.mp4");
  const response = path.join(root, "response.json");
  const output = path.join(root, "sample.srt");
  await writeFile(media, "video");
  const imported = await run(["import", media, "--title", "CLI 测试"]);
  const projectId = imported.projectId;
  const added = await run(["transcript", "add", projectId, "--start", "0", "--end", "2", "--text", "你好，世界"]);
  const task = await run(["task", "create", projectId, "--kind", "translate", "--lang", "en"]);
  const claim = await run(["task", "claim", "--worker", "test-agent"]);
  assert.equal(claim.taskId, task.taskId);
  await writeFile(response, JSON.stringify({ segments: [{ segmentId: added.segment.id, text: "Hello, world." }] }));
  await run(["task", "submit", task.taskId, "--worker", "test-agent", "--response", response]);
  const exported = await run(["transcript", "export", projectId, "--format", "srt", "--lang", "en", "-o", output]);
  assert.equal(exported.status, "ok");
  assert.match(await readFile(output, "utf8"), /Hello, world/);
});
