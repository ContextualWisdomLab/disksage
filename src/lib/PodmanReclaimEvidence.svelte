<script lang="ts">
  import { fmtBytes } from "./fmt";
  import {
    podmanReclaimPlan,
    type PodmanReclaimPlan,
  } from "./podmanApi";
  import {
    podmanActionLabel,
    podmanCandidateCategories,
    podmanCandidateFingerprint,
    podmanEvidenceMetrics,
    podmanIssueCodes,
  } from "./podmanEvidence";

  let {
    initialPlan = null,
    loadPlan = podmanReclaimPlan,
  }: {
    initialPlan?: PodmanReclaimPlan | null;
    loadPlan?: () => Promise<PodmanReclaimPlan>;
  } = $props();

  let plan = $state<PodmanReclaimPlan | null>(initialPlan);
  let busy = $state(false);
  let errorCode = $state("");

  let metrics = $derived(plan ? podmanEvidenceMetrics(plan) : []);
  let candidateCategories = $derived(plan ? podmanCandidateCategories(plan) : []);
  let issueCodes = $derived(plan ? podmanIssueCodes(plan) : []);
  let candidateFingerprint = $derived(plan ? podmanCandidateFingerprint(plan) : null);

  async function loadEvidence() {
    busy = true;
    errorCode = "";
    try {
      plan = await loadPlan();
    } catch {
      plan = null;
      errorCode = "podman-evidence-unavailable";
    } finally {
      busy = false;
    }
  }

  function metricValue(key: string, bytes: number | null): string {
    if (key === "physically_reclaimable_bytes" && bytes === null) return "미검증";
    return bytes === null ? "관측 불가" : fmtBytes(bytes);
  }

  function evidenceClassLabel(
    evidenceClass: "configured" | "observed" | "logical_candidate" | "physical_proof",
  ): string {
    switch (evidenceClass) {
      case "configured": return "설정값";
      case "observed": return "관측값";
      case "logical_candidate": return "논리 후보";
      case "physical_proof": return "물리 증명";
    }
  }
</script>

