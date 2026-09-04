import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("release attestation workflow contract", () => {
  it("checks out the exact source before downloading release artifacts", () => {
    const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/release.yml"), "utf8");
    const attestStart = workflow.indexOf("  attest-release:");
    const publishStart = workflow.indexOf("  publish-release:");
    expect(attestStart).toBeGreaterThanOrEqual(0);
    expect(publishStart).toBeGreaterThan(attestStart);

    const attestJob = workflow.slice(attestStart, publishStart);
    const checkoutIndex = attestJob.indexOf("actions/checkout@");
    const downloadIndex = attestJob.indexOf("name: Download exact release artifact set");
    const verifierIndex = attestJob.indexOf(
      'bash .github/scripts/verify-release-artifacts.sh release-artifacts "${{ github.run_attempt }}"',
    );
    expect(checkoutIndex).toBeGreaterThanOrEqual(0);
    expect(downloadIndex).toBeGreaterThanOrEqual(0);
    expect(verifierIndex).toBeGreaterThan(downloadIndex);
    expect(checkoutIndex).toBeLessThan(downloadIndex);
  });

  it("binds Cargo SBOM metadata to the shipped Rust manifest", () => {
    const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/release.yml"), "utf8");
    const attestStart = workflow.indexOf("  attest-release:");
    const publishStart = workflow.indexOf("  publish-release:");
    const attestJob = workflow.slice(attestStart, publishStart);

    expect(attestJob).toContain(
      "cargo metadata --locked --format-version=1 --manifest-path src-tauri/Cargo.toml",
    );
  });
});
