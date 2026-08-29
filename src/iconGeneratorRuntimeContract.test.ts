import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const generatorPath = resolve(repositoryRoot, "scripts/generate-icons.mjs");
const sourcePath = resolve(repositoryRoot, "src-tauri/icons/icon-source.svg");
const contractPath = resolve(repositoryRoot, "src-tauri/icons/icon-contract.json");
const packagePath = resolve(repositoryRoot, "package.json");

const EXPECTED_RUNTIME = {
  node_major: 20,
  zlib: "1.3.0.1-motley-82a5fec",
};
const EXPECTED_NODE_VERSION = "20.19.0";

describe("deterministic icon generator runtime", () => {
  it("pins Test and Release to the audited Node runtime and compressor ABI", () => {
    const contract = JSON.parse(readFileSync(contractPath, "utf8")) as {
      generator_runtime?: { node_major?: number; zlib?: string };
    };
    const packageMetadata = JSON.parse(readFileSync(packagePath, "utf8")) as {
      engines?: { node?: string };
    };
    const testWorkflow = readFileSync(resolve(repositoryRoot, ".github/workflows/test.yml"), "utf8");
    const releaseWorkflow = readFileSync(
      resolve(repositoryRoot, ".github/workflows/release.yml"),
      "utf8",
    );

    expect(contract.generator_runtime).toEqual(EXPECTED_RUNTIME);
    expect(packageMetadata.engines?.node).toBe(EXPECTED_NODE_VERSION);
    expect(testWorkflow).toContain(`node-version: ${EXPECTED_NODE_VERSION}`);
    expect(releaseWorkflow).toContain(`node-version: ${EXPECTED_NODE_VERSION}`);
    expect(releaseWorkflow).not.toMatch(/node-version:\s*20\s*(?:#.*)?$/m);
  });

  it("fails closed before creating output when the runtime contract does not match", () => {
    const root = mkdtempSync(resolve(tmpdir(), "disksage-icon-runtime-mismatch-"));
    const copiedSource = resolve(root, "icon-source.svg");
    const copiedContract = resolve(root, "icon-contract.json");
    const outputDirectory = resolve(root, "generated");

    try {
      writeFileSync(copiedSource, readFileSync(sourcePath));
      const contract = JSON.parse(readFileSync(contractPath, "utf8")) as Record<string, unknown>;
      writeFileSync(
        copiedContract,
        `${JSON.stringify(
          {
            ...contract,
            generator_runtime: { ...EXPECTED_RUNTIME, node_major: 0 },
          },
          null,
          2,
        )}\n`,
      );

      const result = spawnSync(
        process.execPath,
        [
          generatorPath,
          "--source",
          copiedSource,
          "--contract",
          copiedContract,
          "--output",
          outputDirectory,
        ],
        { encoding: "utf8" },
      );

      expect(result.status).not.toBe(0);
      expect(result.stdout).toBe("");
      expect(result.stderr).toContain("icon-generator-runtime-mismatch");
      expect(existsSync(outputDirectory)).toBe(false);
    } finally {
      rmSync(root, { force: true, recursive: true });
    }
  });

  it("fails closed before creating output when generator bytes are not approved", () => {
    const root = mkdtempSync(resolve(tmpdir(), "disksage-icon-generator-mismatch-"));
    const copiedContract = resolve(root, "icon-contract.json");
    const outputDirectory = resolve(root, "generated");

    try {
      const contract = JSON.parse(readFileSync(contractPath, "utf8")) as Record<string, unknown>;
      writeFileSync(
        copiedContract,
        `${JSON.stringify({ ...contract, generator_sha256: "0".repeat(64) }, null, 2)}\n`,
      );
      const result = spawnSync(
        process.execPath,
        [generatorPath, "--source", sourcePath, "--contract", copiedContract, "--output", outputDirectory],
        { encoding: "utf8" },
      );

      expect(result.status).not.toBe(0);
      expect(result.stdout).toBe("");
      expect(result.stderr).toContain("icon-generator-identity-mismatch");
      expect(existsSync(outputDirectory)).toBe(false);
    } finally {
      rmSync(root, { force: true, recursive: true });
    }
  });

  it("records the exact compressor runtime in the generated integrity manifest", () => {
    const root = mkdtempSync(resolve(tmpdir(), "disksage-icon-runtime-manifest-"));
    try {
      const result = spawnSync(
        process.execPath,
        [
          generatorPath,
          "--source",
          sourcePath,
          "--contract",
          contractPath,
          "--output",
          root,
        ],
        { encoding: "utf8" },
      );
      expect(result.status).toBe(0);
      expect(result.stderr).toBe("");

      const manifest = JSON.parse(
        readFileSync(resolve(root, "icon-manifest.json"), "utf8"),
      ) as { generator_runtime?: { node?: string; zlib?: string } };
      expect(manifest.generator_runtime).toEqual({
        node: process.versions.node,
        zlib: process.versions.zlib,
      });
    } finally {
      rmSync(root, { force: true, recursive: true });
    }
  }, 30_000);
});
