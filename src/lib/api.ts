import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ScanStats {
  files: number;
  dirs: number;
  skipped: number;
  bytes: number;
}
export interface EntryView {
  name: string;
  path: string;
  size: number;
  is_dir: boolean;
}
export interface NodeView {
  path: string;
  size: number;
  entries: EntryView[];
}

export const listRoots = () => invoke<string[]>("list_roots");
export const startScan = (root: string) => invoke<void>("start_scan", { root });
export const cancelScan = () => invoke<void>("cancel_scan");
export const getNode = (path: string) => invoke<NodeView>("get_node", { path });
export const topFiles = (limit = 200) => invoke<EntryView[]>("top_files", { limit });

export interface CacheCandidate {
  id: string;
  label: string;
  path: string;
  bytes: number;
  exists: boolean;
}
export interface CacheTarget {
  path: string;
  bytes: number;
  modified_ms: number;
  object_id: string;
}
export interface DevArtifact {
  path: string;
  kind: string;
  project: string;
  bytes: number;
  files: number;
  skipped: number;
  scan_complete: boolean;
  fingerprint: string;
  object_id: string;
  age_days: number;
}
export interface CleanResult {
  path: string;
  ok: boolean;
  error: string;
}

export interface OrphanRelationEvidence {
  subject: string;
  predicate: string;
  object: string;
  source: string;
}
export interface OrphanCandidate {
  candidate_id: string;
  kind: string;
  bundle_id: string | null;
  bytes: number;
  files: number;
  skipped: number;
  scan_complete: boolean;
  object_id: string;
  metadata_fingerprint: string;
  ontology_class: string;
  confidence: string;
  active_use_evidence_complete: boolean;
  active_use: boolean;
  relations: OrphanRelationEvidence[];
  review_reasons: string[];
  auto_trash_eligible: boolean;
}
export interface OrphanPlan {
  schema_kind: "disksage.orphan-plan/v1";
  schema_version: number;
  generated_at_ms: number;
  plan_fingerprint: string;
  candidate_count: number;
  candidate_bytes: number;
  scan_complete: boolean;
  candidates: OrphanCandidate[];
  notices: string[];
  local_paths_included: false;
  mutation_performed: false;
  exact_approval_phrase: string;
}
export interface OrphanCleanupRequest {
  candidate_id: string;
  metadata_fingerprint: string;
  bytes: number;
  files: number;
  skipped: number;
  scan_complete: boolean;
  object_id: string;
}
export interface OrphanCleanupItemResult {
  candidate_id: string;
  bytes: number;
  attempted: boolean;
  moved_to_trash: boolean;
  error: string | null;
}
export interface OrphanCleanupResult {
  schema_kind: "disksage.orphan-cleanup-result/v1";
  schema_version: number;
  plan_fingerprint: string;
  requested_count: number;
  moved_count: number;
  filesystem_mutation_executed: boolean;
  items: OrphanCleanupItemResult[];
  notices: string[];
}
export interface JournalEntry {
  ts_ms: number;
  op: string;
  path: string;
  bytes: number;
  outcome: string;
}
export interface DupeGroup {
  hash: string;
  size: number;
  paths: string[];
}

export const listCacheCandidates = () => invoke<CacheCandidate[]>("list_cache_candidates");
export const cleanRegenerableCaches = () =>
  invoke<CleanResult[]>("clean_regenerable_caches");
export const listCacheTargets = (dir: string) =>
  invoke<CacheTarget[]>("list_cache_targets", { dir });
export const cleanCacheContents = (dir: string, targets: CacheTarget[]) =>
  invoke<CleanResult[]>("clean_cache_contents", { dir, targets });
export const listDevArtifacts = (root: string, minAgeDays = 30) =>
  invoke<DevArtifact[]>("list_dev_artifacts", { root, minAgeDays });
export const cleanPaths = (paths: string[]) => invoke<CleanResult[]>("clean_paths", { paths });
export const cleanDevArtifacts = (root: string, minAgeDays: number, artifacts: DevArtifact[]) =>
  invoke<CleanResult[]>("clean_dev_artifacts", { root, minAgeDays, artifacts });
export const expandCleanTargets = (dir: string) =>
  invoke<string[]>("expand_clean_targets", { dir });
