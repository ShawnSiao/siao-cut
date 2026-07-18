import { afterEach, describe, expect, it } from "vitest";
import { changeUiLocale, detectUiLocale, getUiLocale, tr, UI_LOCALE_STORAGE_KEY } from "./i18n";
import { enUS, zhCN } from "./locales";

afterEach(() => changeUiLocale("zh-CN"));

describe("desktop localization", () => {
  it("keeps locale resources in exact key parity", () => {
    expect(Object.keys(enUS)).toEqual(Object.keys(zhCN));
  });

  it("detects Chinese explicitly and defaults every other system locale to English", () => {
    expect(detectUiLocale("zh-CN")).toBe("zh-CN");
    expect(detectUiLocale("zh-TW")).toBe("zh-CN");
    expect(detectUiLocale("en-GB")).toBe("en-US");
    expect(detectUiLocale("fr-FR")).toBe("en-US");
  });

  it("persists and applies locale changes without touching project preferences", () => {
    localStorage.setItem("siaocut.exportPreferences.v1", "preserve-me");
    changeUiLocale("en-US");
    expect(getUiLocale()).toBe("en-US");
    expect(localStorage.getItem(UI_LOCALE_STORAGE_KEY)).toBe("en-US");
    expect(document.documentElement.lang).toBe("en-US");
    expect(localStorage.getItem("siaocut.exportPreferences.v1")).toBe("preserve-me");
    expect(tr("app.locale.label")).toBe("Interface language");
  });
});
