import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cache cleanup destructive authority", () => {
  it("uses the shared manifest-and-identity-bound recycle primitive", () => {
    const backend = readSource("src-tauri/src/cache_cleanup.rs");
    const safety = readSource("src-tauri/src/safety.rs");

    expect(backend).toContain("safety::trash_delete_cache_target_if_identity(");
    expect(backend).toContain("cache-cleanup-targets-stale");
    expect(safety).toMatch(
      /pub fn trash_delete_cache_target_if_identity\([\s\S]*trash_delete_if_identity_with_manifest\(/,
    );
    expect(safety).toContain("std::fs::rename(path, &staged)");
    expect(safety).toContain("if moved_id != expected_object_id");
    expect(safety).toContain("if !manifest_matches(&staged)");
  });
});
