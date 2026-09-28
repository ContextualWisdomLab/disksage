import { invoke } from "@tauri-apps/api/core";

/** Stable schema kind emitted by the Rust desktop projection. */
export const PODMAN_DESKTOP_SCHEMA_KIND = "disksage.podman-desktop-evidence";

/** Exact privacy-safe notices emitted by schema version 1. */
const PODMAN_DESKTOP_NOTICES = [
  "Podman-reported logical candidates are not verified host physical reclaimability.",
  "This desktop surface exposes no prune, remove, machine lifecycle, TRIM, or raw-image mutation command.",
] as const;

/** Desktop operating-system identifiers supported by the Tauri application. */
export type PodmanDesktopPlatform = "linux" | "macos" | "windows";

/** Nullable byte value used when an observation could not be collected. */
export type OptionalBytes = number | null;

/** Capacity observations whose meanings must remain visually separate. */
export interface PodmanDesktopCapacityEvidence {
  configured_disk_bytes: OptionalBytes;
  raw_logical_bytes: OptionalBytes;
  host_allocated_bytes: OptionalBytes;
  guest_total_bytes: OptionalBytes;
  guest_used_bytes: OptionalBytes;
  guest_available_bytes: OptionalBytes;
  graph_root_allocated_bytes: OptionalBytes;
  graph_root_used_bytes: OptionalBytes;
}

/** Logical Podman candidates that are not verified host physical reclaimability. */
export interface PodmanDesktopCandidateEvidence {
  image_candidate_bytes: OptionalBytes;
  stopped_container_candidate_bytes: OptionalBytes;
  volume_candidate_bytes: OptionalBytes;
  unused_image_records: number | null;
  stopped_container_records: number | null;
  image_candidate_set_sha256: string | null;
}

/** Separate human-review boundaries for each Podman object class. */
export interface PodmanDesktopReviewBoundaries {
  image_review_required: boolean;
  stopped_container_review_required: boolean;
  volume_review_required: boolean;
}

/** Privacy-safe, read-only Podman evidence returned by the Tauri command. */
export interface PodmanDesktopEvidence {
  schema_kind: typeof PODMAN_DESKTOP_SCHEMA_KIND;
  schema_version: 1;
  platform: PodmanDesktopPlatform;
  evidence_complete: boolean;
  elapsed_ms: number;
  capacity: PodmanDesktopCapacityEvidence;
  candidates: PodmanDesktopCandidateEvidence;
  review_boundaries: PodmanDesktopReviewBoundaries;
  physically_reclaimable_bytes: OptionalBytes;
  podman_reported_reclaimable_bytes: OptionalBytes;
  raw_allocated_minus_guest_used_bytes: OptionalBytes;
  assessment_status: string;
  reason_codes: string[];
  issue_codes: string[];
  notices: string[];
}

/** Display model used by the Svelte component and its headless behavior tests. */
export interface PodmanEvidenceView {
  completeness_label: string;
  completeness_tone: "complete" | "partial";
  physical_reclaim_label: string;
  image_review_label: string;
  container_review_label: string;
  volume_review_label: string;
  has_issues: boolean;
}

/** Return whether the evidence contains a reason that requires a fresh customer review. */
export function hasActionableReasonCodes(
  evidence: Pick<PodmanDesktopEvidence, "reason_codes">,
): boolean {
  return evidence.reason_codes.some((code) => code !== "host-physical-reclaim-unverified");
}

type InvokeFunction = <T>(command: string) => Promise<T>;
type JsonRecord = Record<string, unknown>;

/** Require a plain JSON object and reject arrays, null, and primitive values. */
function record(value: unknown, label: string): JsonRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`invalid-${label}`);
  }
  return value as JsonRecord;
}

/** Require a string value from an untrusted response field. */
function stringValue(value: unknown, label: string): string {
  if (typeof value !== "string") throw new Error(`invalid-${label}`);
  return value;
}

/** Require one supported desktop operating-system identifier. */
function platformValue(value: unknown): PodmanDesktopPlatform {
  if (value !== "linux" && value !== "macos" && value !== "windows") {
    throw new Error("invalid-platform");
  }
  return value;
}

/** Require a boolean value from an untrusted response field. */
function booleanValue(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") throw new Error(`invalid-${label}`);
  return value;
}

/** Require a non-negative JavaScript safe integer. */
function unsignedInteger(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new Error(`invalid-${label}`);
  }
  return value;
}

/** Preserve an unavailable observation as null or validate its unsigned value. */
function optionalUnsignedInteger(value: unknown, label: string): number | null {
  return value === null ? null : unsignedInteger(value, label);
}

/** Require an array containing only strings and return a defensive copy. */
function stringArray(value: unknown, label: string): string[] {
  if (!Array.isArray(value) || !value.every((item) => typeof item === "string")) {
    throw new Error(`invalid-${label}`);
  }
  return [...value];
}

