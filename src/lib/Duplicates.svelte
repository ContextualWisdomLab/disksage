<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { blocksDeletion } from "./dupeGuard";
  import { verdictBadge } from "./verdictBadge";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let groups: api.DupeGroup[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");
  // 각 그룹에서 삭제 대상으로 선택된 경로 (보존할 하나를 제외한 나머지)
  let toDelete: Set<string> = $state(new Set());
  let results: api.CleanResult[] = $state([]);
  let verdicts: Record<string, api.Verdict> = $state({});

  async function loadVerdicts(paths: string[]) {
    try {
      const fvs = await api.fileVerdicts(paths);
      verdicts = Object.fromEntries(fvs.map((f) => [f.path, f.verdict]));
    } catch {
      /* advisory only — ignore */
    }
  }

  async function scan() {
    if (!scannedRoot) return;
    busy = true;
    loadError = "";
    results = [];
    try {
      groups = await api.findDuplicateFiles(scannedRoot);
      // 기본 선택: 각 그룹의 첫 파일을 보존, 나머지를 삭제 후보로
      const next = new Set<string>();
      for (const g of groups) {
        for (const p of g.paths.slice(1)) next.add(p);
      }
      toDelete = next;
      loadVerdicts(groups.flatMap((g) => g.paths));
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  function toggle(path: string) {
    const next = new Set(toDelete);
    next.has(path) ? next.delete(path) : next.add(path);
    toDelete = next;
  }

  let reclaimable = $derived(
    groups.reduce(
      (sum, g) => sum + g.size * g.paths.filter((p) => toDelete.has(p)).length,
      0,
    ),
  );

  async function deleteSelected() {
    const paths = [...toDelete];
    if (paths.length === 0) return;
    // 안전: 그룹 전체가 삭제 선택되면 최소 1개는 보존하도록 막는다
    if (blocksDeletion(groups, toDelete)) {
      alert("중복 그룹 하나가 통째로 삭제 선택됐습니다. 각 그룹에서 최소 1개는 보존해야 합니다.");
      return;
    }
    const okay = confirm(
      `${paths.length}개 중복 파일을 휴지통으로 보냅니다 (${fmtBytes(reclaimable)} 확보).\n` +
        `각 그룹의 사본 1개는 보존됩니다. 휴지통에서 복원할 수 있습니다.`,
    );
    if (!okay) return;
    busy = true;
    try {
      const r = await api.cleanPaths(paths);
      await scan();
      results = r;
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h2>
    중복 파일 {scannedRoot ? "" : "(먼저 스캔하세요)"}
    <button onclick={scan} disabled={busy || !scannedRoot}>{busy ? "찾는 중…" : "중복 찾기"}</button>
  </h2>
  {#if loadError}<p class="error">{loadError}</p>{/if}

  {#if groups.length === 0 && !busy}
    <p class="muted">중복을 찾으려면 스캔 후 "중복 찾기"를 누르세요.</p>
  {/if}

  {#each groups as g (g.hash)}
    <div class="group">
      <div class="ghead">
        {g.paths.length}개 사본 · 각 {fmtBytes(g.size)} · 낭비 {fmtBytes(g.size * (g.paths.length - 1))}
      </div>
      <ul>
        {#each g.paths as p (p)}
          <li>
            <label>
              <input
                type="checkbox"
                disabled={busy}
                checked={toDelete.has(p)}
                onchange={() => toggle(p)}
              />
              <span class="path" title={p}>{p}</span>
              {#if verdicts[p]}
                {@const b = verdictBadge(verdicts[p])}
                <span class={b.cls} title={b.title}>{b.label}</span>
              {/if}
              {#if !toDelete.has(p)}<em class="keep">보존</em>{/if}
            </label>
          </li>
        {/each}
      </ul>
    </div>
  {/each}

  {#if groups.length > 0}
    <div class="actions">
      <button onclick={deleteSelected} disabled={busy || toDelete.size === 0}>
        선택 중복 휴지통으로 ({fmtBytes(reclaimable)})
      </button>
    </div>
  {/if}

  {#if results.length > 0}
    <p>{results.filter((r) => r.ok).length}/{results.length}개 휴지통으로 이동 — 복원 가능합니다.</p>
    {#if results.some((r) => !r.ok)}
      <ul class="errors">
        {#each results.filter((r) => !r.ok) as r (r.path)}
          <li title={r.path}>⚠ {r.path} — {r.error}</li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .group { border: 1px solid #eee; border-radius: 4px; margin: 0.5rem 0; padding: 0.5rem; }
  .ghead { font-size: 0.85rem; color: #555; margin-bottom: 0.25rem; }
  .group ul { list-style: none; padding: 0; margin: 0; }
  .group li { padding: 1px 0; }
  .path { overflow-wrap: anywhere; }
  .keep { color: #080; margin-left: 0.5rem; font-size: 0.8rem; }
  .muted { color: #999; }
  .error { color: #b00; }
  .errors { color: #b00; font-size: 0.85rem; list-style: none; padding: 0; }
  .badge-safe, .badge-caution, .badge-keep, .badge-unrated {
    display: inline-block; margin-left: 0.4rem; padding: 1px 6px; border-radius: 8px;
    font-size: 0.75rem; color: #fff;
  }
  .badge-safe { background: #2a8f4a; }
  .badge-caution { background: #b8860b; }
  .badge-keep { background: #b03030; }
  .badge-unrated { background: #888; }
</style>
