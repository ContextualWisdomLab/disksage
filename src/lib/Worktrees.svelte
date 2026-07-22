<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let searchRoot = $state("");
  let minAgeDays = $state(30);
  let timeoutSeconds = $state(30);
  let busy = $state(false);
  let loadError = $state("");
  let report: api.WorktreeReport | null = $state(null);

  $effect(() => {
    if (!searchRoot && scannedRoot) searchRoot = scannedRoot;
  });

  async function inspect() {
    if (!searchRoot) return;
    busy = true;
    loadError = "";
    report = null;
    try {
      report = await api.listStaleWorktrees(
        searchRoot,
        Math.max(0, Math.floor(minAgeDays)),
        Math.min(600, Math.max(1, Math.floor(timeoutSeconds))),
      );
    } catch (error) {
      loadError = String(error);
    } finally {
      busy = false;
    }
  }

  function dirtyLabel(value: boolean | null): string {
    if (value === true) return "dirty";
    if (value === false) return "clean";
    return "status unknown";
  }
</script>

<section>
  <h2>Git worktree 인벤토리 <span class="dry">READ-ONLY</span></h2>
  <p class="muted">
    로컬 Git 근거만 사용하여 오래된 linked worktree를 찾습니다. fetch·prune·remove·파일 삭제는 수행하지 않습니다.
  </p>
  <div class="controls">
    <label class="root">
      저장소 검색 루트
      <input bind:value={searchRoot} placeholder="/path/to/development/root" disabled={busy} />
    </label>
    <label>
      stale 최소 일수
      <input type="number" min="0" step="1" bind:value={minAgeDays} disabled={busy} />
    </label>
    <label>
      최대 검사 시간(초)
      <input type="number" min="1" max="600" step="1" bind:value={timeoutSeconds} disabled={busy} />
    </label>
    <button onclick={inspect} disabled={busy || !searchRoot}>
      {busy ? "검사 중…" : "worktree 검사"}
    </button>
  </div>
  {#if loadError}<p class="error">{loadError}</p>{/if}

  {#if report}
    <div class="summary">
      linked 등록 저장소 {report.repository_count}개 · 확인한 worktree {report.worktrees.length}개 ·
      고아 worktree {report.orphaned_worktrees.length}개 · 안전 조건 충족 예상 회수
      {fmtBytes(report.potentially_reclaimable_bytes)} · 별도 검토 가능한 재생성 산출물
      {fmtBytes(report.reviewable_generated_artifact_bytes)}
    </div>
    <p class:warning={!report.evidence_complete} class:ok={report.evidence_complete}>
      증거 {report.evidence_complete ? "완전" : "부분"} · {(report.elapsed_ms / 1000).toFixed(1)}초
    </p>
    <p class="muted">저장소 검색 깊이: 선택 루트에서 최대 {report.search_max_depth}단계</p>
    <p class="warning">
      기준 브랜치는 fetch하지 않은 로컬 ref입니다. 시간 제한 또는 부분 측정은 제거 가능 판정을 차단하며,
      설정한 최대 검사 시간이 지나면 수집된 부분 결과만 표시됩니다. 실제 제거 전 원격 동기화와 재검토가 필요합니다.
    </p>
    {#if report.scan_issues.length > 0}
      <details class="issues">
        <summary>검사 실패/timeout {report.scan_issues.length}건</summary>
        <ul>
          {#each report.scan_issues as issue}
            <li title={issue.path}>{issue.operation}: {issue.reason} · {issue.path}</li>
          {/each}
        </ul>
      </details>
    {/if}
    {#if report.worktrees.length === 0}
      <p class="muted">검색 루트 아래에서 등록된 Git worktree를 찾지 못했습니다.</p>
    {:else}
      <ul class="worktrees">
        {#each report.worktrees as worktree (worktree.path)}
          <li class:eligible={worktree.removal_eligible}>
            <div class="line">
              <strong>
                {worktree.filesystem_scanned
                  ? `${fmtBytes(worktree.allocated_bytes)}${worktree.filesystem_scan_complete ? "" : "+ (부분 측정)"}`
                  : "primary 크기 측정 생략"}
              </strong>
              <span>{worktree.branch ?? "detached"}</span>
              <span>{worktree.age_days.toLocaleString()}일</span>
              <span>{dirtyLabel(worktree.dirty)}</span>
              {#if worktree.is_primary}<em>primary 보호</em>{/if}
              {#if worktree.removal_eligible}<em class="ok">제거 검토 가능</em>{/if}
              {#if worktree.metadata_prune_eligible}<em>누락 경로 메타데이터 prune 검토</em>{/if}
            </div>
            <div class="path" title={worktree.path}>{worktree.path}</div>
            <div class="details">
              HEAD {worktree.head.slice(0, 12)} · 기준 {worktree.default_ref ?? "미확인"} ·
              ahead {worktree.ahead ?? "?"} / behind {worktree.behind ?? "?"} ·
              merged {worktree.merged_into_default === null ? "?" : worktree.merged_into_default}
            </div>
            {#if worktree.review_reasons.length > 0}
              <div class="reasons">보류/보호 근거: {worktree.review_reasons.join(", ")}</div>
            {/if}
            {#if worktree.generated_artifacts.length > 0}
              <details class="artifacts">
                <summary>재생성 산출물 {fmtBytes(worktree.generated_artifact_bytes)}</summary>
                <ul>
                  {#each worktree.generated_artifacts as artifact (artifact.path)}
                    <li title={artifact.path}>{artifact.kind} · {fmtBytes(artifact.allocated_bytes)} · {artifact.path}</li>
                  {/each}
                </ul>
              </details>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if report.orphaned_worktrees.length > 0}
      <h3>Git 메타데이터가 끊긴 고아 worktree</h3>
      <p class="warning">
        원래 HEAD와 dirty 상태를 증명할 수 없어 소스 트리 전체 제거는 금지됩니다. 아래 재생성 산출물만 별도로 검토하세요.
      </p>
      <ul class="worktrees orphaned">
        {#each report.orphaned_worktrees as worktree (worktree.path)}
          <li>
            <div class="line">
              <strong>{fmtBytes(worktree.allocated_bytes)}{worktree.filesystem_scan_complete ? "" : "+ (부분 측정)"}</strong>
              <span>재생성 산출물 {fmtBytes(worktree.generated_artifact_bytes)}</span>
              <em>소스 제거 금지</em>
            </div>
            <div class="path" title={worktree.path}>{worktree.path}</div>
            <div class="details" title={worktree.missing_git_dir}>누락 gitdir: {worktree.missing_git_dir}</div>
            <div class="reasons">보호 근거: {worktree.review_reasons.join(", ")}</div>
            {#if worktree.generated_artifacts.length > 0}
              <details class="artifacts">
                <summary>재생성 산출물 경로</summary>
                <ul>
                  {#each worktree.generated_artifacts as artifact (artifact.path)}
                    <li title={artifact.path}>{artifact.kind} · {fmtBytes(artifact.allocated_bytes)} · {artifact.path}</li>
                  {/each}
                </ul>
              </details>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.5rem; align-items: center; }
  .dry { font-size: 0.7rem; color: #fff; background: #59636e; border-radius: 8px; padding: 2px 7px; }
  .controls { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: end; }
  label { display: grid; gap: 0.2rem; font-size: 0.8rem; color: #555; }
  label.root { flex: 1 1 28rem; }
  label.root input { width: 100%; box-sizing: border-box; }
  label:not(.root) input { width: 7rem; }
  .summary { margin-top: 0.8rem; font-weight: 600; }
  .worktrees { list-style: none; padding: 0; max-height: 34rem; overflow-y: auto; }
  .worktrees li { border: 1px solid #e3e3e3; border-radius: 4px; padding: 0.55rem; margin: 0.35rem 0; }
  .worktrees li.eligible { border-color: #2a8f4a; background: #f5fff7; }
  .line { display: flex; flex-wrap: wrap; gap: 0.6rem; font-size: 0.8rem; }
  .line em { color: #9a5b00; }
  .line em.ok { color: #187338; }
  .path { overflow-wrap: anywhere; font-size: 0.85rem; }
  .details, .reasons { color: #777; font-size: 0.75rem; margin-top: 0.2rem; overflow-wrap: anywhere; }
  .issues { color: #9a5b00; font-size: 0.8rem; }
  .issues ul { margin: 0.25rem 0; padding-left: 1.2rem; }
  .artifacts { color: #555; font-size: 0.75rem; margin-top: 0.35rem; }
  .artifacts ul { margin: 0.25rem 0; padding-left: 1.2rem; }
  .artifacts li { border: 0; padding: 0; margin: 0.15rem 0; overflow-wrap: anywhere; }
  .orphaned li { border-color: #b97800; background: #fffaf0; }
  .muted { color: #777; }
  .ok { color: #187338; }
  .warning { color: #8a5700; }
  .error { color: #b00; }
</style>