export const recentOperations = (limit = 20) =>
  invoke<JournalEntry[]>("recent_operations", { limit });
export const findDuplicateFiles = (root: string) =>
  invoke<DupeGroup[]>("find_duplicate_files", { root });
export const planOrphanCleanup = () => invoke<OrphanPlan>("plan_orphan_cleanup");
export const cleanOrphanCandidates = (
  planFingerprint: string,
  requests: OrphanCleanupRequest[],
  confirmationPhrase: string,
  rationale: string,
) => invoke<OrphanCleanupResult>("clean_orphan_candidates", {
  planFingerprint,
  requests,
  confirmationPhrase,
  rationale,
});

export interface PodmanReclaimPlan {
  schema_kind: "disksage.podman-reclaim-plan";
  schema_version: number;
  platform: string;
  evidence_complete: boolean;
  elapsed_ms: number;
  machine: { name: string; state: string; configured_disk_bytes: number | null } | null;
  guest_filesystem: { total_bytes: number; used_bytes: number; available_bytes: number } | null;
  system_df: {
    images: { total: number; active: number; size_bytes: number; reclaimable_bytes: number };
    containers: { total: number; active: number; size_bytes: number; reclaimable_bytes: number };
    local_volumes: { total: number; active: number; size_bytes: number; reclaimable_bytes: number };
  } | null;
  unused_images: {
    total_records: number;
    referenced_records: number;
    unused_records: number;
    unused_untagged_records: number;
    unused_tagged_records: number;
    candidate_record_size_sum: number;
    candidate_set_sha256: string;
  } | null;
  dangling_prune_approval_phrase: string | null;
  assessment: {
    physically_reclaimable_bytes: number | null;
    podman_reported_reclaimable_bytes: number | null;
    raw_allocated_minus_guest_used_bytes: number | null;
    status: string;
    reason_codes: string[];
    recommended_actions: Array<{
      kind: string;
      requires_human_approval: boolean;
      rationale: string;
    }>;
  };
  issues: string[];
}

export const inspectPodmanReclaim = () =>
  invoke<PodmanReclaimPlan>("inspect_podman_reclaim");

export interface PodmanDanglingImagePruneExecution {
  schema_version: number;
  candidate_set_sha256: string;
  command: string[];
  status_code: number;
  stdout: string;
  stderr: string;
  output_truncated: boolean;
  executed: boolean;
  executed_at_ms: number;
  before_available_bytes: number | null;
  after_available_bytes: number | null;
  observed_available_gain_bytes: number | null;
  rationale: string;
}

export const executePodmanDanglingImagePrune = (
  confirmationPhrase: string,
  rationale: string,
) => invoke<PodmanDanglingImagePruneExecution>("execute_podman_dangling_image_prune", {
  confirmationPhrase,
  rationale,
});

export type ContainerRuntimeKind =
  | "docker-native"
  | "docker-colima-context"
  | "podman-machine";

export type OrphanCategory = "container" | "image" | "volume" | "network";

export interface ContainerOrphanPlan {
  schema_kind: "disksage.container-orphan-plan";
  schema_version: number;
  platform: string;
  evidence_complete: boolean;
  elapsed_ms: number;
  runtime: {
    kind: ContainerRuntimeKind;
    display_name: string;
    healthy: boolean;
    detail_issue: string | null;
  };
  categories: Array<{
    category: OrphanCategory;
    evidence_complete: boolean;
    issue: string | null;
    evidence: {
      total_records: number;
      candidate_records: number;
      candidate_size_sum_bytes: number | null;
      candidate_set_sha256: string;
    } | null;
    approval_phrase: string | null;
    prune_command: string[] | null;
  }>;
  issues: string[];
}

export const inspectContainerOrphans = () =>
  invoke<ContainerOrphanPlan[]>("inspect_container_orphans");

export interface ContainerOrphanPruneExecution {
  schema_version: number;
  runtime_display_name: string;
  category: OrphanCategory;
  candidate_set_sha256: string;
  command: string[];
  status_code: number;
  stdout: string;
  stderr: string;
  output_truncated: boolean;
  executed: boolean;
  executed_at_ms: number;
  before_available_bytes: number | null;
  after_available_bytes: number | null;
  observed_available_gain_bytes: number | null;
  rationale: string;
}

