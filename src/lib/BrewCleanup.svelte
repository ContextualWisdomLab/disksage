<script lang="ts">
  import * as api from "./api";
  import { confirm } from "@tauri-apps/plugin-dialog";

  let planning = $state(false);
  let executing = $state(false);
  let error = $state("");
  let judgment: api.BrewCleanupJudgment | null = $state(null);
  let completedJudgment: api.BrewCleanupJudgment | null = $state(null);
  let confirmationPhrase = $state("");
  let rationale = $state("");
  let execution: api.BrewCleanupExecution | null = $state(null);

  function reset() {
    judgment = null;
    completedJudgment = null;
    execution = null;
    confirmationPhrase = "";
    rationale = "";
    error = "";
  }

  async function judgeCleanup() {
    planning = true;
    reset();
    try {
      judgment = await api.judgeBrewCleanup();
    } catch {
      error = "Homebrew 정리 범위를 확인하지 못했습니다. 다시 시도하십시오.";
    } finally {
      planning = false;
    }
  }

  function approvalGuidance(): string {
    if (!judgment || judgment.verdict !== "safe") return "";
    if (!judgment.calibration || judgment.calibration.judgment_id !== judgment.judgment_id) {
      return "추가 안전 확인이 끝나지 않아 실행할 수 없습니다. 정리 계획을 다시 확인하십시오.";
    }
    if (!judgment.calibration.passed) {
      return "안전 확인을 통과하지 않아 실행할 수 없습니다. 정리 계획을 다시 확인하십시오.";
    }
    if (confirmationPhrase.trim() !== judgment.exact_approval_phrase) {
      return "승인 문구가 일치하지 않습니다.";
    }
    if (rationale.trim().length === 0) {
      return "실행 사유를 입력하십시오.";
    }
    return "";
  }

  function executionReady(): boolean {
    return judgment !== null
      && judgment.verdict === "safe"
      && judgment.calibration !== undefined
      && judgment.calibration.judgment_id === judgment.judgment_id
      && judgment.calibration.passed
      && confirmationPhrase.trim() === judgment.exact_approval_phrase
      && rationale.trim().length > 0
      && !executing
      && execution === null;
  }

  async function executeCleanup() {
    if (!judgment || !executionReady()) return;
    const okay = await confirm(
      "Homebrew의 끊어진 심볼릭 링크와 빈 디렉터리만 정리합니다.\n\n"
        + "실행 전에 정리 범위를 다시 확인합니다.",
      { title: "DiskSage Homebrew 정리", kind: "warning" },
    );
    if (!okay) return;
    executing = true;
    error = "";
    const submittedJudgment = judgment;
    completedJudgment = submittedJudgment;
    try {
      execution = await api.executeBrewCleanup(
        submittedJudgment.plan_fingerprint,
        submittedJudgment.judgment_id,
        confirmationPhrase.trim(),
        rationale.trim(),
      );
    } catch {
      error = "Homebrew 정리를 실행하지 못했습니다. 상태를 확인한 뒤 다시 시도하십시오.";
    } finally {
      judgment = null;
      confirmationPhrase = "";
      rationale = "";
      executing = false;
    }
  }
</script>

<div class="brew-panel">
  <strong>Homebrew 정리 (macOS)</strong>
  <p class="muted">
    Homebrew 안의 끊어진 심볼릭 링크와 빈 디렉터리만 확인합니다. 정리 범위를 확인하고 승인 문구와 사유를 입력해야 실행됩니다.
  </p>
  <button onclick={judgeCleanup} disabled={planning || executing}>
    {planning ? "Homebrew 정리 범위 확인 중…" : "Homebrew 정리 범위 확인"}
  </button>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if judgment || completedJudgment}
    {@const report = (judgment ?? completedJudgment)!}
    <div class="report" aria-live="polite">
      <div><strong>{report.verdict === "safe" ? "정리 가능" : "정리 보류"}</strong></div>
      <p class="muted">
        {report.verdict === "safe"
          ? "확인된 Homebrew 정리 범위를 검토한 뒤 승인하십시오."
          : "안전 조건을 충족하지 않아 정리할 수 없습니다. Homebrew 상태를 확인한 뒤 다시 시도하십시오."}
      </p>
      <p class="muted">정리 범위: Homebrew의 끊어진 심볼릭 링크와 빈 디렉터리</p>
      {#if report.calibration}
        <p class:success={report.calibration.passed} class:warning={!report.calibration.passed}>
          추가 안전 확인: {report.calibration.passed ? "완료" : "미완료"}
        </p>
      {:else}
        <p class="muted">추가 안전 확인이 없어 사람의 승인 문구가 계속 필요합니다.</p>
      {/if}
      <p class="muted">정리 대상은 Homebrew 상태를 다시 확인한 뒤에만 처리됩니다.</p>

      {#if judgment && judgment.verdict === "safe" && !execution}
        <div class="approval">
          <p class="warning">아래 승인 문구 전체와 실행 사유를 입력해야 합니다. 실행 직전에 정리 범위를 다시 확인합니다.</p>
          <code>{judgment.exact_approval_phrase}</code>
          <label>
            정확한 승인 문구 확인
            <textarea bind:value={confirmationPhrase} autocomplete="off" spellcheck="false" disabled={executing}></textarea>
          </label>
          <label>
            실행 사유
            <textarea bind:value={rationale} maxlength="1000" placeholder="예: 끊어진 심볼릭 링크와 빈 디렉터리 정리가 필요함" disabled={executing}></textarea>
          </label>
          {#if approvalGuidance()}
            <p class="warning" role="status">{approvalGuidance()}</p>
          {/if}
          <button onclick={executeCleanup} disabled={!executionReady()}>
            {executing ? "재검증 후 Homebrew 정리 중…" : "승인하고 Homebrew 정리"}
          </button>
        </div>
      {:else if judgment && judgment.verdict !== "safe"}
        <p class="warning">안전 조건을 충족하지 않아 실행할 수 없습니다. 계획을 다시 확인하십시오.</p>
      {/if}

      {#if execution}
        <p class:success={execution.executed && execution.status_code === 0} class:error={!execution.executed || execution.status_code !== 0}>
          {execution.executed && execution.status_code === 0
            ? "Homebrew 정리를 완료했습니다."
            : "Homebrew 정리를 완료하지 못했습니다. 상태를 확인한 뒤 다시 시도하십시오."}
        </p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .brew-panel { margin-top: 1rem; padding-top: 1rem; border-top: 1px solid #b7c6d8; display: grid; gap: 0.55rem; }
  .report { display: grid; gap: 0.55rem; padding: 0.75rem; border: 1px solid #72889c; border-radius: 4px; background: #f7fafc; }
  .approval { display: grid; gap: 0.55rem; justify-items: start; padding: 0.7rem; border: 1px solid #b78335; border-radius: 4px; background: #fffaf1; }
  .approval code { max-width: min(60rem, 90vw); overflow-wrap: anywhere; user-select: all; }
  label { display: grid; gap: 0.2rem; font-size: 0.82rem; color: #4d5660; }
  textarea { width: min(60rem, 90vw); min-height: 3.5rem; resize: vertical; }
  .muted { color: var(--ds-text-muted); margin: 0; }
  .warning { color: #8a5700; margin: 0; }
  .success { color: #276437; margin: 0; }
  .error { color: #b00; margin: 0; }
</style>
