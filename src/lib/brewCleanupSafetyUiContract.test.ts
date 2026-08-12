import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Homebrew cleanup safety UX", () => {
  it("describes prune-prefix scope in the visible panel without claiming general old-file deletion", () => {
    const source = readSource("src/lib/BrewCleanup.svelte");
    const panelStart = source.indexOf('<div class="brew-panel">');
    const panelEnd = source.indexOf("<button onclick={judgeCleanup}", panelStart);
    const panelIntroduction = source.slice(panelStart, panelEnd);

    expect(panelStart).toBeGreaterThanOrEqual(0);
    expect(panelEnd).toBeGreaterThan(panelStart);
    expect(panelIntroduction).toContain("Homebrew prefix 안의 끊어진 심볼릭 링크와 빈 디렉터리");
    expect(source).not.toContain("Homebrew의 오래된 파일과 prefix");
    expect(source).not.toContain("오래된 Homebrew 파일");
  });

  it("invalidates the consumed judgment after every execution attempt", () => {
    const source = readSource("src/lib/BrewCleanup.svelte");
    const start = source.indexOf("async function executeCleanup()");
    const end = source.indexOf("</script>", start);
    const executeCleanup = source.slice(start, end);

    expect(start).toBeGreaterThanOrEqual(0);
    expect(executeCleanup).toContain("judgment = null;");
  });
});
