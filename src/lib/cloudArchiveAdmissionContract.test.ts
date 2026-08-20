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
    expect(source).toContain("ICLOUD_HEALTH_BLOCKED_RETRY_INTERVAL_MS");
    expect(source).toContain("icloudHealthNextCheckAt");
  });
});
