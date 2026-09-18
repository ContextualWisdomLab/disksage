import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/test.yml"), "utf8");

describe("macOS deletion/recovery acceptance owner", () => {
  it("runs the real macOS Trash recovery regression when the safety owner source is present", () => {
    expect(workflow).toContain(
      "macOS deletion/recovery regression when owner source is present",
    );
    expect(workflow).toContain(
      'if [[ -f "src-tauri/tests/macos_trash_staging_recovery.rs" ]]; then',
    );
    expect(workflow).toContain(
      "cargo test --locked --manifest-path src-tauri/Cargo.toml --test macos_trash_staging_recovery",
    );
    expect(workflow).toContain(
      "SKIP macos_trash_staging_recovery: owner source absent; no runtime regression executed",
    );
  });

  it("keeps the acceptance on a real macOS runner with exact-head checkout", () => {
    expect(workflow).toMatch(/macos-cache-cleanup:\s*\n\s*runs-on: macos-latest/);
    expect(workflow).toContain(
      "ref: ${{ github.event.pull_request.head.sha || github.sha }}",
    );
    expect(workflow).toContain("persist-credentials: false");
  });
});
