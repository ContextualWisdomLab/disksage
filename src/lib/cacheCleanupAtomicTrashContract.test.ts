import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

function boundedSection(source: string, startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("cache cleanup destructive authority", () => {
  it("uses the shared manifest-and-identity-bound recycle primitive", () => {
    const backend = readSource("src-tauri/src/cache_cleanup.rs");
    const safety = readSource("src-tauri/src/safety.rs");
    const cacheEntryPoint = boundedSection(
      safety,
      "pub fn trash_delete_cache_target_if_identity(",
      "fn trash_delete_if_identity_with_manifest(",
    );
    const sharedPrimitive = boundedSection(
      safety,
      "fn trash_delete_if_identity_with_manifest(",
      "/// Permanently remove one unchanged, current-user-owned generated directory.",
    );

    expect(backend).toContain("safety::trash_delete_cache_target_if_identity(");
    expect(backend).toContain("cache-cleanup-targets-stale");
    expect(cacheEntryPoint).toContain("trash_delete_if_identity_with_manifest(");
    expect(cacheEntryPoint).toContain("Some((expected_modified_ms, expected_manifest_fingerprint))");
    expect(sharedPrimitive).toContain("std::fs::rename(path, &staged)");
    expect(sharedPrimitive).toContain("if moved_id != expected_object_id");
    expect(sharedPrimitive).toContain("if !manifest_matches(&staged)");
  });
});
