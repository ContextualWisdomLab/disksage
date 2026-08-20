import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("hourly contextual-orchestrator loop contract", () => {
  it("keeps the foreign orchestrator dependency read-only and uses only its published runtime API", () => {
    const workflow = readFileSync(
      resolve(repositoryRoot, ".github/workflows/hourly-product-loop.yml"),
      "utf8",
    );

    for (const forbidden of [
      "CONTEXTUAL_ORCHESTRATOR_KV_DSN",
      "CONTEXTUAL_ORCHESTRATOR_KV_PASSPHRASE",
      "BYTEZ_API_KEY",
      "NVIDIA_NIM_API_KEY",
      "NVIDIA_NIM_API_KEY_SUB",
      "OPENROUTER_API_KEY",
      "OPENAI_API_KEY",
      "repository: ContextualWisdomLab/contextual-orchestrator",
      "register-credential",
      "bootstrap-contextual-orchestrator-credentials",
      "python3 -m pip install",
    ]) {
      expect(workflow).not.toContain(forbidden);
    }

    expect(workflow).toContain("ORCHESTRATOR_URL: ${{ secrets.CONTEXTUAL_ORCHESTRATOR_URL }}");
    expect(workflow).toContain("ORCHESTRATOR_TOKEN: ${{ secrets.CONTEXTUAL_ORCHESTRATOR_TOKEN }}");
    expect(workflow).toContain('"${base}/v1/models"');
    expect(workflow).toContain('"${base}/v1/chat/completions"');
    expect(workflow).toContain("persist-credentials: false");
    expect(workflow).toContain("gh pr list --state open --limit 100");
    expect(workflow).not.toContain("COPILOT_GITHUB_TOKEN");
  });

  it("binds repository context to the exact scheduled or manually dispatched commit", () => {
    const workflow = readFileSync(
      resolve(repositoryRoot, ".github/workflows/hourly-product-loop.yml"),
      "utf8",
    );

    expect(workflow).toContain('ref: ${{ github.sha }}');
    expect(workflow).not.toContain("ref: main");
  });
});
