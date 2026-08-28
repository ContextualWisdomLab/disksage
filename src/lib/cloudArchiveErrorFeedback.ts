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
  initialize: "클라우드 상태를 불러오지 못했습니다. 다시 시도하세요.",
  preview: "클라우드 정리 계획을 만들지 못했습니다. 조건을 확인한 뒤 다시 시도하세요.",
  review: "파일 확인 결과를 저장하지 못했습니다. 다시 시도하세요.",
  copy: "클라우드 복사를 실행하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.",
  cancel: "진행 중인 클라우드 복사를 취소하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.",
  "provider-api-copy": "클라우드 업로드를 시작하지 못했습니다. 연결 상태를 확인한 뒤 다시 시도하세요.",
  adopt: "기존 클라우드 파일을 확인하지 못했습니다. 다시 시도하세요.",
  attest: "클라우드 파일 확인을 완료하지 못했습니다. 다시 시도하세요.",
  reconcile: "이전 작업 상태를 확인하지 못했습니다. 다시 시도하세요.",
  "icloud-health": "iCloud 동기화 상태를 확인하지 못했습니다. 다시 확인하세요.",
  "finder-copy-cancel": "Finder 복사 취소 요청을 완료하지 못했습니다. 다시 시도하세요.",
  "provider-sync": "클라우드 전체 동기화 상태를 확인하지 못했습니다. 다시 시도하세요.",
  "provider-recovery": "클라우드 앱을 다시 시작하지 못했습니다. 직접 다시 시작한 뒤 확인하세요.",
  evict: "로컬 사본을 회수하지 못했습니다. iCloud 상태를 확인한 뒤 다시 시도하세요.",
  capacity: "클라우드 저장 공간을 확인하지 못했습니다. 다시 시도하세요.",
  connect: "클라우드 연결을 완료하지 못했습니다. 연결 정보를 확인한 뒤 다시 시도하세요.",
  disconnect: "클라우드 연결을 해제하지 못했습니다. 다시 시도하세요.",
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
