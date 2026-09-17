import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

function functionBody(source: string, signature: string, nextMarker: string): string {
  const start = source.indexOf(signature);
  expect(start, `${signature} must exist`).toBeGreaterThanOrEqual(0);
  const end = source.indexOf(nextMarker, start + signature.length);
  expect(end, `${nextMarker} must follow ${signature}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("cache-Trash owner reconciliation", () => {
  it("retains candidate-bound read-only snapshot authority", () => {
    const backend = readSource("src-tauri/src/cache_cleanup.rs");

    expect(backend).toContain("pub struct CacheTrashSnapshot");
    expect(backend).toContain("pub fn proven_cache_trash_snapshot(");
    expect(backend).toContain("approval_phrase");
    expect(backend).toContain("disksage.cache-trash-purge-approval.v1");
  });

  it("keeps irreversible cache-Trash deletion unavailable until final object binding exists", () => {
    const backend = readSource("src-tauri/src/cache_cleanup.rs");
    const cli = readSource("src-tauri/src/bin/disksage-cache-cleanup.rs");
    const purge = functionBody(
      backend,
      "pub fn purge_proven_cache_trash(",
      "pub(crate) fn clean_cache_contents_inner(",
    );

    expect(backend).toContain("cache-trash-identity-bound-permanent-delete-unavailable");
    expect(purge).not.toContain("remove_dir_all");
    expect(cli).toContain("proven_cache_trash_snapshot");
    expect(cli).toContain("PERMANENT_CACHE_TRASH_DELETE_UNAVAILABLE");
    expect(cli).toMatch(
      /if args\.purge_proven_cache_trash \{\s*return Err\(PERMANENT_CACHE_TRASH_DELETE_UNAVAILABLE\.into\(\)\);\s*\}/,
    );
    expect(cli).not.toContain("purge_proven_cache_trash(&home_directory()?");
  });

  it("preserves the newer catalog and native-prune surface while restoring the safety owner", () => {
    const backend = readSource("src-tauri/src/cache_cleanup.rs");
    const cli = readSource("src-tauri/src/bin/disksage-cache-cleanup.rs");

    for (const id of [
      "edge-code-sign-clones",
      "appmap-download-cache",
      "superset-http-cache",
      "superset-code-cache",
      "playwright-cache",
    ]) {
      expect(backend).toContain(`\"${id}\"`);
    }
    expect(cli).toContain("--prune-uv-cache");
    expect(cli).toContain("--cache-id");
    expect(cli).toContain("clean_catalog_cache_headless");
    expect(cli).toContain("prune_uv_cache_headless");
  });
});
