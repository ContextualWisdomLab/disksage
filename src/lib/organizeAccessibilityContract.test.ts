import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Organize accessibility status contract", () => {
  it("announces asynchronous operation failures as alerts", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain('{#if loadError}<p class="error" role="alert">{loadError}</p>{/if}');
  });

  it("announces move completion counts as non-interrupting status messages", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain('<p role="status">{results.filter((r) => r.ok).length}/{results.length}개 완료');
  });

  it("announces when undo succeeds but there is no recent move record to restore", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain('resultAction === "undo" && results.length === 0');
    expect(source).toContain('<p role="status">되돌릴 최근 이동 기록이 없습니다.</p>');
  });
});