/** Require the exact schema-versioned notices rather than rendering arbitrary local text. */
function canonicalNotices(value: unknown): string[] {
  const notices = stringArray(value, "notices");
  if (
    notices.length !== PODMAN_DESKTOP_NOTICES.length ||
    notices.some((notice, index) => notice !== PODMAN_DESKTOP_NOTICES[index])
  ) {
    throw new Error("invalid-notices");
  }
  return [...PODMAN_DESKTOP_NOTICES];
}

/** Return true only for a bounded lowercase kebab-case code safe across IPC. */
function isStableCode(value: unknown): value is string {
  return typeof value === "string" && /^[a-z][a-z0-9-]{0,95}$/.test(value);
}

/** Require a duplicate-free array of bounded lowercase kebab-case codes. */
function stableCodeArray(value: unknown, label: string): string[] {
  if (
    !Array.isArray(value) ||
    !value.every(isStableCode) ||
    new Set(value).size !== value.length
  ) {
    throw new Error(`invalid-${label}`);
  }
  return [...value];
}

/** Require the only assessment status currently emitted by the Rust authority. */
function assessmentStatus(value: unknown): string {
  if (value !== "unverified") throw new Error("invalid-assessment-status");
  return value;
}

/** Validate an optional lowercase SHA-256 commitment. */
function sha256OrNull(value: unknown): string | null {
  if (value === null) return null;
  const fingerprint = stringValue(value, "image-candidate-set-sha256");
  if (!/^[0-9a-f]{64}$/.test(fingerprint)) {
    throw new Error("invalid-image-candidate-set-sha256");
  }
  return fingerprint;
}

/** Return true only when a nullable observation contains a positive value. */
function hasPositiveObservation(value: number | null): boolean {
  return value !== null && value > 0;
}

/** Parse the capacity section while preserving every measurement as a distinct concept. */
function parseCapacity(value: unknown): PodmanDesktopCapacityEvidence {
  const capacity = record(value, "podman-capacity");
  return {
    configured_disk_bytes: optionalUnsignedInteger(
      capacity.configured_disk_bytes,
      "configured-disk-bytes",
    ),
    raw_logical_bytes: optionalUnsignedInteger(capacity.raw_logical_bytes, "raw-logical-bytes"),
    host_allocated_bytes: optionalUnsignedInteger(
      capacity.host_allocated_bytes,
      "host-allocated-bytes",
    ),
    guest_total_bytes: optionalUnsignedInteger(capacity.guest_total_bytes, "guest-total-bytes"),
    guest_used_bytes: optionalUnsignedInteger(capacity.guest_used_bytes, "guest-used-bytes"),
    guest_available_bytes: optionalUnsignedInteger(
      capacity.guest_available_bytes,
      "guest-available-bytes",
    ),
    graph_root_allocated_bytes: optionalUnsignedInteger(
      capacity.graph_root_allocated_bytes,
      "graph-root-allocated-bytes",
    ),
    graph_root_used_bytes: optionalUnsignedInteger(
      capacity.graph_root_used_bytes,
      "graph-root-used-bytes",
    ),
  };
}

/** Parse logical cleanup candidates without treating them as physical savings. */
function parseCandidates(value: unknown): PodmanDesktopCandidateEvidence {
  const candidates = record(value, "podman-candidates");
  return {
    image_candidate_bytes: optionalUnsignedInteger(
      candidates.image_candidate_bytes,
      "image-candidate-bytes",
    ),
    stopped_container_candidate_bytes: optionalUnsignedInteger(
      candidates.stopped_container_candidate_bytes,
      "stopped-container-candidate-bytes",
    ),
    volume_candidate_bytes: optionalUnsignedInteger(
      candidates.volume_candidate_bytes,
      "volume-candidate-bytes",
    ),
    unused_image_records: optionalUnsignedInteger(
      candidates.unused_image_records,
      "unused-image-records",
    ),
    stopped_container_records: optionalUnsignedInteger(
      candidates.stopped_container_records,
      "stopped-container-records",
    ),
    image_candidate_set_sha256: sha256OrNull(candidates.image_candidate_set_sha256),
  };
}

/** Parse independent review requirements for images, stopped containers, and volumes. */
function parseReviewBoundaries(value: unknown): PodmanDesktopReviewBoundaries {
  const boundaries = record(value, "podman-review-boundaries");
  return {
    image_review_required: booleanValue(
      boundaries.image_review_required,
      "image-review-required",
    ),
    stopped_container_review_required: booleanValue(
      boundaries.stopped_container_review_required,
      "stopped-container-review-required",
    ),
    volume_review_required: booleanValue(
      boundaries.volume_review_required,
      "volume-review-required",
    ),
  };
}

/**
 * Reject semantic contradictions between candidate evidence, fingerprints, and review domains.
 *
 * Complete exact-image evidence must include both an exact-record count and its set commitment.
 * Partial evidence may omit an invalid fingerprint while retaining safe counts, but a fingerprint
 * may never appear without the exact-record observation it commits to. Any positive candidate in
 * a domain conservatively requires its own review boundary; a review signal never authorizes a
 * different domain and remains advisory only.
 */
