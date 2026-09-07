<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import {
    inventoryFailureMessage,
    isCurrentInventoryRequest,
    requestUnknownExtensionInsights,
  } from "./inventoryInsightPolicy";
  import Settings from "./Settings.svelte";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let report: api.InventoryReport | null = $state(null);
  let busy = $state(false);
  let loadError = $state("");
  let loadGeneration = 0;
  let observedRoot: string | null = null;

  let model = $state<api.ModelStatus | null>(null);
  let modelBusy = $state(false);
  let modelError = $state("");
  let modelStatusError = $state("");
  let summary = $state<string | null>(null);
  let summaryLoaded = $state(false);
  let summaryBusy = $state(false);
  let summaryError = $state("");

  // 온톨로지 정합성(advisory) — 집계는 막지 않지만 실패를 조용히 숨기지 않는다.
  let issues = $state<api.Issue[] | null>(null);
  let coherenceError = $state("");

  // 활성 사용자 규칙 개수(advisory) — 손상된 규칙 파일은 조용히 무시하지 않고 안내만(게이트 아님)
  let userRulesCount = $state<number | null>(null);
  let userRulesError = $state("");
  // 미분류 확장자 자문 인사이트(advisory) — 오프라인 LLM + (online_mode일 때만) 웹.
  let insights = $state<api.ExtInsight[]>([]);
  let insightsError = $state("");

  function clearInventoryEvidence() {
    loadError = "";
    report = null;
    summary = null;
    summaryLoaded = false;
    summaryError = "";
    summaryBusy = false;
    issues = null;
    coherenceError = "";
    userRulesCount = null;
    userRulesError = "";
    insights = [];
    insightsError = "";
  }

  function requestIsCurrent(root: string, generation: number): boolean {
    return isCurrentInventoryRequest(root, generation, scannedRoot, loadGeneration);
  }

  async function loadCoherence(root: string, generation: number) {
    try {
      const nextIssues = await api.ontologyCoherence();
      if (!requestIsCurrent(root, generation)) return;
      issues = nextIssues;
      coherenceError = "";
    } catch (error) {
      if (!requestIsCurrent(root, generation)) return;
      issues = null;
      coherenceError = inventoryFailureMessage("ontology-coherence", error);
    }
  }

  async function loadUserRules(root: string, generation: number) {
    try {
      const rules = await api.getUserRules();
      if (!requestIsCurrent(root, generation)) return;
      userRulesCount = rules.length;
      userRulesError = "";
    } catch (error) {
      if (!requestIsCurrent(root, generation)) return;
      userRulesCount = null;
      userRulesError = inventoryFailureMessage("user-rules", error);
    }
  }

  async function load() {
    const root = scannedRoot;
    if (!root) return;
    const generation = ++loadGeneration;
    busy = true;
    clearInventoryEvidence();
    try {
      const nextReport = await api.diskInventory(root);
      if (!requestIsCurrent(root, generation)) return;
      report = nextReport;
      await loadCoherence(root, generation);
      if (!requestIsCurrent(root, generation)) return;
      await loadUserRules(root, generation);
      if (!requestIsCurrent(root, generation)) return;
      // 미분류 확장자 인사이트: 샘플이 있을 때만 비차단으로 요청하고, 이전 root/집계 응답은 새 증거를 덮지 못한다.
      void requestUnknownExtensionInsights(nextReport.unknown_samples, api.reasonUnknownExtensions)
        .then((nextInsights) => {
          if (requestIsCurrent(root, generation) && nextInsights !== null) {
            insights = nextInsights;
            insightsError = "";
          }
        })
        .catch((error) => {
          if (requestIsCurrent(root, generation)) {
            insights = [];
            insightsError = inventoryFailureMessage("unknown-extension-insight", error);
          }
        });
    } catch (error) {
      if (requestIsCurrent(root, generation)) {
        loadError = inventoryFailureMessage("inventory-load", error);
      }
    } finally {
      if (generation === loadGeneration) {
        busy = false;
      }
    }
  }

  async function loadModel() {
    modelStatusError = "";
    try {
      model = await api.modelStatus();
    } catch (error) {
      model = null;
      modelStatusError = inventoryFailureMessage("model-status", error);
    }
  }

  async function doDownload() {
    modelBusy = true;
    modelError = "";
    try {
      await api.downloadModel();
      await loadModel();
    } catch (error) {
      modelError = inventoryFailureMessage("model-download", error);
    } finally {
      modelBusy = false;
    }
  }

  // 미분류 버킷 요약: 스캔된 미분류 파일 경로 샘플(unknown_samples)을 백엔드가 모델로 요약.
  // 샘플이 없거나 모델이 없으면 null(안내 문구로 대체).
  async function summarizeUnknown() {
    const root = scannedRoot;
    if (!root) return;
    const generation = loadGeneration;
    summaryBusy = true;
    summaryLoaded = false;
    summary = null;
    summaryError = "";
    try {
      const result = await api.summarizeUnknownBucket(report?.unknown_samples ?? []);
      if (requestIsCurrent(root, generation)) {
        summary = result;
      }
    } catch (error) {
      if (requestIsCurrent(root, generation)) {
        summaryError = inventoryFailureMessage("unknown-summary", error);
      }
    } finally {
      if (requestIsCurrent(root, generation)) {
        summaryLoaded = true;
        summaryBusy = false;
      }
    }
  }

  $effect(() => {
    const nextRoot = scannedRoot;
    if (nextRoot !== observedRoot) {
      observedRoot = nextRoot;
      ++loadGeneration;
      busy = false;
      clearInventoryEvidence();
    }
  });

  $effect(() => {
    loadModel();
  });

  let totalBytes = $derived.by(() => {
    if (!report) return 0;
    return report.tallies.reduce((s: number, t: api.ClassTally) => s + t.bytes, 0) + report.unknown_bytes;
  });

  function pct(bytes: number): number {
    return totalBytes > 0 ? Math.round((bytes / totalBytes) * 100) : 0;
  }
