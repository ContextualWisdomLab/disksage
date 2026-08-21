import { describe, expect, it } from "vitest";
import { icloudBlockedSinceMs } from "./cloudArchiveHealthTiming";

describe("iCloud health blocker timing", () => {
  it("uses the backend observation clock when persisted blocked-since is absent", () => {
    expect(icloudBlockedSinceMs(null, 20_000)).toBe(20_000);
    expect(icloudBlockedSinceMs(undefined, 30_000)).toBe(30_000);
  });

  it("preserves the backend-provided blocker onset", () => {
    expect(icloudBlockedSinceMs(10_000, 20_000)).toBe(10_000);
  });
});
