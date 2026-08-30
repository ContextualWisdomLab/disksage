import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));

import * as api from "./api";

describe("api wrappers", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
  });

  it("forwards every command to Tauri with the expected payload shape", () => {
    const result = Promise.resolve("ok");
    mocks.invoke.mockReturnValue(result);

    const cases: Array<[() => unknown, string, unknown?]> = [
      [() => api.listRoots(), "list_roots"],
      [() => api.startScan("/root"), "start_scan", { root: "/root" }],
      [() => api.cancelScan(), "cancel_scan"],
      [() => api.getNode("/root"), "get_node", { path: "/root" }],
      [() => api.topFiles(), "top_files", { limit: 200 }],
      [() => api.topFiles(5), "top_files", { limit: 5 }],
      [() => api.listCacheCandidates(), "list_cache_candidates"],
      [() => api.cleanRegenerableCaches(), "clean_regenerable_caches"],
      [() => api.listCacheTargets("/cache"), "list_cache_targets", { dir: "/cache" }],
      [() => api.cleanCacheContents("/cache", []), "clean_cache_contents", { dir: "/cache", targets: [] }],
      [() => api.listDevArtifacts("/repo"), "list_dev_artifacts", { root: "/repo", minAgeDays: 30 }],
      [() => api.listDevArtifacts("/repo", 7), "list_dev_artifacts", { root: "/repo", minAgeDays: 7 }],
      [() => api.cleanPaths(["/tmp/a"]), "clean_paths", { paths: ["/tmp/a"] }],
      [() => api.cleanDevArtifacts("/repo", 30, []), "clean_dev_artifacts", { root: "/repo", minAgeDays: 30, artifacts: [] }],
      [() => api.expandCleanTargets("/tmp"), "expand_clean_targets", { dir: "/tmp" }],
      [() => api.recentOperations(), "recent_operations", { limit: 20 }],
      [() => api.recentOperations(3), "recent_operations", { limit: 3 }],
      [() => api.findDuplicateFiles("/repo"), "find_duplicate_files", { root: "/repo" }],
      [() => api.planOrphanCleanup(), "plan_orphan_cleanup"],
      [() => api.cleanOrphanCandidates("a".repeat(64), [], "phrase", "reviewed cache"), "clean_orphan_candidates", { planFingerprint: "a".repeat(64), requests: [], confirmationPhrase: "phrase", rationale: "reviewed cache" }],
      [() => api.diskInventory("/repo"), "disk_inventory", { root: "/repo" }],
      [() => api.getOntology(), "get_ontology"],
      [() => api.ontologyCoherence(), "ontology_coherence"],
      [() => api.planOrganize("/repo"), "plan_organize", { root: "/repo" }],
      [() => api.exportOrganizationLineage([{ src: "/a", dst: "/b", class_id: "docs" }]), "export_organization_lineage", { plans: [{ src: "/a", dst: "/b", class_id: "docs" }] }],
      [() => api.executeMoves([{ src: "/a", dst: "/b", class_id: "docs" }]), "execute_moves", { plans: [{ src: "/a", dst: "/b", class_id: "docs" }] }],
      [() => api.undoLastMoves(), "undo_last_moves", { limit: 50 }],
      [() => api.undoLastMoves(2), "undo_last_moves", { limit: 2 }],
      [() => api.modelStatus(), "model_status"],
      [() => api.downloadModel(), "download_model"],
      [() => api.fileVerdicts(["/a"]), "file_verdicts", { paths: ["/a"] }],
      [() => api.summarizeUnknownBucket(["/a"]), "summarize_unknown_bucket", { paths: ["/a"] }],
      [() => api.planBrewCleanup(), "plan_brew_cleanup"],
      [() => api.judgeBrewCleanup(), "judge_brew_cleanup"],
      [() => api.inspectPodmanReclaim(), "inspect_podman_reclaim"],
      [() => api.executePodmanDanglingImagePrune("prune", "reviewed dry-run"), "execute_podman_dangling_image_prune", { confirmationPhrase: "prune", rationale: "reviewed dry-run" }],
      [() => api.inspectContainerOrphans(), "inspect_container_orphans"],
      [() => api.executeContainerOrphanPrune("docker-native", null, "container", "prune", "reviewed dry-run"), "execute_container_orphan_prune", { runtimeKind: "docker-native", scopeName: null, category: "container", confirmationPhrase: "prune", rationale: "reviewed dry-run" }],
      [() => api.inspectRuntimeStorage(), "inspect_runtime_storage"],
      [() => api.executeRuntimeStorageTrim("colima", "trim", "reviewed guest trim"), "execute_runtime_storage_trim", { runtime: "colima", confirmationPhrase: "trim", rationale: "reviewed guest trim" }],
      [() => api.executeRuntimeStorageRecovery("colima", "recover", "reviewed guest recovery"), "execute_runtime_storage_recovery", { runtime: "colima", confirmationPhrase: "recover", rationale: "reviewed guest recovery" }],
      [() => api.executeInactivePodmanMachineStop("stop", "reviewed inactive machine"), "execute_inactive_podman_machine_stop", { confirmationPhrase: "stop", rationale: "reviewed inactive machine" }],
      [() => api.validateJudgeCalibration({ schema_version: 1, judgment_id: "a".repeat(64), categories: 2, model_labels: [0, 1], human_labels: [0, 1] }), "validate_judge_calibration", { evidence: { schema_version: 1, judgment_id: "a".repeat(64), categories: 2, model_labels: [0, 1], human_labels: [0, 1] } }],
      [() => api.executeBrewCleanup("a".repeat(64), "b".repeat(64), "DiskSage Homebrew cleanup 승인", "reviewed dry-run"), "execute_brew_cleanup", { planFingerprint: "a".repeat(64), judgmentId: "b".repeat(64), confirmationPhrase: "DiskSage Homebrew cleanup 승인", rationale: "reviewed dry-run" }],
      [() => api.getSettings(), "get_settings"],
      [() => api.setSettings(true), "set_settings", { onlineMode: true }],
      [() => api.reasonUnknownExtensions(["/a.abc"]), "reason_unknown_extensions", { samples: ["/a.abc"] }],
      [() => api.getUserRules(), "user_rules"],
      [() => api.listCloudRoots(), "list_cloud_roots"],
      [() => api.inspectCloudRoots(), "inspect_cloud_roots"],
      [() => api.planIcloudLocalCopyEviction("/cloud", "/cloud/archive.wav"), "plan_icloud_local_copy_eviction", { cloudRoot: "/cloud", path: "/cloud/archive.wav" }],
      [() => api.evictIcloudLocalCopy("/cloud", "/cloud/archive.wav", "a".repeat(64), "a".repeat(64), "verified local cache eviction"), "evict_icloud_local_copy", { cloudRoot: "/cloud", path: "/cloud/archive.wav", approvedPlanFingerprint: "a".repeat(64), confirmPlanFingerprint: "a".repeat(64), rationale: "verified local cache eviction" }],
      [() => api.planStaleGitWorktrees("/repo", ["origin/main", "origin/develop"], true), "plan_stale_git_worktrees", { repositoryRoot: "/repo", retentionReferences: ["origin/main", "origin/develop"], includeClosedPullRequests: true, staleOpenPullRequestCutoffMs: null }],
      [() => api.planStaleGitWorktrees("/repo", ["origin/main"], false, 1_756_000_000_000), "plan_stale_git_worktrees", { repositoryRoot: "/repo", retentionReferences: ["origin/main"], includeClosedPullRequests: false, staleOpenPullRequestCutoffMs: 1_756_000_000_000 }],
      [() => api.removeStaleGitWorktrees("/repo", ["origin/main"], true, null, "b".repeat(64), `DiskSage stale worktree 2 4096 승인 ${"b".repeat(64)}`, "merged and idle worktrees reviewed"), "remove_stale_git_worktrees", { repositoryRoot: "/repo", retentionReferences: ["origin/main"], includeClosedPullRequests: true, staleOpenPullRequestCutoffMs: null, approvedRemovalPlanFingerprint: "b".repeat(64), confirmationExactApprovalPhrase: `DiskSage stale worktree 2 4096 승인 ${"b".repeat(64)}`, rationale: "merged and idle worktrees reviewed" }],
      [() => api.removeStaleGitWorktrees("/repo", ["origin/main"], false, 1_756_000_000_000, "b".repeat(64), `DiskSage stale worktree 2 4096 승인 ${"b".repeat(64)}`, "stale work reviewed"), "remove_stale_git_worktrees", { repositoryRoot: "/repo", retentionReferences: ["origin/main"], includeClosedPullRequests: false, staleOpenPullRequestCutoffMs: 1_756_000_000_000, approvedRemovalPlanFingerprint: "b".repeat(64), confirmationExactApprovalPhrase: `DiskSage stale worktree 2 4096 승인 ${"b".repeat(64)}`, rationale: "stale work reviewed" }],
      [() => api.inventoryStandaloneGitClones(["/repos", "/work"]), "inventory_standalone_git_clones", { roots: ["/repos", "/work"] }],
      [() => api.planStaleGitClone("/repo", ["origin/main"], true), "plan_stale_git_clone", { repositoryRoot: "/repo", retentionReferences: ["origin/main"], includeClosedPullRequests: true, staleOpenPullRequestCutoffMs: null }],
      [() => api.removeStaleGitClone("/repo", ["origin/main"], true, null, "c".repeat(64), `DiskSage stale git clone 승인 ${"c".repeat(64)}`, "stale clone reviewed"), "remove_stale_git_clone", { repositoryRoot: "/repo", retentionReferences: ["origin/main"], includeClosedPullRequests: true, staleOpenPullRequestCutoffMs: null, approvedPlanFingerprint: "c".repeat(64), confirmationExactApprovalPhrase: `DiskSage stale git clone 승인 ${"c".repeat(64)}`, rationale: "stale clone reviewed" }],
      [() => api.listCloudProviderConnections(), "list_cloud_provider_connections"],
      [() => api.verifyCloudProviderCapacity("/cloud"), "verify_cloud_provider_capacity", { cloudRoot: "/cloud" }],
      [() => api.inspectIcloudNewCopyAdmission(), "inspect_icloud_new_copy_admission"],
      [() => api.cancelFinderCopy(), "cancel_finder_copy"],
      [() => api.inspectCloudProviderGlobalSync("/cloud"), "inspect_cloud_provider_global_sync", { cloudRoot: "/cloud" }],
      [() => api.recoverCloudProviderClient("/cloud"), "recover_cloud_provider_client", { cloudRoot: "/cloud" }],
      [() => api.listCloudReviewDecisions(), "list_cloud_review_decisions"],
      [() => api.connectCloudProvider("/cloud", "desktop-client-id"), "connect_cloud_provider", { cloudRoot: "/cloud", clientId: "desktop-client-id", writeAccess: false }],
      [() => api.disconnectCloudProvider("/cloud"), "disconnect_cloud_provider", { cloudRoot: "/cloud" }],
      [() => api.planCloudArchive("/scan", "/cloud"), "plan_cloud_archive", { root: "/scan", cloudRoot: "/cloud", minSizeMib: 256, minAgeDays: 90, limit: 200 }],
      [() => api.planCloudArchive("/scan", "/cloud", 10, 30, 5), "plan_cloud_archive", { root: "/scan", cloudRoot: "/cloud", minSizeMib: 10, minAgeDays: 30, limit: 5 }],
      [() => api.reviewCloudCandidate("/scan", "/cloud", "a".repeat(64), "b".repeat(64), "approved", "verified exact source"), "review_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "a".repeat(64), reviewFingerprint: "b".repeat(64), disposition: "approved", rationale: "verified exact source", minSizeMib: 256, minAgeDays: 90, limit: 200 }],
      [() => api.reviewCloudCandidate("/scan", "/cloud", "c".repeat(64), "d".repeat(64), "held", "needs another look", 10, 30, 5), "review_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "c".repeat(64), reviewFingerprint: "d".repeat(64), disposition: "held", rationale: "needs another look", minSizeMib: 10, minAgeDays: 30, limit: 5 }],
      [() => api.copyCloudCandidate("/scan", "/cloud", "a".repeat(64), "exact copy", "reviewed exact copy"), "copy_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "a".repeat(64), exactConfirmationPhrase: "exact copy", approvalRationale: "reviewed exact copy", minSizeMib: 256, minAgeDays: 90, limit: 200 }],
      [() => api.cancelCloudCopy("a".repeat(64)), "cancel_cloud_copy", { metadataFingerprint: "a".repeat(64) }],
      [() => api.copyCloudCandidateViaProviderApi("/scan", "/cloud", "a".repeat(64), "exact copy", "reviewed exact copy"), "copy_cloud_candidate_via_provider_api", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "a".repeat(64), exactConfirmationPhrase: "exact copy", approvalRationale: "reviewed exact copy", minSizeMib: 256, minAgeDays: 90, limit: 200 }],
      [() => api.copyCloudCandidate("/scan", "/cloud", "b".repeat(64), "exact copy", "reviewed exact copy", 10, 30, 5), "copy_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "b".repeat(64), exactConfirmationPhrase: "exact copy", approvalRationale: "reviewed exact copy", minSizeMib: 10, minAgeDays: 30, limit: 5 }],
      [() => api.adoptExistingCloudCandidate("/scan", "/cloud", "e".repeat(64), "exact adoption", "reviewed exact adoption"), "adopt_existing_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "e".repeat(64), exactConfirmationPhrase: "exact adoption", approvalRationale: "reviewed exact adoption", minSizeMib: 256, minAgeDays: 90, limit: 200 }],
      [() => api.adoptExistingCloudCandidate("/scan", "/cloud", "f".repeat(64), "exact adoption", "reviewed exact adoption", 10, 30, 5), "adopt_existing_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "f".repeat(64), exactConfirmationPhrase: "exact adoption", approvalRationale: "reviewed exact adoption", minSizeMib: 10, minAgeDays: 30, limit: 5 }],
      [() => api.attestCloudCopy("c".repeat(64)), "attest_cloud_copy", { receiptId: "c".repeat(64), objectId: null }],
      [() => api.attestCloudCopy("d".repeat(64), "remote-id"), "attest_cloud_copy", { receiptId: "d".repeat(64), objectId: "remote-id" }],
      [() => api.reconcileCloudReceipts(), "reconcile_cloud_receipts"],
      [() => api.trashVerifiedCloudSource("e".repeat(64), "e".repeat(64), "verified exact source"), "trash_verified_cloud_source", { receiptId: "e".repeat(64), confirmationReceiptId: "e".repeat(64), rationale: "verified exact source", objectId: null }],
      [() => api.trashVerifiedCloudSource("f".repeat(64), "f".repeat(64), "verified exact source", "remote-id"), "trash_verified_cloud_source", { receiptId: "f".repeat(64), confirmationReceiptId: "f".repeat(64), rationale: "verified exact source", objectId: "remote-id" }],
    ];

    for (const [call, command, payload] of cases) {
      expect(call()).toBe(result);
      if (payload === undefined) {
        expect(mocks.invoke).toHaveBeenLastCalledWith(command);
      } else {
        expect(mocks.invoke).toHaveBeenLastCalledWith(command, payload);
      }
    }
  });

  it("subscribes scan callbacks to typed Tauri events", () => {
    const progress = { files: 1, dirs: 2, skipped: 0, bytes: 3 };
    const done = { files: 4, dirs: 5, skipped: 1, bytes: 6 };
    const progressCb = vi.fn();
    const doneCb = vi.fn();

    mocks.listen.mockImplementation((event, cb) => {
      cb({ payload: event === "scan://progress" ? progress : done });
      return Promise.resolve(() => undefined);
    });

    void api.onScanProgress(progressCb);
    void api.onScanDone(doneCb);

    expect(mocks.listen).toHaveBeenNthCalledWith(1, "scan://progress", expect.any(Function));
    expect(mocks.listen).toHaveBeenNthCalledWith(2, "scan://done", expect.any(Function));
    expect(progressCb).toHaveBeenCalledWith(progress);
    expect(doneCb).toHaveBeenCalledWith(done);
  });
});

describe("cloud root identity", () => {
  const root: api.CloudRoot = {
    id: "/Cloud/내 드라이브",
    provider: "google-drive",
    account_scope: "organization",
    label: "Google Drive",
    path: "/Cloud/내 드라이브",
    readable: true,
    access_issue: null,
  };

  const connection: api.OAuthConnection = {
    connection_id: "a".repeat(64),
    provider: "google-drive",
    cloud_root_id: root.id.normalize("NFD"),
    cloud_root_path: root.path.normalize("NFD"),
    client_id: "desktop-client-id",
    scope: "https://www.googleapis.com/auth/drive.metadata.readonly",
    connected_at_ms: 1,
  };

  it("matches NFC and NFD spellings of the same File Provider root", () => {
    expect(connection.cloud_root_path).not.toBe(root.path);
    expect(api.cloudRootIdentityMatches(connection, root)).toBe(true);
  });

  it("rejects a different provider, root id, or path", () => {
    expect(api.cloudRootIdentityMatches({ ...connection, provider: "onedrive" }, root)).toBe(false);
    expect(api.cloudRootIdentityMatches({ ...connection, cloud_root_id: "/Cloud/other" }, root)).toBe(false);
    expect(api.cloudRootIdentityMatches({ ...connection, cloud_root_path: "/Cloud/other" }, root)).toBe(false);
  });
});

describe("native cloud copy headroom", () => {
  const volume: api.LocalVolumeSnapshot = {
    schema_version: 1,
    observed_at_ms: 1,
    total_bytes: 10_000,
    free_bytes: 9_000,
    available_bytes: api.LOCAL_COPY_RESERVE_BYTES + 10,
    used_bytes: 1_000,
    available_basis_points: 9_000,
    allocation_granularity_bytes: 4_096,
    pressure: "normal",
    evidence_kind: "filesystem-native-statvfs",
    limitations: [],
    evidence_fingerprint: "a".repeat(64),
  };

  it("requires the candidate and reserve without numeric overflow", () => {
    expect(api.localCopyHasHeadroom(volume, 10)).toBe(true);
    expect(api.localCopyHasHeadroom({ ...volume, available_bytes: api.LOCAL_COPY_RESERVE_BYTES + 9 }, 10)).toBe(false);
    expect(api.localCopyHasHeadroom(volume, Number.MAX_SAFE_INTEGER)).toBe(false);
    expect(api.localCopyHasHeadroom(undefined, 1)).toBe(false);
  });
});

describe("cloud copy approval phrase", () => {
  const exactPhrase = `DiskSage cloud copy-only ${"a".repeat(64)} 승인`;

  it("returns only the backend-authored phrase for the matching action", () => {
    const candidate = {
      copy_approval_action: "copy-only" as const,
      exact_copy_approval_phrase: exactPhrase,
    };
    expect(api.cloudCopyApprovalPhrase(candidate, "copy-only")).toBe(exactPhrase);
    expect(api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")).toBeNull();
  });

  it("fails closed when the backend omitted the action or exact phrase", () => {
    expect(api.cloudCopyApprovalPhrase({}, "copy-only")).toBeNull();
    expect(api.cloudCopyApprovalPhrase({
      copy_approval_action: "copy-only",
      exact_copy_approval_phrase: null,
    }, "copy-only")).toBeNull();
  });
});

describe("cloud capacity copy gate", () => {
  const snapshot: api.CloudCapacitySnapshot = {
    schema_version: 2,
    provider: "icloud",
    evidence_kind: "provider-native-status",
    observed_at_ms: 1,
    total_bytes: null,
    used_bytes: null,
    remaining_bytes: 2_000,
    trashed_bytes: null,
    max_upload_size_bytes: null,
    state: "available",
    evidence_fingerprint: "a".repeat(64),
    unavailable_reason: null,
  };
  const assessment: api.CloudCapacityAssessment = {
    snapshot,
    requested_bytes: 100,
    largest_candidate_bytes: 100,
    reserve_bytes: 1_000,
    required_bytes: 1_100,
    can_fit: true,
    blockers: [],
    notices: [],
  };

  it("accepts provider-native iCloud evidence when the byte gate fits", () => {
    expect(api.cloudCapacityAllowsCopy(assessment)).toBe(true);
  });

  it("rejects unavailable or failed capacity evidence for every provider", () => {
    expect(api.cloudCapacityAllowsCopy(undefined)).toBe(false);
    expect(api.cloudCapacityAllowsCopy({ ...assessment, can_fit: false })).toBe(false);
    expect(api.cloudCapacityAllowsCopy({
      ...assessment,
      can_fit: null,
      snapshot: {
        ...snapshot,
        evidence_kind: "unavailable",
        state: "unavailable",
        remaining_bytes: null,
        evidence_fingerprint: null,
        unavailable_reason: "icloud-native-quota-unavailable",
      },
    })).toBe(false);
  });

  it("allows personal native-client copy-only mode only with the explicit runtime notice", () => {
    const unavailable: api.CloudCapacityAssessment = {
      ...assessment,
      can_fit: null,
      snapshot: {
        ...snapshot,
        provider: "google-drive",
        evidence_kind: "unavailable",
        state: "unavailable",
        remaining_bytes: null,
        evidence_fingerprint: null,
        unavailable_reason: "provider-oauth-connection-missing",
      },
    };
    const root = { provider: "google-drive" as const, account_scope: "personal" as const };
    const notices = ["provider-client-runtime-observed", "native-client-copy-capacity-unverified"];
    expect(api.cloudNativeClientCopyAllowed(unavailable, root, notices)).toBe(true);
    expect(api.cloudNativeClientCopyAllowed(unavailable, { ...root, account_scope: "organization" }, notices)).toBe(false);
    expect(api.cloudNativeClientCopyAllowed(unavailable, root, notices.slice(0, 1))).toBe(false);
  });
});
