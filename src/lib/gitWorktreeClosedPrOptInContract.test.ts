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
    expect(source).toContain("let includeStaleOpenPullRequests = $state(false);");
    expect(source).toContain("완료된 작업과 연결된 항목도 확인");
    expect(source).toContain("오래된 진행 중 작업도 확인");
    expect(source).toContain("기준일 (이 날짜보다 먼저 생성된 작업)");
    expect(source).toContain("planStaleGitWorktrees(root, references, includeClosedPullRequests, staleCutoffMs)");
    expect(source).toContain("removeStaleGitWorktrees(");
    expect(source).toContain("보존할 작업 기록은 유지됩니다");
  });

  it("discloses that opting in can admit clean worktrees from closed but unmerged pull requests", () => {
    const source = readSource("src/lib/GitWorktreeCleanup.svelte");

    expect(source).toContain("병합 없이 종료된 PR의 깨끗한 보조 폴더도 정리 후보가 될 수 있습니다.");
    expect(source).toContain("브랜치와 커밋은 유지됩니다.");
  });
});
