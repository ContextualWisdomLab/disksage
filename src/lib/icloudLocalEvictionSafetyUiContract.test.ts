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
    expect(source).toContain("로컬 최신본이지만 iCloud 업로드가 아직 확인되지 않았습니다. 업로드 완료 후 다시 확인하십시오.");
    expect(source).toContain('role="status"');
    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("result_record_error");
  });

  it("keeps customer next actions bounded for each native-state failure path", () => {
    const source = readSource();

    expect(source).toContain("iCloud 파일 선택을 완료하지 못했습니다. 다시 시도하십시오.");
    expect(source).toContain("iCloud 로컬 사본 상태를 확인하지 못했습니다. 다시 시도하십시오.");
    expect(source).toContain("iCloud 로컬 사본을 회수하지 못했습니다. 상태를 다시 확인하십시오.");
    expect(source).toContain("function blockerSummary");
    const visible = source.slice(source.indexOf("</script>"), source.indexOf("<style>"));
    expect(visible).not.toContain("File Provider");
    expect(visible).not.toContain("계획 지문");
    expect(visible).not.toContain("활성 사용");
    expect(visible).not.toContain("승인 기록");
    expect(visible).not.toContain("결과 기록");
  });
});
