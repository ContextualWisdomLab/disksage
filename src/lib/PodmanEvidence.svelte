<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fmtBytes } from "./fmt";
  import {
    hasActionableReasonCodes,
    loadPodmanEvidence,
    podmanEvidenceView,
    type OptionalBytes,
    type PodmanDesktopEvidence,
  } from "./podmanEvidence";
  import { podmanEvidenceErrorMessage } from "./podmanEvidenceError";

  let evidence: PodmanDesktopEvidence | null = $state(null);
  let busy = $state(false);
  let error = $state("");
  let view = $derived(evidence ? podmanEvidenceView(evidence) : null);

  function optionalBytes(value: OptionalBytes): string {
    return value === null ? "관측되지 않음" : fmtBytes(value);
  }

  function optionalCount(value: number | null): string {
    return value === null ? "관측되지 않음" : `${value}개`;
  }

  /** Invoke only the privacy-safe read-only Podman projection registered by the desktop bridge. */
  async function invokeDesktopEvidence<T>(_command: string): Promise<T> {
    return invoke<T>("inspect_podman_desktop_evidence");
  }

  async function load() {
    busy = true;
    error = "";
    try {
      evidence = await loadPodmanEvidence(invokeDesktopEvidence);
    } catch (reason) {
      evidence = null;
      error = podmanEvidenceErrorMessage(reason);
    } finally {
      busy = false;
    }
  }
</script>

