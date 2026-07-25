import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("desktop typography", () => {
  it("does not render interface labels below the 12px readability floor in imported CSS", () => {
    const entry = readFileSync(resolve(process.cwd(), "src/styles.css"), "utf8");
    const imports = [...entry.matchAll(/@import\s+"([^"]+)"/g)]
      .map((match) => readFileSync(resolve(process.cwd(), "src", match[1]), "utf8"));
    const css = [entry, ...imports].join("\n");
    const undersized = [...css.matchAll(/font-size:\s*([0-9.]+)px/g)]
      .map((match) => Number(match[1]))
      .filter((size) => size < 12);

    expect(undersized).toEqual([]);
  });
});
