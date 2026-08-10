import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  PODMAN_DESKTOP_SCHEMA_KIND,
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

  it("rejects malformed candidate fingerprints", () => {
    const malformed = cloneFixture();
    malformed.candidates.image_candidate_set_sha256 = "BAD";
    expect(() => parsePodmanDesktopEvidence(malformed)).toThrow(
      "invalid-image-candidate-set-sha256",
    );
  });
});

describe("loadPodmanEvidence", () => {
  it("uses the registered read-only command by default", async () => {
    invokeMock.mockResolvedValue(fixture());
    await expect(loadPodmanEvidence()).resolves.toMatchObject({ schema_version: 1 });
    expect(invokeMock).toHaveBeenCalledWith("inspect_podman_reclaim");
  });
});

describe("podmanEvidenceView", () => {
  it("labels complete evidence while keeping physical reclaim unknown", () => {
    const evidence = parsePodmanDesktopEvidence(fixture());
    expect(podmanEvidenceView(evidence)).toMatchObject({
      completeness_label: "증거 완전",
      physical_reclaim_label: "검증되지 않음",
      image_review_label: "이미지 별도 검토 필요",
      container_review_label: "중지 컨테이너 별도 검토 필요",
      volume_review_label: "볼륨 별도 검토 필요",
    });
  });
});
