import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("Storybook accessibility workflow contract", () => {
  it("binds pull-request execution to the submitted exact head before exercising Storybook", () => {
    const workflow = readFileSync(
      resolve(repositoryRoot, ".github/workflows/storybook-accessibility.yml"),
      "utf8",
    );

    expect(workflow).toContain(
      "EXPECTED_HEAD_SHA: ${{ github.event.pull_request.head.sha || github.sha }}",
    );
    expect(workflow).toContain(
      "SOURCE_REPOSITORY: ${{ github.event.pull_request.head.repo.full_name || github.repository }}",
    );

    const checkoutIndex = workflow.indexOf("uses: actions/checkout@");
    const verifyIndex = workflow.indexOf("name: Verify exact source checkout");
    const dependencyInstallIndex = workflow.indexOf("run: npm ci");

    expect(checkoutIndex).toBeGreaterThanOrEqual(0);
    expect(verifyIndex).toBeGreaterThan(checkoutIndex);
    expect(dependencyInstallIndex).toBeGreaterThan(verifyIndex);

    const checkoutBlock = workflow.slice(checkoutIndex, verifyIndex);
    expect(checkoutBlock).toContain("repository: ${{ env.SOURCE_REPOSITORY }}");
    expect(checkoutBlock).toContain("ref: ${{ env.EXPECTED_HEAD_SHA }}");
    expect(checkoutBlock).toContain("persist-credentials: false");

    const verifyBlock = workflow.slice(verifyIndex, dependencyInstallIndex);
    expect(verifyBlock).toContain('actual_head="$(git rev-parse HEAD)"');
    expect(verifyBlock).toContain('test "$actual_head" = "$EXPECTED_HEAD_SHA"');
  });
});
