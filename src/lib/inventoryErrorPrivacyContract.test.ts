import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Inventory privacy-safe failure feedback", () => {
  it("never renders arbitrary backend exception text", () => {
    const source = readSource("src/lib/Inventory.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).toContain("인벤토리 집계에 실패했습니다.");
    expect(source).toContain("모델 다운로드에 실패했습니다.");
    expect(source).toContain("규칙 파일을 불러오지 못했습니다.");
    expect(source).toContain("미분류 요약에 실패했습니다.");
  });

  it("separates summary failure from summary content and announces failures", () => {
    const source = readSource("src/lib/Inventory.svelte");

    expect(source).toContain("summaryError");
    expect(source).toMatch(/summaryError\s*=\s*""/);
    expect(source).toContain('role="alert"');
  });
});
