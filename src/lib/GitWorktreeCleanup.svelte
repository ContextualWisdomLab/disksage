<script lang="ts">
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let repositoryRoot = $state("");
  let retentionText = $state("");
  let includeClosedPullRequests = $state(false);
  let includeStaleOpenPullRequests = $state(false);
  let staleOpenPullRequestCutoffDate = $state("");
  let planning = $state(false);
  let executing = $state(false);
  let error = $state("");
  let report: api.GitWorktreeAuditReport | null = $state(null);
  let confirmationPhrase = $state("");
  let rationale = $state("");
  let removal: api.StaleGitWorktreeRemovalOutput | null = $state(null);

  $effect(() => {
    if (!repositoryRoot && scannedRoot) repositoryRoot = scannedRoot;
  });

  function candidateEntries(): api.GitWorktreeAuditEntry[] {
    return report?.entries.filter((entry) => entry.disposition === "removal-candidate") ?? [];
  }

  function evidenceGapEntries(): api.GitWorktreeAuditEntry[] {
    return report?.entries.filter((entry) => entry.disposition === "evidence-gap") ?? [];
  }

  function retentionReferences(): string[] {
    return [...new Set(
      retentionText
        .split(/\r?\n/)
        .map((value) => value.trim())
        .filter(Boolean),
    )];
  }

  function resetDecision() {
    report = null;
    confirmationPhrase = "";
    rationale = "";
    removal = null;
    error = "";
  }

  function staleOpenPullRequestCutoffMs(): number | null {
    if (!includeStaleOpenPullRequests || !/^\d{4}-\d{2}-\d{2}$/.test(staleOpenPullRequestCutoffDate)) {
      return null;
    }
    const parsed = Date.parse(`${staleOpenPullRequestCutoffDate}T00:00:00.000Z`);
    return Number.isFinite(parsed)
      && new Date(parsed).toISOString().slice(0, 10) === staleOpenPullRequestCutoffDate
      ? parsed
      : null;
  }

  async function chooseRepository() {
    error = "";
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        defaultPath: repositoryRoot || scannedRoot || undefined,
        title: "Git 저장소 또는 보조 폴더 선택",
      });
      if (typeof selected !== "string") return;
      repositoryRoot = selected;
      resetDecision();
    } catch {
      error = "폴더를 선택하지 못했습니다. 다시 시도하세요.";
    }
  }

  async function inspectWorktrees() {
    const root = repositoryRoot.trim();
    const references = retentionReferences();
    if (!root || references.length === 0) return;
    const staleCutoffMs = staleOpenPullRequestCutoffMs();
    if (includeStaleOpenPullRequests && staleCutoffMs === null) {
      error = "오래된 진행 중 작업을 확인하려면 기준일을 입력하세요.";
      return;
    }
    planning = true;
    resetDecision();
    try {
      report = await api.planStaleGitWorktrees(root, references, includeClosedPullRequests, staleCutoffMs);
      repositoryRoot = report.repository_root;
      retentionText = report.retention_references
        .map((binding) => binding.reference_ref)
        .join("\n");
    } catch {
      error = "보조 폴더를 확인하지 못했습니다. 경로와 보존할 기준을 확인한 뒤 다시 시도하세요.";
    } finally {
      planning = false;
    }
  }

  function executionReady(): boolean {
    return report !== null
      && report.evidence_complete
      && report.removal_candidate_count > 0
      && report.exact_approval_phrase !== null
      && confirmationPhrase === report.exact_approval_phrase
      && rationale.trim().length > 0
      && !executing
      && removal === null;
  }

  async function removeWorktrees() {
    if (!report || !executionReady()) return;
    const approved = await confirm(
        `${report.removal_candidate_count}개 보조 폴더(최대 ${fmtBytes(report.removal_candidate_allocated_bytes)})를 정리합니다.\n\n`
        + "각 항목은 실행 직전에 다시 확인합니다. 보존할 작업 기록은 유지됩니다. 정리한 폴더는 휴지통으로 가지 않습니다.",
      { title: "오래된 보조 폴더 정리", kind: "warning" },
    );
    if (!approved) return;
    executing = true;
    error = "";
    try {
      removal = await api.removeStaleGitWorktrees(
        report.repository_root,
        report.retention_references.map((binding) => binding.reference_ref),
        includeClosedPullRequests,
        report.stale_open_pull_request_cutoff_ms,
        report.removal_plan_fingerprint,
        confirmationPhrase,
        rationale.trim(),
      );
      confirmationPhrase = "";
      rationale = "";
    } catch {
      error = "보조 폴더를 정리하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.";
    } finally {
      executing = false;
    }
  }
