import type { CacheTrashPurgeResult } from "./api";

export interface CacheTrashPurgeSummary {
  successfulCount: number;
  allSucceeded: boolean;
  errors: string[];
}

/**
 * Summarize physical cache-trash deletion without treating an audit-journal gap as success.
 * A physically removed item is cleanly successful only when the backend also reports no error.
 */
export function summarizeCacheTrashPurge(items: CacheTrashPurgeResult[]): CacheTrashPurgeSummary {
  const successfulCount = items.filter((item) => item.purged && item.error.length === 0).length;
  const errors = items
    .filter((item) => item.error.length > 0)
    .map((item) => `${item.name}: ${item.error}`);

  return {
    successfulCount,
    allSucceeded: items.length > 0 && successfulCount === items.length && errors.length === 0,
    errors,
  };
}
