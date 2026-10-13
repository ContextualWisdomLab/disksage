import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/test.yml"), "utf8");

function jobBlock(source: string, jobName: string, nextJobName: string): string {
  return source.split(`  ${jobName}:\n`)[1]?.split(`  ${nextJobName}:\n`)[0] ?? "";
}

describe("canonical Ubuntu Test evidence budget", () => {
  it("does not truncate the full evidence chain at the historical 30-minute ceiling", () => {
    const ubuntuJob = jobBlock(workflow, "test", "macos-cache-cleanup");
    const timeout = ubuntuJob.match(/^    timeout-minutes:\s*(\d+)\s*$/m);

    expect(timeout, "canonical Ubuntu Test job must declare its evidence budget").not.toBeNull();
    expect(Number(timeout?.[1] ?? 0)).toBeGreaterThanOrEqual(60);

    for (const phase of [
      "Rust tests (includes unix symlink test)",
      "Headless cloud planner tests",
      "Exact duplicate audit tests",
      "Extraction-free archive tree proof tests",
      "Run npm test",
      "npm run build",
    ]) {
      expect(ubuntuJob).toContain(phase);
    }
  });
});