<section class="podman-evidence" aria-labelledby="podman-evidence-title">
  <div class="heading-row">
    <div>
      <h3 id="podman-evidence-title">Podman VM 저장공간 증거</h3>
      <p class="boundary">
        읽기 전용 진단입니다. 이 화면은 prune, 삭제, 머신 중지·시작, VM 제거, TRIM 또는
        raw 이미지 변경을 실행하지 않습니다.
      </p>
    </div>
    <button type="button" onclick={loadEvidence} disabled={busy}>
      {busy ? "Podman 증거 확인 중…" : "Podman 증거 확인"}
    </button>
  </div>

  {#if errorCode}
    <p class="error" role="alert">증거를 불러오지 못했습니다. 오류 코드: <code>{errorCode}</code></p>
  {/if}

  {#if plan}
    <div class:incomplete={!plan.evidence_complete} class="status" role="status">
      <strong>{plan.evidence_complete ? "증거 완전" : "증거 부분 수집"}</strong>
      <span>스키마 v{plan.schema_version}</span>
      <span>플랫폼 {plan.platform}</span>
      <span>수집 {Math.round(plan.elapsed_ms)}ms</span>
      {#if plan.machine}<span>머신 상태 {plan.machine.state}</span>{/if}
    </div>

    <p class="warning">
      Podman 보고 후보와 raw 이미지 할당 차이는 호스트 물리 회수 보장이 아닙니다. 호스트
      물리 회수량은 전후 자유공간 관측이 증명하기 전까지 항상 “미검증”으로 표시됩니다.
    </p>

    <dl class="metrics">
      {#each metrics as metric (metric.key)}
        <div class="metric">
          <dt>{metric.label}</dt>
          <dd>
            <strong>{metricValue(metric.key, metric.bytes)}</strong>
            <span>{evidenceClassLabel(metric.evidence_class)}</span>
          </dd>
        </div>
      {/each}
    </dl>

    <div class="counts" aria-label="Podman store counts">
      <span>이미지 {plan.store?.images ?? "관측 불가"}</span>
      <span>컨테이너 전체 {plan.store?.containers_total ?? "관측 불가"}</span>
      <span>실행 {plan.store?.containers_running ?? "관측 불가"}</span>
      <span>중지 {plan.store?.containers_stopped ?? "관측 불가"}</span>
      <span>정확한 미사용 이미지 레코드 {plan.unused_images?.unused_records ?? "관측 불가"}</span>
    </div>

    <div class="candidate-grid" aria-label="서로 독립적인 Podman 검토 후보">
      {#each candidateCategories as category (category.kind)}
        <article class="candidate-card">
          <h4>{category.label}</h4>
          <p>{category.bytes === null ? "관측 불가" : fmtBytes(category.bytes)}</p>
          <p class="approval">별도 사람 승인 필요</p>
        </article>
      {/each}
    </div>

    {#if candidateFingerprint}
      <p class="fingerprint">
        정확한 미사용 이미지 후보 집합 SHA-256
        <code>{candidateFingerprint}</code>
      </p>
    {/if}

    {#if plan.assessment.recommended_actions.length > 0}
      <h4>읽기 전용 권고</h4>
      <ul class="recommendations">
        {#each plan.assessment.recommended_actions as action (action.kind)}
          <li>
            <strong>{podmanActionLabel(action.kind)}</strong>
            <span>{action.requires_human_approval ? "향후 별도 승인 필요" : "읽기 전용 조사"}</span>
            <p>{action.rationale}</p>
          </li>
        {/each}
      </ul>
    {/if}

    {#if issueCodes.length > 0}
      <h4>증거 이슈 코드</h4>
      <ul class="issues">
        {#each issueCodes as issue (issue)}
          <li><code>{issue}</code></li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .podman-evidence {
    margin: 1rem 0;
    padding: 0.9rem;
    border: 1px solid #cfd8e3;
    border-radius: 8px;
    background: #f8fafc;
  }
  .heading-row { display: flex; justify-content: space-between; gap: 1rem; align-items: start; }
  h3, h4 { margin: 0 0 0.35rem; }
  .boundary, .warning { margin: 0.2rem 0 0.75rem; color: #525f6d; font-size: 0.9rem; }
  .warning { padding: 0.55rem; border-left: 4px solid #b88700; background: #fff8df; }
  .status { display: flex; flex-wrap: wrap; gap: 0.75rem; margin: 0.7rem 0; }
  .status.incomplete { color: #8a4b00; }
  .metrics { display: grid; grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr)); gap: 0.55rem; }
  .metric { padding: 0.55rem; border: 1px solid #dbe2ea; border-radius: 6px; background: #fff; }
  .metric dt { color: #59636e; font-size: 0.8rem; }
  .metric dd { margin: 0.25rem 0 0; display: flex; justify-content: space-between; gap: 0.5rem; }
  .metric dd span { color: #6c7682; font-size: 0.75rem; }
  .counts { display: flex; flex-wrap: wrap; gap: 0.75rem; margin: 0.8rem 0; font-size: 0.85rem; }
  .candidate-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr)); gap: 0.55rem; }
  .candidate-card { padding: 0.6rem; border: 1px solid #dbe2ea; border-radius: 6px; background: #fff; }
  .candidate-card p { margin: 0.2rem 0; }
  .approval { color: #8a4b00; font-size: 0.8rem; }
  .fingerprint { overflow-wrap: anywhere; font-size: 0.82rem; }
  .fingerprint code { display: block; margin-top: 0.2rem; }
  .recommendations, .issues { padding-left: 1.25rem; }
  .recommendations li { margin: 0.45rem 0; }
  .recommendations span { margin-left: 0.45rem; color: #8a4b00; font-size: 0.8rem; }
  .recommendations p { margin: 0.15rem 0 0; color: #59636e; }
  .error { color: #a40000; }
  @media (max-width: 680px) {
    .heading-row { display: grid; }
    .heading-row button { justify-self: start; }
  }
</style>
