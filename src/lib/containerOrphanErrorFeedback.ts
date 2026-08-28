const INSPECT_FAILURE_MESSAGE = "개발 환경 확인 실패 — 잠시 후 다시 시도해 주세요.";
const PRUNE_FAILURE_MESSAGE = "정리 실행 실패 — 데이터는 그대로입니다. 상태를 확인한 뒤 다시 시도해 주세요.";

/**
 * Returns privacy-safe customer feedback for a failed container-runtime inspection.
 * Backend diagnostics can contain local paths, runtime stderr, socket details, or account-local
 * context, so the desktop never reflects the original error across the customer-visible boundary.
 */
export function containerOrphanInspectErrorMessage(_error: unknown): string {
  return INSPECT_FAILURE_MESSAGE;
}

/**
 * Maps only stable, documented prune boundary codes to actionable customer feedback.
 * Unknown backend details remain opaque so local paths, runtime output, and tokens cannot leak.
 */
export function containerOrphanPruneErrorMessage(error: unknown): string {
  const detail = typeof error === "string" ? error : "";
  if (detail.startsWith("orphan-prune-confirmation-mismatch")) {
    return "승인 문구가 최신 목록과 일치하지 않습니다. 새로 확인한 뒤 문구를 다시 입력해 주세요.";
  }
  if (detail.startsWith("orphan-prune-empty-candidate-set")) {
    return "삭제 대상이 사라졌습니다. 다시 확인해 주세요.";
  }
  if (detail.startsWith("orphan-prune-evidence-incomplete")) {
    return "확인이 끝나지 않아 실행이 중단되었습니다. 개발 환경 상태를 확인한 뒤 다시 시도해 주세요.";
  }
  return PRUNE_FAILURE_MESSAGE;
}
