import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cache cleanup fail-closed UX", () => {
  it("offers only the identity-bound cache action", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");

    expect(cleanup).toContain("cleanCacheContents");
    expect(cleanup).not.toContain("selectedRules");
    expect(cleanup).toContain('role="status"');
    expect(cleanup).toContain("각 항목의 크기와 수정 시각을 다시 확인합니다");
    expect(cleanup).not.toContain("active-use");
  });
});
