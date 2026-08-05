import { afterEach, describe, expect, it } from "vitest";
import { parseWhisperModelComponent, sanitizeWindowsFileName, serializeWhisperModelComponent, structuredCoreErrorMessage, whisperModelComponentKey } from "./core";
import { changeUiLocale } from "./i18n";

afterEach(() => changeUiLocale("zh-CN"));

describe("desktop Core bridge helpers", () => {
  it("sanitizes project titles before using them as Windows file names", () => {
    expect(sanitizeWindowsFileName('CON:<bad>|title?* .')).toBe("CON__bad__title__");
    expect(sanitizeWindowsFileName("  发布口播：第一期.  ")).toBe("发布口播_第一期");
    expect(sanitizeWindowsFileName("LPT1.txt")).toBe("_LPT1.txt");
    expect(sanitizeWindowsFileName("...")).toBe("未命名项目");
  });

  it("limits file names by Unicode code points without splitting surrogate pairs", () => {
    const safeName = sanitizeWindowsFileName("片".repeat(119) + "🎬🎬");

    expect(Array.from(safeName)).toHaveLength(120);
    expect(safeName.endsWith("🎬")).toBe(true);
  });

  it("localizes structured bridge validation failures", () => {
    changeUiLocale("en-US");

    expect(structuredCoreErrorMessage("structured_core_payload_too_large: request exceeds 64 KiB"))
      .toBe("The structured Core request is too large. Reduce the batch size.");
    expect(structuredCoreErrorMessage("structured_core_request_invalid: invalid project"))
      .toBe("The structured Core request parameters are invalid.");
  });

  it("persists model preferences as exact versioned component keys", () => {
    const serialized = serializeWhisperModelComponent("base");

    expect(JSON.parse(serialized)).toEqual(whisperModelComponentKey("base"));
    expect(parseWhisperModelComponent(serialized)).toBe("base");
    expect(parseWhisperModelComponent("small")).toBe("small");
    expect(parseWhisperModelComponent(JSON.stringify({
      componentId: "whisper-model",
      version: "2",
      variant: { platform: "windows", architecture: "x86_64", model: "small" },
    }))).toBe("tiny");
    expect(parseWhisperModelComponent(JSON.stringify({
      componentId: "whisper-model",
      version: "1",
      variant: { platform: "windows", architecture: "x86_64", model: "small" },
      url: "https://example.invalid/should-not-be-persisted",
    }))).toBe("tiny");
  });
});
