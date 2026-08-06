import { describe, expect, it } from "vitest";
import type { CloudCandidate, CloudReviewDecision } from "./api";
import {
  ORGANIZATION_TENANT_AUTHORITY_ATTESTATION,
  cloudReviewQueueState,
  organizationTenantAuthorityRequired,
} from "./cloudReviewQueue";

const ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON =
  "organization-cloud-sensitive-context-needs-explicit-tenant-approval";

function candidate(
  id: string,
  overrides: Partial<CloudCandidate>,
): CloudCandidate {
  return {
    metadata_fingerprint: id.repeat(64),
    review_fingerprint: `${id}r`.repeat(32),
    src: `/source/${id}.pdf`,
    dst: `/cloud/${id}.pdf`,
    provider: "icloud",
    destination_account_scope: "unknown",
    kind: "document",
    bytes: 1_024,
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

function approvedDecision(
  item: CloudCandidate,
  rationale = "metadata reviewed",
): CloudReviewDecision {
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

describe("organization tenant authority fail-closed validation", () => {
  it("requires tenant authority when either canonical organization signal is present", () => {
    const organizationScopeOnly = candidate("a", {
      destination_account_scope: "organization",
      review_reasons: ["destination-account-scope-unknown"],
    });
    const organizationReasonOnly = candidate("b", {
      destination_account_scope: "personal",
      review_reasons: [ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON],
    });
    const ordinaryPersonal = candidate("c", {
      destination_account_scope: "personal",
      review_reasons: ["personal-cloud-sensitive-context-needs-explicit-approval"],
    });

    expect(organizationTenantAuthorityRequired(organizationScopeOnly)).toBe(true);
    expect(organizationTenantAuthorityRequired(organizationReasonOnly)).toBe(true);
    expect(organizationTenantAuthorityRequired(ordinaryPersonal)).toBe(false);
  });

  it("refuses approval when either organization signal is present without attestation", () => {
    for (const item of [
      candidate("a", {
        destination_account_scope: "organization",
        review_reasons: ["destination-account-scope-unknown"],
      }),
      candidate("b", {
        destination_account_scope: "personal",
        review_reasons: [ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON],
      }),
    ]) {
      expect(cloudReviewQueueState(item, [approvedDecision(item)])).toBe("unreviewed");
      expect(cloudReviewQueueState(item, [approvedDecision(
        item,
        `${ORGANIZATION_TENANT_AUTHORITY_ATTESTATION} Tenant and destination verified.`,
      )])).toBe("approved");
    }
  });
});
