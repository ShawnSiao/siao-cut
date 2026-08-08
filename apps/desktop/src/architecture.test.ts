import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const sourceRoot = resolve(process.cwd(), "src");

function sourceFiles(directory: string): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.(ts|tsx)$/.test(entry) && !/\.test\.(ts|tsx)$/.test(entry) ? [path] : [];
  });
}

describe("desktop architecture boundaries", () => {
  it("keeps App.tsx as a small top-level assembly module", () => {
    const app = readFileSync(join(sourceRoot, "App.tsx"), "utf8");
    expect(app.split(/\r?\n/).length).toBeLessThanOrEqual(500);
    expect(app).toContain("./workbench/workbench-controller");
  });

  it("allows raw runCore calls only in the low-level adapter and typed domain clients", () => {
    const allowed = new Set([
      join(sourceRoot, "core.ts"),
      join(sourceRoot, "domains/project-session-client.ts"),
      join(sourceRoot, "domains/background-task-client.ts"),
      join(sourceRoot, "domains/transcript-editing-client.ts"),
      join(sourceRoot, "domains/agent-review-client.ts"),
      join(sourceRoot, "domains/export-runtime-client.ts"),
      join(sourceRoot, "domains/translation-client.ts"),
      join(sourceRoot, "domains/local-resource-client.ts"),
    ].map((path) => path.replaceAll("\\", "/")));
    const violations = sourceFiles(sourceRoot)
      .map((path) => path.replaceAll("\\", "/"))
      .filter((path) => !allowed.has(path))
      .filter((path) => /\brunCore\s*\(/.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(sourceRoot.replaceAll("\\", "/").length + 1));

    expect(violations).toEqual([]);
  });

  it("keeps technical resource identifiers out of normal resource and URL-import surfaces", () => {
    const normalSurfaces = [
      join(sourceRoot, "components/local-resource-ui.tsx"),
      join(sourceRoot, "components/source-import-dialog.tsx"),
    ];
    const forbidden = /FFmpeg|FFprobe|yt-dlp|whisper\.cpp|SIAOCUT_[A-Z_]+|SHA-?256/;
    const violations = normalSurfaces
      .filter((path) => forbidden.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(sourceRoot.length + 1));

    expect(violations).toEqual([]);
  });
});
