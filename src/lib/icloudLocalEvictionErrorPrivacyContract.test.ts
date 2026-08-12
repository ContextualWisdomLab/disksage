import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("iCloud local eviction privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown exception text", () => {
    const source = readSource("src/lib/IcloudLocalEviction.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).toContain("iCloud 파일 선택에 실패했습니다.");
    expect(source).toContain("iCloud 로컬 사본 상태를 확인하지 못했습니다.");
    expect(source).toContain("iCloud 로컬 사본 축출에 실패했습니다.");
  });

  it("announces bounded failures without changing the operation authority", () => {
    const source = readSource("src/lib/IcloudLocalEviction.svelte");

    expect(source).toContain('role="alert"');
    expect(source).toContain("api.planIcloudLocalCopyEviction(cloudRoot, selectedPath)");
    expect(source).toContain("api.evictIcloudLocalCopy(");
  });
});
