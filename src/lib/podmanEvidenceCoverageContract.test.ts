import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** Read a source-controlled file relative to the repository root. */
function readRepositoryFile(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Podman desktop coverage contract", () => {
  it("keeps both Podman frontend production modules inside the exact 100% coverage gate", () => {
    const config = readRepositoryFile("vitest.config.ts");

    expect(config).toContain('include: ["src/lib/**/*.ts", "src/routes/**/*.ts"]');
    expect(config).toContain('exclude: ["**/*.test.ts", "**/*.d.ts"]');
    for (const modulePath of [
      "src/lib/podmanEvidence.ts",
      "src/lib/podmanEvidenceError.ts",
    ]) {
      expect(readRepositoryFile(modulePath).length).toBeGreaterThan(0);
      expect(modulePath.startsWith("src/lib/")).toBe(true);
      expect(modulePath.endsWith(".ts")).toBe(true);
      expect(modulePath.endsWith(".test.ts")).toBe(false);
      expect(modulePath.endsWith(".d.ts")).toBe(false);
    }
    expect(config).toContain("statements: 100");
    expect(config).toContain("branches: 100");
    expect(config).toContain("functions: 100");
    expect(config).toContain("lines: 100");
  });
});
