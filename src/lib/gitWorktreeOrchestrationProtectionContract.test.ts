import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const protectionSource = readFileSync(
  resolve(repositoryRoot, "src-tauri/src/reclaim_protection.rs"),
  "utf8",
);
const auditSource = readFileSync(
  resolve(repositoryRoot, "src-tauri/src/git_worktree.rs"),
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
const SLEEP_CLEANUP_NOTICE =
  "sleep-session-requires-result-preserve-then-cleanup-then-reaudit";

describe("Git worktree orchestration ownership protection contract", () => {
  it("keeps sleeping Orca sessions and incomplete dispatches as stable fail-closed ownership reasons", () => {
    expect(protectionSource).toContain(
      `REASON_ORCA_SESSION_SLEEPING: &str = "${SLEEP_REASON}"`,
    );
    expect(protectionSource).toContain(
      `REASON_INCOMPLETE_DISPATCH: &str = "${INCOMPLETE_DISPATCH_REASON}"`,
    );
    expect(protectionSource).toMatch(
      /#\[serde\(default\)\][\s\S]{0,120}pub orca_sleep_worktree_paths: Vec<PathBuf>/,
    );
    expect(protectionSource).toMatch(
      /#\[serde\(default\)\][\s\S]{0,120}pub incomplete_dispatch_worktree_paths: Vec<PathBuf>/,
    );
    expect(protectionSource).toContain("parse_orca_sleep_worktree_paths");
    expect(protectionSource).toContain('eq_ignore_ascii_case("sleep")');
    expect(protectionSource).toContain('eq_ignore_ascii_case("sleeping")');
    expect(protectionSource).toContain("context.orca_sleep_worktree_paths");
    expect(protectionSource).toContain("context.incomplete_dispatch_worktree_paths");
    expect(protectionSource).toContain("REASON_ORCA_SESSION_SLEEPING.to_string()");
    expect(protectionSource).toContain("REASON_INCOMPLETE_DISPATCH.to_string()");
  });

  it("retains orchestration ownership evidence and the cleanup-before-reclaim notice in the redacted public summary", () => {
    const summaryStart = auditSource.indexOf("pub fn public_summary");
    expect(summaryStart).toBeGreaterThanOrEqual(0);
    const summaryEnd = auditSource.indexOf("\nfn valid_hex64", summaryStart);
    expect(summaryEnd).toBeGreaterThan(summaryStart);
    const summaryBody = auditSource.slice(summaryStart, summaryEnd);
    expect(summaryBody).toContain("REASON_ORCA_SESSION_SLEEPING");
    expect(summaryBody).toContain("REASON_INCOMPLETE_DISPATCH");
    expect(summaryBody).toContain(SLEEP_CLEANUP_NOTICE);
    expect(summaryBody).toContain("completed_pull_request_commit");
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

  it("blocks artifact reclaim while sleeping ownership remains bound", () => {
    const blockerStart = protectionSource.indexOf("pub fn artifact_blocking_reason_codes");
    expect(blockerStart).toBeGreaterThanOrEqual(0);
    const blockerEnd = protectionSource.indexOf(
      "pub fn whole_worktree_blocking_reason_codes",
      blockerStart,
    );
    expect(blockerEnd).toBeGreaterThan(blockerStart);
    const blockerBody = protectionSource.slice(blockerStart, blockerEnd);
    expect(blockerBody).toContain("REASON_ORCA_SESSION_SLEEPING");
  });

  it("keeps focused Rust coverage for Sleep parsing, preservation, public notice, and incomplete dispatch", () => {
    expect(protectionSource).toContain(
      "fn sleep_session_preserves_and_is_not_deletion_grounds_alone()",
    );
    expect(protectionSource).toContain(
      "fn parse_orca_sleep_worktree_paths_from_workspace_status()",
    );
    expect(protectionSource).toContain(
      "fn incomplete_dispatch_is_fail_closed_ownership_blocker()",
    );
    expect(auditSource).toContain(
      "fn public_summary_warns_before_reclaiming_completed_sleeping_session()",
    );
  });
});
