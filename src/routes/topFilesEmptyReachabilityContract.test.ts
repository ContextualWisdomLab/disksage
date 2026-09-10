import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("TopFiles completed-scan reachability", () => {
  it("mounts TopFiles after a successful result load even when the result list is empty", () => {
    const source = readSource("src/routes/+page.svelte");

    expect(source).toContain("node = nextNode;\n          top = nextTop;");
    expect(source).toContain("{#if node}\n    <TopFiles files={top} />\n  {/if}");
    expect(source).not.toContain("{#if top.length > 0}");
  });

  it("keeps the TopFiles surface hidden before and during a new scan", () => {
    const source = readSource("src/routes/+page.svelte");
    const scanStart = source.indexOf("async function scan()");
    const openStart = source.indexOf("async function open(");
    expect(scanStart).toBeGreaterThanOrEqual(0);
    expect(openStart).toBeGreaterThan(scanStart);
    const scanScope = source.slice(scanStart, openStart);

    expect(scanScope).toMatch(/scanning = true;[\s\S]*node = null;[\s\S]*top = \[\];/);
  });
});
