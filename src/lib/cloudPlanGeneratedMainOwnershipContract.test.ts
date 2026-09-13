import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const buildScriptPath = "src-tauri/build.rs";
const cloudPlanBoundaryPath = "src-tauri/src/bin/disksage-cloud-plan.rs";

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cloud-plan generated process-main ownership", () => {
  it("keeps the binary boundary as the only non-coverage process main", () => {
    const buildScript = readSource(buildScriptPath);
    const boundary = readSource(cloudPlanBoundaryPath);

    expect(boundary).toContain('#[cfg(not(coverage))]\nfn main() {');
    expect(buildScript).toContain('const EMBEDDED_PROCESS_MAIN: &str =');
    expect(buildScript).toContain('.replacen(EMBEDDED_PROCESS_MAIN, "", 1)');
  });
});
