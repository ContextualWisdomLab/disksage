import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      // Measure every source-controlled production TypeScript module. Test files,
      // generated declarations, and Svelte component markup are excluded because
      // they have separate deterministic contract and build verification paths.
      include: ["src/lib/**/*.ts", "src/routes/**/*.ts"],
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
