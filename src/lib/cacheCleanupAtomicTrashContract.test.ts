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
    const safety = readSource("src-tauri/src/safety_authority.rs");
    const cacheEntryPoint = boundedSection(
      safety,
      "pub(crate) fn trash_delete_cache_target_with_outcome(",
      "/// Permanently remove one unchanged generated cache directory.",
    );

    expect(backend).toContain("safety::trash_delete_cache_target_with_outcome(");
    expect(backend).toContain("cache-cleanup-targets-stale");
    expect(cacheEntryPoint).toContain("cache_authority_snapshot(");
    expect(cacheEntryPoint).toContain("std::fs::rename(path, &staged)");
    expect(cacheEntryPoint).toContain("if moved_id != expected_object_id");
    expect(cacheEntryPoint).toContain("if staged_live != staged_baseline");
  });
});
