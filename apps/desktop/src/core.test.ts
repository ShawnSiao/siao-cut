import { afterEach, describe, expect, it } from "vitest";
import { assertCoreCapabilities, sanitizeWindowsFileName, structuredCoreErrorMessage } from "./core";
import { requiredCoreCapabilities } from "./generated/core-contract";
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
});

it("rejects an old Core instead of falling back to unprotected writes",()=>{expect(()=>assertCoreCapabilities(requiredCoreCapabilities)).not.toThrow();expect(()=>assertCoreCapabilities(["editing-v1"])).toThrow(/Core 版本不匹配/);expect(()=>assertCoreCapabilities(undefined)).toThrow(/Core 版本不匹配/);});
