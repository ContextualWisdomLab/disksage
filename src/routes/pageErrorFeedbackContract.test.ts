import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("main scan and navigation failure feedback", () => {
  it("does not expose arbitrary exceptions or hide operation failures in the console", () => {
    const source = readSource("src/routes/+page.svelte");

    expect(source).not.toContain('alert(`스캔 시작 실패: ${e}`)');
    expect(source).not.toContain('console.error("post-scan load failed:", e)');
    expect(source).not.toContain('console.error("getNode failed:", e)');
    expect(source).toContain("디스크 루트 목록을 불러오지 못했습니다.");
    expect(source).toContain("스캔 결과를 불러오지 못했습니다.");
    expect(source).toContain("스캔을 시작하지 못했습니다.");
    expect(source).toContain("폴더 내용을 불러오지 못했습니다.");
  });

  it("preserves scan and navigation authority behind one accessible alert", () => {
    const source = readSource("src/routes/+page.svelte");

    expect(source).toContain('role="alert"');
    expect(source).toContain("api.listRoots()");
    expect(source).toContain("api.onScanProgress(");
    expect(source).toContain("api.onScanDone(");
    expect(source).toContain("api.startScan(selectedRoot)");
    expect(source).toContain("api.getNode(");
    expect(source).toContain("api.topFiles(200)");
  });
});
