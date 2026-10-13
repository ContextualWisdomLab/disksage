import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Duplicates privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown backend exception text", () => {
    const source = readSource("src/lib/Duplicates.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("catch (e)");
    expect(source).not.toContain("{r.error}");
    expect(source).toContain(
      "중복 파일 검색에 실패했습니다. 스캔 대상 폴더의 접근 권한을 확인하고 스캔을 다시 실행한 뒤 중복 찾기를 다시 누르세요.",
    );
    expect(source).toContain(
      "선택한 중복 파일을 휴지통으로 보내지 못했습니다. 파일이 열려 있는지와 휴지통 접근 권한을 확인한 뒤 중복 찾기부터 다시 실행하세요.",
    );
    expect(source).toContain(
      "파일이 사용 중인지와 접근 권한을 확인한 뒤 중복 찾기부터 다시 실행하세요.",
    );
  });

  it("clears stale duplicate and verdict evidence before replacement discovery", () => {
    const source = readSource("src/lib/Duplicates.svelte");
    const scanStart = source.indexOf("async function scan()");
    const duplicateCall = source.indexOf("const nextGroups = await api.findDuplicateFiles(root)", scanStart);
    const scanPrefix = source.slice(scanStart, duplicateCall);

    expect(scanStart).toBeGreaterThanOrEqual(0);
    expect(duplicateCall).toBeGreaterThan(scanStart);
    expect(scanPrefix).toContain("groups = []");
    expect(scanPrefix).toContain("toDelete = new Set()");
    expect(scanPrefix).toContain("verdicts = {}");
    expect(scanPrefix).toContain("results = []");
  });

  it("prevents a verdict response from an older scan replacing current evidence", () => {
    const source = readSource("src/lib/Duplicates.svelte");
    const verdictStart = source.indexOf("async function loadVerdicts");
    const scanStart = source.indexOf("async function scan()");
    const verdictBody = source.slice(verdictStart, scanStart);

    expect(source).toContain("let scanGeneration = $state(0)");
    expect(verdictBody).toContain("generation: number");
    expect(verdictBody).toContain("if (generation !== scanGeneration) return");
    expect(source).toContain("const generation = ++scanGeneration");
    expect(source).toContain("loadVerdicts(groups.flatMap((g) => g.paths), generation)");
    expect(source).toContain("if (generation !== scanGeneration || root !== scannedRoot) return");
    expect(source).toContain("if (generation === scanGeneration && root === scannedRoot) busy = false");
  });

  it("invalidates old-root evidence and serializes destructive confirmation", () => {
    const source = readSource("src/lib/Duplicates.svelte");

    expect(source).toContain("if (root === observedRoot) return;");
    expect(source).toContain("++scanGeneration;");
    expect(source).toContain("if (busy || confirming) return;");
    expect(source).toContain("if (!okay || generation !== scanGeneration || root !== scannedRoot) return;");
    expect(source).toContain("disabled={busy || confirming || toDelete.size === 0}");
    expect(source).toContain("휴지통 이동 확인 창을 열지 못했습니다. 다른 확인 창을 닫은 뒤 다시 시도하세요.");
  });

  it("uses the accessible failure region for an invalid all-selected group", () => {
    const source = readSource("src/lib/Duplicates.svelte");

    expect(source).not.toContain("alert(");
    expect(source).toContain(
      "중복 그룹 전체가 삭제 대상으로 선택됐습니다. 각 그룹에서 최소 1개는 보존하도록 선택을 해제한 뒤 다시 시도하세요.",
    );
    expect(source).toContain('role="alert"');
  });

  it("keeps customer-selected paths but makes result copy actionable", () => {
    const source = readSource("src/lib/Duplicates.svelte");

    expect(source).toContain("<li title={r.path}>⚠ {r.path} —");
    expect(source).toContain("복원이 필요하면 휴지통에서 되돌리세요.");
    expect(source).toContain("api.findDuplicateFiles(root)");
    expect(source).toContain("api.cleanPaths(paths)");
  });
});
