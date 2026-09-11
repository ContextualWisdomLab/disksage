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
 * Returns the SQLite-enabling CLI argument only for the supported Node release that still needs it.
 * Node 22.13+ unflagged `node:sqlite`; newer supported majors should not depend on an obsolete
 * positive experimental flag continuing to be accepted.
 */
function sqliteRuntimeArguments(nodeVersion: string): string[] {
  const match = /^(\d+)\.(\d+)\.(\d+)/.exec(nodeVersion);
  if (!match) throw new Error("translation-ledger-node-version-unparseable");
  const major = Number(match[1]);
  const minor = Number(match[2]);
  return major === 22 && minor === 12 ? ["--experimental-sqlite"] : [];
}

/**
 * Runs the real SQLite migration with the repository's current Node executable while preserving
 * the declared 22.12/24/26+ engine range. Only Node 22.12 receives its required SQLite flag.
 */
function runSqliteLedgerContract(): void {
  const result = spawnSync(
    process.execPath,
    [...sqliteRuntimeArguments(process.versions.node), sqliteContractPath, schemaPath],
    { encoding: "utf8" },
  );
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

  it("uses the experimental SQLite flag only for supported Node 22.12", () => {
    expect(sqliteRuntimeArguments("22.12.0")).toEqual(["--experimental-sqlite"]);
    expect(sqliteRuntimeArguments("22.13.0")).toEqual([]);
    expect(sqliteRuntimeArguments("24.0.0")).toEqual([]);
    expect(sqliteRuntimeArguments("26.0.0")).toEqual([]);
    expect(() => sqliteRuntimeArguments("not-a-node-version")).toThrow(
      "translation-ledger-node-version-unparseable",
    );
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
