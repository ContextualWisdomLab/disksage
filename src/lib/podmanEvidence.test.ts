import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  PODMAN_DESKTOP_SCHEMA_KIND,
  loadPodmanEvidence,
  parsePodmanDesktopEvidence,
  podmanEvidenceView,
  type PodmanDesktopEvidence,
} from "./podmanEvidence";

function fixture(): Record<string, unknown> {
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
    issue_codes: ["partial-evidence"],
    notices: ["read only"],
  };
}

function cloneFixture(): Record<string, any> {
  return JSON.parse(JSON.stringify(fixture()));
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("parsePodmanDesktopEvidence", () => {
  it("accepts the complete privacy-safe schema", () => {
    const parsed = parsePodmanDesktopEvidence(fixture());
    expect(parsed.schema_kind).toBe(PODMAN_DESKTOP_SCHEMA_KIND);
    expect(parsed.capacity.host_allocated_bytes).toBe(700);
    expect(parsed.candidates.image_candidate_set_sha256).toBe("a".repeat(64));
    expect(parsed.reason_codes).toEqual(["host-physical-reclaim-unverified"]);
  });

  it("preserves unknown observations as null", () => {
    const value = cloneFixture();
    for (const key of Object.keys(value.capacity)) value.capacity[key] = null;
    for (const key of [
      "image_candidate_bytes",
      "stopped_container_candidate_bytes",
      "volume_candidate_bytes",
      "unused_image_records",
      "stopped_container_records",
      "image_candidate_set_sha256",
    ]) {
      value.candidates[key] = null;
    }
    value.physically_reclaimable_bytes = null;
    value.podman_reported_reclaimable_bytes = null;
    value.raw_allocated_minus_guest_used_bytes = null;
    const parsed = parsePodmanDesktopEvidence(value);
    expect(Object.values(parsed.capacity).every((entry) => entry === null)).toBe(true);
    expect(Object.values(parsed.candidates).every((entry) => entry === null)).toBe(true);
  });

  it.each([
    [null, "invalid-podman-desktop-evidence"],
    [[], "invalid-podman-desktop-evidence"],
    ["bad", "invalid-podman-desktop-evidence"],
  ])("rejects a non-record response %#", (value, message) => {
    expect(() => parsePodmanDesktopEvidence(value)).toThrow(message);
  });

  it("rejects schema drift", () => {
    const wrongKind = cloneFixture();
    wrongKind.schema_kind = "other";
    expect(() => parsePodmanDesktopEvidence(wrongKind)).toThrow(
      "unsupported-podman-desktop-schema-kind",
    );
    const wrongVersion = cloneFixture();
    wrongVersion.schema_version = 2;
    expect(() => parsePodmanDesktopEvidence(wrongVersion)).toThrow(
      "unsupported-podman-desktop-schema-version",
    );
  });

  it.each([
    ["platform", 1, "invalid-platform"],
    ["evidence_complete", "yes", "invalid-evidence-complete"],
    ["elapsed_ms", "17", "invalid-elapsed-ms"],
    ["elapsed_ms", 1.5, "invalid-elapsed-ms"],
    ["elapsed_ms", -1, "invalid-elapsed-ms"],
    ["assessment_status", false, "invalid-assessment-status"],
    ["reason_codes", "bad", "invalid-reason-codes"],
    ["reason_codes", ["ok", 2], "invalid-reason-codes"],
    ["issue_codes", "bad", "invalid-issue-codes"],
    ["notices", "bad", "invalid-notices"],
  ])("rejects invalid top-level field %s", (field, value, message) => {
    const invalid = cloneFixture();
    invalid[field] = value;
    expect(() => parsePodmanDesktopEvidence(invalid)).toThrow(message);
  });

  it("rejects invalid nested records", () => {
    const capacity = cloneFixture();
    capacity.capacity = [];
    expect(() => parsePodmanDesktopEvidence(capacity)).toThrow("invalid-podman-capacity");

    const candidates = cloneFixture();
    candidates.candidates = null;
    expect(() => parsePodmanDesktopEvidence(candidates)).toThrow("invalid-podman-candidates");

    const boundaries = cloneFixture();
    boundaries.review_boundaries = "bad";
    expect(() => parsePodmanDesktopEvidence(boundaries)).toThrow(
      "invalid-podman-review-boundaries",
    );
  });

  it.each([
    ["configured_disk_bytes", -1, "invalid-configured-disk-bytes"],
    ["raw_logical_bytes", "1", "invalid-raw-logical-bytes"],
    ["host_allocated_bytes", 1.2, "invalid-host-allocated-bytes"],
    ["guest_total_bytes", -1, "invalid-guest-total-bytes"],
    ["guest_used_bytes", "1", "invalid-guest-used-bytes"],
    ["guest_available_bytes", 1.2, "invalid-guest-available-bytes"],
    ["graph_root_allocated_bytes", -1, "invalid-graph-root-allocated-bytes"],
    ["graph_root_used_bytes", "1", "invalid-graph-root-used-bytes"],
  ])("rejects invalid capacity field %s", (field, value, message) => {
    const invalid = cloneFixture();
    invalid.capacity[field] = value;
    expect(() => parsePodmanDesktopEvidence(invalid)).toThrow(message);
  });

  it.each([
    ["image_candidate_bytes", -1, "invalid-image-candidate-bytes"],
    ["stopped_container_candidate_bytes", "1", "invalid-stopped-container-candidate-bytes"],
    ["volume_candidate_bytes", 1.2, "invalid-volume-candidate-bytes"],
    ["unused_image_records", -1, "invalid-unused-image-records"],
    ["stopped_container_records", "1", "invalid-stopped-container-records"],
  ])("rejects invalid candidate field %s", (field, value, message) => {
    const invalid = cloneFixture();
    invalid.candidates[field] = value;
    expect(() => parsePodmanDesktopEvidence(invalid)).toThrow(message);
  });

  it("rejects malformed or non-string candidate fingerprints", () => {
    const malformed = cloneFixture();
    malformed.candidates.image_candidate_set_sha256 = "BAD";
    expect(() => parsePodmanDesktopEvidence(malformed)).toThrow(
      "invalid-image-candidate-set-sha256",
    );
    const wrongType = cloneFixture();
    wrongType.candidates.image_candidate_set_sha256 = 1;
    expect(() => parsePodmanDesktopEvidence(wrongType)).toThrow(
      "invalid-image-candidate-set-sha256",
    );
  });

  it.each([
    ["image_review_required", "yes", "invalid-image-review-required"],
    ["stopped_container_review_required", 1, "invalid-stopped-container-review-required"],
    ["volume_review_required", null, "invalid-volume-review-required"],
  ])("rejects invalid review boundary %s", (field, value, message) => {
    const invalid = cloneFixture();
    invalid.review_boundaries[field] = value;
    expect(() => parsePodmanDesktopEvidence(invalid)).toThrow(message);
  });

  it.each([
    ["physically_reclaimable_bytes", -1, "invalid-physically-reclaimable-bytes"],
    ["podman_reported_reclaimable_bytes", "1", "invalid-podman-reported-reclaimable-bytes"],
    ["raw_allocated_minus_guest_used_bytes", 1.5, "invalid-raw-allocated-minus-guest-used-bytes"],
  ])("rejects invalid assessment byte field %s", (field, value, message) => {
    const invalid = cloneFixture();
    invalid[field] = value;
    expect(() => parsePodmanDesktopEvidence(invalid)).toThrow(message);
  });
});

describe("loadPodmanEvidence", () => {
  it("uses the registered read-only command by default", async () => {
    invokeMock.mockResolvedValue(fixture());
    await expect(loadPodmanEvidence()).resolves.toMatchObject({ schema_version: 1 });
    expect(invokeMock).toHaveBeenCalledWith("inspect_podman_reclaim");
  });

  it("supports an injected invoker for deterministic contract tests", async () => {
    const injected = vi.fn().mockResolvedValue(fixture());
    await expect(loadPodmanEvidence(injected)).resolves.toMatchObject({ platform: "macos" });
    expect(injected).toHaveBeenCalledWith("inspect_podman_reclaim");
  });
});

describe("podmanEvidenceView", () => {
  it("labels complete evidence while keeping physical reclaim unknown", () => {
    const evidence = parsePodmanDesktopEvidence(fixture());
    expect(podmanEvidenceView(evidence)).toEqual({
      completeness_label: "증거 완전",
      completeness_tone: "complete",
      physical_reclaim_label: "검증되지 않음",
      image_review_label: "이미지 별도 검토 필요",
      container_review_label: "중지 컨테이너 별도 검토 필요",
      volume_review_label: "볼륨 별도 검토 필요",
      has_issues: true,
    });
  });

  it("labels partial evidence and keeps all review domains independent", () => {
    const value = cloneFixture();
    value.evidence_complete = false;
    value.physically_reclaimable_bytes = 12;
    value.review_boundaries.image_review_required = false;
    value.review_boundaries.stopped_container_review_required = false;
    value.review_boundaries.volume_review_required = false;
    value.issue_codes = [];
    const evidence = parsePodmanDesktopEvidence(value) as PodmanDesktopEvidence;
    expect(podmanEvidenceView(evidence)).toEqual({
      completeness_label: "부분 증거",
      completeness_tone: "partial",
      physical_reclaim_label: "12 bytes",
      image_review_label: "이미지 검토 신호 없음",
      container_review_label: "중지 컨테이너 검토 신호 없음",
      volume_review_label: "볼륨 검토 신호 없음",
      has_issues: false,
    });
  });
});
