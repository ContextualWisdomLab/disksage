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
});
