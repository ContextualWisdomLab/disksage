import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cache cleanup execution boundary", () => {
  it("keeps cache mutation behind one fail-closed backend authority", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");
    const backend = readSource("src-tauri/src/cache_cleanup.rs");
    const tauri = readSource("src-tauri/src/lib.rs");

    expect(cleanup).toContain("api.listCacheTargets(candidate.path)");
    expect(cleanup).toContain("api.cleanCacheContents(candidate.path, targets)");
    expect(cleanup).toContain("api.cleanRegenerableCaches()");
    expect(cleanup).toContain("객체 지문·크기·수정시각");
    expect(cleanup).toContain("npm·pnpm·Adobe·Edge·uv·Trivy 캐시만 대상으로");
    expect(backend).toContain("pub fn clean_cache_contents(");
    expect(backend).toContain("cache-cleanup-targets-stale");
    expect(backend).toContain("trash_delete_if_identity(");
    expect(tauri).toContain("cache_cleanup::clean_cache_contents");
    expect(tauri).toContain("cache_cleanup::list_cache_targets");
    expect(tauri).toContain("commands::clean_regenerable_caches");
  });

  it("surfaces an actionable status when a cache candidate has no direct cleanup targets", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");

    expect(cleanup).toMatch(
      /if \(targets\.length === 0\) \{[\s\S]*loadError = `\$\{candidate\.label\}에 정리할 직계 항목이 없습니다\.`;[\s\S]*return;/,
    );
  });
});
