<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let caches: api.CacheCandidate[] = $state([]);
  let artifacts: api.DevArtifact[] = $state([]);
  let selected: Set<string> = $state(new Set());
  let selectedRules: Set<string> = $state(new Set());
  let results: api.CleanResult[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");

  async function load() {
    loadError = "";
    try {
      caches = await api.listCacheCandidates();
      artifacts = scannedRoot ? await api.listDevArtifacts(scannedRoot) : [];
    } catch (e) {
      loadError = String(e);
    }
  }

  function toggle(set: Set<string>, key: string) {
    const next = new Set(set);
    next.has(key) ? next.delete(key) : next.add(key);
    return next;
  }

  let totalSelected = $derived(
    caches.filter((c) => selectedRules.has(c.id)).reduce((s, c) => s + c.bytes, 0) +
      artifacts.filter((a) => selected.has(a.path)).reduce((s, a) => s + a.bytes, 0),
  );

  async function executeClean() {
    // 검토·확인 (스펙 §7-6): 명시적 승인 없이는 아무것도 실행되지 않는다
    const ruleDirs = caches.filter((c) => selectedRules.has(c.id) && c.exists);
    const artifactPaths = artifacts.filter((a) => selected.has(a.path)).map((a) => a.path);
    const summary = [
      ...ruleDirs.map((c) => `${c.label} (${fmtBytes(c.bytes)}) — 내용물 비우기`),
      ...artifactPaths,
    ];
    if (summary.length === 0) return;
    const okay = confirm(
      `다음 ${summary.length}개 항목을 휴지통으로 보냅니다 (총 ${fmtBytes(totalSelected)}):\n\n` +
        summary.slice(0, 15).join("\n") +
        (summary.length > 15 ? `\n… 외 ${summary.length - 15}개` : "") +
        "\n\n휴지통에서 언제든 복원할 수 있습니다.",
    );
    if (!okay) return;

    busy = true;
    try {
      const paths: string[] = [...artifactPaths];
      for (const c of ruleDirs) {
        paths.push(...(await api.expandCleanTargets(c.path)));
      }
      results = await api.cleanPaths(paths);
      selected = new Set();
      selectedRules = new Set();
      await load();
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  let failedResults = $derived(results.filter((r) => !r.ok));
</script>

<section>
  <h2>정리 <button onclick={load} disabled={busy}>새로고침</button></h2>
  {#if loadError}<p class="error">{loadError}</p>{/if}

  <h3>캐시</h3>
  <ul class="list">
    {#each caches as c (c.id)}
      <li>
        <label class:disabled={!c.exists}>
          <input
            type="checkbox"
            disabled={!c.exists || busy}
            checked={selectedRules.has(c.id)}
            onchange={() => (selectedRules = toggle(selectedRules, c.id))}
          />
          {c.label}
          <span class="size">{c.exists ? fmtBytes(c.bytes) : "없음"}</span>
        </label>
        <span class="path" title={c.path}>{c.path}</span>
      </li>
    {/each}
  </ul>

  <h3>오래된 개발 아티팩트 {scannedRoot ? `(${scannedRoot}, 30일+)` : "(먼저 스캔하세요)"}</h3>
  <ul class="list">
    {#each artifacts as a (a.path)}
      <li>
        <label>
          <input
            type="checkbox"
            disabled={busy}
            checked={selected.has(a.path)}
            onchange={() => (selected = toggle(selected, a.path))}
          />
          {a.kind} <em>({a.project}, {a.age_days}일)</em>
          <span class="size">{fmtBytes(a.bytes)}</span>
        </label>
        <span class="path" title={a.path}>{a.path}</span>
      </li>
    {/each}
  </ul>

  <div class="actions">
    <button onclick={executeClean} disabled={busy || totalSelected === 0}>
      {busy ? "정리 중…" : `선택 항목 휴지통으로 (${fmtBytes(totalSelected)})`}
    </button>
  </div>

  {#if results.length > 0}
    <p>
      {results.filter((r) => r.ok).length}/{results.length}개 휴지통으로 이동 완료 —
      휴지통에서 복원할 수 있습니다.
    </p>
    {#if failedResults.length > 0}
      <ul class="errors">
        {#each failedResults as r (r.path)}
          <li title={r.path}>⚠ {r.path} — {r.error}</li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .list { list-style: none; padding: 0; max-height: 30vh; overflow-y: auto; }
  .list li { display: flex; justify-content: space-between; gap: 1rem; padding: 2px 0; }
  .size { color: #666; font-variant-numeric: tabular-nums; margin-left: 0.5rem; }
  .path { color: #999; font-size: 0.8rem; overflow-wrap: anywhere; text-align: right; }
  .disabled { color: #aaa; }
  .error, .errors { color: #b00; }
  .errors { font-size: 0.85rem; }
</style>
