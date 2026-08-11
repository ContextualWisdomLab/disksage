import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cache cleanup destructive authority", () => {
  it("fails closed until recycling is bound to the validated filesystem object", () => {
    const backend = readSource("src-tauri/src/cache_cleanup.rs");

    expect(backend).not.toContain("safety::trash_delete(&display_path");
    expect(backend).toContain("cache-cleanup-atomic-trash-unavailable");
  });
});
