import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("hourly contextual-orchestrator loop contract", () => {
  it("bootstraps all provider credentials through the KV stdin boundary", () => {
    const workflow = readFileSync(
      resolve(repositoryRoot, ".github/workflows/hourly-product-loop.yml"),
      "utf8",
    );

    for (const secret of [
      "BYTEZ_API_KEY",
      "NVIDIA_NIM_API_KEY",
      "NVIDIA_NIM_API_KEY_SUB",
      "OPENROUTER_API_KEY",
      "OPENAI_API_KEY",
    ]) {
      expect(workflow).toContain(`secrets.${secret}`);
      expect(workflow).toContain(`register_credential ${secret}`);
    }
    expect(workflow).toContain("register-credential --name \"$name\" --value-stdin");
    expect(workflow).toContain("ref: e226e1197bdfc890c9d8e5b9b648c78857d7e465");
    expect(workflow).toContain("always() && needs.bootstrap-contextual-orchestrator-credentials.result != 'failure'");
    expect(workflow).not.toContain("COPILOT_GITHUB_TOKEN");

    const runtimeAgentJob = workflow.split("contextual-orchestrator-opencode:", 2)[1];
    expect(runtimeAgentJob).not.toContain("BYTEZ_API_KEY");
    expect(runtimeAgentJob).not.toContain("NVIDIA_NIM_API_KEY");
    expect(runtimeAgentJob).not.toContain("OPENROUTER_API_KEY");
    expect(runtimeAgentJob).not.toContain("OPENAI_API_KEY");
  });
});
