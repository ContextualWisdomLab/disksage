import { describe, expect, it } from "vitest";
import type { IcloudSyncHealthReport } from "./api";
import {
  icloudHealthStallClockFingerprint,
  updateIcloudHealthStallClock,
} from "./icloudHealthStallClock";

function report(
  activity: IcloudSyncHealthReport["file_provider_activity"],
  overrides: Partial<IcloudSyncHealthReport> = {},
): IcloudSyncHealthReport {
  return {
    observed_at_ms: 1_000,
    admission_blocked_since_ms: 500,
    evidence_complete: true,
    upload_queue: {
      scheduled_waiting_count: 0,
      scheduled_active_count: 0,
      blocked_on_sync_up_count: 0,
      out_of_quota_count: 0,
      item_error_count: 0,
    },
    file_provider_activity: activity,
    sync_backlog_present: true,
    new_copy_admission_state: "blocked",
    new_copy_admission_blockers: ["icloud-new-copy-admission-blocked"],
    blockers: ["icloud-new-copy-admission-blocked"],
    notices: [],
    local_eviction_authorized: false,
    ...overrides,
  };
}

function activity(overrides: Partial<NonNullable<IcloudSyncHealthReport["file_provider_activity"]>> = {}) {
  return {
    command_succeeded: true,
    timed_out: false,
    output_truncated: false,
    no_progress_fetch_count: 1,
    no_progress_create_count: 0,
    materialization_failure_count: 0,
    staged_item_missing_count: 0,
    sync_excluded_filename_count: 0,
    sync_excluded_root_count: 0,
    pending_indexable_count: 12,
    active_upload_count: 1,
    active_download_count: 0,
    active_upload_progress_millionths: 100,
    active_download_progress_millionths: 0,
    notices: [],
    ...overrides,
  };
}

describe("iCloud health stall clock", () => {
  it("does not reset when only no-progress counters change", () => {
    const previous = report(activity());
    const next = report(activity({ no_progress_fetch_count: 9 }), { observed_at_ms: 2_000 });
    const fingerprint = icloudHealthStallClockFingerprint(previous);

    expect(icloudHealthStallClockFingerprint(next)).toBe(fingerprint);
    expect(updateIcloudHealthStallClock(
      previous,
      { blockedSinceMs: 1_200, fingerprint },
      next,
      2_000,
    )).toEqual({ blockedSinceMs: 1_200, fingerprint });
  });

  it("resets on real transfer progress and keeps that reset on the next poll", () => {
    const previous = report(activity());
    const previousFingerprint = icloudHealthStallClockFingerprint(previous);
    const progressed = report(activity({ active_upload_progress_millionths: 200 }), {
      observed_at_ms: 2_000,
    });
    const reset = updateIcloudHealthStallClock(
      previous,
      { blockedSinceMs: 1_200, fingerprint: previousFingerprint },
      progressed,
      2_000,
    );
    const unchanged = updateIcloudHealthStallClock(
      progressed,
      reset,
      report(activity({ active_upload_progress_millionths: 200 }), { observed_at_ms: 3_000 }),
      3_000,
    );

    expect(reset.blockedSinceMs).toBe(2_000);
    expect(unchanged.blockedSinceMs).toBe(2_000);
  });

  it("uses the provider blocker timestamp when the blocker first appears", () => {
    const next = report(activity(), { admission_blocked_since_ms: 700 });

    expect(updateIcloudHealthStallClock(
      null,
      { blockedSinceMs: 0, fingerprint: "" },
      next,
      2_000,
    ).blockedSinceMs).toBe(700);
  });

  it("clears the clock when admission becomes clear", () => {
    const blocked = report(activity());
    const fingerprint = icloudHealthStallClockFingerprint(blocked);
    const clear = report(null, {
      new_copy_admission_state: "clear",
      new_copy_admission_blockers: [],
      blockers: [],
      sync_backlog_present: false,
    });

    expect(updateIcloudHealthStallClock(
      blocked,
      { blockedSinceMs: 1_200, fingerprint },
      clear,
      2_000,
    )).toEqual({ blockedSinceMs: 0, fingerprint: "" });
  });
});