<section class="podman-evidence" aria-labelledby="podman-evidence-title">
  <div class="heading-row">
    <div>
      <h3 id="podman-evidence-title">Podman 저장 공간 확인</h3>
      <p class="description">
        상태만 확인합니다. 이 화면에서는 이미지·작업·저장 공간을 삭제하거나 Podman 환경을 변경하지 않습니다.
      </p>
    </div>
    <button type="button" onclick={load} disabled={busy}>
      {busy ? "상태 확인 중…" : evidence ? "다시 확인" : "상태 확인"}
    </button>
  </div>

  {#if busy}
    <p role="status" aria-live="polite">Podman 저장 공간 상태를 확인하고 있습니다.</p>
  {/if}

  {#if error}
    <p class="error" role="alert">Podman 저장 공간을 확인하지 못했습니다. 상태를 확인한 뒤 다시 시도하십시오.</p>
  {/if}

  {#if evidence && view}
    <div class="status-row" role="status" aria-live="polite">
      <span class:complete={view.completeness_tone === "complete"} class:partial={view.completeness_tone === "partial"}>
        {view.completeness_label}
      </span>
      <span>실제로 확보할 수 있는 공간: {view.physical_reclaim_label}</span>
      <span>확인 소요 시간: {evidence.elapsed_ms}ms</span>
    </div>

    <p class="boundary">
      표시된 정리 후보가 실제로 확보되는 공간을 보장하지 않습니다. 정리 후 저장 공간을 다시 확인해야 실제 증가량을 알 수 있습니다.
    </p>

    <h4>저장 공간별 확인 결과</h4>
    <dl class="metrics">
      <div><dt>Podman 디스크 크기</dt><dd>{optionalBytes(evidence.capacity.configured_disk_bytes)}</dd></div>
      <div><dt>가상 디스크 논리 크기</dt><dd>{optionalBytes(evidence.capacity.raw_logical_bytes)}</dd></div>
      <div><dt>호스트에서 사용 중인 공간</dt><dd>{optionalBytes(evidence.capacity.host_allocated_bytes)}</dd></div>
      <div><dt>환경 전체 공간</dt><dd>{optionalBytes(evidence.capacity.guest_total_bytes)}</dd></div>
      <div><dt>환경에서 사용 중인 공간</dt><dd>{optionalBytes(evidence.capacity.guest_used_bytes)}</dd></div>
      <div><dt>환경의 여유 공간</dt><dd>{optionalBytes(evidence.capacity.guest_available_bytes)}</dd></div>
      <div><dt>Podman 데이터 할당 공간</dt><dd>{optionalBytes(evidence.capacity.graph_root_allocated_bytes)}</dd></div>
      <div><dt>Podman 데이터 사용 공간</dt><dd>{optionalBytes(evidence.capacity.graph_root_used_bytes)}</dd></div>
      <div><dt>가상 디스크와 환경 차이</dt><dd>{optionalBytes(evidence.raw_allocated_minus_guest_used_bytes)}</dd></div>
      <div><dt>확인된 정리 후보 합계</dt><dd>{optionalBytes(evidence.podman_reported_reclaimable_bytes)}</dd></div>
    </dl>

    <h4>항목별 확인</h4>
    <div class="review-grid">
      <article>
        <h5>이미지</h5><p>{view.image_review_label}</p>
        <dl><div><dt>확인된 정리 후보</dt><dd>{optionalBytes(evidence.candidates.image_candidate_bytes)}</dd></div><div><dt>사용되지 않는 항목</dt><dd>{optionalCount(evidence.candidates.unused_image_records)}</dd></div></dl>
      </article>
      <article>
        <h5>중지된 작업</h5><p>{view.container_review_label}</p>
        <dl><div><dt>확인된 정리 후보</dt><dd>{optionalBytes(evidence.candidates.stopped_container_candidate_bytes)}</dd></div><div><dt>중지된 항목</dt><dd>{optionalCount(evidence.candidates.stopped_container_records)}</dd></div></dl>
      </article>
      <article>
        <h5>연결된 저장 공간</h5><p>{view.volume_review_label}</p>
        <dl><div><dt>확인된 정리 후보</dt><dd>{optionalBytes(evidence.candidates.volume_candidate_bytes)}</dd></div></dl>
      </article>
    </div>

    {#if hasActionableReasonCodes(evidence)}
      <p class="notice">추가 확인이 필요한 항목이 있습니다. 상태를 다시 확인한 뒤 정리 여부를 판단하십시오.</p>
    {/if}
    {#if view.has_issues}
      <p class="error" role="alert">확인 결과가 일부 불완전합니다. 상태를 다시 확인한 뒤 정리 여부를 판단하십시오.</p>
    {/if}
  {/if}
</section>

<style>
  .podman-evidence { margin-top: 1.5rem; border-top: 1px solid var(--ds-border, #ddd); padding-top: 1rem; }
  .heading-row { display: flex; justify-content: space-between; gap: 1rem; align-items: flex-start; }
  .heading-row h3 { margin: 0; }
  .heading-row button {
    min-height: var(--ds-control-min-size, 2.75rem);
    padding: 0.5rem 0.75rem;
  }
  .heading-row button:focus-visible {
    outline: 2px solid var(--ds-action, #1769aa);
    outline-offset: 2px;
  }
  .description { margin: 0.35rem 0 0; color: var(--ds-text-muted, #666); }
  .status-row { display: flex; flex-wrap: wrap; gap: 0.75rem; margin: 1rem 0; }
  .status-row span { border: 1px solid var(--ds-border, #ddd); border-radius: 999px; padding: 0.25rem 0.65rem; }
  .status-row .complete { border-color: var(--ds-success-text, #2a8f4a); }
  .status-row .partial { border-color: var(--ds-warning-text, #8a6508); }
  .boundary { border-left: 4px solid var(--ds-warning-text, #8a6508); padding: 0.65rem 0.8rem; background: var(--ds-warning-surface, #fff8e1); }
  .metrics { display: grid; grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr)); gap: 0.6rem; }
  .metrics > div, article dl > div { border-bottom: 1px solid var(--ds-border, #ddd); padding-bottom: 0.35rem; }
  dt { color: var(--ds-text-muted, #666); font-size: 0.85rem; }
  dd { margin: 0.15rem 0 0; font-variant-numeric: tabular-nums; }
  .review-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr)); gap: 0.75rem; }
  article { border: 1px solid var(--ds-border, #ddd); border-radius: 8px; padding: 0.75rem; }
  article h5 { margin: 0; }
  article p { min-height: 2.5rem; }
  article dl { margin-bottom: 0; }
  .error { color: var(--ds-danger-text, #b00); }
  @media (max-width: 600px) { .heading-row { flex-direction: column; } .heading-row button { width: 100%; } }
</style>
