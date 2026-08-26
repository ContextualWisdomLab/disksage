<script lang="ts">
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { cloudRoot }: { cloudRoot: string } = $props();

  let path = $state("");
  let planning = $state(false);
  let executing = $state(false);
  let error = $state("");
  let plan: api.IcloudLocalEvictionPlan | null = $state(null);
  let confirmation = $state("");
  let rationale = $state("");
  let eviction: api.IcloudLocalCopyEvictionOutput | null = $state(null);

  function resetDecision() {
    plan = null;
    confirmation = "";
    rationale = "";
    eviction = null;
    error = "";
  }

  async function chooseFile() {
    error = "";
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        defaultPath: cloudRoot,
        title: "로컬 사본 상태를 확인할 iCloud 파일 선택",
      });
      if (typeof selected !== "string") return;
      path = selected;
      resetDecision();
    } catch {
      error = "iCloud 파일 선택을 완료하지 못했습니다. 다시 시도하십시오.";
    }
  }

  async function inspectLocalCopy() {
    const selectedPath = path.trim();
    if (!selectedPath) return;
    planning = true;
    resetDecision();
    try {
      plan = await api.planIcloudLocalCopyEviction(cloudRoot, selectedPath);
    } catch {
      error = "iCloud 로컬 사본 상태를 확인하지 못했습니다. 다시 시도하십시오.";
    } finally {
      planning = false;
    }
  }

  function executionReady(): boolean {
    return plan !== null
      && plan.eligible_after_human_approval
      && confirmation === plan.plan_fingerprint
      && rationale.trim().length > 0
      && !executing
      && eviction === null;
  }

  async function evictLocalCopy() {
    if (!plan || !executionReady()) return;
    const approved = await confirm(
      `${fmtBytes(plan.allocated_bytes)}의 로컬 iCloud 사본만 축출합니다.\n` +
        "클라우드 항목은 유지되며 실행 직전에 상태를 다시 검증합니다.",
      { title: "DiskSage iCloud 로컬 사본 축출", kind: "warning" },
    );
    if (!approved) return;
    executing = true;
    error = "";
    try {
      eviction = await api.evictIcloudLocalCopy(
        cloudRoot,
        plan.path,
        plan.plan_fingerprint,
        confirmation,
        rationale.trim(),
      );
      confirmation = "";
      rationale = "";
    } catch {
      error = "iCloud 로컬 사본을 회수하지 못했습니다. 상태를 다시 확인하십시오.";
    } finally {
      executing = false;
    }
  }

  function observationLabel(method: api.IcloudStateObservationMethod): string {
    return method === "file-provider-ctl-evaluate"
      ? "macOS 상태 확인"
      : "iCloud 상태 확인";
  }

  function uploadLabel(state: api.IcloudLocalState): string {
    if (state.is_uploaded && !state.is_uploading) return "완료";
    if (state.is_uploading) return "업로드 중";
    return "미완료";
  }

  function syncLabel(state: api.IcloudLocalState): string {
    if (state.downloading_status_current && !state.is_uploaded && !state.is_uploading) {
      return "로컬 최신본·업로드 미확인";
    }
    if (state.is_uploaded && !state.is_uploading) return "iCloud 동기화 완료";
    if (state.is_uploading) return "iCloud 업로드 중";
    return "iCloud 동기화 미완료";
  }

  function blockerLabel(blocker: string): string {
    const labels: Record<string, string> = {
      "icloud-upload-not-confirmed": "로컬 최신본이지만 iCloud 업로드가 아직 확인되지 않았습니다. 업로드 완료 후 다시 확인하십시오.",
      "icloud-upload-still-running": "iCloud 업로드가 진행 중입니다. 완료 후 다시 확인하십시오.",
      "icloud-current-version-unconfirmed": "로컬 최신본 여부를 확인하지 못했습니다. iCloud 상태가 안정된 후 다시 확인하십시오.",
      "icloud-file-provider-native-status-unavailable": "iCloud 상태를 확인하지 못했습니다. 잠시 후 다시 확인하십시오.",
      "icloud-file-provider-sync-paused-or-unconfirmed": "iCloud 동기화가 일시중지됐거나 상태를 확인하지 못했습니다. 동기화를 재개한 후 다시 확인하십시오.",
      "icloud-unresolved-conflict": "동기화 충돌이 해결되지 않았습니다. 충돌을 해결한 후 다시 확인하십시오.",
      "active-file-use-detected": "현재 사용 중인 파일이라 회수할 수 없습니다. 파일을 닫은 후 다시 확인하십시오.",
      "active-use-evidence-incomplete": "파일이 사용 중인지 확인하지 못했습니다. 잠시 후 다시 확인하십시오.",
    };
    return labels[blocker] ?? "필수 iCloud 상태를 확인하지 못해 회수할 수 없습니다. 상태를 다시 확인하십시오.";
  }

  function blockerSummary(blockers: string[]): string {
    return [...new Set(blockers.map(blockerLabel))].join(" ");
  }
</script>

