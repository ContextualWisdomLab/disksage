import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("runtime storage desktop execution contract", () => {
  it("matches the redacted native receipt while preserving CLI-only streams", () => {
    const api = readFileSync(resolve(repositoryRoot, "src/lib/api.ts"), "utf8");
    const native = readFileSync(
      resolve(repositoryRoot, "src-tauri/src/runtime_storage.rs"),
      "utf8",
    );
    const cli = readFileSync(
      resolve(repositoryRoot, "src-tauri/src/bin/disksage-runtime-storage.rs"),
      "utf8",
    );
    const executionContract = api.match(
      /export interface RuntimeStorageExecution \{(?<body>[\s\S]*?)\n\}/,
    )?.groups?.body;

    expect(executionContract).toBeDefined();
    expect(executionContract).not.toContain("stdout:");
    expect(executionContract).not.toContain("stderr:");
    expect(native).toContain('#[serde(skip_serializing)]\n    pub stdout: String');
    expect(native).toContain('#[serde(skip_serializing)]\n    pub stderr: String');
    expect(cli).toContain('object.insert("stdout".into(), stdout.into())');
    expect(cli).toContain('object.insert("stderr".into(), stderr.into())');
  });
});
