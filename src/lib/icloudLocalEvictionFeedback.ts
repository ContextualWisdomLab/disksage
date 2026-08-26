/** Customer guidance shown when the native file picker cannot return an iCloud file. */
export const ICLOUD_FILE_SELECTION_FAILURE =
  "iCloud 파일을 선택하지 못했습니다. Finder와 파일 접근 권한을 확인한 뒤 다시 선택하세요.";

/** Customer guidance shown when DiskSage cannot build a fresh local-copy plan. */
export const ICLOUD_STATE_INSPECTION_FAILURE =
  "iCloud 상태를 확인하지 못했습니다. 파일이 iCloud Drive에 있고 접근 가능한지 확인한 뒤 다시 판정하세요.";

/** Customer guidance shown when the approved local-copy eviction cannot complete. */
export const ICLOUD_EVICTION_EXECUTION_FAILURE =
  "로컬 사본 축출에 실패했습니다. iCloud 동기화가 완료됐는지 확인한 뒤 새 판정부터 다시 진행하세요.";

/**
 * Customer guidance shown when the operation result exists but its immutable record did not persist.
 * The native error is deliberately not reflected because it can contain implementation detail.
 */
export const ICLOUD_RESULT_RECORD_FAILURE =
  "축출 결과는 위와 같지만 기록을 저장하지 못했습니다. DiskSage 데이터 폴더의 권한과 여유 공간을 확인하고 이 화면의 결과를 별도로 보관하세요.";

const UNKNOWN_PLAN_BLOCKER_ACTION =
  "파일의 iCloud 상태를 다시 확인한 뒤 새 판정을 시작하세요.";
const UNKNOWN_VERIFICATION_BLOCKER_ACTION =
  "Finder와 iCloud.com에서 파일 상태를 확인하고, 확인 전에는 작업을 반복하지 마세요.";

const PLAN_BLOCKER_ACTIONS: Readonly<Record<string, string>> = {
  "icloud-local-copy-not-allocated":
    "이미 로컬 사본이 없을 수 있습니다. Finder에서 다운로드 상태를 확인하세요.",
  "icloud-item-not-ubiquitous": "iCloud Drive 안의 파일을 다시 선택하세요.",
  "icloud-file-provider-native-status-unavailable":
    "iCloud 상태 확인이 끝나지 않았습니다. 잠시 후 다시 확인하세요.",
  "icloud-upload-not-confirmed":
    "iCloud 업로드가 완료될 때까지 기다린 뒤 다시 판정하세요.",
  "icloud-upload-still-running":
    "iCloud 업로드가 완료될 때까지 기다린 뒤 다시 판정하세요.",
  "icloud-download-running": "현재 다운로드가 끝난 뒤 다시 판정하세요.",
  "icloud-current-version-unconfirmed":
    "Finder에서 최신 버전 동기화를 확인한 뒤 다시 판정하세요.",
  "icloud-unresolved-conflict": "Finder에서 파일 충돌을 해결한 뒤 다시 판정하세요.",
  "icloud-item-excluded-from-sync":
    "iCloud 동기화 제외 설정을 해제한 뒤 다시 판정하세요.",
  "icloud-file-provider-sync-paused-or-unconfirmed":
    "iCloud 동기화를 재개하고 정상 상태를 확인한 뒤 다시 판정하세요.",
  "icloud-file-provider-item-trashed-or-unconfirmed":
    "최근 삭제된 항목 여부를 확인하고 정상 위치로 복원한 뒤 다시 판정하세요.",
  "icloud-file-provider-eviction-capability-unconfirmed":
    "Finder에서 ‘다운로드 제거’가 가능한 항목인지 확인한 뒤 다시 판정하세요.",
  "icloud-file-provider-document-size-mismatch":
    "파일 크기 동기화가 끝날 때까지 기다린 뒤 다시 판정하세요.",
  "icloud-file-provider-item-identity-unconfirmed":
    "Finder에서 파일 동기화를 완료한 뒤 다시 판정하세요.",
  "active-use-evidence-incomplete": "파일을 사용하는 앱을 모두 닫고 다시 판정하세요.",
  "active-file-use-detected": "파일을 사용하는 앱을 모두 닫고 다시 판정하세요.",
  "human-local-eviction-approval-required":
    "표시된 상태를 확인한 뒤 계획 지문과 사유로 최종 승인하세요.",
};

const VERIFICATION_BLOCKER_ACTIONS: Readonly<Record<string, string>> = {
  "icloud-cloud-item-path-not-retained":
    "Finder와 iCloud.com에서 원본 항목을 확인하고, 확인 전에는 작업을 반복하지 마세요.",
  "icloud-ubiquitous-identity-not-retained":
    "Finder와 iCloud.com에서 원본 항목을 확인하고, 확인 전에는 작업을 반복하지 마세요.",
  "local-allocation-reduction-unverified":
    "Finder의 다운로드 상태와 macOS 저장 공간을 확인하고, 확인 전에는 같은 작업을 반복하지 마세요.",
};

function uniqueActions(
  codes: readonly string[],
  knownActions: Readonly<Record<string, string>>,
  unknownAction: string,
): string[] {
  if (codes.length === 0) return [unknownAction];

  const seen = new Set<string>();
  const actions: string[] = [];

  for (const code of codes) {
    const action = Object.prototype.hasOwnProperty.call(knownActions, code)
      ? knownActions[code]
      : unknownAction;
    if (!seen.has(action)) {
      seen.add(action);
      actions.push(action);
    }
  }

  return actions;
}

/**
 * Convert native plan blocker codes into deduplicated, bounded customer actions.
 * Unknown or missing values are not reflected into the interface and still produce guidance.
 */
export function planBlockerActions(codes: readonly string[]): string[] {
  return uniqueActions(codes, PLAN_BLOCKER_ACTIONS, UNKNOWN_PLAN_BLOCKER_ACTION);
}

/**
 * Convert post-operation verification blocker codes into bounded stop-and-check actions.
 * Unknown or missing values are not reflected into the interface and still produce guidance.
 */
export function verificationBlockerActions(codes: readonly string[]): string[] {
  return uniqueActions(
    codes,
    VERIFICATION_BLOCKER_ACTIONS,
    UNKNOWN_VERIFICATION_BLOCKER_ACTION,
  );
}
