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

    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("record_error");
    expect(source).not.toContain("저장하지 못했습니다: {");
    expect(source).toContain("Homebrew 정리 계획을 만들지 못했습니다.");
    expect(source).toContain("Homebrew 정리를 실행하지 못했습니다.");
    expect(source).toContain('role="alert"');
  });

  it("preserves the existing judgment and execution authority calls", () => {
    const source = readSource("src/lib/BrewCleanup.svelte");

    expect(source).toContain("api.judgeBrewCleanup()");
    expect(source).toContain("api.executeBrewCleanup(");
    expect(source).toContain("submittedJudgment.plan_fingerprint");
    expect(source).toContain("submittedJudgment.judgment_id");
  });
});
