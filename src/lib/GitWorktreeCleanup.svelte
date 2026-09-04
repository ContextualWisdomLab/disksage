<script lang="ts">
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import {
    GIT_WORKTREE_AUDIT_FAILURE,
    GIT_WORKTREE_CONFIRMATION_FAILURE,
    GIT_WORKTREE_REMOVAL_FAILURE,
    GIT_WORKTREE_REPOSITORY_SELECTION_FAILURE,
    GIT_WORKTREE_RESULT_RECORD_FAILURE,
    evidenceGapActions,
    removalStoppedAction,
  } from "./gitWorktreeFeedback";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let repositoryRoot = $state("");
  let retentionText = $state("");
  let planning = $state(false);
  let choosing = $state(false);
  let confirming = $state(false);
  let executing = $state(false);
  let error = $state("");
  let report: api.GitWorktreeAuditReport | null = $state(null);
  let confirmationPhrase = $state("");
  let rationale = $state("");
  let removal: api.StaleGitWorktreeRemovalOutput | null = $state(null);
  let selectionSeq = 0;
  let auditSeq = 0;
  let removalSeq = 0;

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

  async function chooseRepository() {
    const seq = ++selectionSeq;
    choosing = true;
    error = "";
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        defaultPath: repositoryRoot || scannedRoot || undefined,
        title: "Git 저장소 또는 연결된 worktree 선택",
      });
      if (seq !== selectionSeq || typeof selected !== "string") return;
      repositoryRoot = selected;
      resetDecision();
    } catch {
      if (seq === selectionSeq) error = GIT_WORKTREE_REPOSITORY_SELECTION_FAILURE;
    } finally {
      if (seq === selectionSeq) choosing = false;
    }
  }

  async function inspectWorktrees() {
    const root = repositoryRoot.trim();
    const references = retentionReferences();
    if (!root || references.length === 0) return;
    planning = true;
    resetDecision();
    const seq = ++auditSeq;
    try {
      const nextReport = await api.planStaleGitWorktrees(root, references);
      if (seq !== auditSeq) return;
      report = nextReport;
      repositoryRoot = nextReport.repository_root;
      retentionText = nextReport.retention_references
        .map((binding) => binding.reference_ref)
        .join("\n");
    } catch {
      if (seq === auditSeq) error = GIT_WORKTREE_AUDIT_FAILURE;
    } finally {
      if (seq === auditSeq) planning = false;
    }
  }

  function executionReady(): boolean {
    return report !== null
      && report.evidence_complete
      && report.removal_candidate_count > 0
      && report.exact_approval_phrase !== null
      && confirmationPhrase === report.exact_approval_phrase
      && rationale.trim().length > 0
      && !confirming
      && !executing
      && removal === null;
  }

  async function removeWorktrees() {
    if (!report || !executionReady()) return;
    const approvedReport = report;
    const approvedPhrase = confirmationPhrase;
    const approvedRationale = rationale.trim();
    const seq = ++removalSeq;
    confirming = true;
    try {
      const approved = await confirm(
        `${approvedReport.removal_candidate_count}개 worktree 디렉터리(최대 ${fmtBytes(approvedReport.removal_candidate_allocated_bytes)})를 제거합니다.\n\n`
          + "각 항목은 실행 직전에 다시 검사합니다. 브랜치와 커밋은 유지하며 force·prune은 사용하지 않습니다. 제거된 디렉터리는 휴지통으로 가지 않습니다.",
        { title: "DiskSage 오래된 Git worktree 제거", kind: "warning" },
      );
      if (!approved || seq !== removalSeq || report !== approvedReport) return;
      executing = true;
      error = "";
      try {
        removal = await api.removeStaleGitWorktrees(
          approvedReport.repository_root,
          approvedReport.retention_references.map((binding) => binding.reference_ref),
          approvedReport.removal_plan_fingerprint,
          approvedPhrase,
          approvedRationale,
        );
        confirmationPhrase = "";
        rationale = "";
      } catch {
        report = null;
        confirmationPhrase = "";
        rationale = "";
        error = GIT_WORKTREE_REMOVAL_FAILURE;
      } finally {
        executing = false;
      }
    } catch {
      error = GIT_WORKTREE_CONFIRMATION_FAILURE;
    } finally {
      if (seq === removalSeq) confirming = false;
    }
  }
</script>

