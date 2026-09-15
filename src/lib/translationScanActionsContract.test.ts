import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const pageSource = readFileSync("src/routes/+page.svelte", "utf8");
const i18nPath = "src/lib/i18n.ts";

describe("scan action translation migration contract", () => {
  it("projects scan and cancel labels through the native translation ledger", () => {
    expect(existsSync(i18nPath), "frontend translation adapter must exist").toBe(true);
    if (!existsSync(i18nPath)) return;

    const i18nSource = readFileSync(i18nPath, "utf8");
    expect(i18nSource).toContain('invoke<TranslationMessageView>("get_translation_message"');
    expect(pageSource).toContain('"app.action.scan"');
    expect(pageSource).toContain('"app.action.cancel"');
    expect(pageSource).not.toMatch(/<button[^>]*>\s*스캔\s*<\/button>/);
    expect(pageSource).not.toMatch(/<button[^>]*>\s*취소\s*<\/button>/);
  });

  it("keeps locale selection explicit and translation failure visible instead of falling back silently", () => {
    expect(pageSource).toContain("SUPPORTED_LOCALES");
    expect(pageSource).toContain("translationError");
    expect(pageSource).toContain('role="alert"');
    expect(pageSource).toContain("disabled={actionLabels === null");
    expect(pageSource).not.toContain("translationFallback");
  });
});
