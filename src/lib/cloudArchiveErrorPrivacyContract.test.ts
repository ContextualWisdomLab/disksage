import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

const approvedFailureMessages = [
  "클라우드 정보를 불러오지 못했습니다. 공급자 연결과 선택한 클라우드 루트의 접근 권한을 확인한 뒤 다시 열어 보세요.",
  "클라우드 오프로드 후보를 분석하지 못했습니다. 스캔 결과와 선택한 클라우드 루트를 확인한 뒤 다시 분석하세요.",
  "클라우드 후보 검토 결정을 저장하지 못했습니다. 사유와 조직 테넌트 권한 확인을 점검한 뒤 다시 저장하세요.",
  "클라우드 후보를 복사하지 못했습니다. 원격 용량과 공급자 연결을 확인한 뒤 새 계획부터 다시 진행하세요.",
  "기존 클라우드 복사본을 채택하지 못했습니다. 대상 객체와 승인 문구를 다시 확인한 뒤 새 계획부터 진행하세요.",
  "클라우드 복사 증거를 검증하지 못했습니다. 공급자에서 복사본을 확인한 뒤 다시 검증하세요.",
  "검증된 원본을 휴지통으로 보내지 못했습니다. 원본과 증거 상태를 다시 확인한 뒤 새 검증부터 진행하세요.",
  "클라우드 원격 용량을 검증하지 못했습니다. 공급자 연결을 확인한 뒤 용량 확인을 다시 실행하세요.",
  "클라우드 공급자 연결을 완료하지 못했습니다. OAuth 클라이언트 ID와 공급자 동의 화면을 확인한 뒤 다시 연결하세요.",
  "클라우드 공급자 연결을 해제하지 못했습니다. OS Keychain과 공급자 연결 상태를 확인한 뒤 다시 해제하세요.",
] as const;

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("CloudArchive privacy-safe failure feedback", () => {
  it("restricts every visible loadError assignment to the approved fixed-string allowlist", () => {
    const source = readSource("src/lib/CloudArchive.svelte");
    const assignments = [...source.matchAll(/\bloadError\s*=\s*([^;\n]+);/g)].map((match) =>
      match[1].trim(),
    );
    const failureAssignments = assignments.filter((assignment) => assignment !== '""');
    const approvedLiterals = approvedFailureMessages.map((message) => JSON.stringify(message));

    expect(source).not.toMatch(/\bcatch\s*\([^)]*\)/);
    expect(assignments.length).toBeGreaterThan(approvedFailureMessages.length);
    expect(failureAssignments.sort()).toEqual([...approvedLiterals].sort());

    for (const message of approvedFailureMessages) {
      expect(source).toContain(message);
    }
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
    expect(source).toMatch(
      /\{#if loadError\}\s*<p(?=[^>]*\bclass="error")(?=[^>]*\brole="alert")[^>]*>\{loadError\}<\/p>\s*\{\/if\}/,
    );
  });
});
