import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

describe("cross-platform icon source integrity", () => {
  it("keeps every hashed or mirrored icon input on LF across Windows and Unix checkouts", () => {
    const attributes = readFileSync(resolve(repositoryRoot, ".gitattributes"), "utf8");

    expect(attributes).toContain("src-tauri/icons/icon-source.svg text eol=lf");
    expect(attributes).toContain("static/favicon.svg text eol=lf");
    expect(attributes).toContain("src-tauri/icons/icon-contract.json text eol=lf");
    expect(attributes).toContain("scripts/generate-icons.mjs text eol=lf");
  });
});
