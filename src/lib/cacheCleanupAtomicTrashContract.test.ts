import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cache cleanup destructive authority", () => {
  it("uses the identity-bound recycle primitive", () => {
    const backend = readSource("src-tauri/src/cache_cleanup.rs");

    expect(backend).toContain("safety::trash_delete_if_identity_with_outcome(");
    expect(backend).toContain("outcome.moved_to_trash");
    expect(backend).toContain("cache-cleanup-targets-stale");
  });
});