<div class="local-eviction-panel">
  <strong>iCloud 로컬 사본 회수</strong>
  <p class="muted">
    이미 iCloud에 있는 파일의 로컬 캐시만 검사합니다. 파일 내용과 클라우드 객체는 변경하지 않습니다.
  </p>
  <div class="path-controls">
    <label>
      iCloud 파일 절대 경로
      <input
        class="local-path"
        type="text"
        bind:value={path}
        oninput={resetDecision}
        autocomplete="off"
        spellcheck="false"
        disabled={planning || executing}
      />
    </label>
    <button onclick={chooseFile} disabled={planning || executing}>파일 선택</button>
    <button onclick={inspectLocalCopy} disabled={planning || executing || !path.trim()}>
      {planning ? "iCloud 상태 확인 중…" : "로컬 사본 확인"}
    </button>
  </div>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if plan}
    <div class="plan" aria-live="polite">
      <div>
        <strong>{fmtBytes(plan.allocated_bytes)} 로컬 할당</strong>
        · 논리 크기 {fmtBytes(plan.logical_bytes)}
        · {observationLabel(plan.icloud_state.observation_method)}
      </div>
      <div class="status-grid">
        <span>업로드 {uploadLabel(plan.icloud_state)}</span>
        <span>클라우드 상태 {syncLabel(plan.icloud_state)}</span>
        <span>로컬 최신 상태 {plan.icloud_state.downloading_status_current ? "예" : "아니오"}</span>
        <span>충돌 {plan.icloud_state.has_unresolved_conflicts ? "있음" : "없음"}</span>
        <span>사용 중 {plan.active_use.active ? "감지" : "없음"}</span>
        <span>동기화 일시정지 {plan.icloud_state.is_sync_paused === false ? "아님" : "미확인/해당"}</span>
        <span>동기화 제외 {plan.icloud_state.is_excluded_from_sync ? "해당" : "아님"}</span>
        <span>휴지통 {plan.icloud_state.is_trashed === false ? "아님" : "미확인/해당"}</span>
        <span>로컬 사본 회수 {plan.icloud_state.allows_eviction === true ? "가능" : "미확인/불가"}</span>
      </div>

      {#if eviction}
        {#if eviction.result.verification_complete}
          <p class="safe">
            로컬 할당 {fmtBytes(eviction.result.observed_allocation_reduction_bytes)} 감소를 확인했습니다.
            iCloud 파일과 클라우드 항목은 유지되었습니다.
          </p>
        {:else}
          <p class="warning">
            축출 요청은 처리됐지만 로컬 저장 공간 감소를 확인하지 못했습니다. 저장 공간을 직접 확인하십시오.
          </p>
        {/if}
        {#if eviction.result_path}
          <p class="muted">로컬 사본 회수 결과를 확인했습니다.</p>
        {:else}
          <p class="error" role="alert">
            축출 결과는 위와 같지만 결과를 저장하지 못했습니다. 상태를 다시 확인하십시오.
          </p>
        {/if}
      {:else if plan.eligible_after_human_approval}
        <div class="approval-controls">
          <p class="warning">
            아래 확인 문구 전체를 직접 입력해야 합니다. 실행 시 파일 크기·업로드·충돌·회수 가능 여부를 다시 확인하며 달라지면 중단합니다.
          </p>
          <label>
            전체 확인 문구
            <input
              class="fingerprint-input"
              type="text"
              bind:value={confirmation}
              autocomplete="off"
              spellcheck="false"
              maxlength="64"
              disabled={executing}
            />
          </label>
          <label>
            로컬 사본 축출 사유
            <textarea
              bind:value={rationale}
              maxlength="1000"
              disabled={executing}
              placeholder="예: 업로드 완료를 확인한 이 파일의 로컬 사본만 회수"
            ></textarea>
          </label>
          <button onclick={evictLocalCopy} disabled={!executionReady()}>
            {executing ? "상태 재검증 후 축출 중…" : "재검증하고 로컬 사본만 축출"}
          </button>
        </div>
      {:else}
        <p class="warning" role="status">현재 축출 불가: {blockerSummary(plan.blockers)}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .local-eviction-panel { width: 100%; margin-top: 0.4rem; padding-top: 0.7rem; border-top: 1px solid #b7c6d8; display: grid; gap: 0.5rem; }
  .path-controls { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: end; width: 100%; }
  .path-controls label { flex: 1 1 32rem; }
  .local-path, .fingerprint-input { width: min(56rem, 88vw); font-family: ui-monospace, monospace; }
  .plan { padding: 0.7rem; border: 1px solid #6b8e72; border-radius: 4px; background: #f5fbf6; display: grid; gap: 0.5rem; }
  .status-grid { display: flex; flex-wrap: wrap; gap: 0.65rem; font-size: 0.78rem; color: #3f5368; }
  .approval-controls { padding: 0.7rem; border: 1px solid #b78335; border-radius: 4px; background: #fffaf1; display: grid; gap: 0.55rem; justify-items: start; }
  .approval-controls textarea { width: min(56rem, 88vw); min-height: 3.5rem; resize: vertical; }
  label { display: grid; gap: 0.2rem; font-size: 0.8rem; color: #555; }
  .muted { color: #777; margin: 0; }
  .warning { color: #8a5700; margin: 0; }
  .safe { color: #276437; margin: 0; }
  .error { color: #b00; margin: 0; }
</style>
