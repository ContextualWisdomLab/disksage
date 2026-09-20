import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const rustSource = readFileSync(resolve(repositoryRoot, "src-tauri/src/git_worktree.rs"), "utf8");
const apiSource = readFileSync(resolve(repositoryRoot, "src/lib/api.ts"), "utf8");

const SCHEMA_KIND = "disksage.git-worktree-audit/v5";
const PATH_ALGORITHM = "disksage.git-worktree-path/blake3-v2";
const ENTRY_ALGORITHM = "disksage.git-worktree-entry/blake3-v3";

function rustStructBody(name: string): string {
  const marker = `pub struct ${name} {`;
  const start = rustSource.indexOf(marker);
  expect(start, `${name} must exist`).toBeGreaterThanOrEqual(0);
  const end = rustSource.indexOf("\n}", start);
  expect(end, `${name} must have a closing brace`).toBeGreaterThan(start);
  return rustSource.slice(start, end);
}

describe("Git worktree fingerprint evidence contract", () => {
  it("publishes a new persisted schema instead of reinterpreting historical v4 evidence", () => {
    expect(rustSource).toContain(`GIT_WORKTREE_AUDIT_SCHEMA_KIND: &str = "${SCHEMA_KIND}"`);
    expect(rustSource).toContain("GIT_WORKTREE_AUDIT_VERSION: u32 = 5");

    const marker = "export interface GitWorktreeAuditReport {";
    const start = apiSource.indexOf(marker);
    expect(start, "GitWorktreeAuditReport TypeScript contract must exist").toBeGreaterThanOrEqual(0);
    const end = apiSource.indexOf("\n}", start);
    expect(end, "GitWorktreeAuditReport TypeScript contract must close").toBeGreaterThan(start);
    const body = apiSource.slice(start, end);
    expect(body).toContain(`schema_kind: "${SCHEMA_KIND}"`);
  });

  it("serializes explicit path and entry algorithms on both private and public audit evidence", () => {
    expect(rustSource).toContain(`GIT_WORKTREE_PATH_FINGERPRINT_ALGORITHM: &str = "${PATH_ALGORITHM}"`);
    expect(rustSource).toContain(`GIT_WORKTREE_ENTRY_FINGERPRINT_ALGORITHM: &str = "${ENTRY_ALGORITHM}"`);

    for (const structName of ["GitWorktreeAuditReport", "GitWorktreeAuditPublicSummary"]) {
      const body = rustStructBody(structName);
      expect(body).toContain("pub path_fingerprint_algorithm: String");
      expect(body).toContain("pub entry_fingerprint_algorithm: String");
    }

    expect(rustSource).toContain("path_fingerprint_algorithm: GIT_WORKTREE_PATH_FINGERPRINT_ALGORITHM.into()");
    expect(rustSource).toContain("entry_fingerprint_algorithm: GIT_WORKTREE_ENTRY_FINGERPRINT_ALGORITHM.into()");
    expect(rustSource).toContain("path_fingerprint_algorithm: report.path_fingerprint_algorithm.clone()");
    expect(rustSource).toContain("entry_fingerprint_algorithm: report.entry_fingerprint_algorithm.clone()");
  });

  it("fails removal validation closed when persisted algorithm metadata drifts", () => {
    expect(rustSource).toContain("report.path_fingerprint_algorithm != GIT_WORKTREE_PATH_FINGERPRINT_ALGORITHM");
    expect(rustSource).toContain("report.entry_fingerprint_algorithm != GIT_WORKTREE_ENTRY_FINGERPRINT_ALGORITHM");
    expect(rustSource).toContain("fn removal_rejects_mismatched_fingerprint_algorithms()");
  });

  it("exposes exact algorithm literals to first-party TypeScript consumers", () => {
    const marker = "export interface GitWorktreeAuditReport {";
    const start = apiSource.indexOf(marker);
    expect(start, "GitWorktreeAuditReport TypeScript contract must exist").toBeGreaterThanOrEqual(0);
    const end = apiSource.indexOf("\n}", start);
    expect(end, "GitWorktreeAuditReport TypeScript contract must close").toBeGreaterThan(start);
    const body = apiSource.slice(start, end);
    expect(body).toContain(`path_fingerprint_algorithm: "${PATH_ALGORITHM}"`);
    expect(body).toContain(`entry_fingerprint_algorithm: "${ENTRY_ALGORITHM}"`);
  });
});
