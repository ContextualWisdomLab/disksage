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
  files: number;
  skipped: number;
  scan_complete: boolean;
  fingerprint: string;
  exists: boolean;
}
export interface CacheCleanupRequest {
  id: string;
  path: string;
  bytes: number;
  files: number;
  skipped: number;
  scan_complete: boolean;
  fingerprint: string;
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
  age_days: number;
}
export interface WorktreeCandidate {
  path: string;
  head: string;
  branch: string | null;
  is_primary: boolean;
  detached: boolean;
  exists: boolean;
  locked_reason: string | null;
  prunable_reason: string | null;
  metadata_prune_eligible: boolean;
  review_reasons: string[];
}
export interface WorktreeAudit {
  repository: string;
  generated_at_ms: number;
  registration_fingerprint: string;
  evidence_complete: boolean;
  worktrees: WorktreeCandidate[];
  stale_count: number;
  metadata_prune_eligible_count: number;
  notices: string[];
}
export interface CleanResult {
  path: string;
  ok: boolean;
  error: string;
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
export const listDevArtifacts = (root: string, minAgeDays = 30) =>
  invoke<DevArtifact[]>("list_dev_artifacts", { root, minAgeDays });
export const listStaleWorktrees = (repository: string) =>
  invoke<WorktreeAudit>("list_stale_worktrees", { repository });
export const cleanPaths = (paths: string[]) => invoke<CleanResult[]>("clean_paths", { paths });
export const cleanDevArtifacts = (root: string, minAgeDays: number, artifacts: DevArtifact[]) =>
  invoke<CleanResult[]>("clean_dev_artifacts", { root, minAgeDays, artifacts });
export const cleanCacheCandidates = (requests: CacheCleanupRequest[]) =>
  invoke<CleanResult[]>("clean_cache_candidates", { requests });
export interface OrphanRelation {
  subject: string;
  predicate: string;
  object: string;
  source: string;
}
export interface OrphanCandidate {
  path: string;
  kind: string;
  bundle_id: string | null;
  bytes: number;
  files: number;
  skipped: number;
  scan_complete: boolean;
  fingerprint: string;
  ontology_class: string;
  confidence: string;
  relations: OrphanRelation[];
  review_reasons: string[];
  auto_trash_eligible: boolean;
}
export interface OrphanPlan {
  schema_version: number;
  root: string;
  generated_at_ms: number;
  plan_fingerprint: string;
  candidate_bytes: number;
  scan_complete: boolean;
  candidates: OrphanCandidate[];
  notices: string[];
}
export interface OrphanJudgment {
  path: string;
  plan_fingerprint: string;
  verdict: Verdict;
  reason: string;
  model_name: string;
  judged_at_ms: number;
}
export interface OrphanJudgmentReport {
  plan_fingerprint: string;
  judgments: OrphanJudgment[];
}
export interface OrphanCleanupRequest {
  path: string;
  bytes: number;
  files: number;
  skipped: number;
  scan_complete: boolean;
  fingerprint: string;
}
export const planOrphanCleanup = () => invoke<OrphanPlan>("plan_orphan_cleanup");
export const judgeOrphanCleanup = () => invoke<OrphanJudgmentReport>("judge_orphan_cleanup");
export const cleanOrphanCandidates = (planFingerprint: string, requests: OrphanCleanupRequest[]) =>
  invoke<CleanResult[]>("clean_orphan_candidates", { planFingerprint, requests });
export const expandCleanTargets = (dir: string) =>
  invoke<string[]>("expand_clean_targets", { dir });
export const recentOperations = (limit = 20) =>
  invoke<JournalEntry[]>("recent_operations", { limit });
export const findDuplicateFiles = (root: string) =>
  invoke<DupeGroup[]>("find_duplicate_files", { root });

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
export interface OntologyRelation {
  subject: string;
  predicate: string;
  object: string;
}
export interface Ontology {
  classes: OntoClass[];
  relations: OntologyRelation[];
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
}

export const planOrganize = (root: string) =>
  invoke<MovePlan[]>("plan_organize", { root });

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
  | "incomplete-download";

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

export interface CloudRelationEvidence {
  subject: string;
  predicate: string;
  object: string;
  source: string;
}

export interface CloudCandidate {
  metadata_fingerprint: string;
  review_fingerprint: string;
  src: string;
  dst: string;
  provider: CloudProvider;
  destination_account_scope: CloudAccountScope;
  kind: ArchiveKind;
  ontology_class: string;
  ontology_relations: CloudRelationEvidence[];
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

export interface ExactDuplicateSummary {
  cluster_count: number;
  candidate_count: number;
  candidate_bytes: number;
  redundant_bytes: number;
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
  ontology_class?: string;
  ontology_relations?: CloudRelationEvidence[];
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
  capacity?: CloudCapacityAssessment;
}

export interface CloudCopyOutput {
  action: "copy-only" | "adopt-existing-copy";
  goal_state: CloudOffloadGoalState;
  receipt: CloudCopyReceipt;
  receipt_path: string;
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
  | "eviction-ready";
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
  /** Optional for evidence records written before explicit provider-state detection. */
  sync_state?: ProviderSyncState;
  remote_content: RemoteContentProof | null;
}

export interface ProviderSyncEvidenceRecord {
  version: number;
  record_id: string;
  evidence: ProviderSyncEvidence;
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
  evidence: ProviderSyncEvidence;
  evidence_record: ProviderSyncEvidenceRecord;
  evidence_path: string;
  adr_path: string;
  permit: LocalEvictionPermit | null;
  blockers: string[];
}

export const listCloudRoots = () => invoke<CloudRoot[]>("list_cloud_roots");
export const inspectCloudRoots = () =>
  invoke<CloudRootDiscoveryReport>("inspect_cloud_roots");
export const listCloudProviderConnections = () =>
  invoke<OAuthConnection[]>("list_cloud_provider_connections");
export const verifyCloudProviderCapacity = (cloudRoot: string) =>
  invoke<CloudCapacitySnapshot>("verify_cloud_provider_capacity", { cloudRoot });
export const listCloudReviewDecisions = () =>
  invoke<CloudReviewDecision[]>("list_cloud_review_decisions");
export const connectCloudProvider = (cloudRoot: string, clientId: string) =>
  invoke<OAuthConnection>("connect_cloud_provider", { cloudRoot, clientId });
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
  minSizeMib = 256,
  minAgeDays = 90,
  limit = 200,
) => invoke<CloudCopyOutput>("copy_cloud_candidate", {
  root,
  cloudRoot,
  metadataFingerprint,
  minSizeMib,
  minAgeDays,
  limit,
});
export const adoptExistingCloudCandidate = (
  root: string,
  cloudRoot: string,
  metadataFingerprint: string,
  minSizeMib = 256,
  minAgeDays = 90,
  limit = 200,
) => invoke<CloudCopyOutput>("adopt_existing_cloud_candidate", {
  root,
  cloudRoot,
  metadataFingerprint,
  minSizeMib,
  minAgeDays,
  limit,
});
export const attestCloudCopy = (
  receiptId: string,
  objectId: string | null = null,
) => invoke<CloudAttestationOutput>("attest_cloud_copy", {
  receiptId,
  objectId,
});
