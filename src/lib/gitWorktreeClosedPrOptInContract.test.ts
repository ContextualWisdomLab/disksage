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
    expect(source).toContain("GitHub에서 병합 없이 종료된 PR의 깨끗한 worktree도 포함");
    expect(source).toContain("로그인된 GitHub 연결이 필요합니다");
  });
});
