import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/test.yml"), "utf8");

describe("test workflow path-filter contract", () => {
  it("does not put negative globs under paths-ignore", () => {
    const ignoreBlocks = workflow.matchAll(/paths-ignore:\n((?:\s+-\s+[^\n]+\n?)+)/g);

    for (const match of ignoreBlocks) {
      expect(match[1]).not.toMatch(/^\s*-\s+["']?!/m);
    }
  });
});
