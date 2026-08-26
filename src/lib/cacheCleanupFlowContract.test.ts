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
    expect(cleanup).toContain("api.listProvenCacheTrash()");
    expect(cleanup).toContain("api.provenCacheTrashApprovalPhrase()");
    expect(cleanup).toContain("api.purgeProvenCacheTrash(approvalPhrase)");
    expect(cleanup).toContain("cache-trash-confirmation-mismatch");
    expect(cleanup).toContain("휴지통 내용이 바뀌어 최신 목록을 불러왔습니다");
    expect(cleanup).toContain("observed_available_gain_bytes");
    expect(cleanup).toContain("각 항목의 크기와 수정 시각을 다시 확인합니다");
    expect(cleanup).toContain("재생성할 수 있는 캐시만 대상으로 하며");
    expect(cleanup).not.toContain("active-use");
    expect(cleanup).not.toContain("증거가 바뀐 항목");
    expect(backend).toContain("pub fn clean_cache_contents(");
    expect(backend).toContain("cache-cleanup-targets-stale");
    expect(backend).toContain("trash_delete_if_identity(");
    expect(backend).toContain("pub fn proven_cache_trash_approval_phrase(");
    expect(tauri).toContain("cache_cleanup::clean_cache_contents");
    expect(tauri).toContain("cache_cleanup::list_cache_targets");
    expect(tauri).toContain("commands::clean_regenerable_caches");
    expect(tauri).toContain("commands::list_proven_cache_trash");
    expect(tauri).toContain("commands::proven_cache_trash_approval_phrase");
    expect(tauri).toContain("commands::purge_proven_cache_trash");
  });

  it("surfaces an actionable status when a cache candidate has no direct cleanup targets", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");

    expect(cleanup).toMatch(
      /if \(targets\.length === 0\) \{[\s\S]*loadError = `\$\{candidate\.label\}에 정리할 직계 항목이 없습니다\.`;[\s\S]*return;/,
    );
  });
});
