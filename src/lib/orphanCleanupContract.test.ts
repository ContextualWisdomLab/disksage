import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("orphan cleanup safety contract", () => {
  it("keeps the public plan path-free and requires the exact approval phrase", () => {
    const api = readFileSync(resolve(root, "src/lib/api.ts"), "utf8");
    const component = readFileSync(resolve(root, "src/lib/OrphanCleanup.svelte"), "utf8");
    expect(api).toContain("plan_orphan_cleanup");
    expect(api).toContain("clean_orphan_candidates");
    expect(component).toContain("Application Support");
    expect(component).toContain("candidate.auto_trash_eligible");
    expect(component).toContain("plan.exact_approval_phrase");
    expect(component).toContain("candidate.metadata_fingerprint");
    expect(component).not.toContain("candidate.path");
  });
});
