import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Organize privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown or per-file backend exception text", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("catch (e)");
    expect(source).not.toContain("{r.error}");
    expect(source).toContain(
      "정리 계획을 만들지 못했습니다. 스캔 대상 폴더의 접근 권한을 확인하고 스캔을 다시 실행한 뒤 미리보기를 다시 만드세요.",
    );
    expect(source).toContain(
      "파일 정리를 실행하지 못했습니다. 파일이 열려 있는지와 대상 폴더의 접근 권한을 확인한 뒤 새 미리보기부터 진행하세요.",
    );
    expect(source).toContain(
      "마지막 이동을 되돌리지 못했습니다. 이동한 파일의 현재 위치와 원래 폴더의 접근 권한을 확인한 뒤 다시 되돌리세요.",
    );
  });

  it("clears stale plan, verdict, result, and result-context evidence before replacement planning", () => {
    const source = readSource("src/lib/Organize.svelte");
    const loadStart = source.indexOf("async function loadPlans()");
    const planCall = source.indexOf("plans = await api.planOrganize(scannedRoot)", loadStart);
    const loadPrefix = source.slice(loadStart, planCall);

    expect(loadStart).toBeGreaterThanOrEqual(0);
    expect(planCall).toBeGreaterThan(loadStart);
    expect(loadPrefix).toContain("plans = []");
    expect(loadPrefix).toContain("verdicts = {}");
    expect(loadPrefix).toContain("results = []");
    expect(loadPrefix).toContain("resultAction = null");
    expect(loadPrefix).toContain('loadError = ""');
  });

  it("clears stale error and result context before move or undo replacement actions", () => {
    const source = readSource("src/lib/Organize.svelte");

    const executeStart = source.indexOf("async function executeSelected()");
    const executeCall = source.indexOf("const r = await api.executeMoves(plans)", executeStart);
    const executePrefix = source.slice(executeStart, executeCall);
    expect(executePrefix).toContain('loadError = ""');
    expect(executePrefix).toContain("results = []");
    expect(executePrefix).toContain("resultAction = null");

    const undoStart = source.indexOf("async function undoMoves()");
    const undoCall = source.indexOf("const r = await api.undoLastMoves()", undoStart);
    const undoPrefix = source.slice(undoStart, undoCall);
    expect(undoPrefix).toContain('loadError = ""');
    expect(undoPrefix).toContain("results = []");
    expect(undoPrefix).toContain("resultAction = null");
  });

  it("keeps move and undo outcomes semantically distinct for the customer's next action", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain('let resultAction: "move" | "undo" | null = $state(null);');
    expect(source).toContain('resultAction = "move";');
    expect(source).toContain('resultAction = "undo";');
    expect(source).toContain('{#if resultAction === "undo"}');
    expect(source).toContain(
      "되돌리기를 완료했습니다. 다시 정리하려면 새 미리보기를 만드세요.",
    );
    expect(source).toContain(
      "현재 파일 위치와 원래 폴더의 접근 권한을 확인한 뒤 ‘마지막 이동 되돌리기’를 다시 실행하세요.",
    );
    expect(source).toContain(
      "복원이 필요하면 위 ‘마지막 이동 되돌리기’를 사용하세요.",
    );
    expect(source).toContain(
      "파일이 사용 중인지와 대상 폴더의 접근 권한을 확인한 뒤 새 미리보기부터 진행하세요.",
    );
  });

  it("keeps customer-selected paths and operation authority behind accessible feedback", () => {
    const source = readSource("src/lib/Organize.svelte");

    expect(source).toContain('role="alert"');
    expect(source).toContain("api.planOrganize(scannedRoot)");
    expect(source).toContain("api.executeMoves(plans)");
    expect(source).toContain("api.undoLastMoves()");
    expect(source).toContain("<li title={r.path}>⚠ {r.path} —");
  });
});
