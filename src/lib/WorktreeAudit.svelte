<script lang="ts">
  import * as api from "./api";
  import { confirm } from "@tauri-apps/plugin-dialog";

  let { scannedRoot }: { scannedRoot: string | null } = $props();
  let repository = $state("");
  let report = $state<api.WorktreeAudit | null>(null);
  let busy = $state(false);
  let error = $state("");
  let pruneResult = $state<api.WorktreePruneResult | null>(null);
  let target = $derived(repository.trim() || scannedRoot || "");

  async function audit() {
    if (!target) return;
    busy = true;
    error = "";
    try {
      report = await api.listStaleWorktrees(target);
      pruneResult = null;
    } catch (e) {
      report = null;
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function pruneMetadata() {
    if (!report || !report.evidence_complete || report.metadata_prune_eligible_count === 0) return;
    const okay = await confirm(
      `${report.metadata_prune_eligible_count}개 stale Git 등록 메타데이터를 정리합니다.\n` +
        "worktree 디렉터리와 브랜치, 파일은 삭제하지 않습니다.",
      { title: "DiskSage", kind: "warning" },
    );
    if (!okay) return;
    const confirmation = "DiskSage stale worktree metadata 정리 승인";
    busy = true;
    error = "";
    try {
      pruneResult = await api.pruneStaleWorktreeMetadata(
        report.repository,
        report.registration_fingerprint,
        confirmation,
      );
      report = await api.listStaleWorktrees(report.repository);
    } catch (e) {
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
  <p class="muted">감사는 읽기 전용입니다. 정리는 명시적 승인 뒤 Git 등록 메타데이터만 prune하며 worktree 디렉터리와 파일은 삭제하지 않습니다.</p>
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
    {#if report.evidence_complete && report.metadata_prune_eligible_count > 0}
      <button class="prune" onclick={pruneMetadata} disabled={busy}>
        stale 등록 메타데이터 {report.metadata_prune_eligible_count}개 정리
      </button>
    {/if}
    {#if !report.evidence_complete}
      <p class="warning">Git 목록 timeout으로 관리자 등록을 읽기 전용 fallback으로 확인했습니다. prune/remove는 실행하지 않았습니다.</p>
    {/if}
    {#if pruneResult}
      <p class="ok">Git 등록 메타데이터 {pruneResult.stale_before - pruneResult.stale_after}개를 정리했습니다. 파일시스템 삭제: 없음.</p>
    {/if}
    {#if report.stale_count > 0}
      <ul class="stale-list">
        {#each report.worktrees.filter((worktree) => worktree.prunable_reason !== null || !worktree.exists) as worktree (worktree.path)}
          <li>
            <strong>{worktree.path}</strong>
            <span>{worktree.branch ?? (worktree.detached ? "detached" : "branch 미확인")}</span>
            <small>{worktree.prunable_reason ?? "경로 부재"}</small>
            <small class={worktree.metadata_prune_eligible ? "eligible" : "manual"}>
              {worktree.metadata_prune_eligible ? "메타데이터 prune 가능" : "디렉터리 존재/증거 부족 — 수동 검토"}
            </small>
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
  .prune { margin: 0.5rem 0; }
  .stale-list { padding-left: 1.2rem; }
  .stale-list li { display: grid; gap: 0.15rem; margin: 0.5rem 0; }
  .eligible { color: #2a7; }
  .manual { color: #a65b00; }
</style>
