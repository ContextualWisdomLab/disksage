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

export type RuntimeStorageKind = "podman-machine" | "colima";

export interface RuntimeStoragePlan {
  schema_kind: "disksage.runtime-storage-plan";
  schema_version: number;
  runtime: RuntimeStorageKind;
  display_name: string;
  executable_available: boolean;
  guest_running: boolean | null;
  guest_reachable: boolean | null;
  trim_command: string[] | null;
  recovery_command: string[][] | null;
  host_compaction_supported: boolean;
  host_compaction_blockers: string[];
  observed_at_ms: number;
  plan_fingerprint: string;
  exact_approval_phrase: string | null;
  recovery_approval_phrase: string | null;
  evidence_complete: boolean;
  issue: string | null;
}

export interface RuntimeStorageExecution {
  schema_kind: "disksage.runtime-storage-execution";
  schema_version: number;
  runtime: RuntimeStorageKind;
  command: string[];
  status_code: number;
  stdout: string;
  stderr: string;
  output_truncated: boolean;
  executed: boolean;
  executed_at_ms: number;
  rationale: string;
  volume_comparison: LocalVolumeComparison | null;
  volume_evidence_error: string | null;
}

export interface RuntimeStorageRecoveryExecution {
  schema_kind: "disksage.runtime-storage-recovery-execution";
  schema_version: number;
  runtime: RuntimeStorageKind;
  command: string[][];
  stop_status_code: number;
  start_status_code: number;
  guest_reachable_after_recovery: boolean;
  executed: boolean;
  executed_at_ms: number;
  rationale: string;
}

export const inspectRuntimeStorage = () =>
  invoke<RuntimeStoragePlan[]>("inspect_runtime_storage");

export const executeRuntimeStorageTrim = (
  runtime: RuntimeStorageKind,
  confirmationPhrase: string,
  rationale: string,
) => invoke<RuntimeStorageExecution>("execute_runtime_storage_trim", {
  runtime,
  confirmationPhrase,
  rationale,
});

