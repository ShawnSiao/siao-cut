import i18n from "i18next";
import { enUS, type TranslationKey, zhCN } from "./locales";
import type { UiLocale } from "./types";

export type { UiLocale } from "./types";

export const UI_LOCALE_STORAGE_KEY = "siaocut.uiLocale.v1";

export function detectUiLocale(language = navigator.language): UiLocale {
  return language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

export function getUiLocale(): UiLocale {
  const stored = localStorage.getItem(UI_LOCALE_STORAGE_KEY);
  return stored === "zh-CN" || stored === "en-US" ? stored : detectUiLocale();
}

void i18n.init({
  resources: {
    "zh-CN": { translation: zhCN },
    "en-US": { translation: enUS },
  },
  lng: getUiLocale(),
  fallbackLng: "zh-CN",
  initAsync: false,
  interpolation: { escapeValue: false },
});

export function tr(key: TranslationKey, values?: Record<string, unknown>): string {
  return i18n.t(key, values);
}

export function changeUiLocale(locale: UiLocale): void {
  localStorage.setItem(UI_LOCALE_STORAGE_KEY, locale);
  document.documentElement.lang = locale;
  void i18n.changeLanguage(locale);
}

document.documentElement.lang = getUiLocale();

export default i18n;
