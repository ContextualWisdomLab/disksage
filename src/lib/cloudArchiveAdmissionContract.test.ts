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
    expect(source).toContain("iCloud File Provider 증거를 확인하지 못했습니다.");
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
    expect(source).toContain("icloudHealthBlockedSinceMs");
    expect(source).toContain("icloudHealthFingerprint");
    expect(source).toContain("const admissionClear = next.new_copy_admission_state === \"clear\"");
    expect(source).toContain("동일한 iCloud 차단 상태가 15분 이상 지속되었습니다.");
    expect(source).toContain("refreshIcloudHealth(true)");
    expect(source).toContain("const observedAtMs = Date.now();");
    expect(source).toContain("providerGlobalSyncBlockedSinceMs");
    expect(source).toContain("next.pending_indexable_count !== null && next.pending_indexable_count > 0");
    expect(source).toContain("provider-global-sync-item-not-found");
    expect(source).toContain("동일 차단 지속");
    expect(source).toContain("동일한 공급자 차단 상태가 15분 이상 지속되었습니다.");
    expect(source).toContain("공급자 전역 증거를 확인하지 못했습니다.");
    expect(source).toContain("마지막 관찰 {evidenceObservedAt(providerGlobalSyncObservedAtMs)}");
    expect(source).toContain("1분 후 자동 재확인");
    expect(source).toContain("접근 불가·진단만 가능");
    expect(source).toContain("공급자 전역 상태 진단과 고정된 데스크톱 클라이언트 복구만 허용");
    expect(source).toContain("!selectedRootDetails()?.readable");
    expect(source).toContain("async function cancelFinderCopy()");
    expect(source).toContain("await api.cancelFinderCopy();");
    expect(source).toContain("cancellingFinderCopy || checkingIcloudHealth");
    expect(source).toContain("finderCopyCancelStatus = \"Finder 복사 취소 요청을 보냈습니다. 상태를 다시 확인하십시오.\"");
  });

  it("defaults provider OAuth consent to read-only until write access is explicitly selected", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");
    expect(source).toContain("let oauthWriteAccess = $state(false);");
    expect(source).not.toContain("let oauthWriteAccess = $state(true);");
    expect(source).toContain("bind:checked={oauthWriteAccess}");
    expect(source).toContain("oauthWriteAccess,");
  });
});
