import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ICLOUD_EVICTION_EXECUTION_FAILURE,
  ICLOUD_FILE_SELECTION_FAILURE,
  ICLOUD_RESULT_RECORD_FAILURE,
  ICLOUD_STATE_INSPECTION_FAILURE,
  planBlockerActions,
  verificationBlockerActions,
} from "./icloudLocalEvictionFeedback";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readComponent(): string {
  return readFileSync(resolve(repositoryRoot, "src/lib/IcloudLocalEviction.svelte"), "utf8");
}

describe("iCloud local eviction next-action feedback", () => {
  it("turns every current planning blocker into bounded customer guidance", () => {
    const cases: Array<[string, string]> = [
      [
        "icloud-local-copy-not-allocated",
        "이미 로컬 사본이 없을 수 있습니다. Finder에서 다운로드 상태를 확인하세요.",
      ],
      ["icloud-item-not-ubiquitous", "iCloud Drive 안의 파일을 다시 선택하세요."],
      [
        "icloud-upload-not-confirmed",
        "iCloud 업로드가 완료될 때까지 기다린 뒤 다시 판정하세요.",
      ],
      [
        "icloud-upload-still-running",
        "iCloud 업로드가 완료될 때까지 기다린 뒤 다시 판정하세요.",
      ],
      ["icloud-download-running", "현재 다운로드가 끝난 뒤 다시 판정하세요."],
      [
        "icloud-current-version-unconfirmed",
        "Finder에서 최신 버전 동기화를 확인한 뒤 다시 판정하세요.",
      ],
      ["icloud-unresolved-conflict", "Finder에서 파일 충돌을 해결한 뒤 다시 판정하세요."],
      [
        "icloud-item-excluded-from-sync",
        "iCloud 동기화 제외 설정을 해제한 뒤 다시 판정하세요.",
      ],
      [
        "icloud-file-provider-sync-paused-or-unconfirmed",
        "iCloud 동기화를 재개하고 정상 상태를 확인한 뒤 다시 판정하세요.",
      ],
      [
        "icloud-file-provider-item-trashed-or-unconfirmed",
        "최근 삭제된 항목 여부를 확인하고 정상 위치로 복원한 뒤 다시 판정하세요.",
      ],
      [
        "icloud-file-provider-eviction-capability-unconfirmed",
        "Finder에서 ‘다운로드 제거’가 가능한 항목인지 확인한 뒤 다시 판정하세요.",
      ],
      [
        "icloud-file-provider-document-size-mismatch",
        "파일 크기 동기화가 끝날 때까지 기다린 뒤 다시 판정하세요.",
      ],
      [
        "icloud-file-provider-item-identity-unconfirmed",
        "Finder에서 파일 동기화를 완료한 뒤 다시 판정하세요.",
      ],
      ["active-use-evidence-incomplete", "파일을 사용하는 앱을 모두 닫고 다시 판정하세요."],
      ["active-file-use-detected", "파일을 사용하는 앱을 모두 닫고 다시 판정하세요."],
      [
        "human-local-eviction-approval-required",
        "표시된 상태를 확인한 뒤 계획 지문과 사유로 최종 승인하세요.",
      ],
    ];

    for (const [code, action] of cases) {
      expect(planBlockerActions([code])).toEqual([action]);
    }
  });

  it("turns verification gaps into stop-and-check guidance", () => {
    expect(verificationBlockerActions(["icloud-cloud-item-path-not-retained"])).toEqual([
      "Finder와 iCloud.com에서 원본 항목을 확인하고, 확인 전에는 작업을 반복하지 마세요.",
    ]);
    expect(verificationBlockerActions(["icloud-ubiquitous-identity-not-retained"])).toEqual([
      "Finder와 iCloud.com에서 원본 항목을 확인하고, 확인 전에는 작업을 반복하지 마세요.",
    ]);
    expect(verificationBlockerActions(["local-allocation-reduction-unverified"])).toEqual([
      "Finder의 다운로드 상태와 macOS 저장 공간을 확인하고, 확인 전에는 같은 작업을 반복하지 마세요.",
    ]);
  });

  it("deduplicates equivalent advice and never reflects unknown backend text", () => {
    expect(
      planBlockerActions(["icloud-upload-not-confirmed", "icloud-upload-still-running"]),
    ).toEqual(["iCloud 업로드가 완료될 때까지 기다린 뒤 다시 판정하세요."]);
    expect(
      verificationBlockerActions([
        "icloud-cloud-item-path-not-retained",
        "icloud-ubiquitous-identity-not-retained",
      ]),
    ).toEqual([
      "Finder와 iCloud.com에서 원본 항목을 확인하고, 확인 전에는 작업을 반복하지 마세요.",
    ]);

    const injected = "/Users/example/private-file: provider failed";
    expect(planBlockerActions([injected])).toEqual([
      "파일의 iCloud 상태를 다시 확인한 뒤 새 판정을 시작하세요.",
    ]);
    expect(verificationBlockerActions([injected])).toEqual([
      "Finder와 iCloud.com에서 파일 상태를 확인하고, 확인 전에는 작업을 반복하지 마세요.",
    ]);
    expect(planBlockerActions([injected]).join(" ")).not.toContain(injected);
    expect(verificationBlockerActions([injected]).join(" ")).not.toContain(injected);

    for (const prototypeKey of ["constructor", "toString", "__proto__"]) {
      expect(planBlockerActions([prototypeKey])).toEqual([
        "파일의 iCloud 상태를 다시 확인한 뒤 새 판정을 시작하세요.",
      ]);
      expect(verificationBlockerActions([prototypeKey])).toEqual([
        "Finder와 iCloud.com에서 파일 상태를 확인하고, 확인 전에는 작업을 반복하지 마세요.",
      ]);
    }
  });

  it("never leaves an ineligible or unverified state without a next action", () => {
    expect(planBlockerActions([])).toEqual([
      "파일의 iCloud 상태를 다시 확인한 뒤 새 판정을 시작하세요.",
    ]);
    expect(verificationBlockerActions([])).toEqual([
      "Finder와 iCloud.com에서 파일 상태를 확인하고, 확인 전에는 작업을 반복하지 마세요.",
    ]);
  });

  it("uses action-oriented bounded copy for operation and record failures", () => {
    expect(ICLOUD_FILE_SELECTION_FAILURE).toBe(
      "iCloud 파일을 선택하지 못했습니다. Finder와 파일 접근 권한을 확인한 뒤 다시 선택하세요.",
    );
    expect(ICLOUD_STATE_INSPECTION_FAILURE).toBe(
      "iCloud 상태를 확인하지 못했습니다. 파일이 iCloud Drive에 있고 접근 가능한지 확인한 뒤 다시 판정하세요.",
    );
    expect(ICLOUD_EVICTION_EXECUTION_FAILURE).toBe(
      "로컬 사본 축출에 실패했습니다. iCloud 동기화가 완료됐는지 확인한 뒤 새 판정부터 다시 진행하세요.",
    );
    expect(ICLOUD_RESULT_RECORD_FAILURE).toBe(
      "축출 결과는 위와 같지만 기록을 저장하지 못했습니다. DiskSage 데이터 폴더의 권한과 여유 공간을 확인하고 이 화면의 결과를 별도로 보관하세요.",
    );
  });

  it("projects only bounded guidance in the desktop component", () => {
    const source = readComponent();

    expect(source).toContain("ICLOUD_FILE_SELECTION_FAILURE");
    expect(source).toContain("ICLOUD_STATE_INSPECTION_FAILURE");
    expect(source).toContain("ICLOUD_EVICTION_EXECUTION_FAILURE");
    expect(source).toContain("ICLOUD_RESULT_RECORD_FAILURE");
    expect(source).toContain("planBlockerActions(plan.blockers.filter");
    expect(source).toContain(
      "verificationBlockerActions(eviction.result.verification_blockers)",
    );
    expect(source).not.toContain("{eviction.result_record_error}");
  });
});
