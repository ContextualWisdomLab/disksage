import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** Read the source-controlled Vitest configuration. */
function readVitestConfig(): string {
  return readFileSync(resolve(repositoryRoot, "vitest.config.ts"), "utf8");
}

describe("Podman desktop coverage contract", () => {
  it("keeps both Podman frontend production modules inside the exact 100% coverage gate", () => {
    const config = readVitestConfig();

    expect(config).toContain('"src/lib/podmanEvidence.ts"');
    expect(config).toContain('"src/lib/podmanEvidenceError.ts"');
    expect(config).toContain("statements: 100");
    expect(config).toContain("branches: 100");
    expect(config).toContain("functions: 100");
    expect(config).toContain("lines: 100");
  });
});
