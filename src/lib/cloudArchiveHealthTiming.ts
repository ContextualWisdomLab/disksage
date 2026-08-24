export function blockedSinceMs(
  admissionBlockedSinceMs: number | null | undefined,
  backendObservedAtMs: number,
): number {
  const persistedOnsetIsUsable = typeof admissionBlockedSinceMs === "number"
    && Number.isSafeInteger(admissionBlockedSinceMs)
    && admissionBlockedSinceMs >= 0
    && admissionBlockedSinceMs <= backendObservedAtMs;
  return persistedOnsetIsUsable
    ? admissionBlockedSinceMs
    : backendObservedAtMs;
}

export function icloudBlockedSinceMs(
  admissionBlockedSinceMs: number | null | undefined,
  backendObservedAtMs: number,
): number {
  return blockedSinceMs(admissionBlockedSinceMs, backendObservedAtMs);
}
