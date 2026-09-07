import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const workflow = readFileSync(
  new URL("../../.github/workflows/test.yml", import.meta.url),
  "utf8",
);

describe("Test workflow checkout provenance", () => {
  it("pins every checkout to the exact pull-request head or push SHA", () => {
    const checkoutBlocks =
      workflow.match(
        /- uses: actions\/checkout@[^\n]+\n\s+with:\n(?:\s+[^\n]+\n)+/g,
      ) ?? [];

    expect(checkoutBlocks).toHaveLength(3);
    for (const block of checkoutBlocks) {
      expect(block).toContain("persist-credentials: false");
      expect(block).toContain(
        "ref: ${{ github.event.pull_request.head.sha || github.sha }}",
      );
    }
  });
});
