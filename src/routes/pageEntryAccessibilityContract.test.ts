import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("canonical scan-entry accessibility surface", () => {
  it("keeps the existing entry list as the single keyboard navigation surface", () => {
    const page = readSource("src/routes/+page.svelte");

    expect(page).toContain(
      '<div class="entry-scroll" role="region" tabindex="0" aria-label="현재 폴더 항목 목록">',
    );
    expect(page).toContain('<ul class="entries">');
    expect(page).toContain("{#each node.entries as e}");
    expect(page).toContain('onclick={() => open(e.path)}');
    expect(page).toContain("{fmtBytes(e.size)}");
    expect(page).toContain(".entry-scroll:focus-visible");
  });

  it("gives an empty scan result a visible next action instead of a blank list", () => {
    const page = readSource("src/routes/+page.svelte");

    expect(page).toContain("{#if node.entries.length === 0}");
    expect(page).toContain(
      '<p class="empty-entries" role="status">표시할 항목이 없습니다. 상위 폴더로 이동하거나 다른 폴더를 스캔하세요.</p>',
    );
  });
});
