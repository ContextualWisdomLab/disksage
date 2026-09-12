import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const providerRuntimePath = "src-tauri/src/provider_client_runtime.rs";

function readProviderRuntime(): string {
  return readFileSync(resolve(repositoryRoot, providerRuntimePath), "utf8");
}

describe("provider-client runtime platform ownership", () => {
  it("compiles macOS process-observation dependencies only for their owning platform", () => {
    const source = readProviderRuntime();
    const macosOnlyCfg = '#[cfg(all(not(coverage), target_os = "macos"))]';

    expect(source).toContain(`${macosOnlyCfg}\nuse std::io::Read;`);
    expect(source).toContain(`${macosOnlyCfg}\nuse std::process::{Command, Stdio};`);
    expect(source).toContain(`${macosOnlyCfg}\nuse std::time::{Duration, Instant};`);
    expect(source).toContain(`${macosOnlyCfg}\nconst PROCESS_OUTPUT_LIMIT: u64 = 64 * 1024;`);
    expect(source).toContain(
      `${macosOnlyCfg}\nconst PROCESS_TIMEOUT: Duration = Duration::from_secs(3);`,
    );

    // Persistence remains cross-platform and must not be accidentally narrowed with process polling.
    expect(source).toContain('#[cfg(not(coverage))]\nuse std::io::Write;');
  });
});
