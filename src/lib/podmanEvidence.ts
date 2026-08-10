import { invoke } from "@tauri-apps/api/core";

/** Stable schema kind emitted by the Rust desktop projection. */
export const PODMAN_DESKTOP_SCHEMA_KIND = "disksage.podman-desktop-evidence";

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
  platform: string;
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

type InvokeFunction = <T>(command: string) => Promise<T>;
type JsonRecord = Record<string, unknown>;

/**
 * Require a plain JSON object and reject arrays, null, and primitive values.
 *
 * @param value - Untrusted value received from the Tauri boundary.
 * @param label - Stable field label included in the fail-closed error code.
 * @returns The same value narrowed to a string-keyed JSON record.
 * @throws When the value is not a plain object-shaped record.
 */
function record(value: unknown, label: string): JsonRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`invalid-${label}`);
  }
  return value as JsonRecord;
}

/**
 * Require a string value from an untrusted response field.
 *
 * @param value - Candidate field value.
 * @param label - Stable field label included in the error code.
 * @returns The validated string.
 * @throws When the value is not a string.
 */
function stringValue(value: unknown, label: string): string {
  if (typeof value !== "string") throw new Error(`invalid-${label}`);
  return value;
}

/**
 * Require a boolean value from an untrusted response field.
 *
 * @param value - Candidate field value.
 * @param label - Stable field label included in the error code.
 * @returns The validated boolean.
 * @throws When the value is not a boolean.
 */
function booleanValue(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") throw new Error(`invalid-${label}`);
  return value;
}

/**
 * Require a non-negative JavaScript safe integer.
 *
 * Byte counts and record counts are rejected rather than rounded when Rust-to-JavaScript
 * serialization produces an unsafe, negative, fractional, or nonnumeric value.
 *
 * @param value - Candidate numeric field value.
 * @param label - Stable field label included in the error code.
 * @returns The validated unsigned safe integer.
 * @throws When the value cannot be represented exactly and safely in JavaScript.
 */
function unsignedInteger(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new Error(`invalid-${label}`);
  }
  return value;
}

/**
 * Preserve an explicitly unavailable observation as null or validate its unsigned value.
 *
 * @param value - Candidate field value, where null means the probe could not observe it.
 * @param label - Stable field label included in the error code.
 * @returns Null for an unavailable observation, otherwise a validated unsigned safe integer.
 * @throws When a non-null value is not a safe unsigned integer.
 */
function optionalUnsignedInteger(value: unknown, label: string): number | null {
  return value === null ? null : unsignedInteger(value, label);
}

/**
 * Require an array containing only strings and return a defensive copy.
 *
 * @param value - Candidate list value.
 * @param label - Stable field label included in the error code.
 * @returns A new array containing the validated strings.
 * @throws When the value is not a string-only array.
 */
function stringArray(value: unknown, label: string): string[] {
  if (!Array.isArray(value) || !value.every((item) => typeof item === "string")) {
    throw new Error(`invalid-${label}`);
  }
  return [...value];
}

/** Return true only for a bounded lowercase kebab-case code safe to cross the desktop boundary. */
function isStableCode(value: unknown): value is string {
  return typeof value === "string" && /^[a-z][a-z0-9-]{0,95}$/.test(value);
}

/**
 * Require a duplicate-free array of bounded lowercase kebab-case codes.
 *
 * @param value - Candidate assessment or issue-code list from the untrusted Tauri response.
 * @param label - Stable field label included in the fail-closed error code.
 * @returns A defensive copy of the validated stable codes.
 * @throws When a code is malformed, path-bearing, oversized, or duplicated.
 */
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

/** Require the only assessment status currently emitted by the Rust headless authority. */
function assessmentStatus(value: unknown): string {
  if (value !== "unverified") throw new Error("invalid-assessment-status");
  return value;
}

/**
 * Validate an optional lowercase SHA-256 commitment.
 *
 * @param value - Null when no candidate set was observed, otherwise the encoded digest.
 * @returns Null or a 64-character lowercase hexadecimal SHA-256 string.
 * @throws When a supplied fingerprint is malformed or uses a different encoding.
 */
