import { describe, expect, it } from "vitest";
import { blockedSinceMs, icloudBlockedSinceMs } from "./cloudArchiveHealthTiming";

describe("iCloud health blocker timing", () => {
  it("uses the backend observation clock when persisted blocked-since is absent", () => {
    expect(icloudBlockedSinceMs(null, 20_000)).toBe(20_000);
    expect(icloudBlockedSinceMs(undefined, 30_000)).toBe(30_000);
  });

  it("preserves the backend-provided blocker onset", () => {
    expect(icloudBlockedSinceMs(10_000, 20_000)).toBe(10_000);
  });

  it("rejects impossible persisted onset values", () => {
    for (const onset of [-1, 20_001, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(icloudBlockedSinceMs(onset, 20_000)).toBe(20_000);
    }
  });

  it("uses the same persisted timing contract for third-party providers", () => {
    expect(blockedSinceMs(40_000, 50_000)).toBe(40_000);
    expect(blockedSinceMs(null, 50_000)).toBe(50_000);
  });
});
