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
