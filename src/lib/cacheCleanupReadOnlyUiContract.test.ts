import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cache cleanup fail-closed UX", () => {
  it("does not offer a destructive cache action while atomic trash is unavailable", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");

    expect(cleanup).not.toContain("cleanCacheContents");
    expect(cleanup).not.toContain("selectedRules");
    expect(cleanup).toContain('role="status"');
    expect(cleanup).toContain("캐시 항목은 현재 읽기 전용입니다");
  });
});
