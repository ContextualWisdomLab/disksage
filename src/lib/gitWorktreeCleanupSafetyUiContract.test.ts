import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("Git worktree cleanup customer copy", () => {
  it("keeps implementation details and raw failures out of visible guidance", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/GitWorktreeCleanup.svelte"), "utf8");
    const visible = source.slice(source.indexOf("</script>"), source.indexOf("<style>"));

    expect(visible).toContain("보존할 기준");
    expect(visible).toContain("보존 기준 자동 입력");
    expect(visible).toContain("기준을 직접 입력하십시오");
    expect(visible).toContain("상태를 확인한 뒤 다시 시도하십시오");
    expect(visible).not.toContain("force");
    expect(visible).not.toContain("prune");
    expect(visible).not.toContain("증거 공백");
    expect(visible).not.toContain("활성 사용");
    expect(visible).not.toContain("분리된 HEAD");
    expect(visible).not.toContain("Git 등록 해제");
    expect(visible).not.toContain("사전 할당량 기준");
    expect(source).not.toContain("String(e)");
  });
});
