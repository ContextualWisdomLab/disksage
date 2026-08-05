import { invoke } from "@tauri-apps/api/core";

/** Stable schema identifier emitted by the Rust Podman evidence engine. */
export type PodmanReclaimSchemaKind = "disksage.podman-reclaim-plan";

/** Bounded evidence about the selected Podman virtual machine. */
export interface PodmanMachineEvidence {
  /** Local machine name. This value is local-only and must never enter telemetry or remote logs. */
  name: string;
  /** Provider-reported lifecycle state such as `running` or `stopped`. */
  state: string;
  /** Configured virtual disk capacity, not observed allocation. */
  configured_disk_bytes: number | null;
}

/** Host-side observations for the Podman raw-image file. */
export interface PodmanRawImageEvidence {
  /** Local raw-image path. This value is local-only and is intentionally not rendered. */
  path: string;
  /** File logical length, which is not physical allocation or reclaim proof. */
  logical_bytes: number;
  /** Observed host allocated blocks when supported by the platform. */
  allocated_bytes: number | null;
}

/** Byte counters reported from the Podman guest filesystem. */
export interface PodmanGuestFilesystemEvidence {
  /** Total guest filesystem bytes. */
  total_bytes: number;
  /** Used guest filesystem bytes. */
  used_bytes: number;
  /** Available guest filesystem bytes. */
  available_bytes: number;
}

/** Bounded store counters returned by `podman info`. */
export interface PodmanStoreEvidence {
  /** Local graph-root path. This value is local-only and is intentionally not rendered. */
  graph_root: string;
  /** Podman-reported graph-root allocation. */
  graph_root_allocated_bytes: number;
  /** Podman-reported graph-root used bytes. */
  graph_root_used_bytes: number;
  /** Number of image records in the store. */
  images: number;
  /** Total number of containers. */
  containers_total: number;
  /** Number of running containers. */
  containers_running: number;
  /** Number of stopped containers. */
  containers_stopped: number;
}

/** One logical candidate category reported by `podman system df`. */
export interface PodmanSystemDfCategoryEvidence {
  /** Total records in this category. */
  total: number;
  /** Active records in this category. */
  active: number;
  /** Podman-reported logical/shared size. */
  size_bytes: number;
  /** Podman-reported logical candidate bytes, not host physical reclaim proof. */
  reclaimable_bytes: number;
}

/** Logical image, container, and volume candidate categories. */
export interface PodmanSystemDfEvidence {
  /** Image candidate evidence. */
  images: PodmanSystemDfCategoryEvidence;
  /** Container candidate evidence. */
  containers: PodmanSystemDfCategoryEvidence;
  /** Local-volume candidate evidence. */
  local_volumes: PodmanSystemDfCategoryEvidence;
}

/** Redacted exact-set evidence for unused image records. */
export interface PodmanUnusedImageEvidence {
  /** Total image records examined. */
  total_records: number;
  /** Image records referenced by at least one container. */
  referenced_records: number;
  /** Exact image records with zero container references. */
  unused_records: number;
  /** Unused records without tags. */
  unused_untagged_records: number;
  /** Unused records retaining one or more tags. */
  unused_tagged_records: number;
  /** Sum of record sizes; shared layers make this non-additive physical evidence. */
  candidate_record_size_sum: number;
  /** SHA-256 binding exact image IDs, sorted tags, and sizes without exposing them. */
  candidate_set_sha256: string;
}

/** Stable recommended-action identifiers returned by the Rust assessment. */
export type PodmanRecommendedActionKind =
  | "restore_guest_headroom"
  | "investigate_api"
  | "review_guest_trim"
  | "review_stopped_containers"
  | "review_unused_images"
  | "review_unused_volumes";

/** Read-only recommendation; the desktop integration does not execute the action. */
export interface PodmanRecommendedAction {
  /** Stable action identifier. */
  kind: PodmanRecommendedActionKind;
  /** Whether a separate future workflow would require explicit human approval. */
  requires_human_approval: boolean;
  /** Path-free backend rationale intended for local display. */
  rationale: string;
}

/** Assessment that keeps logical candidates distinct from verified host physical reclaim. */
export interface PodmanReclaimAssessment {
  /** Remains null until before/after host free-space evidence proves physical reclaim. */
  physically_reclaimable_bytes: number | null;
  /** Sum of Podman-reported logical candidate categories. */
  podman_reported_reclaimable_bytes: number | null;
  /** Observed host allocation minus guest used bytes; this is not reclaim proof. */
  raw_allocated_minus_guest_used_bytes: number | null;
  /** Stable assessment status. */
  status: string;
  /** Stable reason codes. */
  reason_codes: string[];
  /** Read-only next-step recommendations. */
  recommended_actions: PodmanRecommendedAction[];
}

/** Complete read-only Podman reclaim evidence report returned by Rust. */
export interface PodmanReclaimPlan {
  /** Stable schema kind. */
  schema_kind: PodmanReclaimSchemaKind;
  /** Schema version. */
  schema_version: number;
  /** Platform identifier used by the probe. */
  platform: string;
  /** Whether every required evidence channel was collected successfully. */
  evidence_complete: boolean;
  /** Probe duration in milliseconds. */
  elapsed_ms: number;
  /** Machine evidence when available. */
  machine: PodmanMachineEvidence | null;
  /** Raw-image evidence when available. */
  raw_image: PodmanRawImageEvidence | null;
  /** Guest filesystem evidence when available. */
  guest_filesystem: PodmanGuestFilesystemEvidence | null;
  /** Podman store evidence when available. */
  store: PodmanStoreEvidence | null;
  /** Logical candidate categories when available. */
  system_df: PodmanSystemDfEvidence | null;
  /** Redacted exact unused-image candidate-set evidence when available. */
  unused_images: PodmanUnusedImageEvidence | null;
  /** Fail-closed assessment. */
  assessment: PodmanReclaimAssessment;
  /** Bounded issue strings; presentation code must reduce these to stable issue codes. */
  issues: string[];
}

/** Request a read-only Podman reclaim report from the Tauri backend. */
export const podmanReclaimPlan = (machine?: string) =>
  invoke<PodmanReclaimPlan>("podman_reclaim_plan", { machine: machine ?? null });
