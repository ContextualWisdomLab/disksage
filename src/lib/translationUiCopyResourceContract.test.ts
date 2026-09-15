import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const resourcePath = "src-tauri/resources/translation/releases/2026.09.15.1.json";
const translationResourceSource = readFileSync(
  "src-tauri/src/translation_resource.rs",
  "utf8",
);
const tauriConfig = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")) as {
  bundle?: { resources?: string[] };
};

const requiredKeys = [
  "app.action.cancel",
  "app.action.scan",
  "app.selector.locale_label",
  "app.selector.root_label",
  "app.status.translation_unavailable",
  "app.error.root_load",
  "app.error.scan_result_load",
  "app.error.scan_prepare",
  "app.error.scan_start",
  "app.error.folder_load",
  "app.stats.files_label",
  "app.stats.folders_label",
  "app.stats.total_label",
  "app.stats.skipped_label",
  "app.a11y.entry_focus",
  "app.a11y.current_entries",
  "app.empty.entries",
] as const;

const locales = ["ko", "en", "ja", "zh", "vi", "es", "de", "fr"] as const;

describe("screen-wide translation resource release contract", () => {
  it("pins the next immutable resource in native admission and bundle packaging", () => {
    expect(existsSync(resourcePath), "next translation resource release must exist").toBe(true);
    expect(translationResourceSource).toContain(
      'CURRENT_TRANSLATION_RESOURCE_VERSION: &str = "2026.09.15.1"',
    );
    expect(translationResourceSource).toContain(
      '"resources/translation/releases/2026.09.15.1.json"',
    );
    expect(tauriConfig.bundle?.resources).toContain(
      "resources/translation/releases/2026.09.15.1.json",
    );
  });

  it("ships every bounded main-screen key with the exact eight locale set", () => {
    if (!existsSync(resourcePath)) return;

    const resource = JSON.parse(readFileSync(resourcePath, "utf8")) as {
      resource_version: string;
      schema_version: number;
      messages: Record<string, Record<string, string>>;
    };

    expect(resource.resource_version).toBe("2026.09.15.1");
    expect(resource.schema_version).toBe(1);
    expect(Object.keys(resource.messages).sort()).toEqual([...requiredKeys].sort());

    for (const key of requiredKeys) {
      expect(Object.keys(resource.messages[key] ?? {}).sort(), key).toEqual([...locales].sort());
      for (const locale of locales) {
        expect(resource.messages[key]?.[locale]?.trim().length, `${key}/${locale}`).toBeGreaterThan(0);
      }
    }
  });

  it("keeps long-locale copy explicit instead of relying on English fallback", () => {
    if (!existsSync(resourcePath)) return;

    const resource = JSON.parse(readFileSync(resourcePath, "utf8")) as {
      messages: Record<string, Record<string, string>>;
    };
    const key = "app.status.translation_unavailable";

    expect(resource.messages[key]?.de).not.toBe(resource.messages[key]?.en);
    expect(resource.messages[key]?.fr).not.toBe(resource.messages[key]?.en);
    expect(resource.messages[key]?.es).not.toBe(resource.messages[key]?.en);
    expect(resource.messages[key]?.vi).not.toBe(resource.messages[key]?.en);
  });
});
