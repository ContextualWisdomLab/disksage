<script lang="ts">
  import { fmtBytes } from "./fmt";
  import {
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

  async function load() {
    busy = true;
    error = "";
    try {
      evidence = await loadPodmanEvidence();
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
      <h3 id="podman-evidence-title">Podman 저장소 증거</h3>
      <p class="description">
        읽기 전용 진단입니다. 이미지, 컨테이너, 볼륨을 삭제하거나 Podman 머신을 변경하지 않습니다.
      </p>
    </div>
    <button type="button" onclick={load} disabled={busy}>
      {busy ? "증거 수집 중…" : evidence ? "다시 조회" : "증거 조회"}
    </button>
  </div>

  {#if busy}
    <p role="status" aria-live="polite">Podman의 제한된 읽기 전용 증거를 수집하고 있습니다.</p>
  {/if}

  {#if error}
    <p class="error" role="alert">Podman 증거를 확인하지 못했습니다: {error}</p>
  {/if}

  {#if evidence && view}
    <div class="status-row" role="status" aria-live="polite">
      <span class:complete={view.completeness_tone === "complete"} class:partial={view.completeness_tone === "partial"}>
        {view.completeness_label}
      </span>
      <span>호스트 물리 회수 가능량: {view.physical_reclaim_label}</span>
      <span>수집 시간: {evidence.elapsed_ms}ms</span>
    </div>

    <p class="boundary">
      Podman이 보고한 논리 후보는 호스트에서 실제로 회수될 물리 공간의 증명이 아닙니다. 실제 회수량은 별도의 전후 호스트 관측이 있어야 확정됩니다.
    </p>

    <h4>서로 다른 용량 관측</h4>
    <dl class="metrics">
      <div><dt>설정된 머신 디스크</dt><dd>{optionalBytes(evidence.capacity.configured_disk_bytes)}</dd></div>
      <div><dt>Raw 이미지 논리 크기</dt><dd>{optionalBytes(evidence.capacity.raw_logical_bytes)}</dd></div>
      <div><dt>호스트 할당 블록</dt><dd>{optionalBytes(evidence.capacity.host_allocated_bytes)}</dd></div>
      <div><dt>게스트 파일시스템 전체</dt><dd>{optionalBytes(evidence.capacity.guest_total_bytes)}</dd></div>
      <div><dt>게스트 파일시스템 사용</dt><dd>{optionalBytes(evidence.capacity.guest_used_bytes)}</dd></div>
      <div><dt>게스트 파일시스템 여유</dt><dd>{optionalBytes(evidence.capacity.guest_available_bytes)}</dd></div>
      <div><dt>Podman graph root 할당</dt><dd>{optionalBytes(evidence.capacity.graph_root_allocated_bytes)}</dd></div>
      <div><dt>Podman graph root 사용</dt><dd>{optionalBytes(evidence.capacity.graph_root_used_bytes)}</dd></div>
      <div><dt>Raw 할당−게스트 사용 차이</dt><dd>{optionalBytes(evidence.raw_allocated_minus_guest_used_bytes)}</dd></div>
      <div><dt>Podman 논리 후보 합계</dt><dd>{optionalBytes(evidence.podman_reported_reclaimable_bytes)}</dd></div>
    </dl>

    <h4>분리된 검토 영역</h4>
    <div class="review-grid">
      <article>
        <h5>이미지</h5><p>{view.image_review_label}</p>
        <dl><div><dt>논리 후보</dt><dd>{optionalBytes(evidence.candidates.image_candidate_bytes)}</dd></div><div><dt>참조 0 레코드</dt><dd>{optionalCount(evidence.candidates.unused_image_records)}</dd></div></dl>
      </article>
      <article>
        <h5>중지 컨테이너</h5><p>{view.container_review_label}</p>
        <dl><div><dt>논리 후보</dt><dd>{optionalBytes(evidence.candidates.stopped_container_candidate_bytes)}</dd></div><div><dt>중지 레코드</dt><dd>{optionalCount(evidence.candidates.stopped_container_records)}</dd></div></dl>
      </article>
      <article>
        <h5>로컬 볼륨</h5><p>{view.volume_review_label}</p>
        <dl><div><dt>논리 후보</dt><dd>{optionalBytes(evidence.candidates.volume_candidate_bytes)}</dd></div></dl>
      </article>
    </div>

    <h4>후보 집합 증거</h4>
    <p>이미지 후보 집합 SHA-256: {#if evidence.candidates.image_candidate_set_sha256}<code>{evidence.candidates.image_candidate_set_sha256}</code>{:else}<span>관측되지 않음</span>{/if}</p>

    {#if evidence.reason_codes.length > 0}
      <h4>판정 사유 코드</h4><ul class="codes">{#each evidence.reason_codes as reason (reason)}<li><code>{reason}</code></li>{/each}</ul>
    {/if}
    {#if view.has_issues}
      <h4>증거 누락·오류 코드</h4><ul class="codes error-codes">{#each evidence.issue_codes as issue (issue)}<li><code>{issue}</code></li>{/each}</ul>
    {/if}
    <ul class="notices">{#each evidence.notices as notice (notice)}<li>{notice}</li>{/each}</ul>
  {/if}
</section>

<style>
  .podman-evidence { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  .heading-row { display: flex; justify-content: space-between; gap: 1rem; align-items: flex-start; }
  .heading-row h3 { margin: 0; }
  .description { margin: 0.35rem 0 0; color: #555; }
  .status-row { display: flex; flex-wrap: wrap; gap: 0.75rem; margin: 1rem 0; }
  .status-row span { border: 1px solid #bbb; border-radius: 999px; padding: 0.25rem 0.65rem; }
  .status-row .complete { border-color: #2a8f4a; }
  .status-row .partial { border-color: #b8860b; }
  .boundary { border-left: 4px solid #b8860b; padding: 0.65rem 0.8rem; background: #fff8df; }
  .metrics { display: grid; grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr)); gap: 0.6rem; }
  .metrics > div, article dl > div { border-bottom: 1px solid #eee; padding-bottom: 0.35rem; }
  dt { color: #666; font-size: 0.85rem; }
  dd { margin: 0.15rem 0 0; font-variant-numeric: tabular-nums; }
  .review-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr)); gap: 0.75rem; }
  article { border: 1px solid #ddd; border-radius: 8px; padding: 0.75rem; }
  article h5 { margin: 0; }
  article p { min-height: 2.5rem; }
  article dl { margin-bottom: 0; }
  code { overflow-wrap: anywhere; }
  .codes { display: flex; flex-wrap: wrap; gap: 0.35rem; list-style: none; padding: 0; }
  .codes li { border: 1px solid #ccc; border-radius: 4px; padding: 0.2rem 0.4rem; }
  .error, .error-codes { color: #b00; }
  .notices { color: #555; padding-left: 1.25rem; }
  @media (max-width: 600px) { .heading-row { flex-direction: column; } .heading-row button { width: 100%; } }
</style>
