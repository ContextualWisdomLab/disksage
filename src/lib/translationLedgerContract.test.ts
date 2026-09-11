import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  SUPPORTED_LOCALES,
  TranslationLookupCache,
  translationLookupCacheKey,
} from "./translationLedger";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const schemaPath = resolve(repositoryRoot, "src-tauri/resources/translation/0001_translation_ledger.sql");

function openLedger(): DatabaseSync {
  const database = new DatabaseSync(":memory:");
  database.exec("PRAGMA foreign_keys = ON;");
  database.exec(readFileSync(schemaPath, "utf8"));
  return database;
}

describe("versioned translation ledger", () => {
  it("executes as normalized, append-only SQLite schema with immutable version/key references", () => {
    const database = openLedger();
    const tables = database
      .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
      .all()
      .map((row) => String(row.name));

    expect(tables).toEqual([
      "translation_messages",
      "translation_resource_versions",
      "translation_screen_keys",
    ]);
    expect(tables.some((name) => name.includes("ontology") || name.includes("settings"))).toBe(false);

    const insertVersion = database.prepare(
      "INSERT INTO translation_resource_versions (resource_version, schema_version, created_at_unix_ms, content_sha256) VALUES (?, ?, ?, ?)",
    );
    insertVersion.run("2026.09.11.1", 1, 1, "a".repeat(64));
    insertVersion.run("2026.09.11.2", 1, 2, "b".repeat(64));
    expect(() => insertVersion.run("bad-digest", 1, 3, "not-a-digest")).toThrow();

    const insertScreenKey = database.prepare(
      "INSERT INTO translation_screen_keys (screen_key, screen_area) VALUES (?, ?)",
    );
    insertScreenKey.run("scan.action.start", "scan");
    insertScreenKey.run("scan.action.cancel", "scan");

    const insertMessage = database.prepare(
      "INSERT INTO translation_messages (resource_version, locale, screen_key, text_value) VALUES (?, ?, ?, ?)",
    );
    insertMessage.run("2026.09.11.1", "ko", "scan.action.start", "스캔");

    expect(() => insertMessage.run("2026.09.11.1", "ko", "scan.action.start", "중복")).toThrow();
    expect(() => insertMessage.run("2026.09.11.1", "pt", "scan.action.start", "Scan")).toThrow();
    expect(() => insertMessage.run("missing", "en", "scan.action.start", "Scan")).toThrow();
    expect(() => insertMessage.run("2026.09.11.1", "en", "scan.action.cancel", "")).toThrow();

    expect(() =>
      database
        .prepare("UPDATE translation_resource_versions SET schema_version = 2 WHERE resource_version = ?")
        .run("2026.09.11.2"),
    ).toThrow("translation-resource-version-immutable");
    expect(() =>
      database.prepare("DELETE FROM translation_resource_versions WHERE resource_version = ?").run("2026.09.11.2"),
    ).toThrow("translation-resource-version-immutable");
    expect(() =>
      database.prepare("UPDATE translation_screen_keys SET screen_area = ? WHERE screen_key = ?").run("other", "scan.action.cancel"),
    ).toThrow("translation-screen-key-immutable");
    expect(() =>
      database.prepare("DELETE FROM translation_screen_keys WHERE screen_key = ?").run("scan.action.cancel"),
    ).toThrow("translation-screen-key-immutable");
    expect(() =>
      database.prepare("UPDATE translation_messages SET text_value = ? WHERE resource_version = ? AND locale = ? AND screen_key = ?")
        .run("변경", "2026.09.11.1", "ko", "scan.action.start"),
    ).toThrow("translation-message-immutable");
    expect(() =>
      database.prepare("DELETE FROM translation_messages WHERE resource_version = ? AND locale = ? AND screen_key = ?")
        .run("2026.09.11.1", "ko", "scan.action.start"),
    ).toThrow("translation-message-immutable");

    const messageColumns = database
      .prepare("PRAGMA table_info(translation_messages)")
      .all()
      .map((row) => String(row.name));
    expect(messageColumns).toEqual(["resource_version", "locale", "screen_key", "text_value"]);

    database.close();
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
