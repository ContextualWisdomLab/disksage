import { describe, expect, it } from "vitest";
import type { CloudCandidate } from "./api";
import { filterCloudReviewQueue } from "./cloudReviewQueue";

/** Build a valid review candidate while keeping each sort fixture intentionally small. */
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
    review_reasons: ["review-needed"],
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

/** Return relative paths so ordering assertions remain compact and readable. */
function paths(items: CloudCandidate[]): string[] {
  return items.map((item) => item.relative_path);
}

describe("cloud review queue exact coverage regressions", () => {
  it("covers both byte-order directions and deterministic ties", () => {
    const small = candidate("a", 100);
    const large = candidate("b", 200);

    expect(paths(filterCloudReviewQueue([small, large], [], "all", "", "bytes-desc")))
      .toEqual(["b.pdf", "a.pdf"]);
    expect(paths(filterCloudReviewQueue([large, small], [], "all", "", "bytes-desc")))
      .toEqual(["b.pdf", "a.pdf"]);

    const metadataB = candidate("b", 100, {
      relative_path: "same.pdf",
      metadata_fingerprint: "b".repeat(64),
    });
    const metadataA = candidate("a", 100, {
      relative_path: "same.pdf",
      metadata_fingerprint: "a".repeat(64),
    });
    expect(filterCloudReviewQueue(
      [metadataA, metadataB],
      [],
      "all",
      "",
      "bytes-desc",
    ).map((item) => item.metadata_fingerprint)).toEqual([
      "a".repeat(64),
      "b".repeat(64),
    ]);
    expect(filterCloudReviewQueue(
      [metadataB, metadataA],
      [],
      "all",
      "",
      "bytes-desc",
    ).map((item) => item.metadata_fingerprint)).toEqual([
      "a".repeat(64),
      "b".repeat(64),
    ]);
  });

  it("covers both production-time directions for ascending and descending sorts", () => {
    const early = candidate("a", 100, { production_time_ms: 100 });
    const late = candidate("b", 100, { production_time_ms: 500 });

    for (const input of [[early, late], [late, early]]) {
      expect(paths(filterCloudReviewQueue(input, [], "all", "", "production-asc")))
        .toEqual(["a.pdf", "b.pdf"]);
      expect(paths(filterCloudReviewQueue(input, [], "all", "", "production-desc")))
        .toEqual(["b.pdf", "a.pdf"]);
    }
  });

  it("covers empty, matching, and missing reason filters without broadening queue state", () => {
    const reviewable = candidate("a", 100, { review_reasons: ["reason-a"] });
    const other = candidate("b", 100, { review_reasons: ["reason-b"] });

    expect(paths(filterCloudReviewQueue(
      [reviewable, other],
      [],
      "all",
      "",
      "production-desc",
    ))).toHaveLength(2);
    expect(paths(filterCloudReviewQueue(
      [reviewable, other],
      [],
      "all",
      "reason-a",
      "production-desc",
    ))).toEqual(["a.pdf"]);
    expect(filterCloudReviewQueue(
      [reviewable, other],
      [],
      "all",
      "missing-reason",
      "production-desc",
    )).toEqual([]);
  });
});
