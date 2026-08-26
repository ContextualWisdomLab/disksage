import { invoke } from "@tauri-apps/api/core";
import type { CacheTrashCandidate, CacheTrashPurgeExecution } from "./api";

export interface CacheTrashReview {
  schema_kind: "disksage.cache-trash-review";
  schema_version: number;
  supported: boolean;
  candidates: CacheTrashCandidate[];
  approval_phrase: string | null;
  notice: string | null;
}

export const reviewProvenCacheTrash = () =>
  invoke<CacheTrashReview>("review_proven_cache_trash");

export const purgeReviewedCacheTrash = (
  approvedCandidates: CacheTrashCandidate[],
  confirmationPhrase: string,
) =>
  invoke<CacheTrashPurgeExecution>("purge_proven_cache_trash", {
    approvedCandidates,
    confirmationPhrase,
  });
