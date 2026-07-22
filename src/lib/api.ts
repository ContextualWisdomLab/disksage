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
export interface WorktreeCandidate {
  repository_common_dir: string;
  path: string;
  head: string;
  branch: string | null;
  default_ref: string | null;
  is_primary: boolean;
  detached: boolean;
  exists: boolean;
  dirty: boolean | null;
  locked_reason: string | null;
  prunable_reason: string | null;
  ahead: number | null;
  behind: number | null;
  merged_into_default: boolean | null;
  last_activity_ms: number;
  age_days: number;
  allocated_bytes: number;
  generated_artifact_bytes: number;
  generated_artifacts: GeneratedArtifact[];
  filesystem_scanned: boolean;
  filesystem_scan_complete: boolean;
  removal_eligible: boolean;
  metadata_prune_eligible: boolean;
  review_reasons: string[];
}
export interface GeneratedArtifact {
  path: string;
  kind: string;
  allocated_bytes: number;
}
export interface OrphanWorktreeCandidate {
  path: string;
  missing_git_dir: string;
  allocated_bytes: number;
  generated_artifact_bytes: number;
  generated_artifacts: GeneratedArtifact[];
  filesystem_scan_complete: boolean;
  removal_eligible: false;
  review_reasons: string[];
}
export interface WorktreeReport {
  scanned_root: string;
  generated_at_ms: number;
  elapsed_ms: number;
  evidence_complete: boolean;
  min_age_days: number;
  search_max_depth: number;
  repository_count: number;
  worktrees: WorktreeCandidate[];
  orphaned_worktrees: OrphanWorktreeCandidate[];
  potentially_reclaimable_bytes: number;
  reviewable_generated_artifact_bytes: number;
  scan_issues: WorktreeScanIssue[];
  notices: string[];
}
export interface WorktreeScanIssue {
  path: string;
  operation: string;
  reason: string;
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
export const listStaleWorktrees = (root: string, minAgeDays = 30, timeoutSeconds = 30) =>
  invoke<WorktreeReport>("list_stale_worktrees", { root, minAgeDays, timeoutSeconds });
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
