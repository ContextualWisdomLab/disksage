import { describe, expect, it } from "vitest";
import {
  PODMAN_DESKTOP_SCHEMA_KIND,
  parsePodmanDesktopEvidence,
} from "./podmanEvidence";

/** Build one otherwise-valid desktop response so each test changes only assessment text. */
function fixture(): Record<string, any> {
  return {
    schema_kind: PODMAN_DESKTOP_SCHEMA_KIND,
    schema_version: 1,
    platform: "macos",
    evidence_complete: false,
    elapsed_ms: 1,
    capacity: {
      configured_disk_bytes: null,
      raw_logical_bytes: null,
      host_allocated_bytes: null,
      guest_total_bytes: null,
      guest_used_bytes: null,
      guest_available_bytes: null,
      graph_root_allocated_bytes: null,
      graph_root_used_bytes: null,
    },
    candidates: {
      image_candidate_bytes: null,
      stopped_container_candidate_bytes: null,
      volume_candidate_bytes: null,
      unused_image_records: null,
      stopped_container_records: null,
      image_candidate_set_sha256: null,
    },
    review_boundaries: {
      image_review_required: false,
      stopped_container_review_required: false,
      volume_review_required: false,
    },
    physically_reclaimable_bytes: null,
    podman_reported_reclaimable_bytes: null,
    raw_allocated_minus_guest_used_bytes: null,
    assessment_status: "unverified",
    reason_codes: ["host-physical-reclaim-unverified"],
    issue_codes: [],
    notices: [],
  };
}

describe("Podman assessment privacy validation", () => {
  it("rejects path-bearing or unsupported assessment status", () => {
    for (const status of [
      "/Users/alice/private-machine.sock",
      "UNVERIFIED",
      "unverified:private-detail",
      "unknown",
    ]) {
      const value = fixture();
      value.assessment_status = status;
      expect(() => parsePodmanDesktopEvidence(value)).toThrow("invalid-assessment-status");
    }
  });

  it("rejects path-bearing, malformed, oversized, or duplicate reason codes", () => {
    const invalidReasonSets = [
      ["/run/user/501/podman.sock"],
      ["UPPERCASE"],
      ["unsafe_code"],
      [`a${"b".repeat(96)}`],
      ["partial-evidence", "partial-evidence"],
    ];

    for (const reasonCodes of invalidReasonSets) {
      const value = fixture();
      value.reason_codes = reasonCodes;
      expect(() => parsePodmanDesktopEvidence(value)).toThrow("invalid-reason-codes");
    }
  });

  it("rejects malformed issue codes at the untrusted Tauri boundary", () => {
    const value = fixture();
    value.issue_codes = ["podman-info-failed:/Users/alice/private.sock"];
    expect(() => parsePodmanDesktopEvidence(value)).toThrow("invalid-issue-codes");
  });
});
