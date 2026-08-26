import type { CacheTrashReview } from "./cacheTrashReviewApi";

export const CACHE_TRASH_OBJECT_BOUND_DELETE_UNAVAILABLE =
  "cache-trash-identity-bound-permanent-delete-unavailable";
export const CACHE_TRASH_NATIVE_ROOT_UNSAFE = "cache-trash-native-root-unsafe";

export interface CacheTrashPurgeAvailability {
  canPurge: boolean;
  instruction: string | null;
}

type ReviewAvailabilityInput = Pick<
  CacheTrashReview,
  "candidates" | "approval_phrase" | "notice"
>;

/**
 * Project the backend review into a fail-closed purge affordance.
 *
 * A review may still expose read-only candidate evidence while irreversible deletion is unavailable.
 * In that state the UI must not manufacture authority from candidate presence alone and instead gives
 * the operator a concrete manual next action.
 */
export function cacheTrashPurgeAvailability(
  review: ReviewAvailabilityInput,
): CacheTrashPurgeAvailability {
  if (review.notice === CACHE_TRASH_OBJECT_BOUND_DELETE_UNAVAILABLE) {
    return {
      canPurge: false,
      instruction:
        "휴지통의 재생성 캐시는 확인했지만 DiskSage에서 안전하게 영구 삭제할 수 없습니다. 저장 공간을 회수하려면 macOS 휴지통에서 항목을 직접 검토한 뒤 비우세요.",
    };
  }

  if (review.notice === CACHE_TRASH_NATIVE_ROOT_UNSAFE) {
    return {
      canPurge: false,
      instruction:
        "휴지통 위치를 확인하지 못했습니다. macOS 휴지통을 확인한 뒤 새로고침하세요.",
    };
  }

  return {
    canPurge: review.candidates.length > 0 && review.approval_phrase !== null,
    instruction: null,
  };
}
