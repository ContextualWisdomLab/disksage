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
  it("executes as normalized SQLite schema with immutable version/key references", () => {
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

    database
      .prepare(
        "INSERT INTO translation_resource_versions (resource_version, schema_version, created_at_unix_ms, content_sha256) VALUES (?, ?, ?, ?)",
      )
      .run("2026.09.11.1", 1, 1, "a".repeat(64));
    database
      .prepare("INSERT INTO translation_screen_keys (screen_key, screen_area) VALUES (?, ?)")
      .run("scan.action.start", "scan");
    database
      .prepare(
        "INSERT INTO translation_messages (resource_version, locale, screen_key, text_value) VALUES (?, ?, ?, ?)",
      )
      .run("2026.09.11.1", "ko", "scan.action.start", "스캔");

    expect(() =>
      database
        .prepare(
          "INSERT INTO translation_messages (resource_version, locale, screen_key, text_value) VALUES (?, ?, ?, ?)",
        )
        .run("2026.09.11.1", "ko", "scan.action.start", "중복"),
    ).toThrow();
    expect(() =>
      database
        .prepare(
          "INSERT INTO translation_messages (resource_version, locale, screen_key, text_value) VALUES (?, ?, ?, ?)",
        )
        .run("2026.09.11.1", "pt", "scan.action.start", "Scan"),
    ).toThrow();
    expect(() =>
      database
        .prepare(
          "INSERT INTO translation_messages (resource_version, locale, screen_key, text_value) VALUES (?, ?, ?, ?)",
        )
        .run("missing", "en", "scan.action.start", "Scan"),
    ).toThrow();
    expect(() =>
      database.prepare("DELETE FROM translation_resource_versions WHERE resource_version = ?").run("2026.09.11.1"),
    ).toThrow();

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

    const cache = new TranslationLookupCache(2);
    cache.set("2026.09.11.1", "ko", "scan.action.start", "스캔");
    cache.set("2026.09.11.1", "en", "scan.action.start", "Scan");
    expect(cache.get("2026.09.11.1", "ko", "scan.action.start")).toBe("스캔");

    cache.set("2026.09.11.1", "ja", "scan.action.start", "スキャン");
    expect(cache.get("2026.09.11.1", "en", "scan.action.start")).toBeUndefined();
    expect(cache.get("2026.09.11.1", "ko", "scan.action.start")).toBe("스캔");
    expect(cache.get("2026.09.11.1", "ja", "scan.action.start")).toBe("スキャン");

    cache.clearVersion("2026.09.11.1");
    expect(cache.get("2026.09.11.1", "ko", "scan.action.start")).toBeUndefined();
  });
});
