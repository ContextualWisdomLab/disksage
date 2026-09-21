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
      error = "Homebrew 정리 계획을 확인하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.";
    } finally {
      planning = false;
    }
  }

  function approvalGuidance(): string {
    if (!judgment || judgment.verdict !== "safe") return "";
    if (!judgment.calibration || judgment.calibration.judgment_id !== judgment.judgment_id) {
      return "자동 안전성 검토가 완료되지 않아 실행할 수 없습니다.";
    }
    if (!judgment.calibration.passed) {
      return "자동 안전성 검토를 통과하지 않아 실행할 수 없습니다.";
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
      "안전성 검토가 완료된 고정 명령을 실행합니다.\n\n"
        + "brew cleanup --prune-prefix\n\n"
        + "Homebrew prefix 안의 끊어진 심볼릭 링크와 빈 디렉터리만 정리하며, 실행 전 dry-run 계획을 다시 검증합니다.",
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
      error = "Homebrew 정리를 실행하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.";
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
    읽기 전용 미리보기로 Homebrew prefix 안의 끊어진 심볼릭 링크와 빈 디렉터리만 확인합니다. 안전하더라도 승인 문구와 사유를 입력해야 고정 명령이 실행됩니다.
  </p>
  <button onclick={judgeCleanup} disabled={planning || executing}>
    {planning ? "Homebrew 정리 계획 확인 중…" : "Homebrew 정리 계획 확인"}
  </button>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if judgment || completedJudgment}
    {@const report = (judgment ?? completedJudgment)!}
    <div class="report" aria-live="polite">
      <div><strong>안전성 검토: {report.verdict === "safe" ? "실행 가능" : "실행 보류"}</strong></div>
      <p class="muted">{report.reason || "안전성 검토 설명이 제공되지 않았습니다."}</p>
      <p class="fingerprint">계획 지문: {report.plan_fingerprint}</p>
      <p class="fingerprint">실행 예정: brew cleanup --prune-prefix</p>
      {#if report.calibration}
        <p class:success={report.calibration.passed} class:warning={!report.calibration.passed}>
          안전성 검토 일치 여부: {report.calibration.passed ? "통과" : "실패"}
          · 표본 {report.calibration.sample_count}개 · 일치율 {Math.round(report.calibration.exact_agreement * 100)}%
        </p>
      {:else}
        <p class="muted">안전성 검토 증거가 없어 독립적인 사람 승인 문구가 계속 필요합니다.</p>
      {/if}
      <pre>{report.plan.dry_run_output || "미리보기에서 정리 대상이 보고되지 않았습니다."}</pre>

      {#if judgment && judgment.verdict === "safe" && !execution}
        <div class="approval">
          <p class="warning">아래 승인 문구 전체를 직접 입력해야 합니다. 실행 직전에 미리보기와 안전성 검토를 다시 대조합니다.</p>
          <code>{judgment.exact_approval_phrase}</code>
          <label>
            정확한 승인 문구 확인
            <textarea bind:value={confirmationPhrase} autocomplete="off" spellcheck="false" disabled={executing}></textarea>
          </label>
          <label>
            실행 사유
            <textarea bind:value={rationale} maxlength="1000" placeholder="예: 미리보기 결과를 검토했고 끊어진 심볼릭 링크와 빈 디렉터리 정리가 필요함" disabled={executing}></textarea>
          </label>
          {#if approvalGuidance()}
            <p class="warning" role="status">{approvalGuidance()}</p>
          {/if}
          <button onclick={executeCleanup} disabled={!executionReady()}>
            {executing ? "재검증 후 Homebrew 정리 중…" : "승인하고 brew cleanup 실행"}
          </button>
        </div>
      {:else if judgment && judgment.verdict !== "safe"}
        <p class="warning">안전성 검토를 통과하지 않아 실행 권한을 만들지 않았습니다.</p>
      {/if}

      {#if execution}
        <p class:success={execution.status_code === 0} class:error={execution.status_code !== 0}>
          {execution.executed ? `실행 완료 (종료 코드 ${execution.status_code})` : "실행되지 않음"}
        </p>
        {#if execution.record_path}
          <p class="muted">감사 기록을 저장했습니다. 다음 정리 전에 최신 상태를 다시 확인하세요.</p>
        {:else}
          <p class="error" role="alert">명령 결과는 반환됐지만 감사 기록을 저장하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.</p>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .brew-panel { margin-top: 1rem; padding-top: 1rem; border-top: 1px solid #b7c6d8; display: grid; gap: 0.55rem; }
  .report { display: grid; gap: 0.55rem; padding: 0.75rem; border: 1px solid #72889c; border-radius: 4px; background: #f7fafc; }
  .fingerprint { margin: 0; overflow-wrap: anywhere; font: 0.75rem ui-monospace, monospace; color: #59636e; }
  .approval { display: grid; gap: 0.55rem; justify-items: start; padding: 0.7rem; border: 1px solid #b78335; border-radius: 4px; background: #fffaf1; }
  .approval code { max-width: min(60rem, 90vw); overflow-wrap: anywhere; user-select: all; }
  label { display: grid; gap: 0.2rem; font-size: 0.82rem; color: #4d5660; }
  textarea { width: min(60rem, 90vw); min-height: 3.5rem; resize: vertical; }
  pre { max-height: 16rem; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; padding: 0.5rem; background: #eef2f5; font-size: 0.78rem; }
  .muted { color: #727b84; margin: 0; }
  .warning { color: #8a5700; margin: 0; }
  .success { color: #276437; margin: 0; }
  .error { color: #b00; margin: 0; }
</style>