export const executeRuntimeStorageRecovery = (
  runtime: RuntimeStorageKind,
  confirmationPhrase: string,
  rationale: string,
) => invoke<RuntimeStorageRecoveryExecution>("execute_runtime_storage_recovery", {
  runtime,
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
  scopeName: string | null,
  category: OrphanCategory,
  confirmationPhrase: string,
  rationale: string,
) => invoke<ContainerOrphanPruneExecution>("execute_container_orphan_prune", {
  runtimeKind,
  scopeName,
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
  production_time_confidence: "high" | "medium" | "low" | "unknown";
  ontology_class: string;
  destination_relation: "targetFolder";
  action: "move";
}

export interface OrganizationLineageBatch {
  schema: "disksage.organization-lineage-batch";
  version: 1;
  generated_at_ms: number;
  complete: true;
  batch_fingerprint_sha256: string;
  items: OrganizationLineageItem[];
}

export const exportOrganizationLineage = (plans: MovePlan[]) =>
  invoke<OrganizationLineageBatch>("export_organization_lineage", { plans });

export interface RuleMatch {
  ext: string | null;
  name_contains: string | null;
  path_contains: string | null;
  min_size: number | null;
  max_size: number | null;
}
export interface Rule {
  match: RuleMatch;
  class: string;
}
export const getUserRules = () => invoke<Rule[]>("user_rules");

export const executeMoves = (plans: MovePlan[]) =>
  invoke<CleanResult[]>("execute_moves", { plans });
export const undoLastMoves = (limit = 50) =>
  invoke<CleanResult[]>("undo_last_moves", { limit });

export type Verdict = "safe" | "caution" | "keep" | "unrated";
export interface FileVerdict {
  path: string;
  verdict: Verdict;
  reason: string;
}
export interface ModelStatus {
  present: boolean;
  name: string;
}

export const modelStatus = () => invoke<ModelStatus>("model_status");
export const downloadModel = () => invoke<void>("download_model");
export const fileVerdicts = (paths: string[]) => invoke<FileVerdict[]>("file_verdicts", { paths });
export const summarizeUnknownBucket = (paths: string[]) =>
  invoke<string | null>("summarize_unknown_bucket", { paths });

export interface BrewCleanupPlan {
  schema_version: number;
  platform: "macos";
  brew_path: string;
  brew_identity: string;
  brew_version: string;
  dry_run_output: string;
  dry_run_output_truncated: boolean;
  observed_at_ms: number;
  plan_fingerprint: string;
  exact_approval_phrase: string;
}

export interface BrewCleanupJudgment {
  schema_version: number;
  plan: BrewCleanupPlan;
  plan_fingerprint: string;
  judgment_id: string;
  verdict: Verdict;
  reason: string;
  model_name: string;
  judged_at_ms: number;
  exact_approval_phrase: string;
  calibration?: JudgeCalibrationResult;
}

export interface JudgeCalibrationEvidence {
  schema_version: number;
  judgment_id: string;
  categories: number;
  model_labels: number[];
  human_labels: number[];
  human_baseline_a?: number[];
  human_baseline_b?: number[];
  subgroup?: number[];
}

export interface JudgeCalibrationResult {
  schema_version: number;
  engine: string;
  judgment_id: string;
  categories: number;
  sample_count: number;
  passed: boolean;
  gates: Array<{ name: string; value: number; threshold: number; pass: boolean }>;
  exact_agreement: number;
  adjacent_agreement: number;
}

export interface BrewCleanupExecution {
  schema_version: number;
  plan_fingerprint: string;
  judgment_id: string;
  command: string[];
  status_code: number;
  stdout: string;
  stderr: string;
  output_truncated: boolean;
  executed: boolean;
  executed_at_ms: number;
  record_path: string | null;
  record_error: string | null;
}

export const planBrewCleanup = () => invoke<BrewCleanupPlan>("plan_brew_cleanup");
export const judgeBrewCleanup = () => invoke<BrewCleanupJudgment>("judge_brew_cleanup");
export const validateJudgeCalibration = (evidence: JudgeCalibrationEvidence) =>
  invoke<JudgeCalibrationResult>("validate_judge_calibration", { evidence });
export const executeBrewCleanup = (
  planFingerprint: string,
  judgmentId: string,
  confirmationPhrase: string,
  rationale: string,
) => invoke<BrewCleanupExecution>("execute_brew_cleanup", {
  planFingerprint,
  judgmentId,
  confirmationPhrase,
  rationale,
});

export interface Settings { online_mode: boolean; }
export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (online_mode: boolean) => invoke<Settings>("set_settings", { onlineMode: online_mode });

export interface ExtInsight { ext: string; type_desc: string | null; suggested_class: string | null; source: string; }
export const reasonUnknownExtensions = (samples: string[]) =>
  invoke<ExtInsight[]>("reason_unknown_extensions", { samples });

export type CloudProvider = "icloud" | "onedrive" | "google-drive";
export type CloudAccountScope = "personal" | "organization" | "shared" | "unknown";
export type ArchiveKind =
  | "document"
  | "media"
  | "archive"
  | "dataset"
  | "backup"
  | "creative"
  | "incomplete-download"
  | "sensitive-config";

export interface CloudRoot {
  id: string;
  provider: CloudProvider;
  account_scope: CloudAccountScope;
  label: string;
  path: string;
  readable: boolean;
  access_issue: string | null;
}

export interface CloudRootDiscoveryIssue {
  provider: CloudProvider | null;
  account_scope: CloudAccountScope;
  label: string;
  path: string;
  reason: string;
}

export interface CloudRootDiscoveryReport {
  roots: CloudRoot[];
  issues: CloudRootDiscoveryIssue[];
}

export type IcloudStateObservationMethod =
  | "file-provider-ctl-evaluate"
  | "foundation-ubiquitous-resource-values";

export interface IcloudLocalState {
  observation_method: IcloudStateObservationMethod;
  is_ubiquitous: boolean;
  is_uploaded: boolean;
  is_uploading: boolean;
  is_downloading: boolean;
  downloading_status_current: boolean;
  has_unresolved_conflicts: boolean;
  is_excluded_from_sync: boolean;
  is_sync_paused: boolean | null;
  is_trashed: boolean | null;
  allows_eviction: boolean | null;
  provider_reported_bytes: number | null;
  item_identifier_fingerprint: string | null;
}

export interface IcloudLocalEvictionPlan {
  version: number;
  provider: "icloud" | "onedrive";
  account_scope: CloudAccountScope;
  cloud_root: string;
  path: string;
  logical_bytes: number;
  allocated_bytes: number;
  filesystem_modified_ms: number;
  observed_at_ms: number;
  icloud_state: IcloudLocalState;
  active_use: ActiveUseEvidence;
  plan_fingerprint: string;
  eligible_after_human_approval: boolean;
  blockers: string[];
  notices: string[];
}

export interface IcloudLocalEvictionApproval {
  version: number;
  approval_id: string;
  plan_fingerprint: string;
  approved_at_ms: number;
  approved_by: string;
  rationale: string;
}

export interface IcloudLocalEvictionResult {
  version: number;
  result_id: string;
  plan_fingerprint: string;
  approval_id: string;
  path: string;
  requested_at_ms: number;
  allocated_bytes_before: number;
  allocated_bytes_after: number;
  observed_allocation_reduction_bytes: number;
  eviction_request_succeeded: boolean;
  cloud_item_path_retained: boolean;
  is_ubiquitous_after: boolean;
  local_allocation_reduction_verified: boolean;
  verification_complete: boolean;
  verification_blockers: string[];
  notices: string[];
}

export interface IcloudLocalCopyEvictionOutput {
  action: "evict-cloud-local-copy";
  plan: IcloudLocalEvictionPlan;
  approval: IcloudLocalEvictionApproval;
  approval_path: string;
  result: IcloudLocalEvictionResult;
  result_path: string | null;
  result_record_error: string | null;
}

export type GitWorktreeDisposition = "removal-candidate" | "preserve" | "evidence-gap";

export interface GitWorktreeSizeEvidence {
  method: string;
  evidence_complete: boolean;
  allocated_bytes: number;
  logical_bytes: number;
  visited_entries: number;
  error: string | null;
}

export interface GitWorktreeActiveUseEvidence {
  method: string;
  assessed: boolean;
  evidence_complete: boolean;
  active: boolean;
  observed_pids: number[];
  results_truncated: boolean;
  error: string | null;
}

export interface GitWorktreeAuditEntry {
  path: string;
  path_fingerprint: string;
  head: string;
  branch: string | null;
  detached: boolean;
  bare: boolean;
  primary: boolean;
  audit_origin: boolean;
  locked: boolean;
  lock_reason: string | null;
  prunable: boolean;
  prunable_reason: string | null;
  status_clean: boolean | null;
  status_entry_count: number | null;
  contained_in_reference: boolean | null;
  closed_pull_request_head: boolean;
  completed_pull_request_commit: boolean;
  open_pull_request_commit: boolean;
  stale_open_pull_request_head: boolean;
  head_is_retained_tip: boolean;
  actor_cwd_inside: boolean | null;
  size: GitWorktreeSizeEvidence;
  active_use: GitWorktreeActiveUseEvidence;
  disposition: GitWorktreeDisposition;
  blockers: string[];
  entry_fingerprint: string;
}

export interface GitWorktreeReferenceBinding {
  reference_ref: string;
  reference_oid: string;
}

export interface GitWorktreeAuditReport {
  schema_kind: "disksage.git-worktree-audit/v4";
  version: number;
  repository_root: string;
  common_dir: string;
  generated_at_ms: number;
  stale_open_pull_request_cutoff_ms: number | null;
  retention_references: GitWorktreeReferenceBinding[];
  retention_reference_set_fingerprint: string;
  removal_authority_fingerprint: string;
  retention_reachable_commit_count: number;
  worktree_count: number;
  removal_candidate_count: number;
  removal_candidate_allocated_bytes: number;
  preserved_count: number;
  evidence_gap_count: number;
  evidence_complete: boolean;
  removal_plan_fingerprint: string;
  exact_approval_phrase: string | null;
  entries: GitWorktreeAuditEntry[];
  issues: string[];
  filesystem_mutation_executed: false;
}

export interface GitWorktreeRemovalApproval {
  version: number;
  approval_id: string;
  removal_plan_fingerprint: string;
  retention_reference_set_fingerprint: string;
  removal_authority_fingerprint: string;
  removal_candidate_count: number;
  removal_candidate_allocated_bytes: number;
  exact_approval_phrase: string;
  approved_at_ms: number;
  approved_by: string;
  rationale: string;
}

export interface GitWorktreeRemovalItemResult {
  path: string;
  path_fingerprint: string;
  entry_fingerprint: string;
  head: string;
  branch: string | null;
  allocated_bytes_upper_bound: number;
  removal_attempted: boolean;
  removal_command_succeeded: boolean;
  path_absence_verified: boolean;
  registration_absence_verified: boolean;
  branch_retained: boolean | null;
  error: string | null;
}

export interface GitWorktreeRemovalResult {
  version: number;
  result_id: string;
  approval_id: string;
  removal_plan_fingerprint: string;
  retention_reference_set_fingerprint: string;
  removal_authority_fingerprint: string;
  requested_at_ms: number;
  completed_at_ms: number;
  planned_candidate_count: number;
  attempted_count: number;
  removed_count: number;
  planned_allocated_bytes_upper_bound: number;
  removed_allocated_bytes_upper_bound: number;
  items: GitWorktreeRemovalItemResult[];
  stopped_reason: string | null;
  branch_delete_executed: false;
  git_prune_executed: false;
  filesystem_mutation_executed: boolean;
  verification_complete: boolean;
  notices: string[];
}

export interface StaleGitWorktreeRemovalOutput {
  action: "remove-stale-git-worktrees";
  report: GitWorktreeAuditReport;
  approval: GitWorktreeRemovalApproval;
  approval_path: string;
  result: GitWorktreeRemovalResult;
  result_path: string | null;
  result_record_error: string | null;
}

export interface GitCloneReclaimPlan {
  schema_kind: "disksage.git-clone-reclaim-plan";
  version: number;
  generated_at_ms: number;
  repository_root: string;
  repository_object_id: string;
  head: string;
  branch: string;
  closed_pull_request_head: boolean;
  stale_open_pull_request_head: boolean;
  stale_open_pull_request_cutoff_ms: number | null;
  size: GitWorktreeSizeEvidence;
  active_use: GitWorktreeActiveUseEvidence;
  authority_fingerprint: string;
  plan_fingerprint: string;
  exact_approval_phrase: string | null;
  eligible_after_human_approval: boolean;
  blockers: string[];
  filesystem_mutation_executed: false;
}

export interface GitCloneReclaimApproval {
  version: number;
  approval_id: string;
  plan_fingerprint: string;
  exact_approval_phrase: string;
  approved_at_ms: number;
  approved_by: string;
  rationale: string;
}

export interface GitCloneReclaimResult {
  version: number;
  approval_id: string;
  plan_fingerprint: string;
  requested_at_ms: number;
  completed_at_ms: number;
  allocated_bytes_upper_bound: number;
  trash_move_executed: boolean;
  path_absence_verified: boolean;
  branch_delete_command_executed: false;
  git_prune_executed: false;
  physically_reclaimed_bytes: number | null;
}

export interface StaleGitCloneRemovalOutput {
  action: "remove-stale-git-clone";
  plan: GitCloneReclaimPlan;
  approval: GitCloneReclaimApproval;
  approval_path: string;
  result: GitCloneReclaimResult;
}

export interface OAuthConnection {
  connection_id: string;
  provider: CloudProvider;
  cloud_root_id: string;
  cloud_root_path: string;
  client_id: string;
  scope: string;
  connected_at_ms: number;
}

export function cloudRootIdentityMatches(
  connection: OAuthConnection,
  root: CloudRoot,
): boolean {
  return connection.provider === root.provider
    && connection.cloud_root_id.normalize("NFC") === root.id.normalize("NFC")
    && connection.cloud_root_path.normalize("NFC") === root.path.normalize("NFC");
}

export interface CloudCandidate {
  metadata_fingerprint: string;
  review_fingerprint: string;
  src: string;
  dst: string;
  provider: CloudProvider;
  destination_account_scope: CloudAccountScope;
  kind: ArchiveKind;
  bytes: number;
  age_days: number;
  created_ms: number;
  modified_ms: number;
  production_time_ms: number;
  production_time_source: string;
  production_time_confidence: string;
  source_root: string;
  relative_path: string;
  source_context: string;
  requires_review: boolean;
  review_reasons: string[];
  content_title: string | null;
  content_authors: string[];
  content_context: string[];
  duration_ms: number | null;
  dataset_profile: DatasetProfile | null;
  metadata_evidence: MetadataEvidence[];
  blocked_reason: string | null;
  /** Backend-selected action available for this candidate's current destination state. */
  copy_approval_action?: CloudCopyApprovalAction | null;
  /** Exact candidate-specific approval phrase generated by Rust, or null when blocked. */
  exact_copy_approval_phrase?: string | null;
  /** Maximum age in milliseconds accepted for an approval created from this plan. */
  copy_approval_max_age_ms?: number;
}

export type CloudReviewDisposition = "approved" | "held";

export interface CloudReviewDecision {
  version: number;
  decision_id: string;
  candidate_fingerprint: string;
  review_fingerprint: string;
  disposition: CloudReviewDisposition;
  reviewed_at_ms: number;
  reviewed_by?: string;
  rationale?: string;
}

export interface DatasetColumnProfile {
  name: string;
  inferred_type: string;
  observed_values: number;
  missing_values: number;
  sensitive_name: boolean;
}

export interface DatasetProfile {
  format: string;
  sampled_rows: number;
  sampled_worksheets: number;
  worksheet_names: string[];
  profile_complete: boolean;
  sample_truncated: boolean;
  columns: DatasetColumnProfile[];
  quality_warnings: string[];
}

export interface MetadataEvidence {
  field: string;
  value: string;
  source: string;
  confidence: string;
}

export interface CloudPlanReport {
  cloud_root: CloudRoot;
  generated_at_ms: number;
  candidates: CloudCandidate[];
  candidate_bytes: number;
  potentially_reclaimable_bytes: number;
  exact_duplicates: ExactDuplicateSummary;
  capacity?: CloudCapacityAssessment;
  local_volume?: LocalVolumeSnapshot;
  pre_copy_evidence?: PreCopyEvidenceCohort;
  notices: string[];
}

export interface PreCopyEvidenceObservation {
  stream: string;
  observed_at_ms: number;
  evidence_complete: boolean;
  fingerprint: string;
}

export interface PreCopyEvidenceCohort {
  schema_version: number;
  observed_at_ms: number;
  observations: PreCopyEvidenceObservation[];
  complete: boolean;
  blockers: string[];
  cohort_fingerprint: string;
}

/** Native File Provider copies need the candidate plus a safety reserve for staging. */
export const LOCAL_COPY_RESERVE_BYTES = 1024 * 1024 * 1024;

export function localCopyHasHeadroom(
  localVolume: LocalVolumeSnapshot | undefined,
  candidateBytes: number,
): boolean {
  if (!localVolume || !Number.isSafeInteger(candidateBytes) || candidateBytes < 0) return false;
  if (candidateBytes > Number.MAX_SAFE_INTEGER - LOCAL_COPY_RESERVE_BYTES) return false;
  return localVolume.available_bytes >= candidateBytes + LOCAL_COPY_RESERVE_BYTES;
}

export type LocalVolumePressure = "normal" | "elevated" | "high" | "critical";

export interface LocalVolumeSnapshot {
  schema_version: number;
  observed_at_ms: number;
  total_bytes: number;
  free_bytes: number;
  available_bytes: number;
  used_bytes: number;
  available_basis_points: number;
  allocation_granularity_bytes: number;
  pressure: LocalVolumePressure;
  evidence_kind: string;
  limitations: string[];
  evidence_fingerprint: string;
}

export interface LocalVolumeComparison {
  schema_version: number;
  before: LocalVolumeSnapshot;
  after: LocalVolumeSnapshot;
  observed_elapsed_ms: number;
  total_bytes_stable: boolean;
  available_change: {
    direction: "increased" | "decreased" | "unchanged";
    bytes: number;
  };
  free_change: {
    direction: "increased" | "decreased" | "unchanged";
    bytes: number;
  };
  logical_removed_bytes: number | null;
  physical_reclaim_bytes: null;
  physical_reclaim_attribution: "unproven";
  reason_codes: string[];
  evidence_fingerprint: string;
}

export interface IcloudSyncHealthReport {
  observed_at_ms: number;
  evidence_complete: boolean;
  managed_database_allocated_bytes?: number;
  upload_queue: {
    scheduled_waiting_count: number;
    scheduled_active_count: number;
    blocked_on_sync_up_count: number;
    out_of_quota_count: number;
    item_error_count: number;
  };
  file_provider_activity?: {
    command_succeeded: boolean;
    timed_out: boolean;
    output_truncated: boolean;
    no_progress_fetch_count: number;
    no_progress_create_count: number;
    materialization_failure_count: number;
    staged_item_missing_count: number;
    sync_excluded_filename_count: number;
    sync_excluded_root_count: number;
    active_upload_count: number;
    active_download_count: number;
    active_upload_progress_millionths?: number | null;
    active_download_progress_millionths?: number | null;
    notices: string[];
  } | null;
  sync_backlog_present: boolean;
  new_copy_admission_state: "clear" | "blocked";
  new_copy_admission_blockers: string[];
  blockers: string[];
  notices: string[];
  local_eviction_authorized: boolean;
}

export type ProviderGlobalSyncState = "clear" | "pending" | "error" | "unavailable";

export interface ProviderGlobalSyncReport {
  schema_version: number;
  provider: Exclude<CloudProvider, "icloud">;
  evidence_kind: string;
  evidence_complete: boolean;
  state: ProviderGlobalSyncState;
  upload_progress_present: boolean;
  download_progress_present: boolean;
  pending_indexable_count: number | null;
  blockers: string[];
  notices: string[];
}

export type CapacityEvidenceKind = "provider-api" | "provider-native-status" | "unavailable";
export type CloudCapacityState =
  | "available"
  | "normal"
  | "nearing"
  | "critical"
  | "exceeded"
  | "unlimited"
  | "unavailable";

export interface CloudCapacitySnapshot {
  schema_version: number;
  provider: CloudProvider;
  evidence_kind: CapacityEvidenceKind;
  observed_at_ms: number;
  total_bytes: number | null;
  used_bytes: number | null;
  remaining_bytes: number | null;
  trashed_bytes: number | null;
  max_upload_size_bytes: number | null;
  state: CloudCapacityState;
  evidence_fingerprint: string | null;
  unavailable_reason: string | null;
}

export interface CloudCapacityAssessment {
  snapshot: CloudCapacitySnapshot;
  requested_bytes: number;
  largest_candidate_bytes: number;
  reserve_bytes: number;
  required_bytes: number | null;
  can_fit: boolean | null;
  blockers: string[];
  notices: string[];
}

export function cloudCapacityAllowsCopy(
  assessment: CloudCapacityAssessment | null | undefined,
): boolean {
  return assessment?.can_fit === true
    && assessment.snapshot.evidence_kind !== "unavailable";
}

/** Personal native-client mode permits copy-only when the desktop sync app is running. */
export function cloudNativeClientCopyAllowed(
  assessment: CloudCapacityAssessment | null | undefined,
  root: Pick<CloudRoot, "provider" | "account_scope"> | null | undefined,
  notices: readonly string[],
): boolean {
  return root?.account_scope === "personal"
    && root.provider !== "icloud"
    && notices.includes("provider-client-runtime-observed")
    && notices.includes("native-client-copy-capacity-unverified")
    && assessment?.can_fit === null
    && assessment.snapshot.evidence_kind === "unavailable"
    && assessment.snapshot.unavailable_reason === "provider-oauth-connection-missing";
}

export interface ExactDuplicateSummary {
  cluster_count: number;
  candidate_count: number;
  candidate_bytes: number;
  redundant_bytes: number;
  clusters: ExactDuplicateClusterRecommendation[];
}

export interface ExactDuplicateClusterRecommendation {
  cluster_fingerprint: string;
  candidate_count: number;
  bytes_per_candidate: number;
  redundant_bytes: number;
  recommended_canonical_metadata_fingerprint: string;
  recommendation_confidence: "high" | "medium" | "low";
  recommendation_reason_codes: string[];
  member_metadata_fingerprints: string[];
  requires_human_confirmation: boolean;
}

export interface CloudCopyReceipt {
  version: number;
  receipt_id: string;
  candidate_fingerprint: string;
  provider: CloudProvider;
  source: string;
  destination: string;
  bytes: number;
  blake3: string;
  sha256: string;
  quick_xor_base64: string;
  source_modified_ms: number;
  copied_at_ms: number;
  copy_verified: boolean;
  provider_sync_confirmed: boolean;
  lineage_fingerprint?: string;
  lineage?: CloudLineageSnapshot;
}

export type CloudCopyVerificationMethod = "copied-by-disk-sage" | "adopted-existing";
/** Identifies the exact cloud-copy action authorized by a human reviewer. */
export type CloudCopyApprovalAction = "copy-only" | "adopt-existing-copy";

/** Records who approved one exact candidate, destination, and action, and when. */
export interface CloudCopyApproval {
  version: number;
  approval_id: string;
  action: CloudCopyApprovalAction;
  candidate_fingerprint: string;
  review_fingerprint: string;
  provider: CloudProvider;
  destination_account_scope: CloudAccountScope;
  cloud_root_id: string;
  approved_at_ms: number;
  approved_by: string;
  rationale: string;
  exact_confirmation_phrase: string;
}

export interface CloudLineageSnapshot {
  candidate_fingerprint: string;
  review_fingerprint: string;
  copy_verification_method?: CloudCopyVerificationMethod;
  review_decision_id: string | null;
  review_disposition: CloudReviewDisposition | null;
  reviewed_at_ms: number | null;
  reviewed_by?: string;
  review_rationale?: string;
  destination_account_scope: CloudAccountScope;
  kind: ArchiveKind;
  created_ms: number;
  modified_ms: number;
  production_time_ms: number;
  production_time_source: string;
  production_time_confidence: string;
  source_root: string;
  relative_path: string;
  source_context: string;
  requires_review: boolean;
  review_reasons: string[];
  content_title: string | null;
  content_authors: string[];
  content_context: string[];
  duration_ms: number | null;
  dataset_profile: DatasetProfile | null;
  metadata_evidence: MetadataEvidence[];
  copy_approval?: CloudCopyApproval;
}

export interface CloudCopyOutput {
  action: "copy-only" | "adopt-existing-copy";
  goal_state: CloudOffloadGoalState;
  goal_status: string | null;
  receipt: CloudCopyReceipt;
  receipt_path: string;
  adr_path: string | null;
  goal_path: string | null;
  projection_warnings: string[];
  provider_object_id: string | null;
}

export type SyncEvidenceKind = "provider-api" | "provider-native-status";
export type ProviderSyncState =
  | "complete"
  | "pending-upload"
  | "not-ubiquitous"
  | "not-local-current"
  | "uploading"
  | "excluded-from-sync"
  | "sync-paused"
  | "remote-unavailable"
  | "content-mismatch"
  | "unknown";
export type CloudOffloadGoalState =
  | "copy-verified"
  | "pending-provider-sync"
  | "provider-sync-confirmed"
  | "eviction-ready"
  | "source-evicted";
export type RemoteChecksumAlgorithm = "sha256" | "quick-xor";

export interface RemoteContentProof {
  object_id: string;
  revision: string;
  algorithm: RemoteChecksumAlgorithm;
  checksum: string;
  location_bound: boolean;
  location_proof?: string;
}

export interface ProviderSyncEvidence {
  receipt_id: string;
  provider: CloudProvider;
  destination: string;
  observed_bytes: number;
  destination_blake3: string;
  confirmed_at_ms: number;
  kind: SyncEvidenceKind;
  evidence_id: string;
  sync_complete: boolean;
  sync_state?: ProviderSyncState;
  remote_content: RemoteContentProof | null;
}

export interface ProviderSyncEvidenceRecord {
  version: number;
  record_id: string;
  evidence: ProviderSyncEvidence;
}

export type ProviderSyncTimeliness = "complete" | "pending" | "overdue";

export interface ProviderSyncTimelinessAssessment {
  state: ProviderSyncTimeliness;
  pending_age_ms: number;
  overdue_after_ms: number;
  reason_codes: string[];
}

export interface LocalEvictionPermit {
  receipt_id: string;
  provider: CloudProvider;
  source: string;
  destination: string;
  bytes: number;
  blake3: string;
  approved_at_ms: number;
  evidence_kind: SyncEvidenceKind;
  evidence_id: string;
  evidence_record_id: string;
}

export interface CloudAttestationOutput {
  goal_state: CloudOffloadGoalState;
  goal_status: "active" | "blocked" | "completed" | null;
  evidence: ProviderSyncEvidence;
  assessment: ProviderSyncTimelinessAssessment;
  evidence_record: ProviderSyncEvidenceRecord;
  evidence_path: string;
  adr_path: string | null;
  goal_path: string | null;
  projection_warnings: string[];
  permit: LocalEvictionPermit | null;
  blockers: string[];
}

export interface CloudReceiptReconciliationEntry {
  receipt_id: string | null;
  provider: CloudProvider | null;
  goal_status: "active" | "blocked" | "completed" | null;
  goal_state: CloudOffloadGoalState | null;
  provider_sync_state: ProviderSyncState | null;
  eviction_permit: boolean;
  blockers: string[];
  error: string | null;
}

export interface CloudReceiptReconciliationOutput {
  schema_version: number;
  observed_at_ms: number;
  receipts_seen: number;
  attested_count: number;
  pending_count: number;
  eviction_ready_count: number;
  error_count: number;
  provider_evidence_written: number;
  unprocessed_count: number;
  incomplete_reconciliation: boolean;
  entries: CloudReceiptReconciliationEntry[];
  cloud_write_executed: false;
  source_eviction_authorized: false;
}

export interface ActiveUseEvidence {
  method: "lsof-fp+ps-command";
  evidence_complete: boolean;
  active: boolean;
  observed_pids: number[];
  results_truncated: boolean;
  error: string | null;
}

export interface CloudSourceEvictionApproval {
  version: number;
  approval_id: string;
  receipt_id: string;
  evidence_record_id: string;
  approved_at_ms: number;
  approved_by: string;
  rationale: string;
  active_use_observed_at_ms: number;
  active_use: ActiveUseEvidence;
}

export interface CloudEvictionResult {
  action: "trash-verified-cloud-source";
  receipt_id: string;
  intent_id: string;
  completion_id: string;
  evidence_record_id: string;
  approval_id: string | null;
  source: string;
  staged_source: string;
  intent_path: string;
  completion_path: string;
  source_trashed: boolean;
  reconciled_after_interruption: boolean;
  already_completed: boolean;
}

export interface CloudSourceEvictionOutput {
  action: "attest-approve-and-trash-verified-cloud-source";
  goal_state: "source-evicted";
  attestation: CloudAttestationOutput;
  approval: CloudSourceEvictionApproval;
  approval_path: string;
  eviction: CloudEvictionResult;
  adr_path: string | null;
  goal_path: string | null;
  projection_warnings: string[];
}

export const listCloudRoots = () => invoke<CloudRoot[]>("list_cloud_roots");
export const inspectCloudRoots = () =>
  invoke<CloudRootDiscoveryReport>("inspect_cloud_roots");
export const planIcloudLocalCopyEviction = (cloudRoot: string, path: string) =>
  invoke<IcloudLocalEvictionPlan>("plan_icloud_local_copy_eviction", { cloudRoot, path });
export const evictIcloudLocalCopy = (
  cloudRoot: string,
  path: string,
  approvedPlanFingerprint: string,
  confirmPlanFingerprint: string,
  rationale: string,
) => invoke<IcloudLocalCopyEvictionOutput>("evict_icloud_local_copy", {
  cloudRoot,
  path,
  approvedPlanFingerprint,
  confirmPlanFingerprint,
  rationale,
});
export const planStaleGitWorktrees = (
  repositoryRoot: string,
  retentionReferences: string[],
  includeClosedPullRequests: boolean,
  staleOpenPullRequestCutoffMs: number | null = null,
) => invoke<GitWorktreeAuditReport>("plan_stale_git_worktrees", {
  repositoryRoot,
  retentionReferences,
  includeClosedPullRequests,
  staleOpenPullRequestCutoffMs,
});
export const removeStaleGitWorktrees = (
  repositoryRoot: string,
  retentionReferences: string[],
  includeClosedPullRequests: boolean,
  staleOpenPullRequestCutoffMs: number | null,
  approvedRemovalPlanFingerprint: string,
  confirmationExactApprovalPhrase: string,
  rationale: string,
) => invoke<StaleGitWorktreeRemovalOutput>("remove_stale_git_worktrees", {
  repositoryRoot,
  retentionReferences,
  includeClosedPullRequests,
  staleOpenPullRequestCutoffMs,
  approvedRemovalPlanFingerprint,
  confirmationExactApprovalPhrase,
  rationale,
});
export const planStaleGitClone = (
  repositoryRoot: string,
  retentionReferences: string[],
  includeClosedPullRequests: boolean,
  staleOpenPullRequestCutoffMs: number | null = null,
) => invoke<GitCloneReclaimPlan>("plan_stale_git_clone", {
  repositoryRoot,
  retentionReferences,
  includeClosedPullRequests,
  staleOpenPullRequestCutoffMs,
});
export const removeStaleGitClone = (
  repositoryRoot: string,
  retentionReferences: string[],
  includeClosedPullRequests: boolean,
  staleOpenPullRequestCutoffMs: number | null,
  approvedPlanFingerprint: string,
  confirmationExactApprovalPhrase: string,
  rationale: string,
) => invoke<StaleGitCloneRemovalOutput>("remove_stale_git_clone", {
  repositoryRoot,
  retentionReferences,
  includeClosedPullRequests,
  staleOpenPullRequestCutoffMs,
  approvedPlanFingerprint,
  confirmationExactApprovalPhrase,
  rationale,
});
export const listCloudProviderConnections = () =>
  invoke<OAuthConnection[]>("list_cloud_provider_connections");
export const verifyCloudProviderCapacity = (cloudRoot: string) =>
  invoke<CloudCapacitySnapshot>("verify_cloud_provider_capacity", { cloudRoot });
export const inspectIcloudNewCopyAdmission = () =>
  invoke<IcloudSyncHealthReport>("inspect_icloud_new_copy_admission");
export const cancelFinderCopy = () => invoke<void>("cancel_finder_copy");
export const inspectCloudProviderGlobalSync = (cloudRoot: string) =>
  invoke<ProviderGlobalSyncReport>("inspect_cloud_provider_global_sync", { cloudRoot });
export interface ProviderRecoveryOutput {
  schema_version: number;
  provider: Exclude<CloudProvider, "icloud">;
  action: string;
  pre_runtime_observed: boolean;
  quit_requested: boolean;
  launch_requested: boolean;
  post_runtime_observed: boolean | null;
  blockers: string[];
  cloud_write_executed: boolean;
  source_eviction_executed: boolean;
}
export const recoverCloudProviderClient = (cloudRoot: string) =>
  invoke<ProviderRecoveryOutput>("recover_cloud_provider_client", { cloudRoot });
export const listCloudReviewDecisions = () =>
  invoke<CloudReviewDecision[]>("list_cloud_review_decisions");
export const connectCloudProvider = (cloudRoot: string, clientId: string, writeAccess = false) =>
  invoke<OAuthConnection>("connect_cloud_provider", { cloudRoot, clientId, writeAccess });
export const disconnectCloudProvider = (cloudRoot: string) =>
  invoke<void>("disconnect_cloud_provider", { cloudRoot });
export const planCloudArchive = (
  root: string,
  cloudRoot: string,
  minSizeMib = 256,
  minAgeDays = 90,
  limit = 200,
) => invoke<CloudPlanReport>("plan_cloud_archive", {
  root,
  cloudRoot,
  minSizeMib,
  minAgeDays,
  limit,
});
export const reviewCloudCandidate = (
  root: string,
  cloudRoot: string,
  metadataFingerprint: string,
  reviewFingerprint: string,
  disposition: CloudReviewDisposition,
  rationale: string,
  minSizeMib = 256,
  minAgeDays = 90,
  limit = 200,
) => invoke<CloudReviewDecision>("review_cloud_candidate", {
  root,
  cloudRoot,
  metadataFingerprint,
  reviewFingerprint,
  disposition,
  rationale,
  minSizeMib,
  minAgeDays,
  limit,
});
export const copyCloudCandidate = (
  root: string,
  cloudRoot: string,
  metadataFingerprint: string,
  exactConfirmationPhrase: string,
  approvalRationale: string,
  minSizeMib = 256,
  minAgeDays = 90,
  limit = 200,
) => invoke<CloudCopyOutput>("copy_cloud_candidate", {
  root,
  cloudRoot,
  metadataFingerprint,
  exactConfirmationPhrase,
  approvalRationale,
  minSizeMib,
  minAgeDays,
  limit,
});
export const copyCloudCandidateViaProviderApi = (
  root: string,
  cloudRoot: string,
  metadataFingerprint: string,
  exactConfirmationPhrase: string,
  approvalRationale: string,
  minSizeMib = 256,
  minAgeDays = 90,
  limit = 200,
) => invoke<CloudCopyOutput>("copy_cloud_candidate_via_provider_api", {
  root,
  cloudRoot,
  metadataFingerprint,
  exactConfirmationPhrase,
  approvalRationale,
  minSizeMib,
  minAgeDays,
  limit,
});
/** Request cancellation of the single in-flight native copy operation. */
export const cancelCloudCopy = (metadataFingerprint: string) => invoke<void>("cancel_cloud_copy", {
  metadataFingerprint,
});
export const adoptExistingCloudCandidate = (
  root: string,
  cloudRoot: string,
  metadataFingerprint: string,
  exactConfirmationPhrase: string,
  approvalRationale: string,
  minSizeMib = 256,
  minAgeDays = 90,
  limit = 200,
) => invoke<CloudCopyOutput>("adopt_existing_cloud_candidate", {
  root,
  cloudRoot,
  metadataFingerprint,
  exactConfirmationPhrase,
  approvalRationale,
  minSizeMib,
  minAgeDays,
  limit,
});

/** Returns the exact backend-authored phrase only for the matching candidate action. */
export const cloudCopyApprovalPhrase = (
  candidate: Pick<CloudCandidate, "copy_approval_action" | "exact_copy_approval_phrase">,
  action: CloudCopyApprovalAction,
): string | null => candidate.copy_approval_action === action
  ? candidate.exact_copy_approval_phrase ?? null
  : null;
export const attestCloudCopy = (
  receiptId: string,
  objectId: string | null = null,
) => invoke<CloudAttestationOutput>("attest_cloud_copy", {
  receiptId,
  objectId,
});
export const reconcileCloudReceipts = () =>
  invoke<CloudReceiptReconciliationOutput>("reconcile_cloud_receipts");
export const trashVerifiedCloudSource = (
  receiptId: string,
  confirmationReceiptId: string,
  rationale: string,
  objectId: string | null = null,
) => invoke<CloudSourceEvictionOutput>("trash_verified_cloud_source", {
  receiptId,
  confirmationReceiptId,
  rationale,
  objectId,
});
