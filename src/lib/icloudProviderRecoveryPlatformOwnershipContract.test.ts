import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const recoveryPath = "src-tauri/src/icloud_provider_recovery.rs";

/** Read the canonical recovery source so platform-only filesystem identity code stays macOS-owned. */
function readRecoverySource(): string {
  return readFileSync(resolve(repositoryRoot, recoveryPath), "utf8");
}

describe("iCloud provider recovery platform ownership", () => {
  it("compiles Path only with the macOS daemon-identity implementation that consumes it", () => {
    const source = readRecoverySource();

    expect(source).toContain('#[cfg(target_os = "macos")]\nuse std::path::Path;');
    expect(source).toContain(
      '#[cfg(target_os = "macos")]\nfn executable_object_id(path: &Path) -> Result<String, String>',
    );
    expect(source).toContain(
      'executable_object_id(Path::new(FILE_PROVIDER_EXECUTABLE))?',
    );

    // Shared planning and authorization types remain available on every supported platform.
    expect(source).toContain('pub struct IcloudFileProviderRecoveryPlan');
    expect(source).toContain('pub fn plan_icloud_file_provider_recovery(');
  });
});
