import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import { purgeReviewedCacheTrash, reviewProvenCacheTrash } from "./cacheTrashReviewApi";

const reviewed = [
  {
    name: "_cacache",
    path: "/Users/example/.Trash/_cacache",
    bytes: 10,
    signature: "npm-cacache",
  },
];

describe("cacheTrashReviewApi", () => {
  beforeEach(() => mocks.invoke.mockReset());

  it("fetches one backend-authored review snapshot", () => {
    const result = Promise.resolve("review");
    mocks.invoke.mockReturnValue(result);
    expect(reviewProvenCacheTrash()).toBe(result);
    expect(mocks.invoke).toHaveBeenCalledWith("review_proven_cache_trash");
  });

  it("sends the exact reviewed candidate vector with its confirmation phrase", () => {
    const result = Promise.resolve("purged");
    mocks.invoke.mockReturnValue(result);
    expect(purgeReviewedCacheTrash(reviewed, "reviewed phrase")).toBe(result);
    expect(mocks.invoke).toHaveBeenCalledWith("purge_proven_cache_trash", {
      approvedCandidates: reviewed,
      confirmationPhrase: "reviewed phrase",
    });
  });
});
