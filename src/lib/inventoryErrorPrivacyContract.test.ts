import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Inventory privacy-safe failure feedback", () => {
  it("never renders arbitrary backend exception text", () => {
    const source = readSource("src/lib/Inventory.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("catch (e)");
    expect(source).toContain(
      "인벤토리 집계에 실패했습니다. 스캔 대상 폴더의 접근 권한을 확인하고 스캔을 다시 실행한 뒤 집계하세요.",
    );
    expect(source).toContain(
      "모델 다운로드에 실패했습니다. 네트워크 연결과 DiskSage 데이터 폴더의 여유 공간을 확인한 뒤 다시 다운로드하세요.",
    );
    expect(source).toContain(
      "규칙 파일을 불러오지 못했습니다. DiskSage 데이터 폴더의 규칙 파일 권한과 형식을 확인한 뒤 인벤토리를 다시 집계하세요.",
    );
    expect(source).toContain(
      "미분류 요약에 실패했습니다. 모델 설치 상태를 확인한 뒤 요약을 다시 실행하세요.",
    );
  });

  it("clears stale inventory and summary evidence before a replacement load", () => {
    const source = readSource("src/lib/Inventory.svelte");
    const loadStart = source.indexOf("async function load()");
    const inventoryCall = source.indexOf("report = await api.diskInventory(scannedRoot)", loadStart);
    const loadPrefix = source.slice(loadStart, inventoryCall);

    expect(loadStart).toBeGreaterThanOrEqual(0);
    expect(inventoryCall).toBeGreaterThan(loadStart);
    expect(loadPrefix).toContain('loadError = ""');
    expect(loadPrefix).toContain("report = null");
    expect(loadPrefix).toContain("summary = null");
    expect(loadPrefix).toContain("summaryLoaded = false");
    expect(loadPrefix).toContain('summaryError = ""');
  });

  it("separates summary failure from summary content and announces failures", () => {
    const source = readSource("src/lib/Inventory.svelte");

    expect(source).toContain("summaryError");
    expect(source).toMatch(/summaryError\s*=\s*""/);
    expect(source).toContain('role="alert"');
  });

  it("preserves inventory, rules, model, and advisory-summary authority calls", () => {
    const source = readSource("src/lib/Inventory.svelte");

    expect(source).toContain("api.diskInventory(scannedRoot)");
    expect(source).toContain("api.getUserRules()");
    expect(source).toContain("api.downloadModel()");
    expect(source).toContain("api.summarizeUnknownBucket(report?.unknown_samples ?? [])");
  });
});
