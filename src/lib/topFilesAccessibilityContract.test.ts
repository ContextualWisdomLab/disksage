import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("TopFiles accessible data table", () => {
  it("gives the table a programmatic name from its visible heading", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain('<h2 id="top-files-heading">');
    expect(source).toContain('<table aria-labelledby="top-files-heading">');
  });

  it("associates both header cells explicitly with their columns", () => {
    const source = readSource("src/lib/TopFiles.svelte");

    expect(source).toContain('<th scope="col">크기</th>');
    expect(source).toContain('<th scope="col">경로</th>');
  });
});
