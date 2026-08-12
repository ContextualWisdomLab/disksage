import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Git worktree privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown exception text", () => {
    const source = readSource("src/lib/GitWorktreeCleanup.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).toContain("Git 저장소 선택에 실패했습니다.");
    expect(source).toContain("Git worktree 감사에 실패했습니다.");
    expect(source).toContain("Git worktree 제거에 실패했습니다.");
  });

  it("keeps the existing planning and removal authority behind an accessible alert", () => {
    const source = readSource("src/lib/GitWorktreeCleanup.svelte");

    expect(source).toContain('role="alert"');
    expect(source).toContain("api.planStaleGitWorktrees(root, references)");
    expect(source).toContain("api.removeStaleGitWorktrees(");
  });
});
