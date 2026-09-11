import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** Reads checked-in source so the contract cannot pass against a detached fixture. */
function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("material UI font fallback", () => {
  it("declares explicit cross-platform CJK fallbacks in the application font stack", () => {
    const page = readSource("src/routes/+page.svelte");
    const declaration = page.match(/font-family:\s*([^;]+);/)?.[1] ?? "";

    expect(declaration).toContain("system-ui");
    expect(declaration).toContain('"Apple SD Gothic Neo"');
    expect(declaration).toContain('"Malgun Gothic"');
    expect(declaration).toContain('"Hiragino Sans"');
    expect(declaration).toContain('"Yu Gothic UI"');
    expect(declaration).toContain('"PingFang SC"');
    expect(declaration).toContain('"PingFang TC"');
    expect(declaration).toContain('"Microsoft YaHei"');
    expect(declaration).toContain('"Microsoft JhengHei"');
    expect(declaration).toContain('"Noto Sans CJK KR"');
    expect(declaration).toContain('"Noto Sans CJK JP"');
    expect(declaration).toContain('"Noto Sans CJK SC"');
    expect(declaration).toContain('"Noto Sans CJK TC"');
    expect(declaration).toContain('"Noto Sans"');
    expect(declaration.trim().endsWith("sans-serif")).toBe(true);
  });
});
