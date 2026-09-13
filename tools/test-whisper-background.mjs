// Explicit local integration check. Uses isolated projects and existing runtimes; no downloads.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { setTimeout as delay } from "node:timers/promises";

const { values } = parseArgs({ options: Object.fromEntries(
  ["core", "whisper", "model", "vad-model", "sample", "backend"].map(key => [key, { type: "string" }]),
) });
for (const key of ["core", "whisper", "model", "vad-model"]) {
  assert(values[key] && existsSync(values[key]), `Provide an existing --${key} path`);
}
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sample = resolve(values.sample ?? join(root, "third_party/whisper.cpp/samples/jfk.wav"));
assert(existsSync(sample), "Provide a local --sample speech WAV");
const backend = values.backend ?? "cpu";
assert(["cpu", "vulkan", "cuda"].includes(backend));
const runDir = mkdtempSync(join(root, ".tmp-whisper-background-"));
const home = join(runDir, "home");
mkdirSync(home);
const sha = path => createHash("sha256").update(readFileSync(path)).digest("hex");
const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.toUpperCase().startsWith("SIAOCUT_")));
Object.assign(env, { SIAOCUT_HOME: home, SIAOCUT_RESOURCE_CONFIG_HOME: join(runDir, "resources"),
  SIAOCUT_DIRECT: "1", SIAOCUT_WHISPER_CLI: resolve(values.whisper),
  SIAOCUT_WHISPER_VAD_MODEL: resolve(values["vad-model"]) });
const selection = { backend, whisperPath: resolve(values.whisper), executableSha256: sha(values.whisper),
  source: "local-integration-test", version: "test", selectedAt: new Date().toISOString() };
const select = value => writeFileSync(join(home, "runtime-selection.json"), JSON.stringify(value));
select(selection);
function program(exe, args, timeout = 180_000) {
  const result = spawnSync(exe, args, { env, encoding: "utf8", windowsHide: true, timeout, maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${exe} failed: ${result.stderr}`);
  return result.stdout;
}
function core(...args) { return JSON.parse(program(resolve(values.core), ["--json", ...args])); }
function request(action, fields = {}) {
  const path = join(runDir, "request.json");
  writeFileSync(path, JSON.stringify({ kind: "transcription_job", request: { action, ...fields } }));
  return core("desktop-request", path);
}
async function waitJob(id, expected) {
  const end = Date.now() + 180_000;
  while (Date.now() < end) {
    const job = request("get", { jobId: id }).transcriptionJob;
    if (!["queued", "running", "finalizing"].includes(job.status)) {
      assert.equal(job.status, expected, JSON.stringify(job));
      return job;
    }
    await delay(500);
  }
  request("cancel", { jobId: id });
  throw new Error(`Timed out waiting for ${id}; cancellation requested. Evidence: ${runDir}`);
}
const fixture = join(runDir, "speech-silence-speech.wav");
program("ffmpeg", ["-y", "-hide_banner", "-loglevel", "error", "-i", sample, "-f", "lavfi", "-t", "4", "-i",
  "anullsrc=r=16000:cl=mono", "-filter_complex", "[0:a][1:a][0:a]concat=n=3:v=0:a=1[out]",
  "-map", "[out]", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", fixture]);
const originalHash = sha(fixture);
const speechDuration = Number(program("ffprobe", ["-v", "error", "-show_entries", "format=duration", "-of",
  "default=nw=1:nk=1", sample]).trim());
const model = resolve(values.model);
const imported = () => core("import", fixture).project;
const version = project => project.history.currentVersionId;
const current = id => core("project", "show", id).project;
const start = project => request("start", { mutationId: randomUUID(), projectId: project.id,
  expectedVersionId: version(project), modelPath: model, language: "en" }).transcriptionJob.id;
const shape = transcript => transcript.segments.map(segment => ({ start: segment.start, end: segment.end,
  text: segment.text, words: transcript.words.filter(word => word.segmentId === segment.id)
    .map(word => [word.start, word.end, word.text]) }));
const report = { backend, coreSha256: sha(values.core), whisperSha256: selection.executableSha256,
  fixtureSha256: originalHash, cases: [] };
console.log(`Local evidence directory: ${runDir}`);
for (const vad of [true, false]) {
  env.SIAOCUT_WHISPER_VAD_MODEL = vad ? resolve(values["vad-model"]) : join(runDir, "absent-vad.bin");
  const mode = vad ? "whisper_verified_vad" : "whisper_no_vad";
  const p = imported();
  const cli = core("transcribe", p.id, "--model", model, "--language", "en", "--expected-version", version(p));
  assert.equal(cli.timingValidation.mode, mode, "Selected runtime must pass the expected VAD gate");
  const desktop = imported();
  const id = start(desktop);
  await waitJob(id, "completed");
  const candidate = JSON.parse(readFileSync(join(home, "transcription-runs", `${id}-attempt-1.json`), "utf8"));
  assert.equal(candidate.timingValidation.mode, mode);
  assert.equal(candidate.timingValidation.vadUsed, vad);
  assert.deepEqual(shape(current(desktop.id).transcript), shape(cli.project.transcript));
  const words = candidate.segments.flatMap(segment => segment.words);
  assert(words.some(word => word.start >= speechDuration + 3.5), "Speech after silence must retain source timing");
  report.cases.push({ mode, segments: candidate.segments.length, words: words.length });
  console.log(`Passed CLI/background parity and source timing: ${mode}`);
  if (vad) {
    const before = current(desktop.id);
    const replacement = start(before);
    await waitJob(replacement, "awaiting_apply");
    assert.equal(version(current(desktop.id)), version(before), "Existing transcript must remain unchanged");
    const preview = request("preview", { jobId: replacement, offset: 0 }).candidatePreview;
    assert.equal(preview.overwrittenSegments, before.transcript.segments.length);
    request("apply", { mutationId: randomUUID(), jobId: replacement, expectedVersionId: version(before) });
    assert.notEqual(version(current(desktop.id)), version(before));
    console.log("Passed replacement candidate review and explicit application");
  }
}
// Changed selected-runtime identity must fail, including in the desktop worker.
select({ ...selection, executableSha256: "0".repeat(64) });
const unchanged = imported();
const failed = await waitJob(start(unchanged), "failed");
assert.match(failed.errorMessage, /runtime_hash_mismatch/);
assert.equal(version(current(unchanged.id)), version(unchanged));
select(selection);
assert.equal(sha(fixture), originalHash);
report.replacementReview = "passed";
report.changedRuntimeRejected = "passed";
writeFileSync(join(runDir, "report.json"), JSON.stringify(report, null, 2));
console.log(`Whisper background integration passed. Report: ${join(runDir, "report.json")}`);
