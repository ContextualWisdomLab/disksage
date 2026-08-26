import { describe, expect, it, vi } from "vitest";
import type { OrphanCleanupResult, OrphanPlan } from "./api";
import { cleanAndRefreshOrphanPlan } from "./orphanCleanupFlow";

const cleanupResult: OrphanCleanupResult = {
  schema_kind: "disksage.orphan-cleanup-result/v1",
  schema_version: 1,
  plan_fingerprint: "a".repeat(64),
  requested_count: 1,
  moved_count: 1,
  filesystem_mutation_executed: true,
  items: [],
  notices: [],
};

const refreshedPlan: OrphanPlan = {
  schema_kind: "disksage.orphan-plan/v1",
  schema_version: 1,
  generated_at_ms: 2,
  plan_fingerprint: "b".repeat(64),
  candidate_count: 0,
  candidate_bytes: 0,
  scan_complete: true,
  candidates: [],
  notices: [],
  local_paths_included: false,
  mutation_performed: false,
  exact_approval_phrase: "approval",
};

describe("orphan cleanup execution flow", () => {
  it("preserves a successful cleanup when the follow-up plan refresh fails", async () => {
    const clean = vi.fn(async () => cleanupResult);
    const refresh = vi.fn(async () => {
      throw new Error("refresh unavailable");
    });

    await expect(cleanAndRefreshOrphanPlan(clean, refresh)).resolves.toEqual({
      result: cleanupResult,
      plan: null,
      refresh_failed: true,
    });
    expect(clean).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("does not refresh when cleanup itself fails", async () => {
    const clean = vi.fn(async () => {
      throw new Error("cleanup failed");
    });
    const refresh = vi.fn(async () => refreshedPlan);

    await expect(cleanAndRefreshOrphanPlan(clean, refresh)).rejects.toThrow("cleanup failed");
    expect(refresh).not.toHaveBeenCalled();
  });

  it("returns the refreshed plan when cleanup and refresh both succeed", async () => {
    await expect(
      cleanAndRefreshOrphanPlan(
        async () => cleanupResult,
        async () => refreshedPlan,
      ),
    ).resolves.toEqual({
      result: cleanupResult,
      plan: refreshedPlan,
      refresh_failed: false,
    });
  });
});
