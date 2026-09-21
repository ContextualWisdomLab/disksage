import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const coreSource = readFileSync(
  resolve(repositoryRoot, "src-tauri/src/git_worktree.rs"),
  "utf8",
);
const publicSource = readFileSync(
  resolve(repositoryRoot, "src-tauri/src/git_worktree_public.rs"),
  "utf8",
);
const realFilesystemAcceptance = readFileSync(
  resolve(repositoryRoot, "src-tauri/tests/git_worktree_ignored_artifact_guard.rs"),
  "utf8",
);

const IGNORED_REASON = "ignored-artifacts-present";
const IGNORED_EVIDENCE_GAP = "ignored-artifact-evidence-incomplete";

describe("Git worktree ignored-artifact core authority", () => {
  it("classifies ignored artifacts inside the core audit before entry and removal-plan fingerprints", () => {
    const auditStart = coreSource.indexOf(
      "pub fn audit_git_worktrees_with_pull_request_membership",
    );
    const summaryStart = coreSource.indexOf("pub fn public_summary", auditStart);
    expect(auditStart).toBeGreaterThanOrEqual(0);
    expect(summaryStart).toBeGreaterThan(auditStart);
    const auditBody = coreSource.slice(auditStart, summaryStart);

    expect(auditBody).toContain("--ignored=matching");
    expect(auditBody).toContain(IGNORED_REASON);
    expect(auditBody).toContain(IGNORED_EVIDENCE_GAP);

    const dispositionIndex = auditBody.indexOf("let disposition = disposition(&blockers)");
    const fingerprintIndex = auditBody.indexOf("entry.entry_fingerprint");
    const ignoredReasonIndex = auditBody.indexOf(IGNORED_REASON);
    expect(ignoredReasonIndex).toBeGreaterThanOrEqual(0);
    expect(ignoredReasonIndex).toBeLessThan(dispositionIndex);
    expect(ignoredReasonIndex).toBeLessThan(fingerprintIndex);
  });

  it("uses the normal core live re-audit as the only ignored-artifact removal authority", () => {
    expect(publicSource).not.toContain("fn apply_ignored_artifact_guard");
    expect(publicSource).not.toContain(
      "git_worktree::ensure_candidates_still_have_no_ignored_artifacts",
    );
    expect(publicSource).not.toContain("ignored_artifacts_present(Path::new(&candidate.path)");
    expect(publicSource).not.toContain(".map(|report| apply_ignored_artifact_guard");
    expect(coreSource).not.toContain(
      "fn ensure_candidates_still_have_no_ignored_artifacts",
    );
  });

  it("keeps the real-filesystem mixed ignored+clean acceptance as behavior evidence", () => {
    expect(realFilesystemAcceptance).toContain(
      "fn ignored_artifacts_preserve_otherwise_clean_removal_candidate()",
    );
    expect(realFilesystemAcceptance).toContain('b"build/\\n"');
    expect(realFilesystemAcceptance).toContain('b"local generated state\\n"');
    expect(realFilesystemAcceptance).toContain(IGNORED_REASON);
    expect(realFilesystemAcceptance).toContain("GitWorktreeDisposition::RemovalCandidate");
    expect(realFilesystemAcceptance).toContain("GitWorktreeDisposition::Preserve");
  });
});
