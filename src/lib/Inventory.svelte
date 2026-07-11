<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let report: api.InventoryReport | null = $state(null);
  let busy = $state(false);
  let loadError = $state("");

  async function load() {
    if (!scannedRoot) return;
    busy = true;
    loadError = "";
    try {
      report = await api.diskInventory(scannedRoot);
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

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
        </li>
      {/if}
    </ul>
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
</style>
