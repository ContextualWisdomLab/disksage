import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  PODMAN_DESKTOP_SCHEMA_KIND,
  hasActionableReasonCodes,
  loadPodmanEvidence,
  parsePodmanDesktopEvidence,
  podmanEvidenceView,
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
    issue_codes: [],
    notices: [
      "Podman-reported logical candidates are not verified host physical reclaimability.",
      "This desktop surface exposes no prune, remove, machine lifecycle, TRIM, or raw-image mutation command.",
    ],
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
  });

  it("preserves unknown observations as null", () => {
    const value = cloneFixture();
    for (const key of Object.keys(value.capacity)) value.capacity[key] = null;
    value.physically_reclaimable_bytes = null;
    value.podman_reported_reclaimable_bytes = null;
    value.raw_allocated_minus_guest_used_bytes = null;
    const parsed = parsePodmanDesktopEvidence(value);
    expect(Object.values(parsed.capacity).every((entry) => entry === null)).toBe(true);
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

  it("rejects unsupported or path-bearing platform values", () => {
    const pathBearing = cloneFixture();
    pathBearing.platform = "/Users/alice/private-machine";
    expect(() => parsePodmanDesktopEvidence(pathBearing)).toThrow("invalid-platform");

    const unsupported = cloneFixture();
    unsupported.platform = "plan9";
    expect(() => parsePodmanDesktopEvidence(unsupported)).toThrow("invalid-platform");
  });

  it("rejects complete evidence that also carries issue codes", () => {
    const inconsistent = cloneFixture();
    inconsistent.issue_codes = ["partial-evidence"];
    expect(() => parsePodmanDesktopEvidence(inconsistent)).toThrow(
      "inconsistent-evidence-completeness",
    );
  });

  it("rejects malformed candidate fingerprints", () => {
    const malformed = cloneFixture();
    malformed.candidates.image_candidate_set_sha256 = "BAD";
    expect(() => parsePodmanDesktopEvidence(malformed)).toThrow(
      "invalid-image-candidate-set-sha256",
    );
  });

  it("rejects complete exact-image evidence without its candidate-set fingerprint", () => {
    const inconsistent = cloneFixture();
    inconsistent.candidates.image_candidate_set_sha256 = null;
    expect(() => parsePodmanDesktopEvidence(inconsistent)).toThrow(
      "inconsistent-image-candidate-fingerprint",
    );
  });

  it("rejects fingerprints that have no exact image-record observation", () => {
    const inconsistent = cloneFixture();
    inconsistent.evidence_complete = false;
    inconsistent.issue_codes = ["partial-evidence"];
    inconsistent.candidates.unused_image_records = null;
    expect(() => parsePodmanDesktopEvidence(inconsistent)).toThrow(
      "inconsistent-image-candidate-fingerprint",
    );
  });

  it("rejects candidate domains whose mandatory review boundary is false", () => {
    const image = cloneFixture();
    image.review_boundaries.image_review_required = false;
    expect(() => parsePodmanDesktopEvidence(image)).toThrow(
      "inconsistent-image-review-boundary",
    );

    const container = cloneFixture();
    container.review_boundaries.stopped_container_review_required = false;
    expect(() => parsePodmanDesktopEvidence(container)).toThrow(
      "inconsistent-stopped-container-review-boundary",
    );

    const volume = cloneFixture();
    volume.review_boundaries.volume_review_required = false;
    expect(() => parsePodmanDesktopEvidence(volume)).toThrow(
      "inconsistent-volume-review-boundary",
    );
  });

  it("rejects physical reclaim claims while the only supported assessment is unverified", () => {
    const inconsistent = cloneFixture();
    inconsistent.physically_reclaimable_bytes = 1;
    expect(() => parsePodmanDesktopEvidence(inconsistent)).toThrow(
      "unverified-physical-reclaim-claim",
    );
  });

  it("rejects path-bearing or noncanonical notices before the UI boundary", () => {
    const pathBearing = cloneFixture();
    pathBearing.notices = ["Podman socket /Users/alice/.local/share/podman.sock failed"];
    expect(() => parsePodmanDesktopEvidence(pathBearing)).toThrow("invalid-notices");

    const duplicate = cloneFixture();
    duplicate.notices = [duplicate.notices[0], duplicate.notices[0]];
    expect(() => parsePodmanDesktopEvidence(duplicate)).toThrow("invalid-notices");
  });
});

describe("loadPodmanEvidence", () => {
  it("uses the registered read-only command by default", async () => {
    invokeMock.mockResolvedValue(fixture());
    await expect(loadPodmanEvidence()).resolves.toMatchObject({ schema_version: 1 });
    expect(invokeMock).toHaveBeenCalledWith("inspect_podman_desktop_evidence");
  });
});

describe("podmanEvidenceView", () => {
  it("labels complete evidence while keeping physical reclaim unknown", () => {
    const evidence = parsePodmanDesktopEvidence(fixture());
    expect(podmanEvidenceView(evidence)).toMatchObject({
      completeness_label: "확인 완료",
      completeness_tone: "complete",
      physical_reclaim_label: "검증되지 않음",
      image_review_label: "이미지 별도 검토 필요",
      container_review_label: "중지된 작업 별도 확인 필요",
      volume_review_label: "저장 공간 별도 확인 필요",
      has_issues: false,
    });
  });

  it("labels partial evidence without candidate review signals and surfaces issue presence", () => {
    const value = cloneFixture();
    value.evidence_complete = false;
    value.issue_codes = ["partial-evidence"];
    value.candidates.image_candidate_bytes = 0;
    value.candidates.stopped_container_candidate_bytes = 0;
    value.candidates.volume_candidate_bytes = 0;
    value.candidates.unused_image_records = 0;
    value.candidates.stopped_container_records = 0;
    value.review_boundaries.image_review_required = false;
    value.review_boundaries.stopped_container_review_required = false;
    value.review_boundaries.volume_review_required = false;

    const evidence = parsePodmanDesktopEvidence(value);
    expect(podmanEvidenceView(evidence)).toMatchObject({
      completeness_label: "확인 불완전",
      completeness_tone: "partial",
      image_review_label: "이미지 검토 신호 없음",
      container_review_label: "중지된 작업 확인 사항 없음",
      volume_review_label: "저장 공간 확인 사항 없음",
      has_issues: true,
    });
  });

  it("does not flag the standing physical-reclaim notice as an action", () => {
    const evidence = parsePodmanDesktopEvidence(fixture());
    expect(hasActionableReasonCodes(evidence)).toBe(false);
    evidence.reason_codes.push("podman-api-evidence-missing");
    expect(hasActionableReasonCodes(evidence)).toBe(true);
  });
});
