import { describe, expect, it } from "vitest";

import { cacheTrashPurgeItemMessage } from "./cacheTrashPurgeItemMessage";

const base = {
  name: "_cacache",
  path: "/Users/example/.Trash/_cacache",
  bytes: 10,
  signature: "npm-cacache",
};

describe("cacheTrashPurgeItemMessage", () => {
  it("distinguishes deletion failure from post-delete journal failure", () => {
    expect(
      cacheTrashPurgeItemMessage({
        ...base,
        purged: false,
        error: "cache-trash-identity-bound-permanent-delete-unavailable",
      }),
    ).toBe("_cacache: 영구 삭제하지 못했습니다. 목록을 확인한 뒤 다시 시도하십시오.");

    expect(
      cacheTrashPurgeItemMessage({
        ...base,
        purged: true,
        error: "purged-but-journal-write-failed:disk-full",
      }),
    ).toBe("_cacache: 영구 삭제는 완료했지만 정리 기록을 남기지 못했습니다. 기록 상태를 확인하십시오.");
  });
});
