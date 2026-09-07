import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  inventoryFailureMessage,
  type InventoryFailureKind,
} from "./inventoryInsightPolicy";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

function functionSlice(source: string, startToken: string, nextToken: string): string {
  const start = source.indexOf(startToken);
  const end = source.indexOf(nextToken, start);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("Inventory privacy-safe failure feedback", () => {
  it("drops arbitrary backend detail at the executable user-message boundary", () => {
    const backendMarker = "backend-secret:/Users/alice/.ssh/id_ed25519";
    const failureKinds: InventoryFailureKind[] = [
      "inventory-load",
      "ontology-coherence",
      "user-rules",
      "model-status",
      "model-download",
      "unknown-extension-insight",
      "unknown-summary",
    ];

    for (const kind of failureKinds) {
      const message = inventoryFailureMessage(kind, new Error(backendMarker));
      expect(message).not.toContain(backendMarker);
      expect(message).not.toContain("/Users/alice");
      expect(message.length).toBeGreaterThan(0);
      expect(message.length).toBeLessThanOrEqual(160);
    }
  });

  it("never renders arbitrary backend exception text", () => {
    const source = readSource("src/lib/Inventory.svelte");
    const policySource = readSource("src/lib/inventoryInsightPolicy.ts");
    const userBoundarySource = `${source}\n${policySource}`;

    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("catch (e)");
    expect(source).not.toContain(".message");
    expect(source.match(/inventoryFailureMessage\(/g)?.length).toBeGreaterThanOrEqual(7);
    expect(userBoundarySource).toContain(
      "인벤토리 집계에 실패했습니다. 스캔 대상 폴더의 접근 권한을 확인하고 스캔을 다시 실행한 뒤 집계하세요.",
    );
    expect(userBoundarySource).toContain(
      "모델 다운로드에 실패했습니다. 네트워크 연결과 DiskSage 데이터 폴더의 여유 공간을 확인한 뒤 다시 다운로드하세요.",
    );
    expect(userBoundarySource).toContain(
      "규칙 파일을 불러오지 못했습니다. DiskSage 데이터 폴더의 규칙 파일 권한과 형식을 확인한 뒤 인벤토리를 다시 집계하세요.",
    );
    expect(userBoundarySource).toContain(
      "미분류 요약에 실패했습니다. 모델 설치 상태를 확인한 뒤 요약을 다시 실행하세요.",
    );
  });

  it("clears stale inventory and summary evidence before a replacement load", () => {
    const source = readSource("src/lib/Inventory.svelte");
    const clearBody = functionSlice(source, "function clearInventoryEvidence()", "function requestIsCurrent(");
    const loadStart = source.indexOf("async function load()");
    const inventoryCall = source.indexOf("await api.diskInventory(root)", loadStart);
    const loadPrefix = source.slice(loadStart, inventoryCall);

    expect(loadStart).toBeGreaterThanOrEqual(0);
    expect(inventoryCall).toBeGreaterThan(loadStart);
    expect(loadPrefix).toContain("clearInventoryEvidence()");
    expect(clearBody).toContain('loadError = ""');
    expect(clearBody).toContain("report = null");
    expect(clearBody).toContain("summary = null");
    expect(clearBody).toContain("summaryLoaded = false");
    expect(clearBody).toContain('summaryError = ""');
    expect(clearBody).toContain("summaryBusy = false");
  });

  it("separates summary failure from summary content and announces failures", () => {
    const source = readSource("src/lib/Inventory.svelte");

    expect(source).toContain("summaryError");
    expect(source).toMatch(/summaryError\s*=\s*""/);
    expect(source).toContain('role="alert"');
  });

  it("preserves inventory, rules, model, and advisory-summary authority calls", () => {
    const source = readSource("src/lib/Inventory.svelte");

    expect(source).toContain("api.diskInventory(root)");
    expect(source).toContain("api.getUserRules()");
    expect(source).toContain("api.downloadModel()");
    expect(source).toContain("api.summarizeUnknownBucket(report?.unknown_samples ?? [])");
  });

  it("keeps advisory failures non-blocking but visible with bounded next actions", () => {
    const source = readSource("src/lib/Inventory.svelte");
    const policySource = readSource("src/lib/inventoryInsightPolicy.ts");
    const userBoundarySource = `${source}\n${policySource}`;

    expect(source).not.toContain(".catch(() => {})");
    expect(source).toContain("coherenceError");
    expect(source).toContain("modelStatusError");
    expect(source).toContain("insightsError");
    expect(userBoundarySource).toContain(
      "온톨로지 정합성 확인에 실패했습니다. DiskSage 리소스와 설정을 확인한 뒤 인벤토리를 다시 집계하세요.",
    );
    expect(userBoundarySource).toContain(
      "모델 상태를 확인하지 못했습니다. 모델 다운로드 여부를 다시 확인하거나 잠시 후 상태를 새로고침하세요.",
    );
    expect(userBoundarySource).toContain(
      "미분류 확장자 자문에 실패했습니다. 인벤토리는 그대로 사용할 수 있으며 필요하면 다시 집계해 자문을 재시도하세요.",
    );
    expect(source).toContain("api.ontologyCoherence()");
    expect(source).toContain("api.modelStatus()");
    expect(source).toContain(
      "requestUnknownExtensionInsights(nextReport.unknown_samples, api.reasonUnknownExtensions)",
    );
  });

  it("invalidates stale advisory state and ignores an older extension-reasoning response", () => {
    const source = readSource("src/lib/Inventory.svelte");
    const clearBody = functionSlice(source, "function clearInventoryEvidence()", "function requestIsCurrent(");
    const loadStart = source.indexOf("async function load()");
    const inventoryCall = source.indexOf("await api.diskInventory(root)", loadStart);
    const loadPrefix = source.slice(loadStart, inventoryCall);

    expect(source).toContain("let loadGeneration = 0");
    expect(loadPrefix).toContain("const generation = ++loadGeneration");
    expect(loadPrefix).toContain("clearInventoryEvidence()");
    expect(clearBody).toContain("issues = null");
    expect(clearBody).toContain('coherenceError = ""');
    expect(clearBody).toContain("insights = []");
    expect(clearBody).toContain('insightsError = ""');
    expect(source).toContain("requestIsCurrent(root, generation)");
  });

  it("ignores a stale unknown-bucket summary that resolves after a newer load started", () => {
    const source = readSource("src/lib/Inventory.svelte");
    const summarizeBody = functionSlice(source, "async function summarizeUnknown()", "\n  $effect(() => {");

    expect(summarizeBody).toContain("const generation = loadGeneration");
    expect(summarizeBody.match(/requestIsCurrent\(root, generation\)/g)?.length).toBeGreaterThanOrEqual(3);

    const clearBody = functionSlice(source, "function clearInventoryEvidence()", "function requestIsCurrent(");
    const loadStart = source.indexOf("async function load()");
    const inventoryCall = source.indexOf("await api.diskInventory(root)", loadStart);
    const loadPrefix = source.slice(loadStart, inventoryCall);
    expect(loadPrefix).toContain("clearInventoryEvidence()");
    expect(clearBody).toContain("summaryBusy = false");
  });
});
