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
    expect(component).toContain("앱이 사용하는 폴더");
    expect(component).not.toContain("Info.plist");
    expect(component).not.toContain("Library 후보");
    expect(component).toContain("candidate.auto_trash_eligible");
    expect(component).toContain("plan.exact_approval_phrase");
    expect(component).toContain("candidate.metadata_fingerprint");
    expect(component).not.toContain("candidate.path");
    expect(component).toContain("cleanAndRefreshOrphanPlan");
    expect(component).toContain("outcome.refresh_failed");

    // HOME identity must not exist in either the public TypeScript contract or the Rust wire type.
    expect(api).not.toContain("root_fingerprint");
    expect(orphan).not.toContain("root_fingerprint: String");

    // A globally incomplete plan is fail-closed in the UI before the backend submission boundary.
    expect(component).toContain("!plan.scan_complete");

    // Frontend admission must match the backend's normalized audit-rationale contract.
    expect(component).toContain("rationale.trim()");

    // Arbitrary backend/native exception text must not cross the customer-visible boundary.
    expect(component).not.toContain("String(e)");
    expect(orphan).not.toContain("error: Some(error.to_string())");
    expect(orphan).toContain('error: Some("orphan-trash-operation-failed".into())');

    // The real user-home root is a valid planning scope; generic cleanup protection intentionally
    // treats HOME itself as protected and therefore cannot be reused as the planner admission test.
    expect(orphan).not.toContain("crate::safety::is_protected(&canonical_home)");
    expect(orphan).toContain("planner_home_scope_is_safe");

    // Validate the complete submitted batch before the first filesystem mutation. A later stale
    // request must not turn an early request into an unreported partial mutation.
    expect(orphan).toContain("validate_cleanup_requests(plan, requests)?");
    expect(orphan).toContain("for candidate in prepared");
  });

  it("owns the child component styles it relies on instead of inheriting scoped parent CSS", () => {
    const component = readFileSync(resolve(root, "src/lib/OrphanCleanup.svelte"), "utf8");

    expect(component).toContain("<style>");
    for (const selector of [".notice", ".list", ".error", ".muted", ".disabled"]) {
      expect(component).toContain(selector);
    }
  });
});
