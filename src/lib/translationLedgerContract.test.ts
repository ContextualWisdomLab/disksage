import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  SUPPORTED_LOCALES,
  TranslationLookupCache,
  translationLookupCacheKey,
} from "./translationLedger";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const schemaPath = resolve(repositoryRoot, "src-tauri/resources/translation/0001_translation_ledger.sql");
const sqliteContractPath = resolve(repositoryRoot, "scripts/ci/translation-ledger-sqlite-contract.mjs");

/**
 * Runs the real SQLite migration under the repository's minimum Node 22.12 runtime,
 * where `node:sqlite` is intentionally available only behind `--experimental-sqlite`.
 */
function runSqliteLedgerContract(): void {
  const result = spawnSync(process.execPath, ["--experimental-sqlite", sqliteContractPath, schemaPath], {
    encoding: "utf8",
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(
      [
        `translation-ledger-sqlite-contract-exit:${String(result.status)}`,
        result.stdout.trim(),
        result.stderr.trim(),
      ]
        .filter(Boolean)
        .join("\n"),
    );
  }
}

describe("versioned translation ledger", () => {
  it("executes as normalized, append-only SQLite schema with immutable version/key references", () => {
    runSqliteLedgerContract();
  });

  it("binds cache identity to resource version, locale, and stable screen key", () => {
    expect(SUPPORTED_LOCALES).toEqual(["ko", "en", "ja", "zh", "vi", "es", "de", "fr"]);
    expect(translationLookupCacheKey("2026.09.11.1", "ko", "scan.action.start")).not.toBe(
      translationLookupCacheKey("2026.09.11.2", "ko", "scan.action.start"),
    );
    expect(() => new TranslationLookupCache(0)).toThrow("translation-cache-capacity-must-be-positive-safe-integer");
    expect(() => new TranslationLookupCache(1.5)).toThrow("translation-cache-capacity-must-be-positive-safe-integer");

    const cache = new TranslationLookupCache(2);
    cache.set("2026.09.11.1", "ko", "scan.action.start", "스캔");
    cache.set("2026.09.11.1", "en", "scan.action.start", "Scan");
    expect(cache.get("2026.09.11.1", "ko", "scan.action.start")).toBe("스캔");

    cache.set("2026.09.11.1", "ja", "scan.action.start", "スキャン");
    expect(cache.get("2026.09.11.1", "en", "scan.action.start")).toBeUndefined();
    expect(cache.get("2026.09.11.1", "ko", "scan.action.start")).toBe("스캔");
    expect(cache.get("2026.09.11.1", "ja", "scan.action.start")).toBe("スキャン");

    cache.set("2026.09.11.2", "ko", "scan.action.start", "새 스캔");
    cache.clearVersion("2026.09.11.1");
    expect(cache.get("2026.09.11.1", "ja", "scan.action.start")).toBeUndefined();
    expect(cache.get("2026.09.11.2", "ko", "scan.action.start")).toBe("새 스캔");
  });

  it("keeps the production translation cache inside the mandatory 100% frontend coverage denominator", () => {
    const config = readFileSync(resolve(repositoryRoot, "vitest.config.ts"), "utf8");
    expect(config).toContain('"src/lib/translationLedger.ts"');
  });
});