export const executeContainerOrphanPrune = (
  runtimeKind: ContainerRuntimeKind,
  category: OrphanCategory,
  confirmationPhrase: string,
  rationale: string,
) => invoke<ContainerOrphanPruneExecution>("execute_container_orphan_prune", {
  runtimeKind,
  category,
  confirmationPhrase,
  rationale,
});

export const onScanProgress = (cb: (s: ScanStats) => void) =>
  listen<ScanStats>("scan://progress", (e) => cb(e.payload));
export const onScanDone = (cb: (s: ScanStats) => void) =>
  listen<ScanStats>("scan://done", (e) => cb(e.payload));

export interface ClassTally {
  class_id: string;
  label: string;
  bytes: number;
  count: number;
}
export interface InventoryReport {
  tallies: ClassTally[];
  unknown_bytes: number;
  unknown_count: number;
  unknown_samples: string[];
}
export interface OntoClass {
  id: string;
  label: string;
  parents: string[];
  equivalents: string[];
  disjoints: string[];
  target_folder: string | null;
}
export interface Ontology {
  classes: OntoClass[];
}

export const diskInventory = (root: string) =>
  invoke<InventoryReport>("disk_inventory", { root });
export const getOntology = () => invoke<Ontology>("get_ontology");

export type Issue = { UnsatisfiableClass: { class: string; via_disjoint: [string, string] } };
export const ontologyCoherence = () => invoke<Issue[]>("ontology_coherence");

export interface MovePlan {
  src: string;
  dst: string;
  class_id: string;
  source_size?: number | null;
  source_mtime_ms?: number | null;
  lineage?: {
    production_time_ms?: number | null;
    production_time_source?: string | null;
    production_time_confidence?: string | null;
    lineage_fingerprint: string;
  };
}

export const planOrganize = (root: string) =>
  invoke<MovePlan[]>("plan_organize", { root });

export interface OrganizationLineageItem {
  lineage_fingerprint: string;
  source_size: number;
  source_mtime_ms: number;
  production_time_ms: number;
  production_time_source: string;
  production_time_confidence: string;
  class_id: string;
  dst: string;
}
export interface OrganizationLineageExport {
  schema_version: number;
  generated_at_ms: number;
  manifest_fingerprint: string;
  source_paths_included: boolean;
  items: OrganizationLineageItem[];
}
export const exportOrganizationLineage = (root: string) =>
  invoke<OrganizationLineageExport>("export_organization_lineage", { root });

export const applyOrganize = (
  root: string,
  plans: MovePlan[],
  confirmationPhrase: string,
  rationale: string,
) => invoke<void>("apply_organize", { root, plans, confirmationPhrase, rationale });

export interface CloudPlanReport {
  version: number;
  created_at_ms: number;
  mode: "single-destination";
  local_volume: {
    filesystem_identity: string;
    total_bytes: number;
    available_bytes: number;
    critical: boolean;
  };
  cloud_roots: Array<{
    id: string;
    provider: "icloud" | "onedrive" | "google_drive";
    root: string;
    reachable: boolean;
  }>;
  candidates: Array<{
    src: string;
    dst: string;
    source_size: number;
    source_mtime_ms: number;
    lineage: {
      production_time_ms?: number | null;
      production_time_source?: string | null;
      production_time_confidence?: string | null;
      lineage_fingerprint: string;
    };
    destination_root_id: string;
  }>;
  skipped_candidates: Array<{
    src: string;
    reason: string;
  }>;
  local_pressure_notices: string[];
  notices: string[];
  non_destructive: true;
  mutation_performed: false;
}

export interface CloudCopyExecution {
  schema_version: number;
  copied_items: number;
  copied_bytes: number;
  filesystem_mutation_executed: boolean;
  provider_native_transfer?: boolean;
  provider_native_operation_id?: string | null;
  provider_native_status?: "queued" | "running" | "completed" | "failed" | null;
  provider_native_completed_bytes?: number | null;
  provider_native_total_bytes?: number | null;
  destination_filesystem_identity?: string | null;
  destination_available_bytes_before?: number | null;
  destination_required_headroom_bytes?: number | null;
  destination_available_bytes_after?: number | null;
  destination_headroom_verified?: boolean | null;
}

export const planCloudArchive = (sourceRoot: string) =>
  invoke<CloudPlanReport>("plan_cloud_archive", { sourceRoot });

