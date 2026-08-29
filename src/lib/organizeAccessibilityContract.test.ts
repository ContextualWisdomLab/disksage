import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Organize accessibility status contract", () => {
  it("keeps the asynchronous failure alert mounted before its text changes", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain(
      '<p class="error live-region" role="alert" aria-live="assertive" aria-atomic="true">{loadError}</p>',
    );
    expect(source).not.toContain('{#if loadError}<p class="error" role="alert">');
  });

  it("keeps one non-interrupting completion status region mounted before result text changes", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain(
      '<p class="result-status live-region" role="status" aria-live="polite" aria-atomic="true">',
    );
    expect(source).toContain('{results.filter((r) => r.ok).length}/{results.length}개 완료');
  });

  it("keeps clipboard handoff feedback mounted before asynchronous status text changes", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain(
      '<p class="muted export-status live-region" role="status" aria-live="polite" aria-atomic="true">{exportStatus}</p>',
    );
    expect(source).not.toContain('{#if exportStatus}<p class="muted">{exportStatus}</p>{/if}');
  });

  it("announces when undo succeeds but there is no recent move record to restore", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain('resultAction === "undo" && results.length === 0');
    expect(source).toContain("되돌릴 최근 이동 기록이 없습니다.");
  });
});
