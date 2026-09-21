import { describe, expect, it } from "vitest";

import {
  PODMAN_DESKTOP_SCHEMA_KIND,
  parsePodmanDesktopEvidence,
} from "./podmanEvidence";

function validEvidence(): Record<string, any> {
  return {
    schema_kind: PODMAN_DESKTOP_SCHEMA_KIND,
    schema_version: 1,
    platform: "macos",
    evidence_complete: true,
    elapsed_ms: 17,
    capacity: {
      configured_disk_bytes: 1000,
      raw_logical_bytes: 900,
      host_allocated_bytes: 700,
      guest_total_bytes: 800,
      guest_used_bytes: 500,
      guest_available_bytes: 300,
      graph_root_allocated_bytes: 600,
      graph_root_used_bytes: 450,
    },
    candidates: {
      image_candidate_bytes: 200,
      stopped_container_candidate_bytes: 30,
      volume_candidate_bytes: 70,
      unused_image_records: 2,
      stopped_container_records: 2,
      image_candidate_set_sha256: "a".repeat(64),
    },
    review_boundaries: {
      image_review_required: true,
      stopped_container_review_required: true,
      volume_review_required: true,
    },
    physically_reclaimable_bytes: null,
    podman_reported_reclaimable_bytes: 300,
    raw_allocated_minus_guest_used_bytes: 200,
    assessment_status: "unverified",
    reason_codes: ["host-physical-reclaim-unverified"],
    issue_codes: [],
    notices: [
      "Podman-reported logical candidates are not verified host physical reclaimability.",
      "This desktop surface exposes no prune, remove, machine lifecycle, TRIM, or raw-image mutation command.",
    ],
  };
}

function cloneEvidence(): Record<string, any> {
  return JSON.parse(JSON.stringify(validEvidence()));
}

describe("Podman desktop evidence validator failure coverage", () => {
  it("rejects primitive, null, array, and null nested records", () => {
    expect(() => parsePodmanDesktopEvidence("not-a-record")).toThrow(
      "invalid-podman-desktop-evidence",
    );
    expect(() => parsePodmanDesktopEvidence(null)).toThrow("invalid-podman-desktop-evidence");
    expect(() => parsePodmanDesktopEvidence([])).toThrow("invalid-podman-desktop-evidence");

    const nestedNull = cloneEvidence();
    nestedNull.capacity = null;
    expect(() => parsePodmanDesktopEvidence(nestedNull)).toThrow("invalid-podman-capacity");
  });

  it("rejects non-string values at the fingerprint string boundary", () => {
    const value = cloneEvidence();
    value.candidates.image_candidate_set_sha256 = 42;
    expect(() => parsePodmanDesktopEvidence(value)).toThrow(
      "invalid-image-candidate-set-sha256",
    );
  });

  it("rejects non-boolean completeness values", () => {
    const value = cloneEvidence();
    value.evidence_complete = "true";
    expect(() => parsePodmanDesktopEvidence(value)).toThrow("invalid-evidence-complete");
  });

  it("rejects every invalid unsigned-integer class", () => {
    const wrongType = cloneEvidence();
    wrongType.elapsed_ms = "17";
    expect(() => parsePodmanDesktopEvidence(wrongType)).toThrow("invalid-elapsed-ms");

    const unsafe = cloneEvidence();
    unsafe.elapsed_ms = Number.MAX_SAFE_INTEGER + 1;
    expect(() => parsePodmanDesktopEvidence(unsafe)).toThrow("invalid-elapsed-ms");

    const negative = cloneEvidence();
    negative.elapsed_ms = -1;
    expect(() => parsePodmanDesktopEvidence(negative)).toThrow("invalid-elapsed-ms");
  });

  it("rejects non-array and non-string notice collections", () => {
    const notArray = cloneEvidence();
    notArray.notices = "not-an-array";
    expect(() => parsePodmanDesktopEvidence(notArray)).toThrow("invalid-notices");

    const nonStringItem = cloneEvidence();
    nonStringItem.notices = [nonStringItem.notices[0], 42];
    expect(() => parsePodmanDesktopEvidence(nonStringItem)).toThrow("invalid-notices");
  });
});
