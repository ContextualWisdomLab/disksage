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
    const api = readSource("src/lib/api.ts");
    const commands = readSource("src-tauri/src/commands.rs");
    const tauri = readSource("src-tauri/src/lib.rs");

    expect(cleanup).not.toContain("api.expandCleanTargets(c.path)");
    expect(cleanup).toContain("api.cleanCacheContents(c.path)");
    expect(api).toContain('invoke<CleanResult[]>("clean_cache_contents"');
    expect(commands).toContain("pub fn clean_cache_contents(");
    expect(tauri).toContain("commands::clean_cache_contents");
  });
});
