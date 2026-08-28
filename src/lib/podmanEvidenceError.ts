/**
 * Convert any untrusted Podman inspection failure into one stable privacy-safe code.
 *
 * Tauri transport failures, operating-system errors, and thrown JavaScript values may contain
 * machine names, account-local paths, socket locations, or command details. The desktop UI must
 * not render those values. Detailed diagnosis remains local to trusted logs and is never copied
 * into the shareable evidence surface.
 *
 * @param reason - Untrusted failure detail intentionally discarded at the UI boundary.
 * @returns A stable, actionable sentence without local paths or command details.
 */
export function podmanEvidenceErrorMessage(_reason: unknown): string {
  return "Podman 저장 공간을 확인하지 못했습니다. 상태를 확인한 뒤 다시 시도하십시오.";
}

const PODMAN_PRUNE_RECOVERY_MESSAGES: Readonly<Record<string, string>> = {
  "podman-prune-confirmation-mismatch":
    "승인 문구가 최신 정리 계획과 일치하지 않습니다. 현재 계획을 다시 확인한 뒤 승인 문구를 다시 입력하십시오.",
  "podman-prune-candidate-set-changed":
    "정리 후보가 변경되었습니다. 최신 Podman 상태를 다시 확인하고 새 계획을 검토하십시오.",
  "podman-prune-machine-not-running":
    "Podman 머신이 실행 중이 아닙니다. 머신 상태를 확인한 뒤 정리 계획을 다시 불러오십시오.",
};

/**
 * Convert a Podman prune failure into bounded recovery guidance without reflecting host detail.
 *
 * Only exact, production-owned failure codes receive specialized guidance. Error messages that
 * contain a stable code plus additional text are deliberately treated as untrusted and collapse
 * to the generic fallback so paths, sockets, command output, and other host-local detail cannot
 * cross the desktop boundary.
 */
export function podmanPruneErrorMessage(reason: unknown): string {
  const code = typeof reason === "string" ? reason : reason instanceof Error ? reason.message : "";
  return (
    PODMAN_PRUNE_RECOVERY_MESSAGES[code] ??
    "Podman 정리를 완료하지 못했습니다. 최신 상태를 다시 확인한 뒤 정리 계획을 재검토하십시오."
  );
}
