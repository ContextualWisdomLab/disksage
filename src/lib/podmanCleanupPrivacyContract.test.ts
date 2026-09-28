import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** Read one production source file relative to the repository root. */
function source(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), "utf8");
}

describe("Cleanup Podman privacy and authority copy", () => {
  it("keeps the detailed Podman plan out of the customer cleanup surface", () => {
    const cleanup = source("src/lib/Cleanup.svelte");
    const page = source("src/routes/+page.svelte");
    const evidence = source("src/lib/PodmanEvidence.svelte");

    expect(cleanup).not.toContain("inspectPodmanReclaim");
    expect(cleanup).not.toContain("PodmanReclaimPlan");
    expect(page).toContain('import PodmanEvidence from "$lib/PodmanEvidence.svelte";');
    expect(page).toContain("<PodmanEvidence />");
    expect(evidence).toContain('invoke<T>("inspect_podman_desktop_evidence")');
  });
});
