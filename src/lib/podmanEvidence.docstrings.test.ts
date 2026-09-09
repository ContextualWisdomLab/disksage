import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./podmanEvidence.ts", import.meta.url), "utf8");
const productionFunctions = [
  ...source.matchAll(/^(?:export\s+)?(?:async\s+)?function\s+([A-Za-z0-9_]+)/gm),
].map((match) => match[1]);

describe("Podman evidence documentation contract", () => {
  it("keeps every production function beginner-readable with an adjacent JSDoc", () => {
    expect(productionFunctions.length).toBeGreaterThan(0);

    for (const functionName of productionFunctions) {
      const documentedFunction = new RegExp(
        String.raw`/\*\*[\s\S]*?\*/\s*(?:export\s+)?(?:async\s+)?function\s+${functionName}\b`,
      );
      expect(source, `missing adjacent JSDoc for ${functionName}`).toMatch(documentedFunction);
    }
  });
});
