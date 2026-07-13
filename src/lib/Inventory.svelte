<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import Settings from "./Settings.svelte";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let report: api.InventoryReport | null = $state(null);
  let busy = $state(false);
  let loadError = $state("");

  let model = $state<api.ModelStatus | null>(null);
  let modelBusy = $state(false);
  let summary = $state<string | null>(null);
  let summaryLoaded = $state(false);
  let summaryBusy = $state(false);

  // 온톨로지 정합성(advisory) — 인벤토리 집계와 별개로 로드 실패해도 조용히 무시(게이트 아님)
  let issues = $state<api.Issue[] | null>(null);

  // 미분류 확장자 자문 인사이트(advisory) — 오프라인 LLM + (online_mode일 때만) 웹. 실패해도 조용히 무시(게이트 아님)
  let insights = $state<api.ExtInsight[]>([]);

  async function loadCoherence() {
    try {
      issues = await api.ontologyCoherence();
    } catch {
      issues = null;
    }
  }

  async function load() {
    if (!scannedRoot) return;
    busy = true;
    loadError = "";
    try {
      report = await api.diskInventory(scannedRoot);
      await loadCoherence();
      // 미분류 확장자 인사이트: 비차단(fire-and-forget) — 실패해도 인벤토리 표시를 막지 않음
      api.reasonUnknownExtensions(report.unknown_samples).then((r) => (insights = r)).catch(() => {});
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  async function loadModel() {
    try {
      model = await api.modelStatus();
    } catch {
      model = null;
    }
  }

  async function doDownload() {
    modelBusy = true;
    try {
      await api.downloadModel();
      await loadModel();
    } catch (e) {
      loadError = String(e);
    } finally {
      modelBusy = false;
    }
  }

  // 미분류 버킷 요약: 스캔된 미분류 파일 경로 샘플(unknown_samples)을 백엔드가 모델로 요약.
  // 샘플이 없거나 모델이 없으면 null(안내 문구로 대체).
  async function summarizeUnknown() {
    summaryBusy = true;
    try {
      summary = await api.summarizeUnknownBucket(report?.unknown_samples ?? []);
    } catch (e) {
      summary = String(e);
    } finally {
      summaryLoaded = true;
      summaryBusy = false;
    }
  }

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
  {#if loadError}<p class="error">{loadError}</p>{/if}

  <div class="model-status">
    {#if model?.present}
      <span>모델: {model.name} ✓</span>
    {:else}
      <button onclick={doDownload} disabled={modelBusy}>{modelBusy ? "다운로드 중…" : "모델 다운로드"}</button>
    {/if}
    <span class="muted small">판정은 참고용(자문)입니다 — 모델 없이도 규칙 기반으로 전체 기능이 동작합니다.</span>
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
            {#if summaryLoaded}
              <span class="summary-text">{summary ?? "미판정 (모델 없음)"}</span>
            {/if}
          </div>
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

    {#if issues !== null}
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
  .model-status { display: flex; align-items: center; gap: 0.5rem; margin: 0.5rem 0; font-size: 0.85rem; }
  .muted.small { color: #999; font-size: 0.75rem; }
  .unknown-summary { margin-top: 0.25rem; display: flex; align-items: center; gap: 0.5rem; font-size: 0.8rem; }
  .summary-text { color: #555; }
  .ext-insights { list-style: none; padding: 0; margin: 0.35rem 0 0; }
  .ext-insights li { font-size: 0.78rem; color: #666; margin: 0.1rem 0; }
  .ext-insights .hint { color: #4a90d9; margin-left: 0.25rem; }
  .coherence { margin-top: 0.75rem; }
  .ok.small { color: #2a7; font-size: 0.8rem; }
  .issues { list-style: none; padding: 0; margin: 0; }
  .issues .warn { color: #a60; font-size: 0.8rem; margin: 0.15rem 0; }
</style>
