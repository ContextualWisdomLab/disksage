import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const rustSource = readFileSync(resolve(repositoryRoot, "src-tauri/src/git_worktree.rs"), "utf8");
const apiSource = readFileSync(resolve(repositoryRoot, "src/lib/api.ts"), "utf8");

const PATH_ALGORITHM = "disksage.git-worktree-path/blake3-v2";
const ENTRY_ALGORITHM = "disksage.git-worktree-entry/blake3-v3";

describe("Git worktree fingerprint evidence contract", () => {
  it("serializes explicit path and entry fingerprint algorithms", () => {
    expect(rustSource).toContain(`GIT_WORKTREE_PATH_FINGERPRINT_ALGORITHM: &str = "${PATH_ALGORITHM}"`);
    expect(rustSource).toContain(`GIT_WORKTREE_ENTRY_FINGERPRINT_ALGORITHM: &str = "${ENTRY_ALGORITHM}"`);
    expect(rustSource).toContain("pub path_fingerprint_algorithm: String");
    expect(rustSource).toContain("pub entry_fingerprint_algorithm: String");
    expect(rustSource).toContain("path_fingerprint_algorithm: GIT_WORKTREE_PATH_FINGERPRINT_ALGORITHM.into()");
    expect(rustSource).toContain("entry_fingerprint_algorithm: GIT_WORKTREE_ENTRY_FINGERPRINT_ALGORITHM.into()");
  });

  it("fails removal validation closed when persisted algorithm metadata drifts", () => {
    expect(rustSource).toContain("report.path_fingerprint_algorithm != GIT_WORKTREE_PATH_FINGERPRINT_ALGORITHM");
    expect(rustSource).toContain("report.entry_fingerprint_algorithm != GIT_WORKTREE_ENTRY_FINGERPRINT_ALGORITHM");
  });

  it("exposes exact algorithm literals to first-party TypeScript consumers", () => {
    expect(apiSource).toContain(`path_fingerprint_algorithm: "${PATH_ALGORITHM}"`);
    expect(apiSource).toContain(`entry_fingerprint_algorithm: "${ENTRY_ALGORITHM}"`);
  });
});
