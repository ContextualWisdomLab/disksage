export type CloudArchiveErrorOperation =
  | "initialize"
  | "preview"
  | "review"
  | "copy"
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

const CLOUD_ARCHIVE_ERROR_MESSAGES: Record<CloudArchiveErrorOperation, string> = {
  initialize: "클라우드 상태를 불러오지 못했습니다.",
  preview: "클라우드 오프로드 계획을 만들지 못했습니다.",
  review: "클라우드 후보 검토 결정을 저장하지 못했습니다.",
  copy: "클라우드 복사를 실행하지 못했습니다.",
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

/**
 * Return a stable user-facing failure message without projecting arbitrary backend details.
 * The caught value is accepted only so callers cannot accidentally stringify it while handling
 * an operation-specific failure; diagnostics remain on trusted backend/audit surfaces.
 */
export function boundedCloudArchiveErrorMessage(
  operation: CloudArchiveErrorOperation,
  _caughtError: unknown,
): string {
  return CLOUD_ARCHIVE_ERROR_MESSAGES[operation];
}
