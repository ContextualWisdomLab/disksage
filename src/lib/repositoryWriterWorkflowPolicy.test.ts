import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflowDirectory = resolve(repositoryRoot, ".github/workflows");

/** Read one source-controlled workflow file as UTF-8 text. */
function readWorkflow(name: string): string {
  return readFileSync(resolve(workflowDirectory, name), "utf8");
}

/** Return every source-controlled GitHub Actions workflow filename. */
function workflowNames(): string[] {
  return readdirSync(workflowDirectory)
    .filter((name) => name.endsWith(".yml") || name.endsWith(".yaml"))
    .sort();
}

describe("repository writer workflow policy", () => {
  it("does not retain branch-local PR repair writers", () => {
    const repairWorkflows = workflowNames().filter((name) =>
      /^repair-pr-\d+\.(?:ya?ml)$/u.test(name),
    );

    expect(repairWorkflows).toEqual([]);
  });

  it("does not hide a legacy repair writer under another filename", () => {
    const suspicious = workflowNames().filter((name) => {
      const workflow = readWorkflow(name);
      return (
        workflow.includes("Apply complete bounded review fixes") ||
        (workflow.includes("permissions:") &&
          workflow.includes("contents: write") &&
          workflow.includes("git push"))
      );
    });

    expect(suspicious).toEqual([]);
  });
});
