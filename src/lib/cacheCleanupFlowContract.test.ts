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
    expect(cleanup).toContain("파일 정보·크기·수정 시각");
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

  it("keeps cleanup failures actionable without exposing runtime diagnostics", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");

    expect(cleanup).not.toContain("String(e)");
    expect(cleanup).not.toContain("r.error");
    expect(cleanup).toContain("최신 목록을 다시 확인한 뒤 재시도하세요");
  });

  it("keeps container cleanup labels focused on the customer's next action", () => {
    const markup = readSource("src/lib/Cleanup.svelte").split("</script>", 2)[1];

    expect(markup).not.toContain("exact record");
    expect(markup).not.toContain(">dangling");
    expect(markup).not.toContain("prune, 삭제, trim");
    expect(markup).toContain("미사용 이미지 정리");
    expect(markup).toContain("최신 상태를 확인하세요");
  });
});
