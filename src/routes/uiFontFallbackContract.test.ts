import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("material UI font fallback", () => {
  it("declares explicit cross-platform CJK fallbacks on the application surface", () => {
    const page = readSource("src/routes/+page.svelte");

    expect(page).toContain('"Apple SD Gothic Neo"');
    expect(page).toContain('"Malgun Gothic"');
    expect(page).toContain('"Hiragino Sans"');
    expect(page).toContain('"Yu Gothic UI"');
    expect(page).toContain('"PingFang SC"');
    expect(page).toContain('"PingFang TC"');
    expect(page).toContain('"Microsoft YaHei"');
    expect(page).toContain('"Microsoft JhengHei"');
    expect(page).toContain('"Noto Sans CJK KR"');
    expect(page).toContain('"Noto Sans CJK JP"');
    expect(page).toContain('"Noto Sans CJK SC"');
    expect(page).toContain('"Noto Sans CJK TC"');
    expect(page).toContain('"Noto Sans"');
  });
});
