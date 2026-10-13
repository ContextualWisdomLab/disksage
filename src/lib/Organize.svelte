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
  let resultAction: "move" | "undo" | null = $state(null);
  let verdicts: Record<string, api.Verdict> = $state({});
  let exportStatus = $state("");

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
    exportStatus = "";
    plans = [];
    verdicts = {};
    results = [];
    resultAction = null;
    try {
      plans = await api.planOrganize(scannedRoot);
      await loadVerdicts(plans.map((p) => p.src));
    } catch {
      loadError = "정리 계획을 만들지 못했습니다. 스캔 대상 폴더의 접근 권한을 확인하고 스캔을 다시 실행한 뒤 미리보기를 다시 만드세요.";
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
    loadError = "";
    exportStatus = "";
    results = [];
    resultAction = null;
    try {
      const r = await api.executeMoves(plans);
      results = r;
      resultAction = "move";
      plans = [];
      verdicts = {};
    } catch {
      plans = [];
      verdicts = {};
      loadError = "파일 정리를 실행하지 못했습니다. 파일이 열려 있는지와 대상 폴더의 접근 권한을 확인한 뒤 새 미리보기부터 진행하세요.";
    } finally {
      busy = false;
    }
  }

  async function undoMoves() {
    busy = true;
    loadError = "";
    results = [];
    resultAction = null;
    try {
      const r = await api.undoLastMoves();
      results = r;
      resultAction = "undo";
    } catch {
      loadError = "마지막 이동을 되돌리지 못했습니다. 이동한 파일의 현재 위치와 원래 폴더의 접근 권한을 확인한 뒤 다시 되돌리세요.";
    } finally {
      busy = false;
    }
  }

  async function copyLineageHandoff() {
    if (plans.length === 0) return;
    busy = true;
    exportStatus = "";
    try {
      const batch = await api.exportOrganizationLineage(plans);
      await navigator.clipboard.writeText(JSON.stringify(batch, null, 2));
      exportStatus = "경로 없는 계보 계약을 클립보드에 복사했습니다.";
    } catch {
      exportStatus = "계보 계약을 클립보드에 복사하지 못했습니다. 클립보드 권한을 확인한 뒤 다시 시도하세요.";
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
  <p class="error live-region" role="alert" aria-live="assertive" aria-atomic="true">{loadError}</p>

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
            {#if p.lineage?.production_time_ms}
              <span class="lineage" title={p.lineage.lineage_fingerprint}>
                생산 {new Date(p.lineage.production_time_ms).toISOString().slice(0, 10)}
                · {p.lineage.production_time_source ?? "미상"}
              </span>
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
      <button onclick={copyLineageHandoff} disabled={busy}>
        계보 계약 복사
      </button>
    </div>
  {/if}

  <p class="muted export-status live-region" role="status" aria-live="polite" aria-atomic="true">{exportStatus}</p>

  <p class="result-status live-region" role="status" aria-live="polite" aria-atomic="true">
    {#if resultAction === "undo" && results.length === 0}
      되돌릴 최근 이동 기록이 없습니다.
    {:else if results.length > 0 && resultAction === "undo"}
      {results.filter((r) => r.ok).length}/{results.length}개 되돌리기를 완료했습니다. 다시 정리하려면 새 미리보기를 만드세요.
    {:else if results.length > 0}
      {results.filter((r) => r.ok).length}/{results.length}개 완료. 복원이 필요하면 위 ‘마지막 이동 되돌리기’를 사용하세요.
    {/if}
  </p>

  {#if results.length > 0 && results.some((r) => !r.ok)}
    <ul class="errors">
      {#each results.filter((r) => !r.ok) as r (r.path)}
        {#if resultAction === "undo"}
          <li title={r.path}>⚠ {r.path} — 현재 파일 위치와 원래 폴더의 접근 권한을 확인한 뒤 ‘마지막 이동 되돌리기’를 다시 실행하세요.</li>
        {:else}
          <li title={r.path}>⚠ {r.path} — 파일이 사용 중인지와 대상 폴더의 접근 권한을 확인한 뒤 새 미리보기부터 진행하세요.</li>
        {/if}
      {/each}
    </ul>
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
  .lineage { color: #666; font-size: 0.75rem; flex-shrink: 0; }
  .muted { color: #999; }
  .error { color: #b00; }
  .live-region:empty { margin: 0; }
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
