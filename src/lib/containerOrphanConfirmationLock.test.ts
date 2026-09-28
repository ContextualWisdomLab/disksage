import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("container orphan confirmation lock", () => {
  it("acquires the cleanup lock before opening the asynchronous confirmation dialog", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/ContainerOrphanCleanup.svelte"), "utf8");
    const start = source.indexOf("async function prune(");
    const end = source.indexOf("</script>", start);
    const pruneBody = source.slice(start, end);
    const lock = pruneBody.indexOf("pruneBusyKey = key;");
    const confirmation = pruneBody.indexOf("await confirm(");

    expect(start).toBeGreaterThanOrEqual(0);
    expect(lock).toBeGreaterThanOrEqual(0);
    expect(confirmation).toBeGreaterThanOrEqual(0);
    expect(lock).toBeLessThan(confirmation);
  });
});
