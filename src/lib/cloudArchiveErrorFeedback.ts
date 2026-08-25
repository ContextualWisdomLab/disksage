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
  initialize: "클라우드 상태를 불러오지 못했습니다.",
  preview: "클라우드 오프로드 계획을 만들지 못했습니다.",
  review: "클라우드 후보 검토 결정을 저장하지 못했습니다.",
  copy: "클라우드 복사를 실행하지 못했습니다.",
  cancel: "진행 중인 클라우드 복사를 취소하지 못했습니다.",
  "provider-api-copy": "공급자 API 업로드를 실행하지 못했습니다.",
  adopt: "기존 클라우드 복사본을 검증·채택하지 못했습니다.",
  attest: "클라우드 복사본의 공급자 증거를 확인하지 못했습니다.",
  reconcile: "기존 클라우드 영수증을 재검증하지 못했습니다.",
  "icloud-health": "iCloud 동기화 상태를 확인하지 못했습니다.",
  "finder-copy-cancel": "Finder 복사 취소 요청을 완료하지 못했습니다.",
  "provider-sync": "공급자 전역 동기화 상태를 확인하지 못했습니다.",
  "provider-recovery": "공급자 앱 복구를 완료하지 못했습니다.",
  evict: "검증된 로컬 원본을 휴지통으로 이동하지 못했습니다.",
  capacity: "클라우드 계정 용량을 확인하지 못했습니다.",
  connect: "클라우드 공급자 OAuth 연결을 완료하지 못했습니다.",
  disconnect: "클라우드 공급자 연결을 해제하지 못했습니다.",
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
