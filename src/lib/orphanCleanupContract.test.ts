import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("orphan cleanup safety contract", () => {
  it("keeps the public plan path-free and requires the exact approval phrase", () => {
    const api = readFileSync(resolve(root, "src/lib/api.ts"), "utf8");
    const component = readFileSync(resolve(root, "src/lib/OrphanCleanup.svelte"), "utf8");
    const orphan = readFileSync(resolve(root, "src-tauri/src/orphan.rs"), "utf8");

    expect(api).toContain("plan_orphan_cleanup");
    expect(api).toContain("clean_orphan_candidates");
    expect(component).toContain("Application Support");
    expect(component).toContain("candidate.auto_trash_eligible");
    expect(component).toContain("plan.exact_approval_phrase");
    expect(component).toContain("candidate.metadata_fingerprint");
    expect(component).not.toContain("candidate.path");

    // Public/browser evidence must not expose even a dictionary-recoverable digest of HOME.
    expect(api).not.toContain("root_fingerprint");

    // A globally incomplete plan is fail-closed in the UI before the backend submission boundary.
    expect(component).toContain("!plan.scan_complete");

    // Frontend admission must match the backend's normalized audit-rationale contract.
    expect(component).toContain("rationale.trim()");

    // Arbitrary backend/native exception text must not cross the customer-visible boundary.
    expect(component).not.toContain("String(e)");
    expect(orphan).not.toContain("error: Some(error.to_string())");

    // The real user-home root is a valid planning scope; generic cleanup protection intentionally
    // treats HOME itself as protected and therefore cannot be reused as the planner admission test.
    expect(orphan).not.toContain("crate::safety::is_protected(&canonical_home)");
  });
});
