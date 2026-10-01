import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/test.yml"), "utf8");
const windowsJob = workflow.split("  windows-home-resolution:\n")[1]?.split("  llm-engine-build:")[0] ?? "";

const regression = "staging_creation_windows_substitution_does_not_delete_replacement";
const ownerContract = "src/lib/stagingCreationWindowsRollbackContract.test.ts";

describe("Windows deletion-safety Test admission", () => {
  it("uses the bounded-context contract as the admission marker instead of source-text discovery", () => {
    expect(windowsJob).toContain(`Test-Path '${ownerContract}'`);
    expect(windowsJob).not.toContain("Select-String -Path 'src-tauri/src/safety.rs'");
  });

  it("executes the exact staging rollback substitution regression when its owner contract is present", () => {
    expect(windowsJob).toContain("Windows staging rollback safety regression when owner contract is present");
    expect(windowsJob).toContain(
      `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib safety::tests::${regression} -- --exact`,
    );
  });

  it("requires positive proof that the Windows regression actually ran", () => {
    expect(windowsJob).toContain(`test safety::tests::${regression} ... ok`);
    expect(windowsJob).toContain("Windows staging rollback regression did not execute");
  });

  it("reports absent owner contract without inventing Windows runtime evidence", () => {
    expect(windowsJob).toContain(
      `SKIP ${regression}: owner contract absent; no runtime regression executed`,
    );
  });
});
