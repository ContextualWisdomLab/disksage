import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("Git cleanup customer copy contract", () => {
  it("describes the next action without exposing Git cleanup internals", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/GitWorktreeCleanup.svelte"), "utf8");
    const scriptEnd = source.indexOf("</script>");
    const styleStart = source.indexOf("<style>");
    expect(scriptEnd).toBeGreaterThanOrEqual(0);
    expect(styleStart).toBeGreaterThan(scriptEnd);
    const visible = source.slice(scriptEnd, styleStart);

    expect(visible).toContain("보존할 기준");
    expect(visible).toContain("상태를 확인한 뒤 다시 시도하십시오");
    for (const internalTerm of ["force", "prune", "증거 공백", "활성 사용", "분리된 HEAD", "String(e)"]) {
      expect(visible).not.toContain(internalTerm);
    }
  });
});
