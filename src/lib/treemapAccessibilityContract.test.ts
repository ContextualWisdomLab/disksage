import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Treemap accessible-equivalent ownership", () => {
  it("keeps the pointer canvas visual-only for assistive technology", () => {
    const treemap = readSource("src/lib/Treemap.svelte");

    expect(treemap).toContain('aria-hidden="true"');
    expect(treemap).toContain("onclick={click}");
  });

  it("does not duplicate the canonical node entry list inside Treemap", () => {
    const treemap = readSource("src/lib/Treemap.svelte");

    expect(treemap).not.toContain('class="accessible-tree"');
    expect(treemap).not.toContain("<details");
    expect(treemap).not.toContain("{#each node.entries");
  });
});
