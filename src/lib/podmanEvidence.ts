import type {
  PodmanReclaimPlan,
  PodmanRecommendedActionKind,
} from "./podmanApi";

/** One byte-valued metric rendered by the Podman evidence panel. */
export interface PodmanEvidenceMetric {
  /** Stable metric key suitable for keyed UI rendering. */
  key: string;
  /** Human-readable metric label. */
  label: string;
  /** Byte value, or null when the evidence channel is unavailable. */
  bytes: number | null;
  /** Clarifies whether this is configuration, observation, a logical candidate, or proof. */
  evidence_class: "configured" | "observed" | "logical_candidate" | "physical_proof";
}

/** One independently reviewed Podman candidate category. */
export interface PodmanCandidateCategory {
  /** Stable category identifier. */
  kind: "images" | "stopped_containers" | "volumes";
  /** Human-readable category label. */
  label: string;
  /** Podman-reported logical candidate bytes. */
  bytes: number | null;
  /** Every future mutation path requires a distinct approval. */
  requires_separate_approval: true;
}

const SAFE_ISSUE_CODE = /^[a-z0-9][a-z0-9-]{0,95}$/u;
const SHA256_HEX = /^[a-f0-9]{64}$/u;

/** Return a stable localized label for a backend recommendation code. */
export function podmanActionLabel(kind: PodmanRecommendedActionKind): string {
  switch (kind) {
    case "restore_guest_headroom":
      return "게스트 최소 여유 확보 검토";
    case "investigate_api":
      return "Podman API 상태 조사";
    case "review_guest_trim":
      return "게스트 TRIM 전후 관측 검토";
    case "review_stopped_containers":
      return "중지 컨테이너 검토";
    case "review_unused_images":
      return "미사용 이미지 검토";
    case "review_unused_volumes":
      return "미사용 볼륨 검토";
  }
}

/** Reduce a potentially detailed backend issue string to a path-free stable code. */
export function safePodmanIssueCode(issue: string): string {
  const code = issue.split(":", 1)[0].trim();
  return SAFE_ISSUE_CODE.test(code) ? code : "podman-evidence-error";
}

/** Return sorted, de-duplicated, path-free issue codes for local presentation. */
export function podmanIssueCodes(plan: PodmanReclaimPlan): string[] {
  return [...new Set(plan.issues.map(safePodmanIssueCode))].sort();
}

/** Return the redacted exact candidate-set fingerprint only when it is valid SHA-256 hex. */
export function podmanCandidateFingerprint(plan: PodmanReclaimPlan): string | null {
  const fingerprint = plan.unused_images?.candidate_set_sha256 ?? "";
  return SHA256_HEX.test(fingerprint) ? fingerprint : null;
}

/** Build the evidence rows shown by the desktop without exposing local names or paths. */
export function podmanEvidenceMetrics(plan: PodmanReclaimPlan): PodmanEvidenceMetric[] {
  return [
    { key: "configured_disk_bytes", label: "VM 설정 디스크 용량", bytes: plan.machine?.configured_disk_bytes ?? null, evidence_class: "configured" },
    { key: "raw_logical_bytes", label: "raw 이미지 논리 크기", bytes: plan.raw_image?.logical_bytes ?? null, evidence_class: "observed" },
    { key: "raw_allocated_bytes", label: "raw 이미지 호스트 할당", bytes: plan.raw_image?.allocated_bytes ?? null, evidence_class: "observed" },
    { key: "guest_total_bytes", label: "게스트 파일시스템 전체", bytes: plan.guest_filesystem?.total_bytes ?? null, evidence_class: "observed" },
    { key: "guest_used_bytes", label: "게스트 파일시스템 사용", bytes: plan.guest_filesystem?.used_bytes ?? null, evidence_class: "observed" },
    { key: "guest_available_bytes", label: "게스트 파일시스템 여유", bytes: plan.guest_filesystem?.available_bytes ?? null, evidence_class: "observed" },
    { key: "graph_root_allocated_bytes", label: "Podman graph root 할당", bytes: plan.store?.graph_root_allocated_bytes ?? null, evidence_class: "observed" },
    { key: "graph_root_used_bytes", label: "Podman graph root 사용", bytes: plan.store?.graph_root_used_bytes ?? null, evidence_class: "observed" },
    { key: "image_candidate_bytes", label: "이미지 논리 후보", bytes: plan.system_df?.images.reclaimable_bytes ?? null, evidence_class: "logical_candidate" },
    { key: "stopped_container_candidate_bytes", label: "중지 컨테이너 논리 후보", bytes: plan.system_df?.containers.reclaimable_bytes ?? null, evidence_class: "logical_candidate" },
    { key: "volume_candidate_bytes", label: "볼륨 논리 후보", bytes: plan.system_df?.local_volumes.reclaimable_bytes ?? null, evidence_class: "logical_candidate" },
    { key: "physically_reclaimable_bytes", label: "호스트 물리 회수량", bytes: plan.assessment.physically_reclaimable_bytes, evidence_class: "physical_proof" },
  ];
}

/** Keep image, stopped-container, and volume approvals as distinct candidate categories. */
export function podmanCandidateCategories(plan: PodmanReclaimPlan): PodmanCandidateCategory[] {
  return [
    { kind: "images", label: "이미지 검토 후보", bytes: plan.system_df?.images.reclaimable_bytes ?? null, requires_separate_approval: true },
    { kind: "stopped_containers", label: "중지 컨테이너 검토 후보", bytes: plan.system_df?.containers.reclaimable_bytes ?? null, requires_separate_approval: true },
    { kind: "volumes", label: "볼륨 검토 후보", bytes: plan.system_df?.local_volumes.reclaimable_bytes ?? null, requires_separate_approval: true },
  ];
}
