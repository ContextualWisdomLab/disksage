import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflow = readFileSync(
  resolve(repositoryRoot, ".github/workflows/hourly-product-loop.yml"),
  "utf8",
);

const centralWorkflowSha = "e6334e229581a918e2f22de18733b76fa65d7e71";

describe("hourly product loop owner contract", () => {
  it("is a thin exact-SHA caller of the canonical review-repair owner", () => {
    expect(workflow).toContain(
      `uses: ContextualWisdomLab/.github/.github/workflows/pr-review-fix-scheduler.yml@${centralWorkflowSha}`,
    );
    expect(workflow).toContain("target_repository: ContextualWisdomLab/disksage");
    expect(workflow).toContain("base_branch: main");
    expect(workflow).toContain('max_prs: "200"');
    expect(workflow).toContain('max_dispatches: "1"');
    expect(workflow).toContain('scan_window_size: "50"');
    expect(workflow).toContain('retry_hours: "2"');
    expect(workflow).toContain("resolve_unreviewed_conflicts: true");
  });

  it("does not duplicate model routing or provider discovery in the product repository", () => {
    for (const forbidden of [
      "/v1/models",
      "/v1/chat/completions",
      "ORCHESTRATOR_URL",
      "ORCHESTRATOR_TOKEN",
      "CONTEXTUAL_ORCHESTRATOR_URL",
      "CONTEXTUAL_ORCHESTRATOR_TOKEN",
      "BYTEZ_API_KEY",
      "NVIDIA_NIM_API_KEY",
      "NVIDIA_NIM_API_KEY_SUB",
      "OPENROUTER_API_KEY",
      "OPENAI_API_KEY",
      "COPILOT_GITHUB_TOKEN",
      "model=",
      "model:\n",
      "provider",
    ]) {
      expect(workflow).not.toContain(forbidden);
    }
  });

  it("keeps the local entry point manual and grants only the reusable scheduler permissions it needs", () => {
    expect(workflow).toContain("workflow_dispatch:");
    expect(workflow).not.toMatch(/^\s*schedule:\s*$/mu);
    expect(workflow).toContain("contents: read");
    expect(workflow).toContain("id-token: write");
    expect(workflow).not.toContain("contents: write");
    expect(workflow).not.toContain("pull-requests: write");
  });
});
