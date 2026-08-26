import { describe, expect, it } from "vitest";
import { summarizeCacheTrashPurge } from "./cacheTrashPurgeSummary";

describe("summarizeCacheTrashPurge", () => {
  it("never counts a physically deleted item with a journal error as clean success", () => {
    const summary = summarizeCacheTrashPurge([
      {
        name: "_cacache",
        path: "/private/Trash/_cacache",
        bytes: 10,
        signature: "npm-cacache",
        purged: true,
        error: "purged-but-journal-write-failed:disk-full",
      },
      {
        name: "db",
        path: "/private/Trash/db",
        bytes: 20,
        signature: "trivy-database-cache",
        purged: true,
        error: "",
      },
      {
        name: "v11",
        path: "/private/Trash/v11",
        bytes: 30,
        signature: "pnpm-store-v11",
        purged: false,
        error: "cache-trash-signature-changed",
      },
    ]);

    expect(summary.purgedCount).toBe(2);
    expect(summary.successfulCount).toBe(1);
    expect(summary.auditFailedCount).toBe(1);
    expect(summary.retryableCount).toBe(1);
    expect(summary.allSucceeded).toBe(false);
    expect(summary.errors).toEqual([
      "_cacache: purged-but-journal-write-failed:disk-full",
      "v11: cache-trash-signature-changed",
    ]);
  });

  it("reports allSucceeded only when every deletion is purged and audit-clean", () => {
    const summary = summarizeCacheTrashPurge([
      {
        name: "db",
        path: "/private/Trash/db",
        bytes: 20,
        signature: "trivy-database-cache",
        purged: true,
        error: "",
      },
    ]);

    expect(summary.purgedCount).toBe(1);
    expect(summary.successfulCount).toBe(1);
    expect(summary.auditFailedCount).toBe(0);
    expect(summary.retryableCount).toBe(0);
    expect(summary.errors).toEqual([]);
    expect(summary.allSucceeded).toBe(true);
  });
});
