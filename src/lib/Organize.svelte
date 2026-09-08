<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { verdictBadge } from "./verdictBadge";
  import { confirm } from "@tauri-apps/plugin-dialog";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let plans: api.MovePlan[] = $state([]);
  let previewLoaded = $state(false);
  let observedFileCount = $state(0);
  let retained: api.OrganizationPreview["retained"] = $state([]);
  let busy = $state(false);
  let bundleParent = $state("");
  let bundlePlan: api.MovePlan | null = $state(null);
  let loadError = $state("");
  let results: api.CleanResult[] = $state([]);
  let verdicts: Record<string, api.Verdict> = $state({});
  let exportStatus = $state("");

  $effect(() => {
    scannedRoot;
    bundleParent;
    bundlePlan = null;
  });

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
    plans = [];
    retained = [];
    previewLoaded = false;
    try {
      const preview = await api.planOrganize(scannedRoot);
      plans = preview.moves;
      observedFileCount = preview.observed_file_count;
      retained = preview.retained;
      previewLoaded = true;
      loadVerdicts(plans.map((p) => p.src));
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  async function loadBundlePlan() {
    if (!scannedRoot) return;
    busy = true;
    loadError = "";
    bundlePlan = null;
    const root = scannedRoot;
    const parent = bundleParent;
    try {
      const planned = await api.planBundleOrganize(root, parent);
      if (root === scannedRoot && parent === bundleParent) bundlePlan = planned;
    } catch (error) {
      loadError = String(error);
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

  async function executeSelected(selected: api.MovePlan[]) {
    if (selected.length === 0) return;
    const okay = await confirm(
      `${selected.length}개 항목을 미리보기에 표시된 폴더로 옮깁니다.\n` +
        `이동 기록을 남깁니다. 변경이나 경로 충돌이 있으면 되돌리기를 보류합니다.`,
      { title: "DiskSage", kind: "warning" },
    );
    if (!okay) return;
    busy = true;
    try {
      const r = await api.executeMoves(selected);
      results = r;
      plans = [];
      bundlePlan = null;
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

  async function copyLineageHandoff() {
    if (plans.length === 0) return;
    busy = true;
    exportStatus = "";
    try {
      const batch = await api.exportOrganizationLineage(plans);
      await navigator.clipboard.writeText(JSON.stringify(batch, null, 2));
      exportStatus = "경로 없는 계보 계약을 클립보드에 복사했습니다.";
    } catch (e) {
      exportStatus = `계보 내보내기 실패: ${String(e)}`;
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
  <details>
    <summary>선택한 폴더를 기존 묶음 그대로 이동</summary>
    <p class="muted">주제를 자동 분류하지 않고 현재 폴더 이름과 구성원을 함께 보존합니다. 현재는 하위 폴더 없이 로컬 파일 32개, 합계 8MiB 이하인 문서 묶음을 지원합니다.</p>
    <label>대상 상위 폴더
      <input bind:value={bundleParent} placeholder="대상 폴더의 절대 경로" disabled={busy} />
    </label>
    <button onclick={loadBundlePlan} disabled={busy || !scannedRoot || !bundleParent}>묶음 미리보기</button>
    {#if bundlePlan?.bundle}
      <p>{bundlePlan.src} → {bundlePlan.dst}</p>
      <ul>{#each bundlePlan.bundle.files as file (file.name)}<li>{file.name} · {fmtBytes(file.bytes)}</li>{/each}</ul>
      <button onclick={() => bundlePlan && executeSelected([bundlePlan])} disabled={busy}>이 묶음 이동</button>
    {/if}
  </details>
  {#if loadError}<p class="error">{loadError}</p>{/if}

  {#if plans.length === 0 && !busy}
    <p class="muted" role="status">{previewLoaded ? "이번 미리보기에서 이동할 파일은 없습니다." : "미리보기를 눌러 정리 계획을 확인하세요."}</p>
  {/if}

  {#if previewLoaded}
    <p class="muted">확인한 파일 {observedFileCount}개를 바탕으로 한 미리보기입니다. 전체 폴더 조사가 완료됐다는 뜻은 아닙니다.</p>
  {/if}

  {#if retained.length > 0}
    <details>
      <summary>현재 위치에 유지할 파일 {retained.length}개</summary>
      <ul>
        {#each retained as item (item.path)}
          <li>{item.path} — {item.reason === "agent_state"
            ? "대화와 작업 상태를 보존하기 위해 현재 위치에 유지합니다."
            : item.reason === "package_boundary"
            ? "앱이나 프로젝트 묶음 내부 파일이므로 따로 옮기지 않습니다."
            : item.reason === "companion_bundle"
              ? "함께 보존할 파일이 있어 한 파일만 따로 옮기지 않습니다."
              : item.reason === "project_boundary_unverified"
              ? "프로젝트 내부 자료이거나 경계를 확인할 수 없어 현재 위치에 보존합니다."
              : "이번 미리보기에는 이동 계획이 없습니다. 현재 위치에 보존합니다."}</li>
        {/each}
      </ul>
    </details>
  {/if}

  {#each grouped as [classId, group] (classId)}
    <div class="group">
      <div class="ghead">{classId} — {group.length}개 파일</div>
      <ul>
        {#each group as p (p.src)}
          <li>
            <span class="path" title={p.src}>{p.src}</span>
            <span class="lineage">{p.classification_source === "user_rule"
              ? "사용자 규칙에 따른 제안 · 내용 검증 안 됨"
              : p.classification_source === "model_picker"
              ? "AI 분류 제안 · 내용 검증 안 됨"
              : p.classification_source === "extension"
              ? "파일 형식에 따른 제안 · 내용 검증 안 됨"
              : "분류 근거 확인 필요"}</span>
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
      <button onclick={() => executeSelected(plans)} disabled={busy}>
        {plans.length}개 파일 정리
      </button>
      <button onclick={copyLineageHandoff} disabled={busy}>
        계보 계약 복사
      </button>
    </div>
    {#if exportStatus}<p class="muted">{exportStatus}</p>{/if}
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
  .lineage { color: #666; font-size: 0.75rem; flex-shrink: 0; }
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
