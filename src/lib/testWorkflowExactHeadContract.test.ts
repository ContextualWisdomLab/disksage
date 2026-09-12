import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const workflow = readFileSync(
  new URL("../../.github/workflows/test.yml", import.meta.url),
  "utf8",
);

/**
 * Splits the workflow into top-level action/run step blocks so named checkout
 * steps and shorthand `- uses:` checkout steps are governed by the same contract.
 */
function workflowStepBlocks(source: string): string[] {
  const lines = source.split(/\r?\n/);
  const starts = lines.flatMap((line, index) =>
    /^(\s*)-\s+(?:name:|uses:|run:)/.test(line) ? [index] : [],
  );

  return starts.map((start, index) =>
    lines.slice(start, starts[index + 1] ?? lines.length).join("\n"),
  );
}

describe("Test workflow checkout provenance", () => {
  it("pins every checkout to the exact pull-request head or push SHA", () => {
    const checkoutBlocks = workflowStepBlocks(workflow).filter((block) =>
      block.includes("uses: actions/checkout@"),
    );

    expect(checkoutBlocks.length).toBeGreaterThanOrEqual(4);
    for (const block of checkoutBlocks) {
      expect(block).toContain("persist-credentials: false");
      expect(block).toContain(
        "ref: ${{ github.event.pull_request.head.sha || github.sha }}",
      );
    }
  });
});
