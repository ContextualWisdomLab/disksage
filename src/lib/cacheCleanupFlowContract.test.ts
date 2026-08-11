import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cache cleanup execution boundary", () => {
  it("uses one backend operation for cache cleanup", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");
    const backend = readSource("src-tauri/src/cache_cleanup.rs");
    const tauri = readSource("src-tauri/src/lib.rs");

    expect(cleanup).not.toContain("api.expandCleanTargets(c.path)");
    expect(cleanup).toContain('invoke<api.CleanResult[]>("clean_cache_contents"');
    expect(backend).toContain("pub fn clean_cache_contents(");
    expect(backend).toContain("guard.still_current()");
    expect(tauri).toContain("cache_cleanup::clean_cache_contents");
  });
});
