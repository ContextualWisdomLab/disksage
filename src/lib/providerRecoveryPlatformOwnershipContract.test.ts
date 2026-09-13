import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const providerRecoveryPath = "src-tauri/src/provider_recovery.rs";

/** Read the checked-in recovery adapter so unsupported-platform parameters remain explicitly owned. */
function readProviderRecovery(): string {
  return readFileSync(resolve(repositoryRoot, providerRecoveryPath), "utf8");
}

describe("provider recovery platform ownership", () => {
  it("consumes the graceful-termination option in the non-macOS unsupported branch", () => {
    const source = readProviderRecovery();

    expect(source).toContain(
      'pub fn recover_provider_client_with_options(\n    provider: CloudProvider,\n    observed_at_ms: u64,\n    allow_graceful_term: bool,\n)',
    );
    expect(source).toContain(
      '#[cfg(not(target_os = "macos"))]\n    {\n        let _ = (provider, observed_at_ms, allow_graceful_term);\n        return Err("provider-recovery-platform-unsupported".into());\n    }',
    );
  });
});
