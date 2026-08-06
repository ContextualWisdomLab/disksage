import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      // Measure every source-controlled production TypeScript module and the
      // cross-platform release-version admission module. Test files, generated
      // declarations, and Svelte component markup use separate contracts.
      include: [
        "src/lib/**/*.ts",
        "src/routes/**/*.ts",
        "scripts/ci/release-version.mjs",
      ],
      exclude: ["**/*.test.ts", "**/*.d.ts"],
      reporter: ["text", "json", "json-summary"],
      thresholds: {
        statements: 100,
        branches: 100,
        functions: 100,
        lines: 100,
      },
    },
  },
});
