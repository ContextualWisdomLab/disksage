import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("CloudArchive iCloud admission contract", () => {
  it("clears stale health evidence when refresh fails", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");
    expect(source).toContain("icloudHealth = null;");
    expect(source).toContain("icloudHealth?.new_copy_admission_state !== \"clear\"");
    expect(source).toContain("managed_database_allocated_bytes");
    expect(source).toContain("시스템 관리 데이터를 삭제하지 않습니다");
    expect(source).toContain("icloud-item-error-octagon-not-signed-in");
    expect(source).toContain("동기화 진단:");
    expect(source).toContain("no_progress_create_count");
    expect(source).toContain("Finder에 남은 복사 대기는 취소");
    expect(source).toContain("File Provider 상태 확인이 제한시간을 넘었습니다");
    expect(source).toContain("Lineage 연결관계");
    expect(source).toContain("검증 복사 영수증 → provider attestation → Goal/ADR");
    expect(source).toContain("candidate.metadata_fingerprint");
    expect(source).toContain("마지막 증거 확인:");
    expect(source).toContain("evidenceObservedAt(icloudHealth.observed_at_ms)");
    expect(source).toContain("ICLOUD_HEALTH_BLOCKED_RETRY_INTERVAL_MS");
    expect(source).toContain("icloudHealthNextCheckAt");
    expect(source).toContain("refreshIcloudHealth(true)");
    expect(source).toContain("providerGlobalSyncObservedAtMs = Date.now();");
    expect(source).toContain("마지막 관찰 {evidenceObservedAt(providerGlobalSyncObservedAtMs)}");
    expect(source).toContain("1분 후 자동 재확인");
    expect(source).toContain("접근 불가·진단만 가능");
    expect(source).toContain("공급자 전역 상태 진단과 고정된 데스크톱 클라이언트 복구만 허용");
    expect(source).toContain("!selectedRootDetails()?.readable");
  });
});
