import type { CacheTrashPurgeResult } from "./api";

export interface CacheTrashPurgeSummary {
  purgedCount: number;
  successfulCount: number;
  retryableCount: number;
  auditFailedCount: number;
  allSucceeded: boolean;
  errors: string[];
}

/**
 * Summarize physical cache-trash deletion without turning an audit gap into a retry instruction.
 * A physically removed item is cleanly successful only when the backend also reports no error,
 * but it must still be counted as removed when its later record write fails.
 */
export function summarizeCacheTrashPurge(items: CacheTrashPurgeResult[]): CacheTrashPurgeSummary {
  const purgedCount = items.filter((item) => item.purged).length;
  const successfulCount = items.filter((item) => item.purged && item.error.length === 0).length;
  const auditFailedCount = items.filter((item) => item.purged && item.error.length > 0).length;
  const retryableCount = items.filter((item) => !item.purged).length;
  const errors = items
    .filter((item) => item.error.length > 0)
    .map((item) => `${item.name}: ${item.error}`);

  return {
    purgedCount,
    successfulCount,
    retryableCount,
    auditFailedCount,
    allSucceeded: items.length > 0 && purgedCount === items.length && errors.length === 0,
    errors,
  };
}
