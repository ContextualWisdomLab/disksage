import { describe, expect, it } from "vitest";
import type { CloudCandidate, CloudReviewDecision } from "./api";
import {
  candidateReviewDecision,
  cloudReviewQueuePage,
  cloudReviewQueueState,
  cloudReviewQueueStats,
  cloudReviewReasons,
  filterCloudReviewQueue,
  matchingReviewDecision,
} from "./cloudReviewQueue";

function candidate(
  id: string,
  bytes: number,
  overrides: Partial<CloudCandidate> = {},
): CloudCandidate {
  return {
    metadata_fingerprint: id.repeat(64),
    review_fingerprint: `${id}r`.repeat(32),
    src: `/source/${id}.pdf`,
    dst: `/cloud/${id}.pdf`,
    provider: "icloud",
    destination_account_scope: "unknown",
    kind: "document",
    bytes,
    age_days: 10,
    created_ms: 100,
    modified_ms: 200,
    production_time_ms: 300,
    production_time_source: "filesystem:created",
    production_time_confidence: "low",
    source_root: "/source",
    relative_path: `${id}.pdf`,
    source_context: ".",
    requires_review: true,
    review_reasons: ["destination-account-scope-unknown"],
    content_title: null,
    content_authors: [],
    content_context: [],
    duration_ms: null,
    dataset_profile: null,
    metadata_evidence: [],
    blocked_reason: null,
    ...overrides,
  };
}

function decision(
  item: CloudCandidate,
  disposition: "approved" | "held",
  reviewFingerprint = item.review_fingerprint,
): CloudReviewDecision {
  return {
    version: 2,
    decision_id: "d".repeat(64),
    candidate_fingerprint: item.metadata_fingerprint,
    review_fingerprint: reviewFingerprint,
    disposition,
    reviewed_at_ms: 400,
    reviewed_by: "human:local:test",
    rationale: "metadata reviewed",
  };
}

describe("cloud review queue", () => {
  it("accepts only a decision bound to the current review fingerprint", () => {
    const item = candidate("a", 10);
    const exact = decision(item, "approved");
    const stale = decision(item, "held", "f".repeat(64));
    expect(candidateReviewDecision(item, [stale])).toBe(stale);
    expect(matchingReviewDecision(item, [stale])).toBeNull();
    expect(cloudReviewQueueState(item, [stale])).toBe("unreviewed");
    expect(matchingReviewDecision(item, [exact])).toBe(exact);
    expect(cloudReviewQueueState(item, [exact])).toBe("approved");
  });

  it("summarizes actionable review progress without counting blocked candidates", () => {
    const approved = candidate("a", 10);
    const held = candidate("b", 20);
    const unreviewed = candidate("c", 30);
    const blocked = candidate("d", 40, { blocked_reason: "incomplete-download" });
    const ready = candidate("e", 50, { requires_review: false });
    expect(cloudReviewQueueStats(
      [approved, held, unreviewed, blocked, ready],
      [decision(approved, "approved"), decision(held, "held")],
    )).toEqual({
      total: 5,
      totalBytes: 150,
      reviewable: 3,
      reviewableBytes: 60,
      reviewed: 2,
      reviewedBytes: 30,
      unreviewed: 1,
      unreviewedBytes: 30,
      approved: 1,
      held: 1,
      blocked: 1,
      blockedBytes: 40,
      ready: 1,
    });
  });

  it("filters by exact queue state and review reason", () => {
    const large = candidate("b", 200, { review_reasons: ["reason-b"] });
    const small = candidate("a", 100, { review_reasons: ["reason-a", "reason-b"] });
    const approved = candidate("c", 300, { review_reasons: ["reason-b"] });
    expect(filterCloudReviewQueue(
      [small, large, approved],
      [decision(approved, "approved")],
      "unreviewed",
      "reason-b",
      "bytes-desc",
    ).map((item) => item.relative_path)).toEqual(["b.pdf", "a.pdf"]);
  });

  it("sorts equal values with a deterministic relative-path tie break", () => {
    const later = candidate("b", 100, { relative_path: "z.pdf", production_time_ms: 500 });
    const earlierB = candidate("c", 100, { relative_path: "b.pdf", production_time_ms: 100 });
    const earlierA = candidate("a", 100, { relative_path: "a.pdf", production_time_ms: 100 });
    expect(filterCloudReviewQueue(
      [later, earlierB, earlierA],
      [],
      "all",
      "",
      "production-asc",
    ).map((item) => item.relative_path)).toEqual(["a.pdf", "b.pdf", "z.pdf"]);
  });

  it("deduplicates and sorts review-reason options", () => {
    expect(cloudReviewReasons([
      candidate("a", 1, { review_reasons: ["z", "a"] }),
      candidate("b", 1, { review_reasons: ["a", "m"] }),
    ])).toEqual(["a", "m", "z"]);
  });

  it("clamps pages and reports the visible range", () => {
    const items = Array.from({ length: 45 }, (_, index) => candidate(String(index), index));
    expect(cloudReviewQueuePage(items, 99, 20)).toMatchObject({
      page: 3,
      totalPages: 3,
      startIndex: 41,
      endIndex: 45,
      totalItems: 45,
    });
    expect(cloudReviewQueuePage([], 2, 20)).toEqual({
      items: [],
      page: 1,
      totalPages: 1,
      startIndex: 0,
      endIndex: 0,
      totalItems: 0,
    });
  });
});
