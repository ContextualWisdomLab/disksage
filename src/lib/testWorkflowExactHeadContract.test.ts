import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const workflow = readFileSync(
  new URL("../../.github/workflows/test.yml", import.meta.url),
  "utf8",
);

describe("Test workflow checkout provenance", () => {
  it("pins every checkout to the exact pull-request head or push SHA", () => {
    const lines = workflow.split("\n");
    const checkoutIndexes = lines.flatMap((line, index) =>
      line.includes("- uses: actions/checkout@") ? [index] : [],
    );

    expect(checkoutIndexes.length).toBeGreaterThanOrEqual(4);
    for (const checkoutIndex of checkoutIndexes) {
      const stepIndent = lines[checkoutIndex].match(/^(\s*)/)?.[1] ?? "";
      let endIndex = checkoutIndex + 1;
      while (
        endIndex < lines.length &&
        !lines[endIndex].startsWith(`${stepIndent}- `)
      ) {
        endIndex += 1;
      }
      const block = lines.slice(checkoutIndex, endIndex).join("\n");

      expect(lines[checkoutIndex + 1]?.trim()).toBe("with:");
      expect(block).toContain("persist-credentials: false");
      expect(block).toContain(
        "ref: ${{ github.event.pull_request.head.sha || github.sha }}",
      );
    }
  });
});
