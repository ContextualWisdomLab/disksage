<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let roots: api.CloudRoot[] = $state([]);
  let rootIssues: api.CloudRootDiscoveryIssue[] = $state([]);
  let connections: api.OAuthConnection[] = $state([]);
  let reviewDecisions: api.CloudReviewDecision[] = $state([]);
  let selectedRoot = $state("");
  let minSizeMib = $state(256);
  let minAgeDays = $state(90);
  let busy = $state(false);
  let loadError = $state("");
  let report: api.CloudPlanReport | null = $state(null);
  let copyingFingerprint = $state("");
  let reviewingFingerprint = $state("");
  let copied: api.CloudCopyOutput | null = $state(null);
  let attesting = $state(false);
  let attestation: api.CloudAttestationOutput | null = $state(null);
  let objectId = $state("");
  let oauthClientId = $state("");
  let connecting = $state(false);
  let disconnecting = $state(false);

  onMount(async () => {
    try {
      const discovery = await api.inspectCloudRoots();
      roots = discovery.roots;
      rootIssues = discovery.issues;
      connections = await api.listCloudProviderConnections();
      reviewDecisions = await api.listCloudReviewDecisions();
      selectedRoot = roots.find((root) => root.readable)?.path ?? "";
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
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    return candidate.blocked_reason === null
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval);
  }

  function adoptEligible(candidate: api.CloudCandidate): boolean {
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    return candidate.blocked_reason === "destination-exists"
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval);
  }

  function reviewDecision(candidate: api.CloudCandidate): api.CloudReviewDecision | null {
    return reviewDecisions.find((decision) =>
      decision.candidate_fingerprint === candidate.metadata_fingerprint
    ) ?? null;
  }

  function matchingReviewDecision(candidate: api.CloudCandidate): api.CloudReviewDecision | null {
    const decision = reviewDecision(candidate);
    return decision?.review_fingerprint === candidate.review_fingerprint ? decision : null;
  }

  async function reviewCandidate(
    candidate: api.CloudCandidate,
    disposition: api.CloudReviewDisposition,
  ) {
    if (!scannedRoot || !selectedRoot || !candidate.requires_review) return;
    reviewingFingerprint = candidate.metadata_fingerprint;
    loadError = "";
    try {
      const decision = await api.reviewCloudCandidate(
        scannedRoot,
        selectedRoot,
        candidate.metadata_fingerprint,
        candidate.review_fingerprint,
        disposition,
        Math.max(1, Math.floor(minSizeMib)),
        Math.max(0, Math.floor(minAgeDays)),
        200,
      );
      reviewDecisions = [
        ...reviewDecisions.filter((entry) =>
          entry.candidate_fingerprint !== decision.candidate_fingerprint
        ),
        decision,
      ];
    } catch (e) {
      loadError = String(e);
    } finally {
      reviewingFingerprint = "";
    }
  }

  async function copyCandidate(candidate: api.CloudCandidate) {
    if (!scannedRoot || !selectedRoot || !copyEligible(candidate)) return;
    copyingFingerprint = candidate.metadata_fingerprint;
    loadError = "";
    copied = null;
    attestation = null;
    objectId = "";
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

  async function adoptExistingCandidate(candidate: api.CloudCandidate) {
    if (!scannedRoot || !selectedRoot || !adoptEligible(candidate)) return;
    copyingFingerprint = candidate.metadata_fingerprint;
    loadError = "";
    copied = null;
    attestation = null;
    objectId = "";
    try {
      copied = await api.adoptExistingCloudCandidate(
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
    attesting = true;
    loadError = "";
    attestation = null;
    try {
      attestation = await api.attestCloudCopy(
        copied.receipt.receipt_id,
        isIcloud ? null : objectId.trim() || null,
      );
    } catch (e) {
      loadError = String(e);
    } finally {
      attesting = false;
    }
  }

  function selectedRootDetails(): api.CloudRoot | null {
    return roots.find((root) => root.path === selectedRoot) ?? null;
  }

  function connectionForSelectedRoot(): api.OAuthConnection | null {
    const root = selectedRootDetails();
    if (!root) return null;
    return connections.find((connection) =>
      connection.provider === root.provider
      && connection.cloud_root_id === root.id
      && connection.cloud_root_path === root.path
    ) ?? null;
  }

  async function connectProvider() {
    const root = selectedRootDetails();
    if (!root || root.provider === "icloud" || !oauthClientId.trim()) return;
    connecting = true;
    loadError = "";
    try {
      const connection = await api.connectCloudProvider(root.path, oauthClientId.trim());
      connections = [
        ...connections.filter((entry) => entry.connection_id !== connection.connection_id),
        connection,
      ];
      oauthClientId = "";
    } catch (e) {
      loadError = String(e);
    } finally {
      connecting = false;
    }
  }

  async function disconnectProvider() {
    const root = selectedRootDetails();
    const connection = connectionForSelectedRoot();
    if (!root || !connection) return;
    disconnecting = true;
    loadError = "";
    try {
      await api.disconnectCloudProvider(root.path);
      connections = connections.filter((entry) => entry.connection_id !== connection.connection_id);
    } catch (e) {
      loadError = String(e);
    } finally {
      disconnecting = false;
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

  function accountScopeLabel(scope: api.CloudAccountScope): string {
    return {
      personal: "개인",
      organization: "조직",
      shared: "공유",
      unknown: "범위 미확인",
    }[scope];
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
            <option value={root.path} disabled={!root.readable}>
              {root.label} · {accountScopeLabel(root.account_scope)}{root.readable ? "" : " · 접근 불가"}
            </option>
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
    {#if roots.some((root) => !root.readable)}
      <p class="warning">
        접근 불가 클라우드 루트는 선택에서 제외했습니다. macOS 개인정보 보호 권한을 허용한 뒤 목록을 다시 불러오세요.
      </p>
    {/if}
    {#if rootIssues.length > 0}
      <p class="warning">
        클라우드 루트 탐지 문제 {rootIssues.length}건: {rootIssues.map((issue) => `${issue.provider ?? "file-provider"}/${issue.account_scope}/${issue.reason}`).join(", ")}
      </p>
    {/if}
    {#if selectedRootDetails() && selectedRootDetails()?.provider !== "icloud"}
      <div class="oauth-panel">
        {#if connectionForSelectedRoot()}
          <strong>읽기 전용 OAuth 연결됨</strong>
          <span class="context">범위: {connectionForSelectedRoot()?.scope}</span>
          <button onclick={disconnectProvider} disabled={disconnecting || connecting}>
            {disconnecting ? "연결 해제 중…" : "보안 저장소 연결 해제"}
          </button>
        {:else}
          <label>
            {selectedRootDetails()?.provider === "onedrive" ? "Microsoft Desktop OAuth Client ID" : "Google Desktop OAuth Client ID"}
            <input
              class="client-id"
              type="text"
              bind:value={oauthClientId}
              autocomplete="off"
              spellcheck="false"
              disabled={connecting}
            />
          </label>
          <button onclick={connectProvider} disabled={connecting || !oauthClientId.trim()}>
            {connecting ? "브라우저 동의 대기 중…" : "시스템 브라우저로 읽기 전용 연결"}
          </button>
          <p class="muted">
            Client ID는 비밀키가 아닙니다. PKCE와 임의 loopback 포트를 사용하고 refresh token만 OS 보안 저장소에 보관합니다.
          </p>
          {#if selectedRootDetails()?.provider === "onedrive"}
            <p class="muted">Microsoft Entra 앱은 Mobile/Desktop public client로 만들고 loopback redirect URI <code>http://localhost</code>를 등록해야 합니다. 실행 시 임의 포트를 붙이며 IPv4·IPv6 loopback만 수신합니다.</p>
          {/if}
          {#if selectedRootDetails()?.provider === "google-drive"}
            <p class="warning">Google OAuth Client 유형은 Desktop app이어야 합니다. 기존 Drive 파일의 원격 메타데이터 확인에는 restricted scope인 drive.metadata.readonly가 필요하므로 OAuth 앱 검증 또는 테스트 사용자 등록이 필요할 수 있습니다.</p>
          {/if}
        {/if}
      </div>
    {/if}
  {/if}

  {#if !scannedRoot}<p class="muted">먼저 스캔을 완료하세요.</p>{/if}
  {#if loadError}<p class="error">{loadError}</p>{/if}

  {#if report}
    <div class="summary">
      {report.candidates.length}개 후보 · 총 {fmtBytes(report.candidate_bytes)} ·
      충돌 제외 잠재 회수 {fmtBytes(report.potentially_reclaimable_bytes)}
    </div>
    {#if report.exact_duplicates.candidate_count > 0}
      <p class="warning">
        정확 중복 {report.exact_duplicates.candidate_count.toLocaleString()}개 ·
        {report.exact_duplicates.cluster_count.toLocaleString()}개 콘텐츠 클러스터 ·
        대표본 외 중복 경로 {fmtBytes(report.exact_duplicates.redundant_bytes)}.
        동일 크기 후보만 로컬 SHA-256·BLAKE3로 확인했으며, 대표 lineage를 선택하기 전에는 자동 복사하지 않습니다.
      </p>
    {/if}
    <p class="warning">
      생산일 우선순위는 내장 메타데이터 → 명시적 파일명 날짜 → 파일시스템 생성 → 수정 시각입니다. 파일명 날짜와 파일시스템 시각은 저신뢰 잠정값이며, 현재 메타데이터와 목적지에 결박된 명시적 승인 없이는 복사할 수 없습니다. 이미 존재하는 클라우드 파일은 전체 콘텐츠 해시가 모두 같을 때만 채택합니다. 앱 UI는 원본을 삭제하지 않으며, 업로드 증거가 확인되어도 허가 정보만 표시합니다.
    </p>
    {#if copied}
      <div class="receipt">
        <strong>{copied.action === "adopt-existing-copy" ? "기존 클라우드 복사본 검증·채택 완료" : "검증 복사 완료"} · 원본 보존됨</strong>
        <div class="context">영수증 {copied.receipt.receipt_id} · {fmtBytes(copied.receipt.bytes)}</div>
        <div class="path">{copied.receipt.destination}</div>
        {#if copied.receipt.provider !== "icloud"}
          <div class="provider-auth">
            <label>
              {copied.receipt.provider === "onedrive" ? "OneDrive item ID (선택)" : "Google Drive file ID (선택)"}
              <input type="text" bind:value={objectId} autocomplete="off" disabled={attesting} />
            </label>
          </div>
          <p class="muted">먼저 macOS File Provider의 업로드·최신 버전 메타데이터를 확인합니다. item/file ID를 입력하면 네이티브 증거가 불완전할 때만 OAuth API 체크섬 검증으로 보완합니다.</p>
          <p class="muted">API 보완 시 access token은 OS 보안 저장소의 refresh token으로 Rust 내부에서 한 번만 갱신하며 UI·설정·영수증에 노출하지 않습니다.</p>
        {/if}
        <button
          onclick={attestCopy}
          disabled={attesting}
        >
          {attesting ? "검증 중…" : "클라우드 업로드 상태·콘텐츠 확인"}
        </button>
        {#if attestation}
          {#if attestation.permit}
            <p class="safe">업로드 상태와 복사 콘텐츠 검증 완료. 로컬 제거 허가 증거가 생성되었지만 파일은 그대로 보존됩니다.</p>
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
          <li class:blocked={candidate.blocked_reason !== null} class:adoptable={adoptEligible(candidate)}>
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
            {#if candidate.dataset_profile}
              <div class="dataset-profile">
                <strong>
                  데이터 메타데이터: {candidate.dataset_profile.format.toUpperCase()} ·
                  표본 {candidate.dataset_profile.sampled_rows.toLocaleString()}행 ·
                  {candidate.dataset_profile.columns.length.toLocaleString()}열
                </strong>
                <div class="metadata">
                  {candidate.dataset_profile.profile_complete ? "스키마 표본 완료" : "스키마 표본 불완전·검토 필요"}
                  {candidate.dataset_profile.sample_truncated ? " · 제한 범위까지만 읽음" : ""}
                </div>
                {#if candidate.dataset_profile.columns.length > 0}
                  <ul class="schema-columns">
                    {#each candidate.dataset_profile.columns as column}
                      <li>
                        {column.name}: {column.inferred_type} · 관측 {column.observed_values.toLocaleString()} ·
                        결측 {column.missing_values.toLocaleString()}
                        {#if column.sensitive_name}<em>민감 컬럼명 징후</em>{/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
                {#if candidate.dataset_profile.quality_warnings.length > 0}
                  <div class="context">데이터 품질 경고: {candidate.dataset_profile.quality_warnings.join(", ")}</div>
                {/if}
                <div class="context">셀 값은 저장하거나 표시하지 않습니다.</div>
              </div>
            {/if}
            <div class="arrow">→ {candidate.dst}</div>
            <div class="context">
              맥락: {candidate.source_context} · 대상 계정: {accountScopeLabel(candidate.destination_account_scope)} · lineage: {candidate.metadata_fingerprint.slice(0, 12)}
            </div>
            {#if candidate.requires_review}
              <div class="review-controls">
                {#if matchingReviewDecision(candidate)?.disposition === "approved"}
                  <strong class="approved">현재 메타데이터 증거 검토 승인됨</strong>
                {:else if matchingReviewDecision(candidate)?.disposition === "held"}
                  <strong class="held">현재 메타데이터 증거 보류됨</strong>
                {:else if reviewDecision(candidate)}
                  <strong class="held">메타데이터 증거가 바뀌어 이전 결정이 만료됨</strong>
                {:else}
                  <span class="context">아래 증거를 확인한 뒤 승인 또는 보류하세요.</span>
                {/if}
                <button
                  onclick={() => reviewCandidate(candidate, "approved")}
                  disabled={reviewingFingerprint !== "" || matchingReviewDecision(candidate)?.disposition === "approved"}
                >
                  {reviewingFingerprint === candidate.metadata_fingerprint ? "기록 중…" : "메타데이터 검토 승인"}
                </button>
                <button
                  onclick={() => reviewCandidate(candidate, "held")}
                  disabled={reviewingFingerprint !== "" || matchingReviewDecision(candidate)?.disposition === "held"}
                >보류</button>
              </div>
            {/if}
            {#if copyEligible(candidate)}
              <button
                class="copy"
                onclick={() => copyCandidate(candidate)}
                disabled={copyingFingerprint !== "" || copied?.receipt.candidate_fingerprint === candidate.metadata_fingerprint}
              >
                {copyingFingerprint === candidate.metadata_fingerprint ? "복사·해시 검증 중…" : "원본을 유지하고 클라우드에 복사"}
              </button>
            {/if}
            {#if adoptEligible(candidate)}
              <button
                class="copy"
                onclick={() => adoptExistingCandidate(candidate)}
                disabled={copyingFingerprint !== "" || copied?.receipt.candidate_fingerprint === candidate.metadata_fingerprint}
              >
                {copyingFingerprint === candidate.metadata_fingerprint ? "기존 파일 전체 해시 검증 중…" : "기존 클라우드 복사본 해시 검증·채택"}
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
  .oauth-panel { margin-top: 0.75rem; padding: 0.75rem; border: 1px solid #b7c6d8; border-radius: 4px; background: #f6f9fc; display: grid; gap: 0.45rem; justify-items: start; }
  label { display: grid; gap: 0.2rem; font-size: 0.8rem; color: #555; }
  select { max-width: 32rem; }
  input { width: 7rem; }
  .provider-auth input { width: min(32rem, 75vw); }
  .client-id { width: min(40rem, 80vw); }
  .summary { margin-top: 0.8rem; font-weight: 600; }
  .receipt { margin: 0.75rem 0; padding: 0.75rem; border: 1px solid #6b8e72; border-radius: 4px; background: #f5fbf6; }
  .review-controls { display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; margin: 0.5rem 0; }
  .approved { color: #25643b; }
  .held { color: #8a4b16; }
  .candidates { list-style: none; margin: 0.5rem 0; padding: 0; max-height: 34rem; overflow-y: auto; }
  .candidates li { padding: 0.6rem; border: 1px solid #e3e3e3; border-radius: 4px; margin-bottom: 0.4rem; }
  .candidates li.blocked { border-color: #b03030; background: #fff7f7; }
  .candidates li.adoptable { border-color: #6b8e72; background: #f5fbf6; }
  .line { display: flex; flex-wrap: wrap; gap: 0.6rem; font-size: 0.8rem; }
  .line em { color: #9a5b00; }
  .path, .arrow { overflow-wrap: anywhere; font-size: 0.85rem; }
  .arrow { color: #555; margin-top: 0.2rem; }
  .metadata { color: #3f5368; font-size: 0.78rem; margin-top: 0.2rem; }
  .dataset-profile { margin-top: 0.4rem; padding: 0.45rem; border: 1px solid #c8d4df; border-radius: 4px; background: #f8fafc; font-size: 0.78rem; }
  .schema-columns { margin: 0.25rem 0; padding-left: 1.2rem; max-height: 10rem; overflow-y: auto; }
  .schema-columns em { margin-left: 0.4rem; color: #9a5b00; }
  .context { color: #777; font-size: 0.75rem; margin-top: 0.2rem; }
  .copy { margin-top: 0.4rem; }
  details { margin-top: 0.3rem; color: #59636e; font-size: 0.75rem; }
  .evidence { margin: 0.25rem 0 0; padding-left: 1.2rem; }
  .muted { color: #777; }
  .warning { color: #8a5700; }
  .safe { color: #276437; }
  .error { color: #b00; }
</style>
