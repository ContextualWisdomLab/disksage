import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("post-scan result generation", () => {
  it("does not let an older result load overwrite a newer scan", () => {
    const source = readSource("src/routes/+page.svelte");
    const mountStart = source.indexOf("onMount(async () => {");
    const scanStart = source.indexOf("async function scan()");
    expect(mountStart).toBeGreaterThanOrEqual(0);
    expect(scanStart).toBeGreaterThan(mountStart);
    const mountScope = source.slice(mountStart, scanStart);

    expect(mountScope).toMatch(
      /api\.onScanDone\(async \(s\) => \{[\s\S]*const resultSeq = navSeq;[\s\S]*await Promise\.all\([\s\S]*if \(resultSeq !== navSeq\) return;[\s\S]*crumbs = \[scannedRoot\]/,
    );
  });
});
