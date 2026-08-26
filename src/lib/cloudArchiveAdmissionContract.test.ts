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
    expect(source).toContain("providerProgressPercent");
    expect(source).toContain("active_upload_progress_millionths");
    expect(source).toContain("Finder가 “복사 준비 중”에서 멈춘 동안 File Provider의 no-progress 요청이 함께 관찰되었습니다.");
    expect(source).not.toContain("Finder가 “복사 준비 중”에서 멈춘 원인은");
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
    expect(source).toContain("refreshProviderGlobalSync(true)");
    expect(source).toContain("const observedAtMs = Date.now();");
    expect(source).toContain("providerGlobalSyncBlockedSinceMs");
    expect(source).toContain("PROVIDER_GLOBAL_SYNC_BLOCKED_RETRY_INTERVAL_MS");
    expect(source).toContain("providerGlobalSyncNextCheckAt");
    expect(source).toContain("checkingProviderGlobalSync || (!force && Date.now() < providerGlobalSyncNextCheckAt)");
    expect(source).toContain("next.pending_indexable_count !== null && next.pending_indexable_count > 0");
    expect(source).toContain("provider-global-sync-item-not-found");
    expect(source).toContain("icloud-file-provider-item-locked");
    expect(source).toContain("File Provider 항목이 전파 잠금 상태임");
    expect(source).toContain("File Provider 항목의 전파 잠금 상태가 Finder 복사 준비 지연과 함께 관찰되었습니다.");
    expect(source).toContain("File Provider 큐에서 15분 이상 묵은 fetch/create 오류가 관찰되었습니다.");
    expect(source).toContain("icloud-file-provider-stalled");
    expect(source).not.toContain("Finder의 복사 준비가 진행되지 않습니다.");
    expect(source).toContain("동일 차단 지속");
    expect(source).toContain("동일한 공급자 차단 상태가 15분 이상 지속되었습니다.");
    expect(source).toContain("공급자 전역 증거를 확인하지 못했습니다.");
    expect(source).toContain("마지막 관찰 {evidenceObservedAt(providerGlobalSyncObservedAtMs)}");
    expect(source).toContain('providerGlobalSync.blockers.length === 0 ? "1분" : "5분"');
    expect(source).toContain("후 자동 재확인");
    expect(source).toContain("접근 불가·진단만 가능");
    expect(source).toContain("공급자 전역 상태 진단과 고정된 데스크톱 클라이언트 복구만 허용");
    expect(source).toContain("!selectedRootDetails()?.readable");
    expect(source).toContain("async function cancelFinderCopy()");
    expect(source).toContain("await api.cancelFinderCopy();");
    expect(source).toContain("cancellingFinderCopy || checkingIcloudHealth");
    expect(source).toContain("canCancelFinderCopyForProviderGlobalSync");
    expect(source).toContain("provider-global-sync-reconciliation-pending");
    expect(source).toContain("provider-global-sync-local-disk-full");
    expect(source).toContain("provider-global-sync-item-not-found");
    expect(source).toContain("cancellingFinderCopy || checkingProviderGlobalSync");
    expect(source).toContain("finderCopyCancelStatus = \"Finder 복사 취소 요청을 보냈습니다. 상태를 다시 확인하십시오.\"");
  });

  it("exposes cancellation only for the cancellable native copy path", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");
    const copyStart = source.indexOf("async function copyCandidate(candidate: api.CloudCandidate)");
    const cancelStart = source.indexOf("async function cancelCopy()", copyStart);
    const providerApiStart = source.indexOf("async function copyCandidateViaProviderApi", cancelStart);
    const adoptStart = source.indexOf("async function adoptExistingCandidate", providerApiStart);

    expect(copyStart).toBeGreaterThanOrEqual(0);
    expect(cancelStart).toBeGreaterThan(copyStart);
    expect(providerApiStart).toBeGreaterThan(cancelStart);
    expect(adoptStart).toBeGreaterThan(providerApiStart);

    const copyBody = source.slice(copyStart, cancelStart);
    const providerApiBody = source.slice(providerApiStart, adoptStart);
    const adoptBody = source.slice(adoptStart, source.indexOf("async function", adoptStart + 1));
    const cancelBody = source.slice(cancelStart, providerApiStart);

    expect(copyBody).toMatch(
      /copyingFingerprint = candidate\.metadata_fingerprint;\s*(?:\/\/[^\n]*\n\s*)?nativeCopyActive = true;/,
    );
    expect(providerApiBody).toMatch(
      /copyingFingerprint = candidate\.metadata_fingerprint;\s*(?:\/\/[^\n]*\n\s*)?nativeCopyActive = false;/,
    );
    expect(adoptBody).toMatch(
      /copyingFingerprint = candidate\.metadata_fingerprint;\s*(?:\/\/[^\n]*\n\s*)?nativeCopyActive = false;/,
    );
    expect(cancelBody).toContain("if (!nativeCopyActive || !copyingFingerprint || cancellingCopy) return;");
    expect(cancelBody).toContain("await api.cancelCloudCopy(copyingFingerprint);");
  });

  it("keeps native-copy cancellation reachable while copy eligibility or preview state changes", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");
    const markupStart = source.indexOf("</script>");
    const reportStart = source.indexOf("{#if report}", markupStart);
    const cancelControl = source.indexOf('aria-label="진행 중인 DiskSage 클라우드 복사 취소"', markupStart);

    expect(markupStart).toBeGreaterThanOrEqual(0);
    expect(reportStart).toBeGreaterThan(markupStart);
    expect(cancelControl).toBeGreaterThan(markupStart);
    expect(cancelControl).toBeLessThan(reportStart);
    expect(source).toContain("if (!scannedRoot || !selectedRoot || nativeCopyActive) return;");
    expect(source).toContain("disabled={busy || nativeCopyActive}");
    expect(source).toContain("disabled={busy || nativeCopyActive || !scannedRoot || !selectedRoot || !selectedRootDetails()?.readable}");
  });

  it("does not run the heavy iCloud probe for non-iCloud selected roots", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");
    const refreshStart = source.indexOf("async function refreshIcloudHealth(force = false)");
    const providerGuard = source.indexOf('if (!root || root.provider !== "icloud")', refreshStart);
    const probeCall = source.indexOf("const next = await api.inspectIcloudNewCopyAdmission();", refreshStart);

    expect(refreshStart).toBeGreaterThanOrEqual(0);
    expect(providerGuard).toBeGreaterThan(refreshStart);
    expect(probeCall).toBeGreaterThan(providerGuard);
  });

  it("defaults provider OAuth consent to read-only until write access is explicitly selected", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");
    expect(source).toContain("let oauthWriteAccess = $state(false);");
    expect(source).not.toContain("let oauthWriteAccess = $state(true);");
    expect(source).toContain("bind:checked={oauthWriteAccess}");
    expect(source).toContain("oauthWriteAccess,");
  });
});
