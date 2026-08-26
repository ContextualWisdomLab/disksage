<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import { fmtBytes } from "$lib/fmt";
  import TopFiles from "$lib/TopFiles.svelte";
  import Treemap from "$lib/Treemap.svelte";
  import Cleanup from "$lib/Cleanup.svelte";
  import Duplicates from "$lib/Duplicates.svelte";
  import Inventory from "$lib/Inventory.svelte";
  import Organize from "$lib/Organize.svelte";
  import CloudArchive from "$lib/CloudArchive.svelte";

  let roots: string[] = $state([]);
  let selectedRoot = $state("");
  let scanning = $state(false);
  let stats: api.ScanStats | null = $state(null);
  let node: api.NodeView | null = $state(null);
  let crumbs: string[] = $state([]);
  let top: api.EntryView[] = $state([]);
  let navSeq = 0;
  let loadError = $state("");
  let scanMessage = $state("");

  onMount(() => {
    let disposed = false;
    let unlistenProgress: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;

    const initialize = async () => {
      try {
        const [progressCleanup, doneCleanup] = await Promise.all([
          api.onScanProgress((s) => (stats = s)),
          api.onScanDone(async (s) => {
            stats = s;
            scanning = false;
            scanMessage = `스캔 완료: ${s.files.toLocaleString()}개 파일, ${fmtBytes(s.bytes)}`;
            try {
              crumbs = [selectedRoot];
              node = await api.getNode(selectedRoot);
              top = await api.topFiles(200);
            } catch {
              loadError = "스캔 결과를 화면에 불러오지 못했습니다.";
              scanMessage = "스캔 결과를 화면에 불러오지 못했습니다.";
            }
          }),
        ]);
        if (disposed) {
          progressCleanup();
          doneCleanup();
          return;
        }
        unlistenProgress = progressCleanup;
        unlistenDone = doneCleanup;
        roots = await api.listRoots();
        if (disposed) return;
        selectedRoot = roots[0] ?? "";
      } catch {
        if (disposed) return;
        loadError = "스캔할 수 있는 위치를 불러오지 못했습니다.";
      }
    };

    void initialize();
    return () => {
      disposed = true;
      unlistenProgress?.();
      unlistenDone?.();
    };
  });

  async function scan() {
    if (!selectedRoot || scanning) return;
    scanning = true;
    node = null;
    top = [];
    loadError = "";
    scanMessage = `${selectedRoot} 스캔을 시작했습니다.`;
    try {
      await api.startScan(selectedRoot);
    } catch {
      scanning = false;
      loadError = "스캔을 시작하지 못했습니다. 위치를 확인한 뒤 다시 시도하십시오.";
      scanMessage = "스캔을 시작하지 못했습니다.";
    }
  }

  async function open(path: string) {
    const seq = ++navSeq;
    try {
      const n = await api.getNode(path);
      if (seq !== navSeq) return; // 더 새로운 내비게이션이 이미 시작됨
      crumbs = [...crumbs, path];
      node = n;
    } catch (e) {
      console.error("getNode failed:", e);
    }
  }

  async function jump(i: number) {
    const seq = ++navSeq;
    try {
      const n = await api.getNode(crumbs[i]);
      if (seq !== navSeq) return;
      crumbs = crumbs.slice(0, i + 1);
      node = n;
    } catch (e) {
      console.error("getNode failed:", e);
    }
  }
</script>

<svelte:head>
  <title>DiskSage · 로컬 저장공간과 클라우드 증거</title>
  <meta
    name="description"
    content="메타데이터와 공급자 증거를 먼저 확인하고 로컬 저장공간을 안전하게 정리합니다."
  />
</svelte:head>

<main id="main-content" tabindex="-1">
  <h1>DiskSage</h1>
  <div class="controls" role="group" aria-label="스캔 제어">
    <label for="scan-root">스캔 위치</label>
    <select id="scan-root" bind:value={selectedRoot} disabled={scanning || roots.length === 0}>
      {#each roots as r}<option value={r}>{r}</option>{/each}
    </select>
    {#if scanning}
      <button class="ds-control scan-action" type="button" onclick={() => api.cancelScan()} aria-label="현재 스캔 취소">취소</button>
    {:else}
      <button class="ds-control scan-action" type="button" onclick={scan} disabled={!selectedRoot}>스캔</button>
    {/if}
    {#if stats}
      <span class="stats">
        파일 {stats.files.toLocaleString()} · {fmtBytes(stats.bytes)}
        {#if stats.skipped > 0}· 스킵 {stats.skipped.toLocaleString()}건{/if}
      </span>
    {/if}
  </div>
  {#if loadError}<p class="error" role="alert">{loadError}</p>{/if}
  <p class="sr-only" role="status" aria-live="polite" aria-atomic="true">{scanMessage}</p>

  {#if node}
    <nav class="crumbs" aria-label="현재 폴더 경로">
      {#each crumbs as c, i}
        <button type="button" class="crumb" onclick={() => jump(i)}>{c}</button>
        {#if i < crumbs.length - 1}<span>›</span>{/if}
      {/each}
    </nav>
    <Treemap {node} onOpen={open} />
    <ul class="entries">
      {#each node.entries as e}
        <li>
          {#if e.is_dir}
            <button type="button" class="dir" onclick={() => open(e.path)}>📁 {e.name}</button>
          {:else}
            <span>📄 {e.name}</span>
          {/if}
          <span class="size">{fmtBytes(e.size)}</span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if top.length > 0}
    <TopFiles files={top} />
  {/if}

  <Cleanup scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Inventory scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <CloudArchive scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Organize scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Duplicates scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />
</main>

<style>
  main { max-width: 90rem; margin: 0 auto; padding: var(--ds-space-4); }
  .controls { display: flex; gap: var(--ds-space-2); align-items: center; flex-wrap: wrap; }
  .stats { color: var(--ds-text-muted); font-size: 0.9rem; }
  .error { color: var(--ds-danger-text); background: var(--ds-danger-surface); padding: var(--ds-space-2); border-radius: var(--ds-radius-sm); }
  .crumbs { margin: var(--ds-space-3) 0; display: flex; gap: var(--ds-space-1); flex-wrap: wrap; align-items: center; }
  .crumb { background: transparent; border: none; color: var(--ds-action); cursor: pointer; }
  .entries { list-style: none; padding: 0; max-height: 40vh; overflow-y: auto; }
  .entries li { display: flex; justify-content: space-between; gap: var(--ds-space-3); padding: var(--ds-space-1) 0; }
  .dir { background: transparent; border: none; cursor: pointer; font: inherit; text-align: left; }
  .size { color: var(--ds-text-muted); font-variant-numeric: tabular-nums; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
  @media (max-width: 40rem) {
    .controls > * { width: 100%; }
    .controls button { width: 100%; }
  }
</style>