export const executeCloudArchive = (
  sourceRoot: string,
  plan: CloudPlanReport,
  confirmationPhrase: string,
  rationale: string,
) => invoke<CloudCopyExecution>("execute_cloud_archive", {
  sourceRoot,
  plan,
  confirmationPhrase,
  rationale,
});

export const cancelCloudCopy = (operationId: string, rationale: string) =>
  invoke<void>("cancel_cloud_copy", { operationId, rationale });

export interface ProviderCloudRoot {
  id: string;
  provider: "icloud" | "onedrive" | "google_drive";
  root: string;
  account_scope: string;
  reachable: boolean;
  configured_bytes: number | null;
  available_bytes: number | null;
  evidence_complete: boolean;
  issue: string | null;
}

export interface ProviderCloudLocalEvidence {
  provider: ProviderCloudRoot["provider"];
  account_scope: string;
  cloud_root_id: string;
  cloud_root: string;
  allocated_bytes: number;
  allocation_candidates: number;
  evidence_complete: boolean;
  issues: string[];
  notices: string[];
  enumeration_truncated: boolean;
  timeout_truncated: boolean;
}

export interface ProviderGlobalSyncState {
  schema_version: number;
  provider: "onedrive" | "google_drive";
  account_scope: string;
  cloud_root_id: string;
  trigger_command: string;
  planned: boolean;
  triggered: boolean;
  changed: boolean;
  detail: string | null;
}

export interface CloudLocalEvictionExecution {
  schema_version: number;
  provider: ProviderCloudRoot["provider"];
  account_scope: string;
  cloud_root_id: string;
  relative_path: string;
  requested_bytes: number;
  operation: string;
  evidence_complete: boolean;
  executed: boolean;
  local_reclaim_observed: boolean;
  local_reclaim_bytes: number | null;
  notices: string[];
}

export interface BatchCloudLocalEvictionExecution {
  schema_version: number;
  cloud_root_id: string;
  requested_items: number;
  executed_items: number;
  skipped_items: number;
  observed_reclaimed_bytes: number | null;
  evidence_complete: boolean;
  notices: string[];
}

export const inspectProviderCloudRoots = () =>
  invoke<ProviderCloudRoot[]>("inspect_provider_cloud_roots");

export const inspectProviderCloudLocal = (
  cloudRootId: string,
  relativeSubpath: string | null = null,
  minAllocatedBytes = 64 * 1024 * 1024,
  maxEntries = 100_000,
  maxResults = 200,
  maxDepth = 12,
  maxDurationMs = 30_000,
  maxIssues = 50,
) => invoke<ProviderCloudLocalEvidence>("inspect_provider_cloud_local", {
  cloudRootId,
  relativeSubpath,
  minAllocatedBytes,
  maxEntries,
  maxResults,
  maxDepth,
  maxDurationMs,
  maxIssues,
});

export const triggerProviderGlobalSync = (cloudRootId: string) =>
  invoke<ProviderGlobalSyncState>("trigger_provider_global_sync", { cloudRootId });

export const evictProviderCloudLocal = (
  cloudRootId: string,
  relativePath: string,
  expectedBytes: number,
  expectedIdentity: string,
  confirmationPhrase: string,
  rationale: string,
) => invoke<CloudLocalEvictionExecution>("evict_provider_cloud_local", {
  cloudRootId,
  relativePath,
  expectedBytes,
  expectedIdentity,
  confirmationPhrase,
  rationale,
});

export const evictProviderCloudLocalBatch = (
  cloudRootId: string,
  requests: Array<{
    relative_path: string;
    expected_bytes: number;
    expected_identity: string;
  }>,
  confirmationPhrase: string,
  rationale: string,
) => invoke<BatchCloudLocalEvictionExecution>("evict_provider_cloud_local_batch", {
  cloudRootId,
  requests,
  confirmationPhrase,
  rationale,
});

export interface IncompleteDownloadState {
  schema_version: number;
  mode: string;
  root: string;
  fragments: Array<{
    relative_path: string;
    logical_bytes: number;
    allocated_bytes: number;
    fingerprint: string;
    extension: string;
  }>;
  issues: string[];
}

export const inspectIncompleteDownloads = (root: string) =>
  invoke<IncompleteDownloadState>("inspect_incomplete_downloads", { root });
