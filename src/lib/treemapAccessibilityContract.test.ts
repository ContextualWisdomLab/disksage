import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Treemap accessible equivalent", () => {
  it("does not make the pointer-only canvas the sole navigation surface", () => {
    const source = readSource("src/lib/Treemap.svelte");

    expect(source).toContain('aria-hidden="true"');
    expect(source).toContain('class="accessible-tree"');
    expect(source).toContain("{#each node.entries as entry (entry.path)}");
    expect(source).toContain("{#if entry.is_dir}");
    expect(source).toContain("onclick={() => onOpen(entry.path)}");
    expect(source).toContain("fmtBytes(entry.size)");
  });

  it("exposes directory navigation through native controls with an action label", () => {
    const source = readSource("src/lib/Treemap.svelte");

    expect(source).toContain("<details");
    expect(source).toContain(
      "<summary>키보드로 폴더 열기 및 파일 확인 ({node.entries.length}개)</summary>",
    );
    expect(source).toMatch(/<button[\s\S]*onclick=\{\(\) => onOpen\(entry\.path\)\}/);
    expect(source).toContain("폴더 열기 · {fmtBytes(entry.size)}");
  });

  it("makes overflowing file-only lists keyboard scrollable and visibly focused", () => {
    const source = readSource("src/lib/Treemap.svelte");

    expect(source).toContain(
      '<div class="entry-scroll" role="region" tabindex="0" aria-label="현재 폴더 항목 목록">',
    );
    expect(source).toContain(".entry-scroll { max-height: 14rem; overflow-y: auto;");
    expect(source).toContain(".entry-scroll:focus-visible");
    expect(source).not.toContain(".accessible-tree ul { list-style: none; padding: 0; margin: 0.5rem 0 0; max-height: 14rem; overflow-y: auto;");
  });

  it("replaces an empty equivalent list with the customer's next action", () => {
    const source = readSource("src/lib/Treemap.svelte");

    expect(source).toContain("{#if node.entries.length === 0}");
    expect(source).toContain(
      '<p class="empty" role="status">표시할 항목이 없습니다. 상위 폴더로 이동하거나 다른 폴더를 스캔하세요.</p>',
    );
  });
});
