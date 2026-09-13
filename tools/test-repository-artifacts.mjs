import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checker = path.join(root, "tools/check-repository-artifacts.ps1");
const policy = JSON.parse(fs.readFileSync(path.join(root, "tools/repository-local-paths.json"), "utf8"));
const shell = process.env.SIAOCUT_POLICY_TEST_SHELL || "powershell";
const environment = { ...process.env };
for (const key of Object.keys(environment)) {
  if (key.startsWith("GIT_")) delete environment[key];
}

function fixture(run) {
  const directory = fs.mkdtempSync(path.join(root, ".tmp-artifact-policy-tests-"));
  const git = (...args) => {
    const result = spawnSync("git", args, { cwd: directory, env: environment, encoding: "utf8", windowsHide: true });
    assert.equal(result.status, 0, result.stderr || String(result.error));
    return result.stdout;
  };
  const write = (relative, content = "synthetic fixture") => {
    const target = path.join(directory, relative);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, content);
  };
  const check = (staged = true) => {
    const command = `[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false); $ProgressPreference = 'SilentlyContinue'; try { & '${checker.replaceAll("'", "''")}'${staged ? " -Staged" : ""}; exit 0 } catch { [Console]::WriteLine($_.Exception.Message); exit 1 }`;
    const result = spawnSync(shell, ["-NoProfile", "-ExecutionPolicy", "Bypass", "-OutputFormat", "Text", "-EncodedCommand", Buffer.from(command, "utf16le").toString("base64")], {
      cwd: directory, env: environment, encoding: "utf8", windowsHide: true, timeout: 60000,
    });
    assert.ifError(result.error);
    assert.notEqual(result.status, null, "Artifact check did not complete");
    return { status: result.status, output: result.stdout + result.stderr };
  };
  try {
    git("init", "--quiet");
    fs.copyFileSync(path.join(root, ".gitignore"), path.join(directory, ".gitignore"));
    git("add", ".gitignore");
    git("-c", "user.name=Policy Test", "-c", "user.email=policy@example.invalid", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "fixture baseline");
    git("update-ref", "refs/remotes/origin/main", "HEAD");
    run({ directory, git, write, check });
  } finally {
    // This directory was created by this test; never clean an existing workspace.
    assert.equal(path.dirname(path.resolve(directory)), root);
    assert.ok(path.basename(directory).startsWith(".tmp-artifact-policy-tests-"));
    assert.equal(fs.lstatSync(directory).isSymbolicLink(), false);
    fs.rmSync(directory, { recursive: true, force: true, maxRetries: 3 });
  }
}

test("public assets and contracts remain allowed; normal add excludes local files", () => fixture(({ git, write, check }) => {
  for (const file of ["README.md", "tools/helper.ps1", "skills/siaocut/SKILL.md", "apps/desktop/src/generated/core-contract.ts", "apps/desktop/src-tauri/icons/32x32.png", "tests/fixtures/example.json", ".env.example"]) write(file);
  const local = [...policy.files, ...policy.directories.map(dir => `${dir}/sample.md`), "scratch/__pycache__/module.pyc", "scratch/module.pyo"];
  for (const file of local) write(file);
  git("add", "--all");
  const indexed = new Set(git("ls-files", "-z").split("\0"));
  for (const file of local) assert.equal(indexed.has(file), false, file);
  assert.equal(indexed.has("apps/desktop/src-tauri/icons/32x32.png"), true);
  const result = check(false);
  assert.equal(result.status, 0, result.output);
}));

test("force-added private paths and Python caches are rejected even when tiny", () => fixture(({ git, write, check }) => {
  const local = [...policy.files, ...policy.directories.map(dir => `${dir}/small.json`), "scratch/__pycache__/state.txt", "scratch/module.pyc", "scratch/module.pyo", ".env.local"];
  for (const file of local) write(file);
  git("add", "--force", "--", ...local);
  const result = check();
  assert.notEqual(result.status, 0);
  for (const file of local) assert.ok(result.output.includes(file), file);
}));

test("redacting the working copy cannot conceal a staged token", () => fixture(({ git, write, check }) => {
  const token = "ghp_" + "A".repeat(40);
  write("notes.txt", token);
  git("add", "notes.txt");
  write("notes.txt", "redacted locally");
  for (const staged of [true, false]) {
    const result = check(staged);
    assert.notEqual(result.status, 0);
    assert.match(result.output, /GitHub token \(index\): notes.txt/);
    assert.equal(result.output.includes(token), false, "Do not print secret values");
  }
}));

test("unstaged deletion cannot conceal forbidden staged files", () => fixture(({ directory, git, write, check }) => {
  write("designs/hidden.md");
  git("add", "--force", "designs/hidden.md");
  fs.unlinkSync(path.join(directory, "designs/hidden.md"));
  const result = check(false);
  assert.notEqual(result.status, 0);
  assert.match(result.output, /local-only directory \(index\): designs\/hidden.md/);
}));

test("index byte size is enforced after the working file is shortened", () => fixture(({ git, write, check }) => {
  write("large.txt", Buffer.alloc(5 * 1024 * 1024 + 1, 65));
  git("add", "large.txt");
  write("large.txt", "small");
  const result = check();
  assert.notEqual(result.status, 0);
  assert.match(result.output, /file exceeds 5 MiB \(index\): large.txt/);
}));

test("removing a file from the index preserves its ignored local copy", () => fixture(({ directory, git, write, check }) => {
  write("designs/retained.md");
  git("add", "--force", "designs/retained.md");
  git("rm", "--cached", "designs/retained.md");
  assert.equal(fs.readFileSync(path.join(directory, "designs/retained.md"), "utf8"), "synthetic fixture");
  const result = check(false);
  assert.equal(result.status, 0, result.output);
}));

test("staged scope ignores unsent edits; default scope still checks the working tree", () => fixture(({ write, check }) => {
  write("untracked.txt", "ghp_" + "B".repeat(40));
  assert.equal(check().status, 0);
  const result = check(false);
  assert.notEqual(result.status, 0);
  assert.match(result.output, /GitHub token \(working tree\): untracked.txt/);
}));

test("UTF-8 paths with spaces and unsanitized index content are checked", () => fixture(({ git, write, check }) => {
  const file = "notes/中文 with spaces.md";
  const privatePath = ["C:", "Users", "fixture", "media"].join(String.fromCharCode(92));
  write(file, privatePath);
  git("add", file);
  write(file, "public text");
  const result = check();
  assert.notEqual(result.status, 0);
  assert.ok(result.output.includes(file), result.output);
  assert.match(result.output, /Windows user or workspace path \(index\)/);
}));
