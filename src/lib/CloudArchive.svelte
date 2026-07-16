<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let roots: api.CloudRoot[] = $state([]);
  let selectedRoot = $state("");
  let minSizeMib = $state(256);
  let minAgeDays = $state(90);
  let busy = $state(false);
  let loadError = $state("");
  let report: api.CloudPlanReport | null = $state(null);
  let copyingFingerprint = $state("");
  let copied: api.CloudCopyOutput | null = $state(null);
  let attesting = $state(false);
  let attestation: api.CloudAttestationOutput | null = $state(null);
  let objectId = $state("");
  let accessToken = $state("");

  onMount(async () => {
    try {
      roots = await api.listCloudRoots();
      selectedRoot = roots[0]?.path ?? "";
    } catch (e) {
      loadError = String(e);
    }
  });

  async function preview() {
    if (!scannedRoot || !selectedRoot) return;
    busy = true;
    loadError = "";
    report = null;
    copied = null;
    attestation = null;
    objectId = "";
    accessToken = "";
    try {
      report = await api.planCloudArchive(
        scannedRoot,
        selectedRoot,
        Math.max(1, Math.floor(minSizeMib)),
        Math.max(0, Math.floor(minAgeDays)),
        200,
      );
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  function copyEligible(candidate: api.CloudCandidate): boolean {
    return candidate.blocked_reason === null
      && !candidate.requires_review
      && candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
  }

  async function copyCandidate(candidate: api.CloudCandidate) {
    if (!scannedRoot || !selectedRoot || !copyEligible(candidate)) return;
    copyingFingerprint = candidate.metadata_fingerprint;
    loadError = "";
    copied = null;
    attestation = null;
    objectId = "";
    accessToken = "";
    try {
      copied = await api.copyCloudCandidate(
        scannedRoot,
        selectedRoot,
        candidate.metadata_fingerprint,
        Math.max(1, Math.floor(minSizeMib)),
        Math.max(0, Math.floor(minAgeDays)),
        200,
      );
    } catch (e) {
      loadError = String(e);
    } finally {
      copyingFingerprint = "";
    }
  }

  async function attestCopy() {
    if (!copied) return;
    const isIcloud = copied.receipt.provider === "icloud";
    if (!isIcloud && (!objectId.trim() || !accessToken.trim())) return;
    attesting = true;
    loadError = "";
    attestation = null;
    try {
      attestation = await api.attestCloudCopy(
        copied.receipt.receipt_id,
        isIcloud ? null : objectId.trim(),
        isIcloud ? null : accessToken,
      );
    } catch (e) {
      loadError = String(e);
    } finally {
      accessToken = "";
      attesting = false;
    }
  }

  function productionDate(ms: number): string {
    return new Date(ms).toLocaleDateString();
  }

  function duration(ms: number): string {
    const totalMinutes = Math.floor(ms / 60_000);
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return hours > 0 ? `${hours}시간 ${minutes}분` : `${minutes}분`;
  }
</script>

<section>
  <h2>클라우드 오프로드 <span class="dry">DRY-RUN</span></h2>
  <p class="muted">
    iCloud Drive·OneDrive·Google Drive의 로컬 루트를 탐지하고, 파일 내부 메타데이터를 우선하여 생산 시점과 원래 상대 경로를 보존하는 이동 계획만 만듭니다.
  </p>

  {#if roots.length === 0}
    <p class="warning">쓰기 가능한 클라우드 루트를 찾지 못했습니다.</p>
  {:else}
    <div class="controls">
      <label>
        대상
        <select bind:value={selectedRoot} disabled={busy}>
          {#each roots as root (root.id)}
            <option value={root.path}>{root.label}</option>
          {/each}
        </select>
      </label>
      <label>
        최소 크기(MiB)
        <input type="number" min="1" step="1" bind:value={minSizeMib} disabled={busy} />
      </label>
      <label>
        마지막 수정 후 최소 일수
        <input type="number" min="0" step="1" bind:value={minAgeDays} disabled={busy} />
      </label>
      <button onclick={preview} disabled={busy || !scannedRoot || !selectedRoot}>
        {busy ? "계획 중…" : "오프로드 후보 미리보기"}
      </button>
    </div>
  {/if}

  {#if !scannedRoot}<p class="muted">먼저 스캔을 완료하세요.</p>{/if}
  {#if loadError}<p class="error">{loadError}</p>{/if}

  {#if report}
    <div class="summary">
      {report.candidates.length}개 후보 · 총 {fmtBytes(report.candidate_bytes)} ·
      충돌 제외 잠재 회수 {fmtBytes(report.potentially_reclaimable_bytes)}
    </div>
    <p class="warning">
      복사는 내부 메타데이터가 고신뢰이고 별도 검토 사유가 없는 후보만 가능합니다. 원본 삭제 기능은 제공하지 않으며, 업로드 증거가 확인되어도 허가 정보만 표시합니다.
    </p>
    {#if copied}
      <div class="receipt">
        <strong>검증 복사 완료 · 원본 보존됨</strong>
        <div class="context">영수증 {copied.receipt.receipt_id} · {fmtBytes(copied.receipt.bytes)}</div>
        <div class="path">{copied.receipt.destination}</div>
        {#if copied.receipt.provider !== "icloud"}
          <div class="provider-auth">
            <label>
              {copied.receipt.provider === "onedrive" ? "OneDrive item ID" : "Google Drive file ID"}
              <input type="text" bind:value={objectId} autocomplete="off" disabled={attesting} />
            </label>
            <label>
              일회성 OAuth access token
              <input type="password" bind:value={accessToken} autocomplete="off" disabled={attesting} />
            </label>
          </div>
          <p class="muted">토큰은 이 원격 메타데이터 확인 요청에만 사용하고 저장하거나 응답에 포함하지 않습니다.</p>
        {/if}
        <button
          onclick={attestCopy}
          disabled={attesting || (copied.receipt.provider !== "icloud" && (!objectId.trim() || !accessToken.trim()))}
        >
          {attesting ? "검증 중…" : "클라우드 업로드 증거 확인"}
        </button>
        {#if attestation}
          {#if attestation.permit}
            <p class="safe">업로드·원격 체크섬 검증 완료. 로컬 제거 허가 증거가 생성되었지만 파일은 그대로 보존됩니다.</p>
          {:else}
            <p class="warning">아직 제거 불가: {attestation.blockers.join(", ")}</p>
          {/if}
        {/if}
      </div>
    {/if}
    {#if report.candidates.length === 0}
      <p class="muted">현재 크기·경과일·지원 파일 유형 조건에 맞는 후보가 없습니다.</p>
    {:else}
      <ul class="candidates">
        {#each report.candidates as candidate (candidate.metadata_fingerprint)}
          <li class:blocked={candidate.blocked_reason !== null}>
            <div class="line">
              <strong>{fmtBytes(candidate.bytes)}</strong>
              <span>{candidate.kind}</span>
              <span>생산 {productionDate(candidate.production_time_ms)}</span>
              <span>근거 {candidate.production_time_source} ({candidate.production_time_confidence})</span>
              <span>수정 후 {candidate.age_days.toLocaleString()}일</span>
              {#if candidate.requires_review}<em>맥락/민감정보 검토 필요</em>{/if}
              {#if candidate.blocked_reason}<em>{candidate.blocked_reason}</em>{/if}
            </div>
            <div class="path" title={candidate.src}>{candidate.src}</div>
            {#if candidate.content_title}
              <div class="metadata">내장 제목: {candidate.content_title}</div>
            {/if}
            {#if candidate.content_authors.length > 0}
              <div class="metadata">작성자/아티스트: {candidate.content_authors.join(", ")}</div>
            {/if}
            {#if candidate.content_context.length > 0}
              <div class="metadata">내장 맥락: {candidate.content_context.join(" · ")}</div>
            {/if}
            {#if candidate.duration_ms !== null}
              <div class="metadata">재생 시간: {duration(candidate.duration_ms)}</div>
            {/if}
            <div class="arrow">→ {candidate.dst}</div>
            <div class="context">맥락: {candidate.source_context} · lineage: {candidate.metadata_fingerprint.slice(0, 12)}</div>
            {#if copyEligible(candidate)}
              <button
                class="copy"
                onclick={() => copyCandidate(candidate)}
                disabled={copyingFingerprint !== "" || copied?.receipt.candidate_fingerprint === candidate.metadata_fingerprint}
              >
                {copyingFingerprint === candidate.metadata_fingerprint ? "복사·해시 검증 중…" : "원본을 유지하고 클라우드에 복사"}
              </button>
            {/if}
            <details>
              <summary>메타데이터 증거 {candidate.metadata_evidence.length}건</summary>
              <ul class="evidence">
                {#each candidate.metadata_evidence as evidence}
                  <li>{evidence.field}: {evidence.value} · {evidence.source} · {evidence.confidence}</li>
                {/each}
              </ul>
            </details>
            {#if candidate.review_reasons.length > 0}
              <div class="context">검토 사유: {candidate.review_reasons.join(", ")}</div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.5rem; align-items: center; }
  .dry { font-size: 0.7rem; color: #fff; background: #59636e; border-radius: 8px; padding: 2px 7px; }
  .controls { display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: end; }
  .provider-auth { display: flex; flex-wrap: wrap; gap: 0.75rem; margin: 0.5rem 0; }
  label { display: grid; gap: 0.2rem; font-size: 0.8rem; color: #555; }
  select { max-width: 32rem; }
  input { width: 7rem; }
  .provider-auth input { width: min(32rem, 75vw); }
  .summary { margin-top: 0.8rem; font-weight: 600; }
  .receipt { margin: 0.75rem 0; padding: 0.75rem; border: 1px solid #6b8e72; border-radius: 4px; background: #f5fbf6; }
  .candidates { list-style: none; margin: 0.5rem 0; padding: 0; max-height: 34rem; overflow-y: auto; }
  .candidates li { padding: 0.6rem; border: 1px solid #e3e3e3; border-radius: 4px; margin-bottom: 0.4rem; }
  .candidates li.blocked { border-color: #b03030; background: #fff7f7; }
  .line { display: flex; flex-wrap: wrap; gap: 0.6rem; font-size: 0.8rem; }
  .line em { color: #9a5b00; }
  .path, .arrow { overflow-wrap: anywhere; font-size: 0.85rem; }
  .arrow { color: #555; margin-top: 0.2rem; }
  .metadata { color: #3f5368; font-size: 0.78rem; margin-top: 0.2rem; }
  .context { color: #777; font-size: 0.75rem; margin-top: 0.2rem; }
  .copy { margin-top: 0.4rem; }
  details { margin-top: 0.3rem; color: #59636e; font-size: 0.75rem; }
  .evidence { margin: 0.25rem 0 0; padding-left: 1.2rem; }
  .muted { color: #777; }
  .warning { color: #8a5700; }
  .safe { color: #276437; }
  .error { color: #b00; }
</style>
