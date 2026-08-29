import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("TopFiles accessible data table", () => {
  it("gives the table a programmatic name from its visible heading", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain('<h2 id="top-files-heading">');
    expect(source).toContain("가장 큰 파일 {files.length}개");
    expect(source).toContain('<table aria-labelledby="top-files-heading">');
  });

  it("associates both header cells explicitly with their columns", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain('<th scope="col">크기</th>');
    expect(source).toContain('<th scope="col">경로</th>');
  });

  it("provides a sequential keyboard link to the named scroll target", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain(
      '<a class="table-focus" href="#top-files-table">파일 표 탐색 시작</a>',
    );
    expect(source).toContain(
      '<div id="top-files-table" class="table-scroll" role="region" tabindex="-1" aria-labelledby="top-files-heading">',
    );
    expect(source).toContain(".table-scroll { max-height: 40vh; max-width: 100%; overflow: auto;");
    expect(source).toContain("table-layout: fixed");
    expect(source).toContain("word-break: break-all");
    expect(source).toContain(".table-scroll:focus-visible { outline: 2px solid currentColor;");
    expect(source).not.toContain("section { max-height: 40vh; overflow-y: auto;");
  });

  it("replaces an empty table with guidance for the next scan", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain("{#if files.length === 0}");
    expect(source).toContain(
      '<p class="empty" role="status">표시할 대용량 파일이 없습니다. 다른 폴더를 선택해 다시 스캔하세요.</p>',
    );
    expect(source).not.toContain("스캔 범위를 넓히세요");
    expect(source).toContain("{:else}");
  });

  it("uses platform colors instead of a light-only table surface", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain("background: Canvas; color: CanvasText;");
    expect(source).not.toMatch(/color:\s*#(?:444|555)\b|background:\s*#fff\b/);
  });
});
