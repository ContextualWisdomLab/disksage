<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let plans: api.MovePlan[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");
  let results: api.CleanResult[] = $state([]);

  async function loadPlans() {
    if (!scannedRoot) return;
    busy = true;
    loadError = "";
    results = [];
    try {
      plans = await api.planOrganize(scannedRoot);
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  // Group plans by class_id for display
  let grouped = $derived.by(() => {
    const g = new Map<string, api.MovePlan[]>();
    for (const p of plans) {
      if (!g.has(p.class_id)) g.set(p.class_id, []);
      g.get(p.class_id)!.push(p);
    }
    return Array.from(g.entries());
  });

  async function executeSelected() {
    if (plans.length === 0) return;
    const okay = confirm(
      `${plans.length}개 파일을 정리합니다 (온톨로지 targetFolder로 이동).\n` +
        `되돌리기 버튼으로 복원할 수 있습니다.`,
    );
    if (!okay) return;
    busy = true;
    try {
      const r = await api.executeMoves(plans);
      results = r;
      plans = [];
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  async function undoMoves() {
    busy = true;
    try {
      const r = await api.undoLastMoves();
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
    정리정돈 {scannedRoot ? "" : "(먼저 스캔하세요)"}
    <button onclick={loadPlans} disabled={busy || !scannedRoot}>{busy ? "계획 중…" : "정리정돈 미리보기"}</button>
  </h2>
  {#if loadError}<p class="error">{loadError}</p>{/if}

  {#if plans.length === 0 && !busy}
    <p class="muted">미리보기를 눌러 정리 계획을 확인하세요.</p>
  {/if}

  {#each grouped as [classId, group] (classId)}
    <div class="group">
      <div class="ghead">{classId} — {group.length}개 파일</div>
      <ul>
        {#each group as p (p.src)}
          <li>
            <span class="path" title={p.src}>{p.src}</span>
            <span class="arrow">→</span>
            <span class="path" title={p.dst}>{p.dst}</span>
          </li>
        {/each}
      </ul>
    </div>
  {/each}

  {#if plans.length > 0}
    <div class="actions">
      <button onclick={executeSelected} disabled={busy}>
        {plans.length}개 파일 정리
      </button>
    </div>
  {/if}

  {#if results.length > 0}
    <p>{results.filter((r) => r.ok).length}/{results.length}개 파일 이동 완료 — 되돌리기 가능합니다.</p>
    {#if results.some((r) => !r.ok)}
      <ul class="errors">
        {#each results.filter((r) => !r.ok) as r (r.path)}
          <li title={r.path}>⚠ {r.path} — {r.error}</li>
        {/each}
      </ul>
    {/if}
    <div class="actions">
      <button onclick={undoMoves} disabled={busy}>
        마지막 이동 되돌리기
      </button>
    </div>
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .group { border: 1px solid #eee; border-radius: 4px; margin: 0.5rem 0; padding: 0.5rem; }
  .ghead { font-size: 0.85rem; color: #555; margin-bottom: 0.25rem; }
  .group ul { list-style: none; padding: 0; margin: 0; }
  .group li { padding: 1px 0; display: flex; gap: 0.5rem; align-items: center; }
  .path { overflow-wrap: anywhere; flex: 1; }
  .arrow { color: #999; flex-shrink: 0; }
  .muted { color: #999; }
  .error { color: #b00; }
  .errors { color: #b00; font-size: 0.85rem; list-style: none; padding: 0; }
  .actions { margin-top: 0.5rem; display: flex; gap: 0.5rem; }
</style>