function validateCandidateConsistency(
  candidates: PodmanDesktopCandidateEvidence,
  reviewBoundaries: PodmanDesktopReviewBoundaries,
  evidenceComplete: boolean,
): void {
  const hasExactImageRecords = candidates.unused_image_records !== null;
  const hasImageFingerprint = candidates.image_candidate_set_sha256 !== null;
  if (
    (hasImageFingerprint && !hasExactImageRecords) ||
    (evidenceComplete && (!hasExactImageRecords || !hasImageFingerprint))
  ) {
    throw new Error("inconsistent-image-candidate-fingerprint");
  }

  if (
    (hasPositiveObservation(candidates.image_candidate_bytes) ||
      hasPositiveObservation(candidates.unused_image_records)) &&
    !reviewBoundaries.image_review_required
  ) {
    throw new Error("inconsistent-image-review-boundary");
  }
  if (
    (hasPositiveObservation(candidates.stopped_container_candidate_bytes) ||
      hasPositiveObservation(candidates.stopped_container_records)) &&
    !reviewBoundaries.stopped_container_review_required
  ) {
    throw new Error("inconsistent-stopped-container-review-boundary");
  }
  if (
    hasPositiveObservation(candidates.volume_candidate_bytes) &&
    !reviewBoundaries.volume_review_required
  ) {
    throw new Error("inconsistent-volume-review-boundary");
  }
}

/** Parse the Rust response and fail closed on schema, type, range, or semantic drift. */
export function parsePodmanDesktopEvidence(value: unknown): PodmanDesktopEvidence {
  const evidence = record(value, "podman-desktop-evidence");
  if (evidence.schema_kind !== PODMAN_DESKTOP_SCHEMA_KIND) {
    throw new Error("unsupported-podman-desktop-schema-kind");
  }
  if (evidence.schema_version !== 1) {
    throw new Error("unsupported-podman-desktop-schema-version");
  }
  const platform = platformValue(evidence.platform);
  const evidence_complete = booleanValue(evidence.evidence_complete, "evidence-complete");
  const issue_codes = stableCodeArray(evidence.issue_codes, "issue-codes");
  if (evidence_complete && issue_codes.length > 0) {
    throw new Error("inconsistent-evidence-completeness");
  }
  const assessment_status = assessmentStatus(evidence.assessment_status);
  const physically_reclaimable_bytes = optionalUnsignedInteger(
    evidence.physically_reclaimable_bytes,
    "physically-reclaimable-bytes",
  );
  if (assessment_status === "unverified" && physically_reclaimable_bytes !== null) {
    throw new Error("unverified-physical-reclaim-claim");
  }
  const candidates = parseCandidates(evidence.candidates);
  const review_boundaries = parseReviewBoundaries(evidence.review_boundaries);
  validateCandidateConsistency(candidates, review_boundaries, evidence_complete);

  return {
    schema_kind: PODMAN_DESKTOP_SCHEMA_KIND,
    schema_version: 1,
    platform,
    evidence_complete,
    elapsed_ms: unsignedInteger(evidence.elapsed_ms, "elapsed-ms"),
    capacity: parseCapacity(evidence.capacity),
    candidates,
    review_boundaries,
    physically_reclaimable_bytes,
    podman_reported_reclaimable_bytes: optionalUnsignedInteger(
      evidence.podman_reported_reclaimable_bytes,
      "podman-reported-reclaimable-bytes",
    ),
    raw_allocated_minus_guest_used_bytes: optionalUnsignedInteger(
      evidence.raw_allocated_minus_guest_used_bytes,
      "raw-allocated-minus-guest-used-bytes",
    ),
    assessment_status,
    reason_codes: stableCodeArray(evidence.reason_codes, "reason-codes"),
    issue_codes,
    notices: canonicalNotices(evidence.notices),
  };
}

/** Invoke the read-only Tauri command and validate the returned contract. */
export async function loadPodmanEvidence(
  invokeFunction: InvokeFunction = invoke,
): Promise<PodmanDesktopEvidence> {
  return parsePodmanDesktopEvidence(
    await invokeFunction<unknown>("inspect_podman_desktop_evidence"),
  );
}

/** Derive stable user-facing state labels without granting any cleanup authority. */
export function podmanEvidenceView(evidence: PodmanDesktopEvidence): PodmanEvidenceView {
  return {
    completeness_label: evidence.evidence_complete ? "확인 완료" : "확인 불완전",
    completeness_tone: evidence.evidence_complete ? "complete" : "partial",
    physical_reclaim_label: "검증되지 않음",
    image_review_label: evidence.review_boundaries.image_review_required
      ? "이미지 별도 검토 필요"
      : "이미지 검토 신호 없음",
    container_review_label: evidence.review_boundaries.stopped_container_review_required
      ? "중지된 작업 별도 확인 필요"
      : "중지된 작업 확인 사항 없음",
    volume_review_label: evidence.review_boundaries.volume_review_required
      ? "저장 공간 별도 확인 필요"
      : "저장 공간 확인 사항 없음",
    has_issues: evidence.issue_codes.length > 0,
  };
}
