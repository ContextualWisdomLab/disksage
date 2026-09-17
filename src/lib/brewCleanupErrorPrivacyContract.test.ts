import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("BrewCleanup privacy-safe failure feedback", () => {
  it("never renders arbitrary backend exception text", () => {
    const source = readSource("src/lib/BrewCleanup.svelte");
    const evictionSource = readSource("src/lib/IcloudLocalEviction.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("record_error");
    expect(source).not.toContain("저장하지 못했습니다: {");
    expect(source).not.toMatch(/\{execution\.record_error\}/);
    expect(evictionSource).not.toContain("String(e)");
    expect(evictionSource).not.toContain("result_record_error");
    expect(evictionSource).not.toMatch(/\{eviction\.result_record_error\}/);
    expect(evictionSource).toContain("파일 선택 창을 열지 못했습니다.");
    expect(evictionSource).toContain("iCloud 로컬 사본 상태를 확인하지 못했습니다.");
    expect(evictionSource).toContain("iCloud 로컬 사본 축출을 실행하지 못했습니다.");
    expect(source).toContain("Homebrew 정리 계획을 만들지 못했습니다.");
    expect(source).toContain("Homebrew 정리를 실행하지 못했습니다.");
    expect(source).toContain('role=\"alert\"');
  });

  it("preserves the existing judgment and execution authority calls", () => {
    const source = readSource("src/lib/BrewCleanup.svelte");

    expect(source).toContain("api.judgeBrewCleanup()");
    expect(source).toContain("api.executeBrewCleanup(");
    expect(source).toContain("submittedJudgment.plan_fingerprint");
    expect(source).toContain("submittedJudgment.judgment_id");
  });
});
