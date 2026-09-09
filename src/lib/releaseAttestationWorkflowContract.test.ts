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
      'bash .github/scripts/verify-release-artifacts.sh release-artifacts "${{ github.run_id }}"',
    );
    expect(checkoutIndex).toBeGreaterThanOrEqual(0);
    expect(downloadIndex).toBeGreaterThanOrEqual(0);
    expect(verifierIndex).toBeGreaterThan(downloadIndex);
    expect(checkoutIndex).toBeLessThan(downloadIndex);
    expect(attestJob).toContain("expected exactly 43 regular files before SBOM generation");
    expect(attestJob).toContain("expected exactly 44 regular files after SBOM generation");
    const verifier = readFileSync(resolve(repositoryRoot, ".github/scripts/verify-release-artifacts.sh"), "utf8");
    expect(verifier).toContain("expected exactly 43 regular files");
    expect(verifier).toContain('require_exactly_one_file "${expected_dirs[0]}" "$required_cli"');
    expect(verifier).toContain('require_exactly_one_file "${expected_dirs[2]}" "$required_cli"');
    expect(verifier).toContain('disksage-podman-storage-repair-linux-x86_64');
  });

  it("ships the cleanup executables exposed by this release line", () => {
    const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/release.yml"), "utf8");

    for (const executable of [
      "disksage-podman-storage-repair",
      "disksage-photo-similarity-audit",
      "disksage-shared-temp-reclaim-plan",
    ]) {
      expect(workflow).toContain(`--bin ${executable}`);
      expect(workflow).toContain(`${executable}-macos-arm64`);
      expect(workflow).toContain(`${executable}-macos-arm64.sha256`);
    }
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