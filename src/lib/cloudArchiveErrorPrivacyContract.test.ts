import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("CloudArchive privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown provider or filesystem exception text", () => {
    const source = readSource("src/lib/CloudArchive.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).toContain("클라우드 정보를 불러오지 못했습니다.");
    expect(source).toContain("클라우드 오프로드 후보를 분석하지 못했습니다.");
    expect(source).toContain("클라우드 후보 검토 결정을 저장하지 못했습니다.");
    expect(source).toContain("클라우드 후보를 복사하지 못했습니다.");
    expect(source).toContain("기존 클라우드 복사본을 채택하지 못했습니다.");
    expect(source).toContain("클라우드 복사 증거를 검증하지 못했습니다.");
    expect(source).toContain("검증된 원본을 휴지통으로 보내지 못했습니다.");
    expect(source).toContain("클라우드 원격 용량을 검증하지 못했습니다.");
    expect(source).toContain("클라우드 공급자 연결을 완료하지 못했습니다.");
    expect(source).toContain("클라우드 공급자 연결을 해제하지 못했습니다.");
  });

  it("preserves the existing cloud authority operations while announcing failures", () => {
    const source = readSource("src/lib/CloudArchive.svelte");

    expect(source).toContain("api.planCloudArchive(");
    expect(source).toContain("api.reviewCloudCandidate(");
    expect(source).toContain("api.copyCloudCandidate(");
    expect(source).toContain("api.adoptExistingCloudCandidate(");
    expect(source).toContain("api.attestCloudCopy(");
    expect(source).toContain("api.trashVerifiedCloudSource(");
    expect(source).toContain("api.verifyCloudProviderCapacity(");
    expect(source).toContain("api.connectCloudProvider(");
    expect(source).toContain("api.disconnectCloudProvider(");
    expect(source).toContain('role="alert"');
  });
});
