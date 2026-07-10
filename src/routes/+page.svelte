<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import { fmtBytes } from "$lib/fmt";

  let roots: string[] = $state([]);
  let selectedRoot = $state("");
  let scanning = $state(false);
  let stats: api.ScanStats | null = $state(null);
  let node: api.NodeView | null = $state(null);
  let crumbs: string[] = $state([]);
  let top: api.EntryView[] = $state([]);

  onMount(async () => {
    roots = await api.listRoots();
    selectedRoot = roots[0] ?? "";
    await api.onScanProgress((s) => (stats = s));
    await api.onScanDone(async (s) => {
      stats = s;
      scanning = false;
      crumbs = [selectedRoot];
      node = await api.getNode(selectedRoot);
      top = await api.topFiles(200);
    });
  });

  async function scan() {
    scanning = true;
    node = null;
    top = [];
    await api.startScan(selectedRoot);
  }

  async function open(path: string) {
    crumbs = [...crumbs, path];
    node = await api.getNode(path);
  }

  async function jump(i: number) {
    crumbs = crumbs.slice(0, i + 1);
    node = await api.getNode(crumbs[i]);
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
