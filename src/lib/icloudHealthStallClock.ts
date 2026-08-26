import type { IcloudSyncHealthReport } from "./api";

export interface IcloudHealthStallClock {
  blockedSinceMs: number;
  fingerprint: string;
}

function admissionFingerprint(report: IcloudSyncHealthReport): string {
  return [
    report.new_copy_admission_state,
    report.new_copy_admission_blockers.join(","),
  ].join("|");
}

function progressFingerprint(report: IcloudSyncHealthReport): string {
  const activity = report.file_provider_activity;
  return [
    activity?.pending_indexable_count ?? "",
    activity?.active_upload_count ?? 0,
    activity?.active_download_count ?? 0,
    activity?.active_upload_progress_millionths ?? "",
    activity?.active_download_progress_millionths ?? "",
  ].join("|");
}

function transferProgressFingerprint(report: IcloudSyncHealthReport): readonly [number | null, number | null] {
  const activity = report.file_provider_activity;
  return [
    activity?.active_upload_progress_millionths ?? null,
    activity?.active_download_progress_millionths ?? null,
  ];
}

function indexingBacklogDrained(
  previousReport: IcloudSyncHealthReport,
  next: IcloudSyncHealthReport,
): boolean {
  const previous = previousReport.file_provider_activity?.pending_indexable_count;
  const current = next.file_provider_activity?.pending_indexable_count;
  return previous != null && current != null && current < previous;
}

function hasRealProgress(
  previousReport: IcloudSyncHealthReport,
  next: IcloudSyncHealthReport,
): boolean {
  const [previousUpload, previousDownload] = transferProgressFingerprint(previousReport);
  const [nextUpload, nextDownload] = transferProgressFingerprint(next);
  const transferProgressed = (previous: number | null, current: number | null): boolean =>
    previous != null && current != null && current > previous;
  return transferProgressed(previousUpload, nextUpload)
    || transferProgressed(previousDownload, nextDownload)
    || indexingBacklogDrained(previousReport, next);
}

export function icloudHealthStallClockFingerprint(report: IcloudSyncHealthReport): string {
  return [admissionFingerprint(report), progressFingerprint(report)].join("|");
}

export function updateIcloudHealthStallClock(
  previousReport: IcloudSyncHealthReport | null,
  previousClock: IcloudHealthStallClock,
  next: IcloudSyncHealthReport,
  observedAtMs: number,
): IcloudHealthStallClock {
  const fingerprint = icloudHealthStallClockFingerprint(next);
  const admissionClear = next.new_copy_admission_state === "clear"
    && next.new_copy_admission_blockers.length === 0;
  if (admissionClear) return { blockedSinceMs: 0, fingerprint: "" };

  const admissionChanged = !previousReport
    || admissionFingerprint(previousReport) !== admissionFingerprint(next);
  if (admissionChanged) {
    return {
      blockedSinceMs: next.admission_blocked_since_ms
        ?? (next.observed_at_ms > 0 ? next.observed_at_ms : observedAtMs),
      fingerprint,
    };
  }

  const newlySuppliedBlockedSinceMs = next.admission_blocked_since_ms;
  if (
    newlySuppliedBlockedSinceMs != null
    && newlySuppliedBlockedSinceMs > 0
    && newlySuppliedBlockedSinceMs !== previousReport.admission_blocked_since_ms
  ) {
    return { blockedSinceMs: newlySuppliedBlockedSinceMs, fingerprint };
  }

  if (previousClock.fingerprint !== fingerprint && hasRealProgress(previousReport, next)) {
    return {
      blockedSinceMs: next.observed_at_ms > 0 ? next.observed_at_ms : observedAtMs,
      fingerprint,
    };
  }

  return {
    blockedSinceMs: previousClock.blockedSinceMs > 0
      ? previousClock.blockedSinceMs
      : next.admission_blocked_since_ms
        ?? (next.observed_at_ms > 0 ? next.observed_at_ms : observedAtMs),
    fingerprint,
  };
}
