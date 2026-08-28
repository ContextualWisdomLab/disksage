import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Git worktree closed-PR opt-in", () => {
  it("keeps forge-backed closed-PR discovery off until the operator explicitly enables it", () => {
    const source = readSource("src/lib/GitWorktreeCleanup.svelte");

    expect(source).toContain("let includeClosedPullRequests = $state(false);");
    expect(source).toContain("완료된 작업 기록도 확인");
    expect(source).toContain("planStaleGitWorktrees(root, references, includeClosedPullRequests)");
    expect(source).toContain("removeStaleGitWorktrees(");
    expect(source).toContain("브랜치와 커밋은 유지됩니다");
  });
});
