import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("cloud-offload Goal projection contract", () => {
  it("keeps recovery guidance and evidence policy in the replaceable Goal", () => {
    const goal = JSON.parse(
      readFileSync(
        resolve(repositoryRoot, "docs/architecture/goals/cloud-offload-goal.json"),
        "utf8",
      ),
    ) as {
      operator_actions?: string[];
      runtime_evidence_failure_policy?: string;
      pre_copy_evidence_streams?: string[];
      lineage_relation_identifier_rule?: string;
      states?: string[];
      completion_gates?: string[];
    };

    expect(goal.operator_actions).toContain("cancel-finder-copy");
    expect(goal.operator_actions).toEqual(expect.arrayContaining([
      "plan_orphan_cleanup",
      "clean_orphan_candidates",
    ]));
    expect(goal.runtime_evidence_failure_policy).toContain("fail-closed");
    expect(goal.runtime_evidence_failure_policy).toContain("not process absence");
    expect(goal.states).toContain("provider-sync-incomplete");
    expect(goal.completion_gates).toContain("destination-headroom-bound");
    expect(goal.pre_copy_evidence_streams).toEqual(expect.arrayContaining([
      "provider-client-runtime-evidence",
      "icloud-sync-health-evidence",
      "provider-global-sync-evidence",
    ]));
    expect(goal.lineage_relation_identifier_rule).toContain("never a raw local or provider path");
  });
});
