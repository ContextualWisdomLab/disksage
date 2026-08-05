import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import type { PodmanReclaimPlan } from "./podmanApi";
import PodmanReclaimEvidence from "./PodmanReclaimEvidence.svelte";

function plan(overrides: Partial<PodmanReclaimPlan> = {}): PodmanReclaimPlan {
  return {
    schema_kind: "disksage.podman-reclaim-plan",
    schema_version: 3,
    platform: "darwin",
    evidence_complete: true,
    elapsed_ms: 250,
    machine: { name: "sensitive-machine-name", state: "running", configured_disk_bytes: 1000 },
    raw_image: { path: "/Users/alice/.local/share/containers/podman-machine.raw", logical_bytes: 900, allocated_bytes: 700 },
    guest_filesystem: { total_bytes: 800, used_bytes: 500, available_bytes: 300 },
    store: {
      graph_root: "/Users/alice/.local/share/containers/storage",
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
      candidate_set_sha256: "b".repeat(64),
    },
    assessment: {
      physically_reclaimable_bytes: null,
      podman_reported_reclaimable_bytes: 280,
      raw_allocated_minus_guest_used_bytes: 200,
      status: "unverified",
      reason_codes: ["host-physical-reclaim-unverified"],
      recommended_actions: [
        { kind: "review_unused_images", requires_human_approval: true, rationale: "이미지와 볼륨은 서로 다른 승인으로 검토합니다." },
      ],
    },
    issues: ["podman-info-failed:/Users/alice/private.sock"],
    ...overrides,
  };
}

describe("Podman reclaim evidence component", () => {
  it("renders distinct evidence classes and candidate approvals without local identifiers", () => {
    const { body } = render(PodmanReclaimEvidence, { props: { initialPlan: plan() } });

    expect(body).toContain("Podman VM 저장공간 증거");
    expect(body).toContain("VM 설정 디스크 용량");
    expect(body).toContain("raw 이미지 호스트 할당");
    expect(body).toContain("호스트 물리 회수량");
    expect(body).toContain("미검증");
    expect(body).toContain("이미지 검토 후보");
    expect(body).toContain("중지 컨테이너 검토 후보");
    expect(body).toContain("볼륨 검토 후보");
    expect(body).toContain("별도 사람 승인 필요");
    expect(body).toContain("b".repeat(64));
    expect(body).not.toContain("sensitive-machine-name");
    expect(body).not.toContain("/Users/alice/.local/share/containers/podman-machine.raw");
    expect(body).not.toContain("/Users/alice/.local/share/containers/storage");
    expect(body).not.toContain("/Users/alice/private.sock");
    expect(body).toContain("podman-info-failed");
  });

  it("renders a partial-evidence state without inventing unavailable byte values", () => {
    const partial = plan({
      evidence_complete: false,
      machine: null,
      raw_image: null,
      guest_filesystem: null,
      store: null,
      system_df: null,
      unused_images: null,
      issues: ["invalid issue with spaces"],
      assessment: {
        physically_reclaimable_bytes: null,
        podman_reported_reclaimable_bytes: null,
        raw_allocated_minus_guest_used_bytes: null,
        status: "unverified",
        reason_codes: ["partial-evidence"],
        recommended_actions: [],
      },
    });
    const { body } = render(PodmanReclaimEvidence, { props: { initialPlan: partial } });

    expect(body).toContain("증거 부분 수집");
    expect(body).toContain("관측 불가");
    expect(body).toContain("podman-evidence-error");
    expect(body).not.toContain("invalid issue with spaces");
  });

  it("starts with one read-only load control and no fabricated report", () => {
    const { body } = render(PodmanReclaimEvidence);

    expect(body).toContain("Podman 증거 확인");
    expect(body).toContain("읽기 전용 진단");
    expect(body).not.toContain("증거 완전");
    expect(body).not.toContain("정확한 미사용 이미지 후보 집합 SHA-256");
  });
});
