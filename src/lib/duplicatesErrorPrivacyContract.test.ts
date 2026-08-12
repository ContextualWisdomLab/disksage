import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Duplicates privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown backend exception text", () => {
    const source = readSource("src/lib/Duplicates.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).toContain("중복 파일 검색에 실패했습니다.");
    expect(source).toContain("선택한 중복 파일을 휴지통으로 보내지 못했습니다.");
  });

  it("announces operation failures without changing focus", () => {
    const source = readSource("src/lib/Duplicates.svelte");

    expect(source).toContain('role="alert"');
  });
});
