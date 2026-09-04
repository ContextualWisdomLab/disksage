import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function readAppShell(): string {
  return readFileSync(resolve(repositoryRoot, "src/app.html"), "utf8");
}

describe("application shell metadata", () => {
  it("identifies the primary document language as Korean", () => {
    expect(readAppShell()).toContain('<html lang="ko">');
  });

  it("uses the DiskSage product name instead of starter-template branding", () => {
    const source = readAppShell();

    expect(source).toContain("<title>DiskSage</title>");
    expect(source).not.toContain("Tauri + SvelteKit + Typescript App");
  });

  it("loads the reviewable DiskSage vector icon instead of the Svelte starter favicon", () => {
    const source = readAppShell();

    expect(source).toContain(
      '<link rel="icon" href="%sveltekit.assets%/favicon.svg" type="image/svg+xml" />',
    );
    expect(source).not.toContain("favicon.png");
  });
});
