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
      [() => api.listDevArtifacts("/repo"), "list_dev_artifacts", { root: "/repo", minAgeDays: 30 }],
      [() => api.listDevArtifacts("/repo", 7), "list_dev_artifacts", { root: "/repo", minAgeDays: 7 }],
      [() => api.cleanPaths(["/tmp/a"]), "clean_paths", { paths: ["/tmp/a"] }],
      [() => api.expandCleanTargets("/tmp"), "expand_clean_targets", { dir: "/tmp" }],
      [() => api.recentOperations(), "recent_operations", { limit: 20 }],
      [() => api.recentOperations(3), "recent_operations", { limit: 3 }],
      [() => api.findDuplicateFiles("/repo"), "find_duplicate_files", { root: "/repo" }],
      [() => api.diskInventory("/repo"), "disk_inventory", { root: "/repo" }],
      [() => api.getOntology(), "get_ontology"],
      [() => api.ontologyCoherence(), "ontology_coherence"],
      [() => api.planOrganize("/repo"), "plan_organize", { root: "/repo" }],
      [() => api.executeMoves([{ src: "/a", dst: "/b", class_id: "docs" }]), "execute_moves", { plans: [{ src: "/a", dst: "/b", class_id: "docs" }] }],
      [() => api.undoLastMoves(), "undo_last_moves", { limit: 50 }],
      [() => api.undoLastMoves(2), "undo_last_moves", { limit: 2 }],
      [() => api.modelStatus(), "model_status"],
      [() => api.downloadModel(), "download_model"],
      [() => api.fileVerdicts(["/a"]), "file_verdicts", { paths: ["/a"] }],
      [() => api.summarizeUnknownBucket(["/a"]), "summarize_unknown_bucket", { paths: ["/a"] }],
      [() => api.getSettings(), "get_settings"],
      [() => api.setSettings(true), "set_settings", { onlineMode: true }],
      [() => api.reasonUnknownExtensions(["/a.abc"]), "reason_unknown_extensions", { samples: ["/a.abc"] }],
      [() => api.getUserRules(), "user_rules"],
      [() => api.listCloudRoots(), "list_cloud_roots"],
      [() => api.listCloudProviderConnections(), "list_cloud_provider_connections"],
      [() => api.connectCloudProvider("/cloud", "desktop-client-id"), "connect_cloud_provider", { cloudRoot: "/cloud", clientId: "desktop-client-id" }],
      [() => api.disconnectCloudProvider("/cloud"), "disconnect_cloud_provider", { cloudRoot: "/cloud" }],
      [() => api.planCloudArchive("/scan", "/cloud"), "plan_cloud_archive", { root: "/scan", cloudRoot: "/cloud", minSizeMib: 256, minAgeDays: 90, limit: 200 }],
      [() => api.planCloudArchive("/scan", "/cloud", 10, 30, 5), "plan_cloud_archive", { root: "/scan", cloudRoot: "/cloud", minSizeMib: 10, minAgeDays: 30, limit: 5 }],
      [() => api.copyCloudCandidate("/scan", "/cloud", "a".repeat(64)), "copy_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "a".repeat(64), minSizeMib: 256, minAgeDays: 90, limit: 200 }],
      [() => api.copyCloudCandidate("/scan", "/cloud", "b".repeat(64), 10, 30, 5), "copy_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "b".repeat(64), minSizeMib: 10, minAgeDays: 30, limit: 5 }],
      [() => api.adoptExistingCloudCandidate("/scan", "/cloud", "e".repeat(64)), "adopt_existing_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "e".repeat(64), minSizeMib: 256, minAgeDays: 90, limit: 200 }],
      [() => api.adoptExistingCloudCandidate("/scan", "/cloud", "f".repeat(64), 10, 30, 5), "adopt_existing_cloud_candidate", { root: "/scan", cloudRoot: "/cloud", metadataFingerprint: "f".repeat(64), minSizeMib: 10, minAgeDays: 30, limit: 5 }],
      [() => api.attestCloudCopy("c".repeat(64)), "attest_cloud_copy", { receiptId: "c".repeat(64), objectId: null }],
      [() => api.attestCloudCopy("d".repeat(64), "remote-id"), "attest_cloud_copy", { receiptId: "d".repeat(64), objectId: "remote-id" }],
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
