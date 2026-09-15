import { invoke } from "@tauri-apps/api/core";

export const SUPPORTED_LOCALES = ["ko", "en", "ja", "zh", "vi", "es", "de", "fr"] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

export interface TranslationMessageView {
  resource_version: string;
  locale: SupportedLocale;
  screen_key: string;
  text: string;
}

export const getTranslationMessage = (locale: SupportedLocale, screenKey: string) =>
  invoke<TranslationMessageView>("get_translation_message", { locale, screenKey });
