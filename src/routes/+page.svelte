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
  let operationError = $state("");
  let navSeq = 0;

  onMount(async () => {
    try {
      roots = await api.listRoots();
      selectedRoot = roots[0] ?? "";
    } catch {
      console.error("disk root load failed");
      operationError = "디스크 목록을 불러오지 못했습니다. DiskSage를 다시 열어 주세요.";
    }

    try {
      await api.onScanProgress((s) => (stats = s));
      await api.onScanDone(async (s) => {
        stats = s;
        scanning = false;
        operationError = "";
        const resultSeq = navSeq;
        const scannedRoot = selectedRoot;
        try {
          const [nextNode, nextTop] = await Promise.all([
            api.getNode(scannedRoot),
            api.topFiles(200),
          ]);
          if (resultSeq !== navSeq) return;
          crumbs = [scannedRoot];
          node = nextNode;
          top = nextTop;
        } catch {
          if (resultSeq !== navSeq) return;
          node = null;
          top = [];
          console.error("post-scan result load failed");
          operationError = "스캔 결과를 불러오지 못했습니다. 같은 폴더를 다시 스캔하세요.";
        }
      });
    } catch {
      console.error("scan event registration failed");
      operationError = "스캔을 준비하지 못했습니다. DiskSage를 다시 열어 주세요.";
    }
  });

  async function scan() {
    ++navSeq;
    operationError = "";
    scanning = true;
    node = null;
    top = [];
    try {
      await api.startScan(selectedRoot);
    } catch {
      scanning = false;
      console.error("scan start failed");
      operationError = "스캔을 시작하지 못했습니다. 폴더를 다시 선택한 뒤 재시도하세요.";
    }
  }

  async function open(path: string) {
    const seq = ++navSeq;
    operationError = "";
    try {
      const n = await api.getNode(path);
      if (seq !== navSeq) return; // 더 새로운 내비게이션이 이미 시작됨
      crumbs = [...crumbs, path];
      node = n;
    } catch {
      if (seq === navSeq) {
        console.error("folder navigation failed");
        operationError = "폴더 내용을 불러오지 못했습니다. 상위 폴더로 돌아가 다시 여세요.";
      }
    }
  }

  async function jump(i: number) {
    const seq = ++navSeq;
    operationError = "";
    try {
      const n = await api.getNode(crumbs[i]);
      if (seq !== navSeq) return;
      crumbs = crumbs.slice(0, i + 1);
      node = n;
    } catch {
      if (seq === navSeq) {
        console.error("folder navigation failed");
        operationError = "폴더 내용을 불러오지 못했습니다. 상위 폴더로 돌아가 다시 여세요.";
      }
    }
  }
</script>

<main>
  <h1>DiskSage</h1>
  <div class="controls">
    <select bind:value={selectedRoot} disabled={scanning}>
      {#each roots as r}<option value={r}>{r}</option>{/each}
    </select>
    {#if scanning}
      <button onclick={() => api.cancelScan()}>취소</button>
    {:else}
      <button onclick={scan}>스캔</button>
    {/if}
    {#if stats}
      <span class="stats">
        파일 {stats.files.toLocaleString()} · {fmtBytes(stats.bytes)}
        {#if stats.skipped > 0}· 스킵 {stats.skipped.toLocaleString()}건{/if}
      </span>
    {/if}
  </div>

  {#if operationError}
    <p class="error" role="alert">{operationError}</p>
  {/if}

  {#if node}
    <nav class="crumbs">
      {#each crumbs as c, i}
        <button class="crumb" onclick={() => jump(i)}>{c}</button>
        {#if i < crumbs.length - 1}<span>›</span>{/if}
      {/each}
    </nav>
    <Treemap {node} onOpen={open} />
    <a class="entry-focus" href="#current-folder-entries">폴더 항목 탐색 시작</a>
    <div id="current-folder-entries" class="entry-scroll" role="region" tabindex="-1" aria-label="현재 폴더 항목 목록">
      {#if node.entries.length === 0}
        <p class="empty-entries" role="status">표시할 항목이 없습니다. 상위 폴더로 이동하거나 다른 폴더를 스캔하세요.</p>
      {:else}
        <ul class="entries">
          {#each node.entries as e}
            <li>
              {#if e.is_dir}
                <button class="dir" onclick={() => open(e.path)}>📁 {e.name}</button>
              {:else}
                <span>📄 {e.name}</span>
              {/if}
              <span class="size">{fmtBytes(e.size)}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

  {#if node}
    <TopFiles files={top} />
  {/if}

  <Cleanup scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Inventory scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <CloudArchive scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Organize scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Duplicates scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />
</main>

<style>
  main { font-family: system-ui, sans-serif; padding: 1rem; }
  .controls { display: flex; gap: 0.5rem; align-items: center; }
  .stats { color: #666; font-size: 0.9rem; }
  .error { margin: 0.75rem 0; font-weight: 600; }
  .crumbs { margin: 0.75rem 0; display: flex; gap: 0.25rem; flex-wrap: wrap; }
  .crumb { background: none; border: none; color: #06c; cursor: pointer; padding: 0; }
  .entry-scroll { max-height: 40vh; overflow-y: auto; }
  .entry-scroll:focus-visible { outline: 2px solid currentColor; outline-offset: 2px; }
  .entry-focus { display: inline-block; margin-block-end: 0.35rem; }
  .entries { list-style: none; padding: 0; margin: 0; }
  .entries li { display: flex; justify-content: space-between; padding: 2px 0; }
  .dir { background: none; border: none; cursor: pointer; font: inherit; padding: 0; }
  .size { color: #666; font-variant-numeric: tabular-nums; }
  .empty-entries { margin: 0; color: #555; }
</style>
