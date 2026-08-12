import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("settings persistence feedback", () => {
  it("surfaces load and save failures instead of silently swallowing them", () => {
    const source = readSource("src/lib/Settings.svelte");

    expect(source).not.toContain(".catch(() => {})");
    expect(source).not.toMatch(/catch\s*\{\s*\}/);
    expect(source).toContain("설정을 불러오지 못했습니다.");
    expect(source).toContain("설정을 저장하지 못했습니다.");
    expect(source).toContain('role="alert"');
  });

  it("clears stale failure feedback before retrying an operation", () => {
    const source = readSource("src/lib/Settings.svelte");

    expect(source).toContain('error = "";');
  });

  it("keeps the toggle disabled until the initial persisted value settles", () => {
    const source = readSource("src/lib/Settings.svelte");

    expect(source).toContain("let busy = $state(true);");
    expect(source).toContain(".finally(() => {");
    expect(source).toContain("busy = false;");
    expect(source).toContain("disabled={busy}");
  });
});
