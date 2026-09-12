import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("hourly contextual-orchestrator loop contract", () => {
  it("keeps the foreign orchestrator dependency read-only and delegates route discovery to orchestrator/free", () => {
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
      '"${base}/v1/models"',
      "OPENCODE_MODEL_CANDIDATES",
      "--max-time 120",
    ]) {
      expect(workflow).not.toContain(forbidden);
    }

    expect(workflow).toContain("ORCHESTRATOR_URL: ${{ secrets.CONTEXTUAL_ORCHESTRATOR_URL }}");
    expect(workflow).toContain("ORCHESTRATOR_TOKEN: ${{ secrets.CONTEXTUAL_ORCHESTRATOR_TOKEN }}");
    expect(workflow).toContain('--arg model "orchestrator/free"');
    expect(workflow).toContain('"${base}/v1/chat/completions"');
    expect(workflow).toContain("--connect-timeout 30");
    expect(workflow).toContain("persist-credentials: false");
    expect(workflow).toContain("gh pr list --state open --limit 100");
    expect(workflow).not.toContain("COPILOT_GITHUB_TOKEN");
    expect(workflow).toContain("--max-filesize 65536");
    expect(workflow).toContain("response_sha256");
    expect(workflow).toContain("hourly-product-loop-receipt-${{ github.run_id }}");
    expect(workflow).toContain("actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a");
    expect(workflow).toContain('status=" + .status');
    expect(workflow).not.toContain('model=" + .model + " status=');
    expect(workflow).not.toContain("/tmp/agent-ok.txt");
  });

  it("fails closed before repository or model work when gateway configuration is absent", () => {
    const workflow = readFileSync(
      resolve(repositoryRoot, ".github/workflows/hourly-product-loop.yml"),
      "utf8",
    );
    const configStart = workflow.indexOf("- name: Check orchestrator configuration");
    const checkoutStart = workflow.indexOf("- name: Checkout exact event commit");

    expect(configStart).toBeGreaterThanOrEqual(0);
    expect(checkoutStart).toBeGreaterThan(configStart);
    const configStep = workflow.slice(configStart, checkoutStart);
    expect(configStep).toContain('echo "configured=false" >> "$GITHUB_OUTPUT"');
    expect(configStep).toContain("exit 1");
    expect(workflow).not.toContain("- name: Explain missing orchestrator configuration");
    expect(workflow).not.toContain("if: steps.config.outputs.configured != 'true'");
  });

  it("binds repository context to the exact manually dispatched commit", () => {
    const workflow = readFileSync(
      resolve(repositoryRoot, ".github/workflows/hourly-product-loop.yml"),
      "utf8",
    );

    expect(workflow).toContain('ref: ${{ github.sha }}');
    expect(workflow).not.toContain("ref: main");
  });

  it("records orchestrator/free as a separate proposed decision without rewriting accepted ADR-0008", () => {
    const acceptedDecision = readFileSync(
      resolve(
        repositoryRoot,
        "docs/architecture/adr/0008-hourly-loop-foreign-dependencies-read-only.md",
      ),
      "utf8",
    );
    const proposedDecision = readFileSync(
      resolve(repositoryRoot, "docs/architecture/adr/0024-orchestrator-free-advisory-routing.md"),
      "utf8",
    );
    const decisionIndex = readFileSync(
      resolve(repositoryRoot, "docs/architecture/adr/README.md"),
      "utf8",
    );

    expect(acceptedDecision).toContain("**Status:** Accepted");
    expect(acceptedDecision).toContain("discovers a model through `/v1/models`");
    expect(proposedDecision).toContain("**Status:** Proposed");
    expect(proposedDecision).toContain("`orchestrator/free`");
    expect(proposedDecision).toContain("must not call `/v1/models`");
    expect(proposedDecision).toContain("connection-establishment timeout");
    expect(proposedDecision).toContain("total elapsed-time cutoff");
    expect(proposedDecision).toContain("fails closed when gateway configuration is missing");
    expect(decisionIndex).toContain(
      "[0024](0024-orchestrator-free-advisory-routing.md)",
    );
    expect(decisionIndex).toContain(
      "Delegate local advisory model selection to contextual-orchestrator | Proposed",
    );
  });
});
