export type CloudArchiveErrorOperation =
  | "initialize"
  | "preview"
  | "review"
  | "copy"
  | "cancel"
  | "provider-api-copy"
  | "adopt"
  | "attest"
  | "reconcile"
  | "icloud-health"
  | "finder-copy-cancel"
  | "provider-sync"
  | "provider-recovery"
  | "evict"
  | "capacity"
  | "connect"
  | "disconnect";

const CLOUD_COPY_CANCELLED = "cloud-copy-cancelled";
const CLOUD_COPY_NOT_ACTIVE = "cloud-copy-not-active";

const CLOUD_ARCHIVE_ERROR_MESSAGES: Record<CloudArchiveErrorOperation, string> = {
  initialize: "클라우드 상태를 불러오지 못했습니다. 연결 상태를 확인한 뒤 새로고침하십시오.",
  preview: "클라우드 정리 계획을 만들지 못했습니다. 경로와 용량을 확인한 뒤 다시 시도하십시오.",
  review: "파일 정보 검토 결과를 저장하지 못했습니다. 입력 내용을 확인한 뒤 다시 시도하십시오.",
  copy: "클라우드 복사를 실행하지 못했습니다. 연결 상태와 대상 위치를 확인한 뒤 다시 시도하십시오.",
  cancel: "진행 중인 클라우드 복사를 취소하지 못했습니다. 잠시 기다린 뒤 상태를 새로고침하십시오.",
  "provider-api-copy": "연결된 클라우드 서비스로 업로드하지 못했습니다. 연결 상태와 권한을 확인한 뒤 다시 시도하십시오.",
  adopt: "기존 클라우드 복사본을 확인하지 못했습니다. 대상 위치와 전체 파일을 확인한 뒤 다시 시도하십시오.",
  attest: "클라우드 복사본을 확인하지 못했습니다. 업로드가 끝난 뒤 다시 확인하십시오.",
  reconcile: "저장된 클라우드 작업 결과를 다시 확인하지 못했습니다. 연결 상태를 확인한 뒤 다시 시도하십시오.",
  "icloud-health": "iCloud 동기화 상태를 확인하지 못했습니다. iCloud 연결을 확인한 뒤 다시 시도하십시오.",
  "finder-copy-cancel": "Finder 복사 취소 요청을 완료하지 못했습니다. Finder 상태를 확인한 뒤 다시 시도하십시오.",
  "provider-sync": "클라우드 전체 동기화 상태를 확인하지 못했습니다. 클라우드 앱을 확인한 뒤 다시 시도하십시오.",
  "provider-recovery": "클라우드 앱 복구를 완료하지 못했습니다. 클라우드 앱을 열어 상태를 확인한 뒤 다시 시도하십시오.",
  evict: "검증된 로컬 원본을 휴지통으로 이동하지 못했습니다. 파일 사용 여부를 확인한 뒤 다시 시도하십시오.",
  capacity: "클라우드 저장 공간을 확인하지 못했습니다. 연결 상태를 확인한 뒤 다시 시도하십시오.",
  connect: "클라우드 연결을 완료하지 못했습니다. 앱 권한을 확인한 뒤 다시 연결하십시오.",
  disconnect: "클라우드 연결을 해제하지 못했습니다. 연결 상태를 확인한 뒤 다시 시도하십시오.",
};

function caughtErrorMessage(caughtError: unknown): string | null {
  if (typeof caughtError === "string") return caughtError;
  if (typeof caughtError !== "object" || caughtError === null) return null;

  // Treat caught values as untrusted. `instanceof` and direct property access can both execute
  // Proxy/accessor code from a thrown value. Accept only an own data-property string; accessor and
  // Proxy traps fail closed without escaping this bounded UI error boundary.
  try {
    const descriptor = Object.getOwnPropertyDescriptor(caughtError, "message");
    return descriptor && "value" in descriptor && typeof descriptor.value === "string"
      ? descriptor.value
      : null;
  } catch {
    return null;
  }
}

/**
 * Return a stable user-facing failure message without projecting arbitrary backend details.
 * A late cancel can race with successful completion; once the backend reports no active native
 * operation, the cancellation request is an idempotent no-op and must not surface as a failure.
 */
export function boundedCloudArchiveErrorMessage(
  operation: CloudArchiveErrorOperation,
  caughtError: unknown,
): string {
  if (operation === "cancel" && caughtErrorMessage(caughtError) === CLOUD_COPY_NOT_ACTIVE) {
    return "";
  }
  return CLOUD_ARCHIVE_ERROR_MESSAGES[operation];
}

/** Return true only for the backend's deliberate user-cancellation outcome. */
export function isCloudCopyCancelled(caughtError: unknown): boolean {
  const message = caughtErrorMessage(caughtError);
  return (
    message === CLOUD_COPY_CANCELLED
    || message?.startsWith(`${CLOUD_COPY_CANCELLED};`) === true
  );
}
