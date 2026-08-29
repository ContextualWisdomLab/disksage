import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

function between(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing scope start: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing scope end: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("main scan and navigation failure feedback", () => {
  it("does not expose arbitrary exceptions or hide operation failures in the console", () => {
    const source = readSource("src/routes/+page.svelte");
    const mountScope = between(source, "onMount(async () => {", "async function scan()");
    const scanScope = between(source, "async function scan()", "async function open(");
    const openScope = between(source, "async function open(", "async function jump(");
    const jumpScope = between(source, "async function jump(", "</script>");

    expect(source).not.toContain('alert(`스캔 시작 실패: ${e}`)');
    expect(source).not.toContain('console.error("post-scan load failed:", e)');
    expect(source).not.toContain('console.error("getNode failed:", e)');

    expect(mountScope).toContain('console.error("disk root load failed");');
    expect(mountScope).toContain('console.error("post-scan result load failed");');
    expect(mountScope).toContain('console.error("scan event registration failed");');
    expect(scanScope).toContain('console.error("scan start failed");');
    expect(openScope).toContain('console.error("folder navigation failed");');
    expect(jumpScope).toContain('console.error("folder navigation failed");');

    expect(mountScope).toContain("디스크 목록을 불러오지 못했습니다. DiskSage를 다시 열어 주세요.");
    expect(mountScope).toContain("스캔 결과를 불러오지 못했습니다. 같은 폴더를 다시 스캔하세요.");
    expect(mountScope).toContain("스캔을 준비하지 못했습니다. DiskSage를 다시 열어 주세요.");
    expect(scanScope).toContain("스캔을 시작하지 못했습니다. 폴더를 다시 선택한 뒤 재시도하세요.");
    expect(openScope).toContain("폴더 내용을 불러오지 못했습니다. 상위 폴더로 돌아가 다시 여세요.");
    expect(jumpScope).toContain("폴더 내용을 불러오지 못했습니다. 상위 폴더로 돌아가 다시 여세요.");
  });

  it("clears stale feedback and invalidates navigation before issuing new requests", () => {
    const source = readSource("src/routes/+page.svelte");
    const scanScope = between(source, "async function scan()", "async function open(");
    const openScope = between(source, "async function open(", "async function jump(");
    const jumpScope = between(source, "async function jump(", "</script>");

    expect(scanScope).toMatch(/\+\+navSeq;[\s\S]*operationError = "";[\s\S]*api\.startScan\(selectedRoot\)/);
    expect(openScope).toMatch(/const seq = \+\+navSeq;[\s\S]*operationError = "";[\s\S]*api\.getNode\(path\)/);
    expect(jumpScope).toMatch(/const seq = \+\+navSeq;[\s\S]*operationError = "";[\s\S]*api\.getNode\(crumbs\[i\]\)/);
    expect(openScope).toContain("if (seq !== navSeq) return;");
    expect(jumpScope).toContain("if (seq !== navSeq) return;");
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
