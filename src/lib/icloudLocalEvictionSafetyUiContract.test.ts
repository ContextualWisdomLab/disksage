import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(): string {
  return readFileSync(resolve(repositoryRoot, "src/lib/IcloudLocalEviction.svelte"), "utf8");
}

describe("iCloud local eviction safety UI", () => {
  it("explains local-current plus unconfirmed upload without exposing backend details", () => {
    const source = readSource();

    expect(source).toContain("로컬 최신본·업로드 미확인");
    expect(source).toContain("로컬 최신본이지만 공급자 업로드가 아직 확인되지 않았습니다. 업로드 완료 후 다시 확인하십시오.");
    expect(source).toContain('role="status"');
    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("result_record_error");
  });

  it("keeps customer next actions bounded for each native-state failure path", () => {
    const source = readSource();

    expect(source).toContain("파일 선택 창을 열지 못했습니다.");
    expect(source).toContain("iCloud 로컬 사본 상태를 확인하지 못했습니다.");
    expect(source).toContain("iCloud 로컬 사본 축출을 실행하지 못했습니다.");
    expect(source).toContain("function blockerSummary");
  });

  it("retains provider-aware progress and deduplicated blocker feedback", () => {
    const source = readSource();

    expect(source).toContain("function uploadLabel");
    expect(source).toContain("업로드 중");
    expect(source).toContain("function syncLabel");
    expect(source).toContain("공급자 상태");
    expect(source).toContain("new Set(blockers.map(blockerLabel))");
    expect(source).toContain("blockerSummary(plan.blockers)");
    expect(source).not.toContain("plan.blockers.join(\", \")");
  });
});
