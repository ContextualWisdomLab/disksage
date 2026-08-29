import type { OrphanCleanupResult, OrphanPlan } from "./api";

export interface OrphanCleanupRefreshOutcome {
  result: OrphanCleanupResult;
  plan: OrphanPlan | null;
  refresh_failed: boolean;
}

/**
 * Execute an approved orphan cleanup, then refresh its read-only evidence separately.
 *
 * Cleanup failure remains a rejected operation. Once cleanup succeeds, a later refresh failure
 * cannot retroactively turn that filesystem mutation into a reported cleanup failure or leave the
 * caller with a stale actionable plan.
 */
export async function cleanAndRefreshOrphanPlan(
  clean: () => Promise<OrphanCleanupResult>,
  refresh: () => Promise<OrphanPlan>,
): Promise<OrphanCleanupRefreshOutcome> {
  const result = await clean();
  try {
    return {
      result,
      plan: await refresh(),
      refresh_failed: false,
    };
  } catch {
    return {
      result,
      plan: null,
      refresh_failed: true,
    };
  }
}
