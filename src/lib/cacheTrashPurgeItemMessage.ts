import type { CacheTrashPurgeResult } from "./api";

/** Return a bounded operator message without exposing backend error details. */
export function cacheTrashPurgeItemMessage(item: CacheTrashPurgeResult): string {
  if (item.purged) {
    return `${item.name}: 영구 삭제는 완료했지만 정리 기록을 남기지 못했습니다. 기록 상태를 확인하십시오.`;
  }
  return `${item.name}: 영구 삭제하지 못했습니다. 목록을 확인한 뒤 다시 시도하십시오.`;
}
