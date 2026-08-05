import { describe, expect, it } from "vitest";
import type { PodmanReclaimPlan } from "./podmanApi";
import {
  podmanActionLabel,
  podmanCandidateCategories,
  podmanCandidateFingerprint,
  podmanEvidenceMetrics,
  podmanIssueCodes,
  safePodmanIssueCode,
} from "./podmanEvidence";

const fingerprint = "a".repeat(64);

function completePlan(): PodmanReclaimPlan {
  return {
    schema_kind: "disksage.podman-reclaim-plan",
    schema_version: 3,
    platform: "darwin",
    evidence_complete: true,
    elapsed_ms: 250,
    machine: {
      name: "private-machine-name",
      state: "running",
      configured_disk_bytes: 1000,
    },
    raw_image: {
      path: "/Users/private/.local/share/podman/raw-image",
      logical_bytes: 900,
      allocated_bytes: 700,
    },
    guest_filesystem: {
      total_bytes: 800,
      used_bytes: 500,
      available_bytes: 300,
    },
    store: {
      graph_root: "/var/private/containers/storage",
      graph_root_allocated_bytes: 600,
      graph_root_used_bytes: 450,
      images: 12,
      containers_total: 7,
      containers_running: 2,
      containers_stopped: 5,
    },
    system_df: {
      images: { total: 12, active: 8, size_bytes: 400, reclaimable_bytes: 210 },
      containers: { total: 7, active: 2, size_bytes: 90, reclaimable_bytes: 40 },
      local_volumes: { total: 5, active: 3, size_bytes: 80, reclaimable_bytes: 30 },
    },
    unused_images: {
      total_records: 12,
      referenced_records: 8,
      unused_records: 4,
      unused_untagged_records: 3,
      unused_tagged_records: 1,
      candidate_record_size_sum: 180,
      candidate_set_sha256: fingerprint,
    },
    assessment: {
      physically_reclaimable_bytes: null,
      podman_reported_reclaimable_bytes: 280,
      raw_allocated_minus_guest_used_bytes: 200,
      status: "unverified",
      reason_codes: ["host-physical-reclaim-unverified"],
      recommended_actions: [
        {
          kind: "review_unused_images",
          requires_human_approval: true,
          rationale: "Review image candidates separately.",
        },
      ],
    },
    issues: [
      "podman-info-failed:/Users/private/socket",
      "guest-df-empty",
      "guest-df-empty",
    ],
  };
}

function emptyPlan(): PodmanReclaimPlan {
  return {
    schema_kind: "disksage.podman-reclaim-plan",
    schema_version: 3,
    platform: "linux",
    evidence_complete: false,
    elapsed_ms: 1,
    machine: null,
    raw_image: null,
    guest_filesystem: null,
    store: null,
    system_df: null,
    unused_images: null,
    assessment: {
      physically_reclaimable_bytes: null,
      podman_reported_reclaimable_bytes: null,
      raw_allocated_minus_guest_used_bytes: null,
      status: "unverified",
      reason_codes: [],
      recommended_actions: [],
    },
    issues: [],
  };
}

describe("Podman evidence presentation", () => {
  it("labels every stable recommendation without a generic fallback", () => {
    expect(podmanActionLabel("restore_guest_headroom")).toBe("게스트 최소 여유 확보 검토");
    expect(podmanActionLabel("investigate_api")).toBe("Podman API 상태 조사");
    expect(podmanActionLabel("review_guest_trim")).toBe("게스트 TRIM 전후 관측 검토");
    expect(podmanActionLabel("review_stopped_containers")).toBe("중지 컨테이너 검토");
    expect(podmanActionLabel("review_unused_images")).toBe("미사용 이미지 검토");
    expect(podmanActionLabel("review_unused_volumes")).toBe("미사용 볼륨 검토");
  });

  it("reduces detailed failures to bounded path-free issue codes", () => {
    expect(safePodmanIssueCode("podman-info-failed:/Users/private/socket")).toBe(
      "podman-info-failed",
    );
    expect(safePodmanIssueCode(" guest-df-empty ")).toBe("guest-df-empty");
    expect(safePodmanIssueCode("UPPERCASE detail")).toBe("podman-evidence-error");
    expect(safePodmanIssueCode("")).toBe("podman-evidence-error");
    expect(podmanIssueCodes(completePlan())).toEqual([
      "guest-df-empty",
      "podman-info-failed",
    ]);
  });

  it("returns only a valid redacted SHA-256 candidate fingerprint", () => {
    expect(podmanCandidateFingerprint(completePlan())).toBe(fingerprint);
    const invalid = completePlan();
    invalid.unused_images!.candidate_set_sha256 = "not-a-digest";
    expect(podmanCandidateFingerprint(invalid)).toBeNull();
    expect(podmanCandidateFingerprint(emptyPlan())).toBeNull();
  });

  it("keeps configured, observed, logical-candidate, and physical-proof metrics distinct", () => {
    const metrics = podmanEvidenceMetrics(completePlan());
    expect(metrics).toEqual([
      { key: "configured_disk_bytes", label: "VM 설정 디스크 용량", bytes: 1000, evidence_class: "configured" },
      { key: "raw_logical_bytes", label: "raw 이미지 논리 크기", bytes: 900, evidence_class: "observed" },
      { key: "raw_allocated_bytes", label: "raw 이미지 호스트 할당", bytes: 700, evidence_class: "observed" },
      { key: "guest_total_bytes", label: "게스트 파일시스템 전체", bytes: 800, evidence_class: "observed" },
      { key: "guest_used_bytes", label: "게스트 파일시스템 사용", bytes: 500, evidence_class: "observed" },
      { key: "guest_available_bytes", label: "게스트 파일시스템 여유", bytes: 300, evidence_class: "observed" },
      { key: "graph_root_allocated_bytes", label: "Podman graph root 할당", bytes: 600, evidence_class: "observed" },
      { key: "graph_root_used_bytes", label: "Podman graph root 사용", bytes: 450, evidence_class: "observed" },
      { key: "image_candidate_bytes", label: "이미지 논리 후보", bytes: 210, evidence_class: "logical_candidate" },
      { key: "stopped_container_candidate_bytes", label: "중지 컨테이너 논리 후보", bytes: 40, evidence_class: "logical_candidate" },
      { key: "volume_candidate_bytes", label: "볼륨 논리 후보", bytes: 30, evidence_class: "logical_candidate" },
      { key: "physically_reclaimable_bytes", label: "호스트 물리 회수량", bytes: null, evidence_class: "physical_proof" },
    ]);
    expect(podmanEvidenceMetrics(emptyPlan()).every((metric) => metric.bytes === null)).toBe(true);
  });

  it("keeps image, stopped-container, and volume review approvals separate", () => {
    expect(podmanCandidateCategories(completePlan())).toEqual([
      { kind: "images", label: "이미지 검토 후보", bytes: 210, requires_separate_approval: true },
      { kind: "stopped_containers", label: "중지 컨테이너 검토 후보", bytes: 40, requires_separate_approval: true },
      { kind: "volumes", label: "볼륨 검토 후보", bytes: 30, requires_separate_approval: true },
    ]);
    expect(podmanCandidateCategories(emptyPlan()).map((category) => category.bytes)).toEqual([
      null,
      null,
      null,
    ]);
  });
});
