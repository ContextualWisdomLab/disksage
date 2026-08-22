export function icloudBlockedSinceMs(
  admissionBlockedSinceMs: number | null | undefined,
  backendObservedAtMs: number,
): number {
  return typeof admissionBlockedSinceMs === "number"
    ? admissionBlockedSinceMs
    : backendObservedAtMs;
}
