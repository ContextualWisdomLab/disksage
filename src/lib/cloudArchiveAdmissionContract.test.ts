import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("CloudArchive iCloud admission contract", () => {
  it("clears stale health evidence when refresh fails", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/CloudArchive.svelte"), "utf8");
    expect(source).toContain("icloudHealth = null;");
    expect(source).toContain("icloudHealth?.new_copy_admission_state !== \"clear\"");
  });
});
