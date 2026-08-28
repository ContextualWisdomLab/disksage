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
      error = "Homebrew 정리 계획을 만들지 못했습니다. 다시 시도하세요.";
    } finally {
      planning = false;
    }
  }

  function approvalGuidance(): string {
    if (!judgment || judgment.verdict !== "safe") return "";
    if (!judgment.calibration || judgment.calibration.judgment_id !== judgment.judgment_id) {
      return "정리 판단을 확인할 수 없습니다. 잠시 후 다시 시도하세요.";
    }
    if (!judgment.calibration.passed) {
      return "정리 안전성 확인이 끝나지 않아 실행할 수 없습니다. 다시 확인하세요.";
    }
    if (confirmationPhrase.trim() !== judgment.exact_approval_phrase) {
      return "승인 문구가 일치하지 않습니다. 표시된 문구를 다시 입력하세요.";
    }
    if (rationale.trim().length === 0) {
      return "정리 사유를 입력하십시오.";
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
      "Homebrew에서 사용하지 않는 파일을 정리합니다.\n\n"
        + "끊어진 연결과 비어 있는 폴더만 대상으로 하며 실행 전에 목록을 다시 확인합니다.",
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
    사용하지 않는 Homebrew 파일을 먼저 확인합니다. 끊어진 연결과 비어 있는 폴더만 대상으로 하며, 목록 확인과 승인 후에 정리합니다.
  </p>
  <button onclick={judgeCleanup} disabled={planning || executing}>
    {planning ? "Homebrew 정리 계획 확인 중…" : "Homebrew 정리 계획 확인"}
  </button>

  {#if error}<p class="error" role="alert">Homebrew 정리를 다시 시도하세요. {error}</p>{/if}

  {#if judgment || completedJudgment}
    {@const report = (judgment ?? completedJudgment)!}
    <div class="report" aria-live="polite">
      <div><strong>정리 안전성: {report.verdict === "safe" ? "확인됨" : "추가 확인 필요"}</strong></div>
      <p class="muted">{report.reason || "정리 사유를 확인할 수 없습니다. 목록을 직접 확인하세요."}</p>
      {#if report.calibration}
        <p class:success={report.calibration.passed} class:warning={!report.calibration.passed}>
          정리 안전성 확인: {report.calibration.passed ? "통과" : "추가 확인 필요"}
        </p>
      {:else}
        <p class="muted">자동 확인이 끝나지 않았습니다. 목록을 직접 확인하고 승인하세요.</p>
      {/if}
      <p class="muted">정리 범위: 끊어진 연결과 비어 있는 Homebrew 폴더</p>

      {#if judgment && judgment.verdict === "safe" && !execution}
        <div class="approval">
          <p class="warning">아래 승인 문구를 입력하면 실행 직전에 정리 목록을 다시 확인합니다.</p>
          <code>{judgment.exact_approval_phrase}</code>
          <label>
            정확한 승인 문구 확인
            <textarea bind:value={confirmationPhrase} autocomplete="off" spellcheck="false" disabled={executing}></textarea>
          </label>
          <label>
            실행 사유
            <textarea bind:value={rationale} maxlength="1000" placeholder="예: 목록을 확인했고 사용하지 않는 파일 정리가 필요함" disabled={executing}></textarea>
          </label>
          {#if approvalGuidance()}
            <p class="warning" role="status">안내를 확인하세요. {approvalGuidance()}</p>
          {/if}
          <button onclick={executeCleanup} disabled={!executionReady()}>
            {executing ? "목록 확인 후 Homebrew 정리 중…" : "확인하고 Homebrew 정리"}
          </button>
        </div>
      {:else if judgment && judgment.verdict !== "safe"}
        <p class="warning">추가 확인이 필요해 정리를 실행하지 않았습니다. 목록을 확인하세요.</p>
      {/if}

      {#if execution}
        <p class:success={execution.status_code === 0} class:error={execution.status_code !== 0}>
          정리 결과를 확인하세요. {execution.executed ? "정리를 완료했습니다." : "정리를 실행하지 않았습니다."}
        </p>
        {#if execution.record_path}
          <p class="muted">정리 결과를 저장했습니다.</p>
        {:else}
          <p class="error" role="alert">정리 결과를 저장하지 못했습니다. 권한과 여유 공간을 확인하세요.</p>
        {/if}
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
  .muted { color: #727b84; margin: 0; }
  .warning { color: #8a5700; margin: 0; }
  .success { color: #276437; margin: 0; }
  .error { color: #b00; margin: 0; }
</style>
