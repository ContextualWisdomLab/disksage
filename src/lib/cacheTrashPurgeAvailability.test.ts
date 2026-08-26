import { describe, expect, it } from "vitest";

import { cacheTrashPurgeAvailability } from "./cacheTrashPurgeAvailability";

const candidate = {
  name: "_cacache",
  path: "/Users/example/.Trash/_cacache",
  bytes: 10,
  signature: "npm-cacache",
};

describe("cacheTrashPurgeAvailability", () => {
  it("suppresses permanent-delete authority and provides a manual next action when object-bound deletion is unavailable", () => {
    expect(
      cacheTrashPurgeAvailability({
        candidates: [candidate],
        approval_phrase: null,
        notice: "cache-trash-identity-bound-permanent-delete-unavailable",
      }),
    ).toEqual({
      canPurge: false,
      instruction:
        "휴지통의 재생성 캐시는 확인했지만 DiskSage에서 안전하게 영구 삭제할 수 없습니다. 저장 공간을 회수하려면 macOS 휴지통에서 항목을 직접 검토한 뒤 비우세요.",
    });
  });

  it("enables the action only when the backend minted approval for a non-empty reviewed snapshot", () => {
    expect(
      cacheTrashPurgeAvailability({
        candidates: [candidate],
        approval_phrase: "reviewed phrase",
        notice: null,
      }),
    ).toEqual({ canPurge: true, instruction: null });

    expect(
      cacheTrashPurgeAvailability({ candidates: [], approval_phrase: "stale phrase", notice: null }),
    ).toEqual({ canPurge: false, instruction: null });
  });
});
