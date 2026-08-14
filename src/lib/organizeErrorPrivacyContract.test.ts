import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Organize privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown backend exception text", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).toContain("정리 계획을 만들지 못했습니다.");
    expect(source).toContain("파일 정리를 실행하지 못했습니다.");
    expect(source).toContain("마지막 이동을 되돌리지 못했습니다.");
  });

  it("keeps failures in the existing alert boundary", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain('role="alert"');
  });
});
