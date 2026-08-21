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
      blockedSinceMs: next.admission_blocked_since_ms ?? next.observed_at_ms,
      fingerprint,
    };
  }

  if (previousClock.fingerprint !== fingerprint) {
    return { blockedSinceMs: observedAtMs, fingerprint };
  }

  return {
    blockedSinceMs: previousClock.blockedSinceMs > 0
      ? previousClock.blockedSinceMs
      : next.admission_blocked_since_ms ?? next.observed_at_ms,
    fingerprint,
  };
}
