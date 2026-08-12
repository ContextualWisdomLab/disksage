import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Treemap accessible equivalent", () => {
  it("does not make the pointer-only canvas the sole navigation surface", () => {
    const source = readSource("src/lib/Treemap.svelte");

    expect(source).toContain('aria-hidden="true"');
    expect(source).toContain('class="accessible-tree"');
    expect(source).toContain("{#each node.entries as entry (entry.path)}");
    expect(source).toContain("{#if entry.is_dir}");
    expect(source).toContain("onclick={() => onOpen(entry.path)}");
    expect(source).toContain("fmtBytes(entry.size)");
  });

  it("exposes the equivalent navigation through native keyboard controls", () => {
    const source = readSource("src/lib/Treemap.svelte");

    expect(source).toContain("<details");
    expect(source).toContain("<summary>접근 가능한 항목 목록</summary>");
    expect(source).toMatch(/<button[\s\S]*onclick=\{\(\) => onOpen\(entry\.path\)\}/);
  });
});