<div class="worktree-panel">
  <strong>오래된 Git worktree</strong>
  <p class="muted">
    명시한 보존 ref에 이미 포함된 깨끗하고 사용 중이 아닌 보조 worktree만 찾습니다. 감사 단계는 읽기 전용입니다.
  </p>

  <div class="inputs">
    <label>
      저장소 또는 worktree 절대 경로
      <input
        class="path-input"
        type="text"
        bind:value={repositoryRoot}
        oninput={resetDecision}
        autocomplete="off"
        spellcheck="false"
        disabled={choosing || planning || confirming || executing}
      />
    </label>
    <button onclick={chooseRepository} disabled={choosing || planning || confirming || executing}>
      {choosing ? "폴더 선택 창 여는 중…" : "폴더 선택"}
    </button>
  </div>
  <label>
    보존할 Git ref — 한 줄에 하나, 현재 로컬에서 해석되는 정확한 ref
    <textarea
      class="references"
      bind:value={retentionText}
      oninput={resetDecision}
      placeholder={'예: origin/main\norigin/develop'}
      autocomplete="off"
      spellcheck="false"
      disabled={choosing || planning || confirming || executing}
    ></textarea>
  </label>
  <button
    onclick={inspectWorktrees}
    disabled={choosing || planning || confirming || executing || !repositoryRoot.trim() || retentionReferences().length === 0}
  >
    {planning ? "worktree·브랜치·활성 사용 확인 중…" : "읽기 전용 worktree 감사"}
  </button>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if report}
    <div class="report" aria-live="polite">
      <div class="summary">
        <strong>제거 후보 {report.removal_candidate_count}개 · 최대 {fmtBytes(report.removal_candidate_allocated_bytes)}</strong>
        <span>보존 {report.preserved_count}개</span>
        <span>증거 공백 {report.evidence_gap_count}개</span>
      </div>
      <p class="fingerprint">계획 지문: {report.removal_plan_fingerprint}</p>
      <p class="fingerprint">보존 ref 지문: {report.retention_reference_set_fingerprint}</p>

      {#if candidateEntries().length > 0}
        <ul class="worktrees">
          {#each candidateEntries() as candidate (candidate.path_fingerprint)}
            <li>
              <div><strong>{candidate.branch ?? "분리된 HEAD"}</strong> · {fmtBytes(candidate.size.allocated_bytes)}</div>
              <div class="path" title={candidate.path}>{candidate.path}</div>
              <div class="oid">HEAD {candidate.head}</div>
            </li>
          {/each}
        </ul>
      {/if}

      {#if evidenceGapEntries().length > 0}
        <div class="blocked">
          <strong>증거가 부족해 전체 실행을 차단했습니다. 다음 항목을 확인하세요.</strong>
          <ul>
            {#each evidenceGapEntries() as entry (entry.path_fingerprint)}
              <li>
                <span class="path">{entry.path}</span>
                <ul class="evidence-actions">
                  {#each evidenceGapActions(entry.blockers) as action}
                    <li>{action}</li>
                  {/each}
                </ul>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if removal}
        {#if removal.result.verification_complete}
          <p class="safe">
            {removal.result.removed_count}개 worktree 제거와 Git 등록 해제, 브랜치 보존을 확인했습니다.
            사전 할당량 기준 최대 {fmtBytes(removal.result.removed_allocated_bytes_upper_bound)}입니다.
          </p>
        {:else}
          <p class="warning" role="alert">
            {removalStoppedAction(removal.result.stopped_reason)}
            확인된 제거 {removal.result.removed_count}/{removal.result.planned_candidate_count}개입니다.
          </p>
        {/if}
        <p class="muted">승인 기록을 DiskSage 데이터 폴더에 저장했습니다.</p>
        {#if removal.result_path}
          <p class="muted">결과 기록을 DiskSage 데이터 폴더에 저장했습니다.</p>
        {:else}
          <p class="error" role="alert">{GIT_WORKTREE_RESULT_RECORD_FAILURE}</p>
        {/if}
      {:else if report.evidence_complete && report.exact_approval_phrase}
        <div class="approval">
          <p class="warning">
            아래 승인 문구 전체를 직접 입력해야 합니다. 실행 시 전체 계획과 각 후보를 재검증하며 한 항목이라도 달라지면 중단합니다.
          </p>
          <code>{report.exact_approval_phrase}</code>
          <label>
            정확한 승인 문구 확인
            <textarea
              class="confirmation"
              bind:value={confirmationPhrase}
              autocomplete="off"
              spellcheck="false"
              disabled={executing}
            ></textarea>
          </label>
          <label>
            제거 사유
            <textarea
              bind:value={rationale}
              maxlength="1000"
              placeholder="예: main에 병합되고 활성 사용이 없는 보조 worktree임을 검토"
              disabled={executing}
            ></textarea>
          </label>
          <button onclick={removeWorktrees} disabled={!executionReady()}>
            {executing ? "재검증 후 worktree 제거 중…" : confirming ? "제거 확인 대기 중…" : "재검증하고 worktree만 제거"}
          </button>
        </div>
      {:else if report.removal_candidate_count === 0}
        <p class="muted">현재 엄격한 제거 조건을 모두 만족하는 보조 worktree가 없습니다.</p>
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
  .fingerprint, .oid { margin: 0; overflow-wrap: anywhere; font: 0.75rem ui-monospace, monospace; color: #59636e; }
  .worktrees { list-style: none; margin: 0; padding: 0; max-height: 30vh; overflow-y: auto; }
  .worktrees li { padding: 0.45rem 0; border-bottom: 1px solid #d9e0e6; }
  .path { overflow-wrap: anywhere; color: #66717d; font-size: 0.78rem; }
  .blocked { padding: 0.6rem; border: 1px solid #b74a4a; background: #fff6f6; }
  .blocked > ul { margin-bottom: 0; }
  .evidence-actions { margin: 0.25rem 0 0; }
  .approval { display: grid; gap: 0.55rem; justify-items: start; padding: 0.7rem; border: 1px solid #b78335; border-radius: 4px; background: #fffaf1; }
  .approval code { max-width: min(60rem, 90vw); overflow-wrap: anywhere; user-select: all; }
  .approval textarea { width: min(60rem, 90vw); resize: vertical; }
  .muted { color: #727b84; margin: 0; }
  .warning { color: #8a5700; margin: 0; }
  .safe { color: #276437; margin: 0; }
  .error { color: #b00; margin: 0; }
</style>
