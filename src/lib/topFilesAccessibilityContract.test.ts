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
    expect(source).toContain('<table aria-labelledby="top-files-heading">');
  });

  it("associates both header cells explicitly with their columns", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain('<th scope="col">크기</th>');
    expect(source).toContain('<th scope="col">경로</th>');
  });

  it("puts the overflowing rows in a named sequential keyboard focus target", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain(
      '<div class="table-scroll" tabindex="0" aria-labelledby="top-files-heading">',
    );
    expect(source).toContain(".table-scroll { max-height: 40vh; overflow-y: auto;");
    expect(source).not.toContain("section { max-height: 40vh; overflow-y: auto;");
  });

  it("replaces an empty table with guidance for the next scan", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain("{#if files.length === 0}");
    expect(source).toContain(
      '<p class="empty" role="status">표시할 대용량 파일이 없습니다. 다른 폴더를 스캔하거나 스캔 범위를 넓히세요.</p>',
    );
    expect(source).toContain("{:else}");
  });
});
