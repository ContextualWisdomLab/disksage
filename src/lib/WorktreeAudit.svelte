<script lang="ts">
  import * as api from "./api";

  let { scannedRoot }: { scannedRoot: string | null } = $props();
  let repository = $state("");
  let report = $state<api.WorktreeAudit | null>(null);
  let busy = $state(false);
  let error = $state("");
  let target = $derived(repository.trim() || scannedRoot || "");

  async function audit() {
    if (!target) return;
    busy = true;
    error = "";
    try {
      report = await api.listStaleWorktrees(target);
    } catch (e) {
      report = null;
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h2>
    Git worktree 감사
    <button onclick={audit} disabled={busy || !target}>{busy ? "확인 중…" : "등록 확인"}</button>
  </h2>
  <label class="repo-input">
    저장소 경로
    <input bind:value={repository} placeholder={scannedRoot ?? "/path/to/repository"} />
  </label>
  <p class="muted">읽기 전용 감사입니다. `git worktree prune/remove`와 파일 삭제는 호출하지 않습니다.</p>
  {#if error}<p class="error">{error}</p>{/if}
  {#if report}
    <p class="summary">
      등록 {report.worktrees.length}개 · stale/prunable {report.stale_count}개 ·
      metadata prune 검토 후보 {report.metadata_prune_eligible_count}개
    </p>
    <p class="fingerprint">
      registration fingerprint: <code>{report.registration_fingerprint.slice(0, 16)}…</code>
    </p>
    <p class={report.evidence_complete ? "ok" : "warning"}>
      증거 상태: {report.evidence_complete ? "완전" : "불완전 — 수동 검토 필요"}
    </p>
    {#if !report.evidence_complete}
      <p class="warning">Git 목록 timeout으로 관리자 등록을 읽기 전용 fallback으로 확인했습니다. prune/remove는 실행하지 않았습니다.</p>
    {/if}
    {#if report.stale_count > 0}
      <ul class="stale-list">
        {#each report.worktrees.filter((worktree) => worktree.metadata_prune_eligible) as worktree (worktree.path)}
          <li>
            <strong>{worktree.path}</strong>
            <span>{worktree.branch ?? (worktree.detached ? "detached" : "branch 미확인")}</span>
            <small>{worktree.prunable_reason ?? "경로 부재"}</small>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="ok">stale 등록 없음</p>
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .repo-input { display: flex; gap: 0.5rem; align-items: center; }
  input { flex: 1; min-width: 16rem; }
  .muted, .summary, .fingerprint, small { color: #666; font-size: 0.8rem; }
  .error { color: #b00; }
  .ok { color: #2a7; }
  .warning { color: #a65b00; }
  .stale-list { padding-left: 1.2rem; }
  .stale-list li { display: grid; gap: 0.15rem; margin: 0.5rem 0; }
</style>