</script>

<div class="worktree-panel">
  <strong>오래된 보조 폴더</strong>
  <p class="muted">
    지정한 보존 기준에 이미 포함된 깨끗하고 사용하지 않는 보조 폴더만 찾습니다. 확인 단계에서는 파일을 변경하지 않습니다.
  </p>

  <div class="inputs">
    <label>
    저장소 또는 보조 폴더 위치
      <input
        class="path-input"
        type="text"
        bind:value={repositoryRoot}
        oninput={resetDecision}
        autocomplete="off"
        spellcheck="false"
        disabled={planning || executing}
      />
    </label>
    <button onclick={chooseRepository} disabled={planning || executing}>폴더 선택</button>
  </div>
  <label>
    보존할 버전 기준 — 한 줄에 하나
    <textarea
      class="references"
      bind:value={retentionText}
      oninput={resetDecision}
      placeholder={'예: origin/main\norigin/develop'}
      autocomplete="off"
      spellcheck="false"
      disabled={planning || executing}
  ></textarea>
  </label>
  <label class="option">
    <input type="checkbox" bind:checked={includeClosedPullRequests} onchange={resetDecision} disabled={planning || executing} />
    완료된 작업과 연결된 항목도 확인
  </label>
  {#if includeClosedPullRequests}
    <p class="muted">
      병합 없이 종료된 PR의 깨끗한 보조 폴더도 정리 후보가 될 수 있습니다. 브랜치와 커밋은 유지됩니다. 선택한 저장소에 로그인된 GitHub 연결이 필요합니다.
    </p>
  {/if}
  <label class="option">
    <input type="checkbox" bind:checked={includeStaleOpenPullRequests} onchange={resetDecision} disabled={planning || executing} />
    오래된 진행 중 작업도 확인
  </label>
  {#if includeStaleOpenPullRequests}
    <label>
      기준일 (이 날짜보다 먼저 생성된 작업)
      <input
        type="date"
        bind:value={staleOpenPullRequestCutoffDate}
        onchange={resetDecision}
        disabled={planning || executing}
      />
    </label>
  {/if}
  <button
    onclick={inspectWorktrees}
    disabled={planning || executing || !repositoryRoot.trim() || retentionReferences().length === 0}
  >
    {planning ? "보조 폴더와 사용 여부 확인 중…" : "보조 폴더 확인"}
  </button>

  {#if error}<p class="error" role="alert">작업에 실패했습니다. 상태를 확인한 뒤 다시 시도하세요: {error}</p>{/if}

  {#if report}
    <div class="report" aria-live="polite">
      <div class="summary">
        <strong>제거 후보 {report.removal_candidate_count}개 · 최대 {fmtBytes(report.removal_candidate_allocated_bytes)}</strong>
        <span>보존 {report.preserved_count}개</span>
        <span>확인 필요 {report.evidence_gap_count}개</span>
        {#if report.stale_open_pull_request_cutoff_ms !== null}
          <span>진행 중 작업 기준일 {new Date(report.stale_open_pull_request_cutoff_ms).toISOString().slice(0, 10)}</span>
        {/if}
      </div>
      <p class="fingerprint">승인 확인 코드: {report.removal_plan_fingerprint}</p>

      {#if candidateEntries().length > 0}
        <ul class="worktrees">
          {#each candidateEntries() as candidate (candidate.path_fingerprint)}
            <li>
              <div><strong>보존 기준 연결 확인됨</strong> · {fmtBytes(candidate.size.allocated_bytes)}</div>
              <div class="path" title={candidate.path}>{candidate.path}</div>
            </li>
          {/each}
        </ul>
      {/if}

      {#if evidenceGapEntries().length > 0}
        <div class="blocked">
          <strong>확인이 끝나지 않아 정리를 시작할 수 없습니다.</strong>
          <ul>
            {#each evidenceGapEntries() as entry (entry.path_fingerprint)}
              <li><span class="path">{entry.path}</span> — 확인이 필요한 항목이 있습니다. 상태를 다시 확인하세요.</li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if removal}
        {#if removal.result.verification_complete}
          <p class="safe">
            {removal.result.removed_count}개 보조 폴더를 정리했고 보존할 작업 기록은 유지했습니다.
            사전 할당량 기준 최대 {fmtBytes(removal.result.removed_allocated_bytes_upper_bound)}입니다.
          </p>
        {:else}
          <p class="warning">
            일부 항목의 확인이 끝나지 않았습니다. 확인된 정리 {removal.result.removed_count}/{removal.result.planned_candidate_count}개입니다. 상태를 확인한 뒤 다시 시도하세요.
          </p>
        {/if}
        <p class="muted">승인 기록을 저장했습니다.</p>
        {#if removal.result_path}
          <p class="muted">정리 결과를 저장했습니다.</p>
        {:else}
          <p class="error" role="alert">
            실행 결과는 확인됐지만 기록을 저장하지 못했습니다. 결과를 별도로 보관하세요.
          </p>
        {/if}
      {:else if report.evidence_complete && report.exact_approval_phrase}
        <div class="approval">
          <p class="warning">
            아래 승인 확인 코드를 입력하세요. 실행 직전에 전체 계획과 각 후보를 다시 확인하며 달라지면 중단합니다.
          </p>
          <code>{report.exact_approval_phrase}</code>
          <label>
            승인 확인 코드
            <textarea
              class="confirmation"
              bind:value={confirmationPhrase}
              autocomplete="off"
              spellcheck="false"
              disabled={executing}
            ></textarea>
          </label>
          <label>
            정리 사유
            <textarea
              bind:value={rationale}
              maxlength="1000"
              placeholder="예: 보존 기준에 포함되고 사용하지 않는 보조 폴더임을 확인"
              disabled={executing}
            ></textarea>
          </label>
          <button onclick={removeWorktrees} disabled={!executionReady()}>
            {executing ? "상태 재확인 후 정리 중…" : "상태를 재확인하고 보조 폴더 정리"}
          </button>
        </div>
      {:else if report.removal_candidate_count === 0}
        <p class="muted">현재 정리 조건을 모두 만족하는 보조 폴더가 없습니다.</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .worktree-panel { margin-top: 1rem; padding-top: 1rem; border-top: 1px solid #b7c6d8; display: grid; gap: 0.55rem; }
  .inputs { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: end; }
  .inputs label { flex: 1 1 30rem; }
  label { display: grid; gap: 0.2rem; font-size: 0.82rem; color: #4d5660; }
  .path-input, .references, .confirmation { width: min(60rem, 90vw); font-family: ui-monospace, monospace; }
  .references { min-height: 4rem; }
  .confirmation { min-height: 4.5rem; }
  .report { display: grid; gap: 0.55rem; padding: 0.75rem; border: 1px solid #72889c; border-radius: 4px; background: #f7fafc; }
  .summary { display: flex; flex-wrap: wrap; gap: 0.8rem; align-items: baseline; }
  .fingerprint { margin: 0; overflow-wrap: anywhere; font: 0.75rem ui-monospace, monospace; color: #59636e; }
  .worktrees { list-style: none; margin: 0; padding: 0; max-height: 30vh; overflow-y: auto; }
  .worktrees li { padding: 0.45rem 0; border-bottom: 1px solid #d9e0e6; }
  .path { overflow-wrap: anywhere; color: #66717d; font-size: 0.78rem; }
  .blocked { padding: 0.6rem; border: 1px solid #b74a4a; background: #fff6f6; }
  .blocked ul { margin-bottom: 0; }
  .approval { display: grid; gap: 0.55rem; justify-items: start; padding: 0.7rem; border: 1px solid #b78335; border-radius: 4px; background: #fffaf1; }
  .approval code { max-width: min(60rem, 90vw); overflow-wrap: anywhere; user-select: all; }
  .approval textarea { width: min(60rem, 90vw); resize: vertical; }
  .muted { color: #727b84; margin: 0; }
  .warning { color: #8a5700; margin: 0; }
  .safe { color: #276437; margin: 0; }
  .error { color: #b00; margin: 0; }
</style>
