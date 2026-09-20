import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const publicSource = readFileSync(
  resolve(repositoryRoot, "src-tauri/src/git_worktree_public.rs"),
  "utf8",
);

const FAILURE_REASON = "git-worktree-removal-windows-identity-bound-mutation-unavailable";

function rustFunctionBody(name: string): string {
  const marker = `pub fn ${name}(`;
  const start = publicSource.indexOf(marker);
  expect(start, `${name} must exist on the public worktree boundary`).toBeGreaterThanOrEqual(0);
  const nextPublicFunction = publicSource.indexOf("\npub fn ", start + marker.length);
  const end = nextPublicFunction >= 0 ? nextPublicFunction : publicSource.length;
  return publicSource.slice(start, end);
}

describe("Windows Git-worktree mutation capability boundary", () => {
  it("publishes one stable fail-closed reason for the unsupported identity-bound Windows mutation", () => {
    expect(publicSource).toContain(FAILURE_REASON);
    expect(publicSource).toMatch(
      /#\[cfg\(windows\)\][\s\S]*?fn ensure_identity_bound_worktree_mutation_available\(\) -> Result<\(\), String>[\s\S]*?Err\([\s\S]*?git-worktree-removal-windows-identity-bound-mutation-unavailable/,
    );
    expect(publicSource).toMatch(
      /#\[cfg\(not\(windows\)\)\][\s\S]*?fn ensure_identity_bound_worktree_mutation_available\(\) -> Result<\(\), String>[\s\S]*?Ok\(\(\)\)/,
    );
  });

  it("guards every public stale-worktree execute entry point before ignored-artifact probing or private delegation", () => {
    const entries = [
      "execute_stale_worktree_removal",
      "execute_stale_worktree_removal_with_github_closed_pull_requests",
      "execute_stale_worktree_removal_with_github_pull_requests",
    ];

    for (const name of entries) {
      const body = rustFunctionBody(name);
      const capability = body.indexOf("ensure_identity_bound_worktree_mutation_available()?");
      const ignoredArtifactProbe = body.indexOf("ensure_candidates_still_have_no_ignored_artifacts");
      const privateDelegation = body.indexOf("crate::git_worktree_impl::");

      expect(capability, `${name} must check the platform mutation capability`).toBeGreaterThanOrEqual(0);
      expect(ignoredArtifactProbe, `${name} must retain the ignored-artifact guard`).toBeGreaterThanOrEqual(0);
      expect(privateDelegation, `${name} must retain the private implementation delegation on supported platforms`).toBeGreaterThanOrEqual(0);
      expect(capability, `${name} must fail before any per-candidate execution probe`).toBeLessThan(ignoredArtifactProbe);
      expect(capability, `${name} must fail before the pathname-based private remover is reachable`).toBeLessThan(privateDelegation);
    }
  });
});