</script>

<section>
  <h2>
    인벤토리 {scannedRoot ? "" : "(먼저 스캔하세요)"}
    <button onclick={load} disabled={busy || !scannedRoot}>{busy ? "집계 중…" : "인벤토리 집계"}</button>
  </h2>
  {#if loadError}<p class="error" role="alert">{loadError}</p>{/if}

  <div class="model-status">
    {#if model?.present}
      <span>모델: {model.name} ✓</span>
    {:else}
      <button onclick={doDownload} disabled={modelBusy}>{modelBusy ? "다운로드 중…" : "모델 다운로드"}</button>
    {/if}
    <span class="muted small">판정은 참고용(자문)입니다 — 모델 없이도 규칙 기반으로 전체 기능이 동작합니다.</span>
    {#if modelError}<span class="error small" role="alert">{modelError}</span>{/if}
    {#if modelStatusError}<span class="warn small" role="alert">{modelStatusError}</span>{/if}
  </div>

  <Settings />

  {#if report}
    <ul class="bars">
      {#each report.tallies as t (t.class_id)}
        <li>
          <div class="row">
            <span class="label">{t.label}</span>
            <span class="size">{fmtBytes(t.bytes)} · {t.count}개 · {pct(t.bytes)}%</span>
          </div>
          <div class="bar"><div class="fill" style="width:{pct(t.bytes)}%"></div></div>
        </li>
      {/each}
      {#if report.unknown_count > 0}
        <li class="unknown">
          <div class="row">
            <span class="label">미분류 <em>(무엇인지 모르는 용량)</em></span>
            <span class="size">{fmtBytes(report.unknown_bytes)} · {report.unknown_count}개 · {pct(report.unknown_bytes)}%</span>
          </div>
          <div class="bar"><div class="fill unk" style="width:{pct(report.unknown_bytes)}%"></div></div>
          <div class="unknown-summary">
            <button onclick={summarizeUnknown} disabled={summaryBusy}>{summaryBusy ? "요약 중…" : "요약 보기"}</button>
            {#if summaryError}
              <span class="error small" role="alert">{summaryError}</span>
            {:else if summaryLoaded}
              <span class="summary-text">{summary ?? "미판정 (모델 없음)"}</span>
            {/if}
          </div>
          {#if insightsError}
            <p class="warn small" role="alert">{insightsError}</p>
          {/if}
          {#if insights.length > 0}
            <ul class="ext-insights">
              {#each insights as i (i.ext)}
                <li>
                  .{i.ext}: {i.type_desc ?? "?"}
                  {#if i.suggested_class}<span class="hint">→ {i.suggested_class}</span>{/if}
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/if}
    </ul>

    {#if coherenceError}
      <p class="warn small" role="alert">{coherenceError}</p>
    {:else if issues !== null}
      <div class="coherence">
        {#if issues.length === 0}
          <span class="ok small">온톨로지 정합 ✓</span>
        {:else}
          <ul class="issues">
            {#each issues as i}
              <li class="warn">
                불충족 클래스: {i.UnsatisfiableClass.class}
                (분리 공리: {i.UnsatisfiableClass.via_disjoint[0]} ↔ {i.UnsatisfiableClass.via_disjoint[1]})
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

    {#if userRulesCount}
      <p class="ok small">사용자 규칙 {userRulesCount}개 적용 중</p>
    {:else if userRulesError}
      <p class="warn small" role="alert">규칙 파일 오류: {userRulesError}</p>
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .bars { list-style: none; padding: 0; }
  .bars li { margin: 0.4rem 0; }
  .row { display: flex; justify-content: space-between; font-size: 0.9rem; }
  .size { color: #666; font-variant-numeric: tabular-nums; }
  .bar { background: #eee; border-radius: 3px; height: 8px; overflow: hidden; }
  .fill { background: #4a90d9; height: 100%; }
  .fill.unk { background: #d98a4a; }
  .unknown .label em { color: #a60; font-style: normal; font-size: 0.8rem; }
  .error { color: #b00; }
  .error.small { font-size: 0.8rem; }
  .model-status { display: flex; align-items: center; gap: 0.5rem; margin: 0.5rem 0; font-size: 0.85rem; }
  .muted.small { color: #999; font-size: 0.75rem; }
  .unknown-summary { margin-top: 0.25rem; display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; }
  .summary-text { color: #555; }
  .ext-insights { list-style: none; padding: 0; margin: 0.35rem 0 0; }
  .ext-insights li { font-size: 0.78rem; color: #666; margin: 0.1rem 0; }
  .ext-insights .hint { color: #4a90d9; margin-left: 0.25rem; }
  .coherence { margin-top: 0.75rem; }
  .ok.small { color: #2a7; font-size: 0.8rem; }
  .warn.small { color: #a60; font-size: 0.8rem; }
  .issues { list-style: none; padding: 0; margin: 0; }
  .issues .warn { color: #a60; font-size: 0.8rem; margin: 0.15rem 0; }
</style>
