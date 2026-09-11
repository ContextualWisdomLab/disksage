import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function expectFailure(operation, expectedMessage) {
  try {
    operation();
  } catch (error) {
    if (expectedMessage) {
      const message = error instanceof Error ? error.message : String(error);
      assert(
        message.includes(expectedMessage),
        `expected SQLite failure containing ${JSON.stringify(expectedMessage)}, got ${JSON.stringify(message)}`,
      );
    }
    return;
  }
  throw new Error(
    expectedMessage
      ? `expected SQLite failure containing ${JSON.stringify(expectedMessage)}`
      : "expected SQLite operation to fail",
  );
}

const schemaPath = process.argv[2];
assert(schemaPath, "translation-ledger-schema-path-required");

const database = new DatabaseSync(":memory:");
try {
  database.exec("PRAGMA foreign_keys = ON;");
  database.exec(readFileSync(schemaPath, "utf8"));

  const tables = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
    .all()
    .map((row) => String(row.name));
  assert(
    JSON.stringify(tables) ===
      JSON.stringify([
        "translation_messages",
        "translation_resource_versions",
        "translation_screen_keys",
      ]),
    `unexpected translation ledger tables: ${JSON.stringify(tables)}`,
  );
  assert(
    !tables.some((name) => name.includes("ontology") || name.includes("settings")),
    "translation ledger must not own ontology or settings tables",
  );

  const insertVersion = database.prepare(
    "INSERT INTO translation_resource_versions (resource_version, schema_version, created_at_unix_ms, content_sha256) VALUES (?, ?, ?, ?)",
  );
  insertVersion.run("2026.09.11.1", 1, 1, "a".repeat(64));
  insertVersion.run("2026.09.11.2", 1, 2, "b".repeat(64));
  expectFailure(() => insertVersion.run("bad-digest", 1, 3, "not-a-digest"));

  const insertScreenKey = database.prepare(
    "INSERT INTO translation_screen_keys (screen_key, screen_area) VALUES (?, ?)",
  );
  insertScreenKey.run("scan.action.start", "scan");
  insertScreenKey.run("scan.action.cancel", "scan");

  const insertMessage = database.prepare(
    "INSERT INTO translation_messages (resource_version, locale, screen_key, text_value) VALUES (?, ?, ?, ?)",
  );
  insertMessage.run("2026.09.11.1", "ko", "scan.action.start", "스캔");

  expectFailure(() => insertMessage.run("2026.09.11.1", "ko", "scan.action.start", "중복"));
  expectFailure(() => insertMessage.run("2026.09.11.1", "pt", "scan.action.start", "Scan"));
  expectFailure(() => insertMessage.run("missing", "en", "scan.action.start", "Scan"));
  expectFailure(() => insertMessage.run("2026.09.11.1", "en", "scan.action.cancel", ""));

  expectFailure(
    () =>
      database
        .prepare("UPDATE translation_resource_versions SET schema_version = 2 WHERE resource_version = ?")
        .run("2026.09.11.2"),
    "translation-resource-version-immutable",
  );
  expectFailure(
    () =>
      database.prepare("DELETE FROM translation_resource_versions WHERE resource_version = ?").run("2026.09.11.2"),
    "translation-resource-version-immutable",
  );
  expectFailure(
    () =>
      database.prepare("UPDATE translation_screen_keys SET screen_area = ? WHERE screen_key = ?").run("other", "scan.action.cancel"),
    "translation-screen-key-immutable",
  );
  expectFailure(
    () => database.prepare("DELETE FROM translation_screen_keys WHERE screen_key = ?").run("scan.action.cancel"),
    "translation-screen-key-immutable",
  );
  expectFailure(
    () =>
      database
        .prepare(
          "UPDATE translation_messages SET text_value = ? WHERE resource_version = ? AND locale = ? AND screen_key = ?",
        )
        .run("변경", "2026.09.11.1", "ko", "scan.action.start"),
    "translation-message-immutable",
  );
  expectFailure(
    () =>
      database
        .prepare("DELETE FROM translation_messages WHERE resource_version = ? AND locale = ? AND screen_key = ?")
        .run("2026.09.11.1", "ko", "scan.action.start"),
    "translation-message-immutable",
  );

  const messageColumns = database
    .prepare("PRAGMA table_info(translation_messages)")
    .all()
    .map((row) => String(row.name));
  assert(
    JSON.stringify(messageColumns) === JSON.stringify(["resource_version", "locale", "screen_key", "text_value"]),
    `unexpected translation_messages columns: ${JSON.stringify(messageColumns)}`,
  );
} finally {
  database.close();
}
