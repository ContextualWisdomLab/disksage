import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/test.yml"), "utf8");
const windowsJob = workflow.split("  windows-home-resolution:\n")[1]?.split("  llm-engine-build:")[0] ?? "";

const ownerFixture = "src-tauri/tests/cargo_target_reclaim_windows.rs";

describe("Windows Cargo target reclaim Test admission", () => {
  it("runs the real owner integration fixture when its source is present", () => {
    expect(windowsJob).toContain("Windows Cargo target reclaim regression when owner source is present");
    expect(windowsJob).toContain(`Test-Path '${ownerFixture}'`);
    expect(windowsJob).toContain(
      "cargo test --manifest-path src-tauri/Cargo.toml --locked --test cargo_target_reclaim_windows",
    );
  });

  it("does not invent Windows reclaim evidence when the owner fixture is absent", () => {
    expect(windowsJob).toContain(
      "SKIP cargo_target_reclaim_windows: owner source absent; no runtime regression executed",
    );
  });
});
