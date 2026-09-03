import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/test.yml"), "utf8");

function negativePathsIgnoreEntries(source: string): string[] {
  const entries: string[] = [];
  const ignoreBlocks = source.matchAll(/paths-ignore:\n((?:\s+-\s+[^\n]+\n?)+)/g);
  for (const match of ignoreBlocks) {
    for (const line of match[1].split("\n")) {
      const item = line.match(/^\s*-\s+["']?(![^"'\s]+)["']?\s*$/);
      if (item) entries.push(item[1]);
    }
  }
  return entries;
}

describe("test workflow path-filter contract", () => {
  it("detects negative paths-ignore entries after comments and in inline lists", () => {
    const fixtures = [
      `pull_request:\n  paths-ignore:\n    - "docs/**"\n    # contract exception\n    - "!docs/example.md"\n`,
      `push:\n  paths-ignore: ["docs/**", "!docs/example.md"]\n`,
    ];

    for (const fixture of fixtures) {
      expect(negativePathsIgnoreEntries(fixture)).toContain("!docs/example.md");
    }
  });

  it("does not put negative globs under paths-ignore", () => {
    expect(negativePathsIgnoreEntries(workflow)).toEqual([]);
  });
});
