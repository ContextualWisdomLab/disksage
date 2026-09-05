import { describe, expect, it } from "vitest";
import type { CloudCandidate, CloudReviewDecision } from "./api";
import { matchingReviewDecision } from "./cloudReviewQueue";

function candidate(): CloudCandidate {
  return {
    metadata_fingerprint: "a".repeat(64),
    review_fingerprint: "b".repeat(64),
    src: "/source/a.pdf",
    dst: "/cloud/a.pdf",
    provider: "icloud",
    destination_account_scope: "personal",
    kind: "document",
    bytes: 1_024,
    age_days: 10,
    created_ms: 100,
    modified_ms: 200,
    production_time_ms: 300,
    production_time_source: "filesystem:created",
    production_time_confidence: "low",
    source_root: "/source",
    relative_path: "a.pdf",
    source_context: ".",
    requires_review: true,
    review_reasons: ["personal-cloud-sensitive-context-needs-explicit-approval"],
    content_title: null,
    content_authors: [],
    content_context: [],
    duration_ms: null,
    dataset_profile: null,
    metadata_evidence: [],
    blocked_reason: null,
  };
}

function decision(item: CloudCandidate, rationale: string): CloudReviewDecision {
  return {
    version: 2,
    decision_id: "d".repeat(64),
    candidate_fingerprint: item.metadata_fingerprint,
    review_fingerprint: item.review_fingerprint,
    disposition: "approved",
    reviewed_at_ms: 400,
    reviewed_by: "human:local:test",
    rationale,
  };
}

describe("cloud review rationale format-control hardening", () => {
  it("rejects representative code points from every supported Unicode format-control class", () => {
    const item = candidate();
    const representativeCodepoints = [
      0x00ad,
      0x0600,
      0x061c,
      0x0890,
      0x200c,
      0x202a,
      0x2061,
      0x2066,
      0xfff9,
      0x110bd,
      0x13430,
      0x1bca0,
      0x1d173,
      0xe0020,
    ];

    for (const codepoint of representativeCodepoints) {
      const rationale = `metadata${String.fromCodePoint(codepoint)}reviewed`;
      expect(
        matchingReviewDecision(item, [decision(item, rationale)]),
        `U+${codepoint.toString(16).toUpperCase()}`,
      ).toBeNull();
    }
  });
});
