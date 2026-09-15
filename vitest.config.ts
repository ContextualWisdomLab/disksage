import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    // GitHub-hosted Ubuntu runners can expose enough logical CPUs for Vitest's
    // default forks pool to exceed the job's memory budget. Keep file isolation
    // and the complete test set; only bound concurrent worker processes in CI.
    maxWorkers: process.env.CI ? 2 : undefined,
    coverage: {
      provider: "v8",
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
