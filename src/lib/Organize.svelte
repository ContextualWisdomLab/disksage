<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { verdictBadge } from "./verdictBadge";
  import { confirm } from "@tauri-apps/plugin-dialog";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let plans: api.MovePlan[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");
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

  async function loadPlans() {
    if (!scannedRoot) return;
    busy = true;
    loadError = "";
    results = [];
    try {
      plans = await api.planOrganize(scannedRoot);
      loadVerdicts(plans.map((p) => p.src));
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
    const okay = await confirm(
      `${plans.length}개 파일을 정리합니다 (온톨로지 targetFolder로 이동).\n` +
        `되돌리기 버튼으로 복원할 수 있습니다.`,
      { title: "DiskSage", kind: "warning" },
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
    <!-- 되돌리기는 상시 안전장치 — 저널에 이동 기록이 있으면 언제든 최근 이동을 복원한다.
         미리보기/실행 상태와 무관하게 항상 노출되어야 한다(그렇지 않으면 재-미리보기로 사라짐). -->
    <button class="undo" onclick={undoMoves} disabled={busy}>마지막 이동 되돌리기</button>
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
            {#if verdicts[p.src]}
              {@const b = verdictBadge(verdicts[p.src])}
              <span class={b.cls} title={b.title}>{b.label}</span>
            {/if}
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
    <p>{results.filter((r) => r.ok).length}/{results.length}개 완료 — 위 "되돌리기"로 복원할 수 있습니다.</p>
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
  .group li { padding: 1px 0; display: flex; gap: 0.5rem; align-items: center; }
  .path { overflow-wrap: anywhere; flex: 1; }
  .arrow { color: #999; flex-shrink: 0; }
  .muted { color: #999; }
  .error { color: #b00; }
  .errors { color: #b00; font-size: 0.85rem; list-style: none; padding: 0; }
  .actions { margin-top: 0.5rem; display: flex; gap: 0.5rem; }
  .undo { margin-left: auto; font-size: 0.85rem; }
  .badge-safe, .badge-caution, .badge-keep, .badge-unrated {
    display: inline-block; flex-shrink: 0; padding: 1px 6px; border-radius: 8px;
    font-size: 0.75rem; color: #fff;
  }
  .badge-safe { background: #2a8f4a; }
  .badge-caution { background: #b8860b; }
  .badge-keep { background: #b03030; }
  .badge-unrated { background: #888; }
</style>