function sha256OrNull(value: unknown): string | null {
  if (value === null) return null;
  const fingerprint = stringValue(value, "image-candidate-set-sha256");
  if (!/^[0-9a-f]{64}$/.test(fingerprint)) {
    throw new Error("invalid-image-candidate-set-sha256");
  }
  return fingerprint;
}

/**
 * Parse the capacity section while preserving every measurement as a distinct concept.
 *
 * @param value - Untrusted capacity object from the Rust response.
 * @returns Validated capacity observations with unavailable values preserved as null.
 * @throws When the section or any member violates the versioned desktop contract.
 */
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

/**
 * Parse logical cleanup candidates without treating them as verified physical savings.
 *
 * @param value - Untrusted candidate object from the Rust response.
 * @returns Validated candidate counts, byte observations, and optional set commitment.
 * @throws When a candidate field violates its type, range, or fingerprint contract.
 */
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

/**
 * Parse independent review requirements for images, stopped containers, and volumes.
 *
 * @param value - Untrusted review-boundary object from the Rust response.
 * @returns Three validated booleans that remain advisory and mutually non-authorizing.
 * @throws When any review boundary is absent or not boolean.
 */
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

/** Parse the Rust response and fail closed on schema, type, range, or fingerprint drift. */
export function parsePodmanDesktopEvidence(value: unknown): PodmanDesktopEvidence {
  const evidence = record(value, "podman-desktop-evidence");
  if (evidence.schema_kind !== PODMAN_DESKTOP_SCHEMA_KIND) {
    throw new Error("unsupported-podman-desktop-schema-kind");
  }
  if (evidence.schema_version !== 1) {
    throw new Error("unsupported-podman-desktop-schema-version");
  }
  const assessment_status = assessmentStatus(evidence.assessment_status);
  const physically_reclaimable_bytes = optionalUnsignedInteger(
    evidence.physically_reclaimable_bytes,
    "physically-reclaimable-bytes",
  );
  if (assessment_status === "unverified" && physically_reclaimable_bytes !== null) {
    throw new Error("unverified-physical-reclaim-claim");
  }
  return {
    schema_kind: PODMAN_DESKTOP_SCHEMA_KIND,
    schema_version: 1,
    platform: stringValue(evidence.platform, "platform"),
    evidence_complete: booleanValue(evidence.evidence_complete, "evidence-complete"),
    elapsed_ms: unsignedInteger(evidence.elapsed_ms, "elapsed-ms"),
    capacity: parseCapacity(evidence.capacity),
    candidates: parseCandidates(evidence.candidates),
    review_boundaries: parseReviewBoundaries(evidence.review_boundaries),
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
    issue_codes: stableCodeArray(evidence.issue_codes, "issue-codes"),
    notices: stringArray(evidence.notices, "notices"),
  };
}

/** Invoke the read-only Tauri command and validate the returned contract. */
export async function loadPodmanEvidence(
  invokeFunction: InvokeFunction = invoke,
): Promise<PodmanDesktopEvidence> {
  return parsePodmanDesktopEvidence(
    await invokeFunction<unknown>("inspect_podman_reclaim"),
  );
}

/** Derive stable user-facing state labels without granting any cleanup authority. */
export function podmanEvidenceView(evidence: PodmanDesktopEvidence): PodmanEvidenceView {
  return {
    completeness_label: evidence.evidence_complete ? "증거 완전" : "부분 증거",
    completeness_tone: evidence.evidence_complete ? "complete" : "partial",
    physical_reclaim_label:
      evidence.physically_reclaimable_bytes === null
        ? "검증되지 않음"
        : `${evidence.physically_reclaimable_bytes} bytes`,
    image_review_label: evidence.review_boundaries.image_review_required
      ? "이미지 별도 검토 필요"
      : "이미지 검토 신호 없음",
    container_review_label: evidence.review_boundaries.stopped_container_review_required
      ? "중지 컨테이너 별도 검토 필요"
      : "중지 컨테이너 검토 신호 없음",
    volume_review_label: evidence.review_boundaries.volume_review_required
      ? "볼륨 별도 검토 필요"
      : "볼륨 검토 신호 없음",
    has_issues: evidence.issue_codes.length > 0,
  };
}
