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

  it("describes non-macOS support as review-only scope without implying macOS has in-app permanent deletion", () => {
    expect(
      cacheTrashPurgeAvailability({
        candidates: [],
        approval_phrase: null,
        notice: "cache-trash-native-review-macos-only",
      }),
    ).toEqual({
      canPurge: false,
      instruction:
        "휴지통 속 재생성 캐시 검토는 현재 macOS 기본 휴지통에서만 지원합니다. 앱 내 영구 삭제는 안전한 객체 결합 삭제를 제공할 때까지 모든 플랫폼에서 비활성화되어 있습니다.",
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

  it("asks the operator to restore the native Trash location before retrying", () => {
    expect(
      cacheTrashPurgeAvailability({ candidates: [], approval_phrase: null, notice: "cache-trash-native-root-unsafe" }),
    ).toEqual({
      canPurge: false,
      instruction: "휴지통 위치를 확인하지 못했습니다. macOS 휴지통을 확인한 뒤 새로고침하세요.",
    });
  });
});
