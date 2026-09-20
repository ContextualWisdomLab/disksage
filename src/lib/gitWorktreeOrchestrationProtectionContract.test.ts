import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const protectionSource = readFileSync(
  resolve(repositoryRoot, "src-tauri/src/reclaim_protection.rs"),
  "utf8",
);
const auditCliSource = readFileSync(
  resolve(repositoryRoot, "src-tauri/src/bin/disksage-git-worktree-audit.rs"),
  "utf8",
);
const removeCliSource = readFileSync(
  resolve(repositoryRoot, "src-tauri/src/bin/disksage-git-worktree-remove.rs"),
  "utf8",
);
const hostedProtectionTest = readFileSync(
  resolve(repositoryRoot, "src-tauri/tests/git_worktree_orca_protection_context.rs"),
  "utf8",
);

const SLEEP_REASON = "orca-session-sleeping";
const INCOMPLETE_DISPATCH_REASON = "incomplete-dispatch-evidence-incomplete";

describe("Git worktree orchestration ownership protection contract", () => {
  it("keeps sleeping Orca sessions and incomplete dispatches as stable fail-closed ownership reasons", () => {
    expect(protectionSource).toContain(
      `REASON_ORCA_SESSION_SLEEPING: &str = "${SLEEP_REASON}"`,
    );
    expect(protectionSource).toContain(
      `REASON_INCOMPLETE_DISPATCH: &str = "${INCOMPLETE_DISPATCH_REASON}"`,
    );
    expect(protectionSource).toContain("pub orca_sleep_worktree_paths: Vec<PathBuf>");
    expect(protectionSource).toContain("pub incomplete_dispatch_worktree_paths: Vec<PathBuf>");
    expect(protectionSource).toContain("parse_orca_sleep_worktree_paths");
    expect(protectionSource).toContain("context.orca_sleep_worktree_paths");
    expect(protectionSource).toContain("context.incomplete_dispatch_worktree_paths");
    expect(protectionSource).toContain("REASON_ORCA_SESSION_SLEEPING.to_string()");
    expect(protectionSource).toContain("REASON_INCOMPLETE_DISPATCH.to_string()");
  });

  it("acquires the same ownership evidence on audit and mutation CLIs", () => {
    for (const cliSource of [auditCliSource, removeCliSource]) {
      expect(cliSource).toContain("--orca-worktree-json");
      expect(cliSource).toContain("--incomplete-dispatch-worktree-path");
      expect(cliSource).toContain("parse_orca_sleep_worktree_paths");
      expect(cliSource).toContain("orca_sleep_worktree_paths");
      expect(cliSource).toContain("incomplete_dispatch_worktree_paths");
    }
  });

  it("keeps the existing hosted Windows protection acquisition test as real CLI evidence", () => {
    expect(hostedProtectionTest).toContain(
      "fn shipped_audit_cli_acquires_live_protection_inputs_and_requires_explicit_recent_window()",
    );
    expect(hostedProtectionTest).toContain("--orca-worktree-json");
    expect(hostedProtectionTest).toContain("--incomplete-dispatch-worktree-path");
    expect(hostedProtectionTest).toContain("REASON_ORCA_SESSION_SLEEPING");
    expect(hostedProtectionTest).toContain("REASON_INCOMPLETE_DISPATCH");
  });

  it("does not treat sleeping ownership as reclaimable artifact evidence", () => {
    const blockerStart = protectionSource.indexOf("pub fn artifact_blocking_reason_codes");
    expect(blockerStart).toBeGreaterThanOrEqual(0);
    const blockerEnd = protectionSource.indexOf("pub fn whole_worktree_blocking_reason_codes", blockerStart);
    expect(blockerEnd).toBeGreaterThan(blockerStart);
    const blockerBody = protectionSource.slice(blockerStart, blockerEnd);
    expect(blockerBody).toContain("REASON_ORCA_SESSION_SLEEPING");
  });
});
