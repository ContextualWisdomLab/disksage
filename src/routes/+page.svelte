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

  let roots: string[] = $state([]);
  let selectedRoot = $state("");
  let scanning = $state(false);
  let stats: api.ScanStats | null = $state(null);
  let node: api.NodeView | null = $state(null);
  let crumbs: string[] = $state([]);
  let top: api.EntryView[] = $state([]);
  let navSeq = 0;

  onMount(async () => {
    roots = await api.listRoots();
    selectedRoot = roots[0] ?? "";
    await api.onScanProgress((s) => (stats = s));
    await api.onScanDone(async (s) => {
      stats = s;
      scanning = false;
      try {
        crumbs = [selectedRoot];
        node = await api.getNode(selectedRoot);
        top = await api.topFiles(200);
      } catch (e) {
        console.error("post-scan load failed:", e);
      }
    });
  });

  async function scan() {
    scanning = true;
    node = null;
    top = [];
    try {
      await api.startScan(selectedRoot);
    } catch (e) {
      scanning = false;
      alert(`스캔 시작 실패: ${e}`);
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

  {#if node}
    <nav class="crumbs">
      {#each crumbs as c, i}
        <button class="crumb" onclick={() => jump(i)}>{c}</button>
        {#if i < crumbs.length - 1}<span>›</span>{/if}
      {/each}
    </nav>
    <Treemap {node} onOpen={open} />
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

  {#if top.length > 0}
    <TopFiles files={top} />
  {/if}

  <Cleanup scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Inventory scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Organize scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />

  <Duplicates scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />
</main>

<style>
  main { font-family: system-ui, sans-serif; padding: 1rem; }
  .controls { display: flex; gap: 0.5rem; align-items: center; }
  .stats { color: #666; font-size: 0.9rem; }
  .crumbs { margin: 0.75rem 0; display: flex; gap: 0.25rem; flex-wrap: wrap; }
  .crumb { background: none; border: none; color: #06c; cursor: pointer; padding: 0; }
  .entries { list-style: none; padding: 0; max-height: 40vh; overflow-y: auto; }
  .entries li { display: flex; justify-content: space-between; padding: 2px 0; }
  .dir { background: none; border: none; cursor: pointer; font: inherit; padding: 0; }
  .size { color: #666; font-variant-numeric: tabular-nums; }
</style>
