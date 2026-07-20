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
export interface DevArtifact {
  path: string;
  kind: string;
  project: string;
  bytes: number;
  age_days: number;
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
export const cleanPaths = (paths: string[]) => invoke<CleanResult[]>("clean_paths", { paths });
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
  notices: string[];
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
}

export interface CloudCopyOutput {
  action: "copy-only" | "adopt-existing-copy";
  receipt: CloudCopyReceipt;
  receipt_path: string;
}

export type SyncEvidenceKind = "provider-api" | "provider-native-status";
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
}

export interface CloudAttestationOutput {
  evidence: ProviderSyncEvidence;
  evidence_record: ProviderSyncEvidenceRecord;
  evidence_path: string;
  permit: LocalEvictionPermit | null;
  blockers: string[];
}

export const listCloudRoots = () => invoke<CloudRoot[]>("list_cloud_roots");
export const inspectCloudRoots = () =>
  invoke<CloudRootDiscoveryReport>("inspect_cloud_roots");
export const listCloudProviderConnections = () =>
  invoke<OAuthConnection[]>("list_cloud_provider_connections");
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
