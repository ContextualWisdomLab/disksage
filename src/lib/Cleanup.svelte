<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { verdictBadge } from "./verdictBadge";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import GitWorktreeCleanup from "./GitWorktreeCleanup.svelte";
  import BrewCleanup from "./BrewCleanup.svelte";
  import OrphanCleanup from "./OrphanCleanup.svelte";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let caches: api.CacheCandidate[] = $state([]);
  let artifacts: api.DevArtifact[] = $state([]);
  let selected: Set<string> = $state(new Set());
  let results: api.CleanResult[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");
  let cacheRetryMessage = $state("");
  let podmanPlan: api.PodmanReclaimPlan | null = $state(null);
  let podmanBusy = $state(false);
  let podmanError = $state("");
  let podmanPruneBusy = $state(false);
  let podmanPruneError = $state("");
  let podmanPrunePhrase = $state("");
  let podmanPruneRationale = $state("");
  let podmanPruneExecution: api.PodmanDanglingImagePruneExecution | null = $state(null);
  // ponytail: 배지는 개별 파일/디렉토리 후보(artifacts)에만 표시 — caches는 소수의 고정 규칙 카테고리라 LLM 판정 가치가 낮음.
  let verdicts: Record<string, api.Verdict> = $state({});

  async function loadVerdicts(paths: string[]) {
    try {
      const fvs = await api.fileVerdicts(paths);
      verdicts = Object.fromEntries(fvs.map((f) => [f.path, f.verdict]));
    } catch {
      /* advisory only — ignore */
    }
  }

  async function load() {
    loadError = "";
    try {
      caches = await api.listCacheCandidates();
      artifacts = scannedRoot ? await api.listDevArtifacts(scannedRoot) : [];
      loadVerdicts(artifacts.map((a) => a.path));
    } catch (e) {
      loadError = String(e);
    }
  }

  async function inspectPodman() {
    if (podmanBusy) return;
    podmanBusy = true;
    podmanError = "";
    try {
      podmanPlan = await api.inspectPodmanReclaim();
    } catch (e) {
      podmanError = String(e);
      podmanPlan = null;
    } finally {
      podmanBusy = false;
    }
  }

  function podmanPruneReady(): boolean {
    const phrase = podmanPlan?.dangling_prune_approval_phrase;
    return phrase !== null
      && phrase !== undefined
      && podmanPrunePhrase.trim() === phrase
      && podmanPruneRationale.trim().length > 0
      && !podmanPruneBusy;
  }

  async function prunePodmanDanglingImages() {
    if (!podmanPlan || !podmanPruneReady()) return;
    const okay = await confirm(
      "참조 컨테이너가 없고 tag가 없는 Podman 이미지만 삭제합니다. volume·컨테이너·tagged image·VM은 건드리지 않습니다.\n\n실행 직전에 이미지 목록을 다시 읽어 지문을 검증합니다.",
      { title: "DiskSage Podman 정리", kind: "warning" },
    );
    if (!okay) return;
    podmanPruneBusy = true;
    podmanPruneError = "";
    try {
      podmanPruneExecution = await api.executePodmanDanglingImagePrune(
        podmanPrunePhrase.trim(),
        podmanPruneRationale.trim(),
      );
      podmanPrunePhrase = "";
      podmanPruneRationale = "";
      podmanPlan = await api.inspectPodmanReclaim();
    } catch (e) {
      podmanPruneError = String(e);
    } finally {
      podmanPruneBusy = false;
    }
  }

  async function cleanCache(candidate: api.CacheCandidate) {
    if (busy || !candidate.exists || candidate.bytes === 0) return;
    busy = true;
    loadError = "";
    cacheRetryMessage = "";
    try {
      const targets = await api.listCacheTargets(candidate.path);
      if (targets.length === 0) {
        loadError = `${candidate.label}에 정리할 직계 항목이 없습니다.`;
        return;
      }
      const targetBytes = targets.reduce((sum, target) => sum + target.bytes, 0);
      const okay = await confirm(
        `${candidate.label}의 직계 캐시 ${targets.length}개(${fmtBytes(targetBytes)})를 휴지통으로 보냅니다.\n\n` +
          "캐시 루트는 보존하며, 각 항목은 객체 지문·크기·수정시각·active-use를 다시 검증합니다. 사용 중이거나 증명이 불완전한 항목은 건너뜁니다. 휴지통에서 복원할 수 있습니다.",
        { title: "DiskSage", kind: "warning" },
      );
      if (!okay) return;
      results = await api.cleanCacheContents(candidate.path, targets);
      await load();
    } catch (e) {
      const error = String(e);
      if (error.includes("cache-cleanup-targets-stale")) {
        await load();
        cacheRetryMessage = "캐시 내용이 바뀌어 최신 목록을 불러왔습니다. 다시 휴지통으로를 눌러 검토하세요.";
      } else {
        loadError = error;
      }
    } finally {
      busy = false;
    }
  }

  async function cleanRegenerableCaches() {
    if (busy) return;
    busy = true;
    loadError = "";
    try {
      results = await api.cleanRegenerableCaches();
      await load();
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  function toggle(set: Set<string>, key: string) {
    const next = new Set(set);
    next.has(key) ? next.delete(key) : next.add(key);
    return next;
  }

  let totalSelected = $derived(
    artifacts
      .filter((a) => selected.has(a.path) && a.scan_complete && a.skipped === 0)
      .reduce((sum, artifact) => sum + artifact.bytes, 0),
  );

  let selectionCount = $derived(
    artifacts.filter((a) => selected.has(a.path) && a.scan_complete && a.skipped === 0).length,
  );

  async function executeClean() {
    // 검토·확인 (스펙 §7-6): 명시적 승인 없이는 아무것도 실행되지 않는다
    const selectedArtifacts = artifacts.filter(
      (a) => selected.has(a.path) && a.scan_complete && a.skipped === 0,
    );
    if (selectedArtifacts.length === 0 || !scannedRoot) return;
    const summary = selectedArtifacts.map(
      (a) => `${a.path} (${fmtBytes(a.bytes)}, ${a.files}개) — 메타데이터 지문 ${a.fingerprint.slice(0, 12)}`,
    );
    const okay = await confirm(
      `다음 ${summary.length}개 항목을 휴지통으로 보냅니다 (논리 크기 합계 ${fmtBytes(totalSelected)}):\n\n` +
        summary.slice(0, 15).join("\n") +
        (summary.length > 15 ? `\n… 외 ${summary.length - 15}개` : "") +
        "\n\n휴지통에서 언제든 복원할 수 있습니다. 휴지통을 비우기 전에는 물리 공간이 회수되지 않으며, APFS 공유 블록 때문에 실제 회수량은 논리 크기보다 작을 수 있습니다.",
      { title: "DiskSage", kind: "warning" },
    );
    if (!okay) return;

    busy = true;
    try {
      results = await api.cleanDevArtifacts(scannedRoot, 30, selectedArtifacts);
      selected = new Set();
      await load();
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  let failedResults = $derived(results.filter((r) => !r.ok));
</script>

<section>
  <h2>정리 <button onclick={load} disabled={busy}>새로고침</button></h2>
  {#if loadError}<p class="error" role="alert">{loadError}</p>{/if}

  <h3>캐시</h3>
  <p class="notice" role="status">
    알려진 캐시 루트의 직계 항목만 객체 지문·크기·수정시각을 재검증한 뒤 휴지통으로 보냅니다. 캐시 루트 자체는 보존됩니다.
  </p>
  <button onclick={cleanRegenerableCaches} disabled={busy}>
    {busy ? "재생성 캐시 확인 중…" : "관측된 재생성 캐시 자동 정리"}
  </button>
  <p class="notice" role="status">
    npm·pnpm·Adobe·Edge·uv·Trivy 캐시만 대상으로 하며, 사용 중이거나 증거가 바뀐 항목은 자동으로 건너뜁니다.
  </p>
  {#if cacheRetryMessage}<p class="notice" role="status">{cacheRetryMessage}</p>{/if}
  <ul class="list">
    {#each caches as c (c.id)}
      <li>
        <div>
          <span class:disabled={!c.exists}>{c.label}</span>
          <span class="size">{c.exists ? fmtBytes(c.bytes) : "없음"}</span>
          {#if c.exists}
            <button onclick={() => cleanCache(c)} disabled={busy || c.bytes === 0}>휴지통으로</button>
          {/if}
        </div>
        <span class="path" title={c.path}>{c.path}</span>
      </li>
    {/each}
  </ul>

  <h3>오래된 개발 아티팩트 {scannedRoot ? `(${scannedRoot}, 30일+)` : "(먼저 스캔하세요)"}</h3>
  <ul class="list">
    {#each artifacts as a (a.path)}
      <li>
        <label class:disabled={!a.scan_complete || a.skipped > 0}>
          <input
            type="checkbox"
            disabled={busy || !a.scan_complete || a.skipped > 0}
            checked={selected.has(a.path)}
            onchange={() => (selected = toggle(selected, a.path))}
          />
          {a.kind} <em>({a.project}, {a.age_days}일)</em>
          <span class="size">
            {!a.scan_complete
              ? `${fmtBytes(a.bytes)} · 메타데이터 스캔 미완료`
              : a.skipped > 0
                ? `${fmtBytes(a.bytes)} · 읽기 오류 ${a.skipped}`
                : fmtBytes(a.bytes)}
          </span>
          {#if verdicts[a.path]}
            {@const b = verdictBadge(verdicts[a.path])}
            <span class={b.cls} title={b.title}>{b.label}</span>
          {/if}
        </label>
        <span class="path" title={a.path}>{a.path}</span>
      </li>
    {/each}
  </ul>

  <div class="actions">
    <button onclick={executeClean} disabled={busy || selectionCount === 0}>
      {busy ? "정리 중…" : `선택 항목 휴지통으로 (논리 ${fmtBytes(totalSelected)})`}
    </button>
  </div>

  {#if results.length > 0}
    <p>
      {results.filter((r) => r.ok).length}/{results.length}개 휴지통으로 이동 완료 —
      휴지통에서 복원할 수 있습니다.
    </p>
    {#if failedResults.length > 0}
      <ul class="errors">
        {#each failedResults as r (r.path)}
          <li title={r.path}>⚠ {r.path} — {r.error}</li>
        {/each}
      </ul>
    {/if}
  {/if}

  <GitWorktreeCleanup {scannedRoot} />
  <BrewCleanup />

  <h3>Podman VM 저장소</h3>
  <p class="notice">
    게스트·이미지·volume 증거만 읽습니다. prune, 삭제, trim, 중지는 이 화면에서 실행하지 않습니다.
    실제 물리 회수량은 전후 호스트 관측 없이는 확정하지 않습니다.
  </p>
  <button onclick={inspectPodman} disabled={podmanBusy}>
    {podmanBusy ? "확인 중…" : "Podman 상태 확인"}
  </button>
  {#if podmanError}<p class="error" role="alert">{podmanError}</p>{/if}
  {#if podmanPlan}
    <div class="podman-evidence" aria-live="polite">
      <p>
        {podmanPlan.evidence_complete ? "증거 완전" : "증거 불완전"} ·
        게스트 여유 {podmanPlan.guest_filesystem ? fmtBytes(podmanPlan.guest_filesystem.available_bytes) : "확인 불가"} ·
        보고 reclaimable {podmanPlan.assessment.podman_reported_reclaimable_bytes === null
          ? "미확인"
          : fmtBytes(podmanPlan.assessment.podman_reported_reclaimable_bytes)}
      </p>
      {#if podmanPlan.unused_images}
        <p>미사용 이미지 {podmanPlan.unused_images.unused_records}개 · exact record 합계 {fmtBytes(podmanPlan.unused_images.candidate_record_size_sum)}</p>
      {/if}
      {#if podmanPlan.dangling_prune_approval_phrase}
        <div class="podman-prune">
          <p>dangling 이미지(무tag·참조 컨테이너 0)만 실행 대상으로 확인되었습니다.</p>
          <label>정확한 승인 문구
            <input bind:value={podmanPrunePhrase} placeholder={podmanPlan.dangling_prune_approval_phrase} disabled={podmanPruneBusy} />
          </label>
          <label>정리 사유
            <textarea bind:value={podmanPruneRationale} maxlength="1000" placeholder="예: 재생성 가능한 미사용 dangling 이미지라 정리함" disabled={podmanPruneBusy}></textarea>
          </label>
          <button onclick={prunePodmanDanglingImages} disabled={!podmanPruneReady()}>
            {podmanPruneBusy ? "재검증 후 dangling 이미지 정리 중…" : "dangling 이미지 정리"}
          </button>
          {#if podmanPruneError}<p class="error" role="alert">{podmanPruneError}</p>{/if}
        </div>
      {/if}
      {#if podmanPruneExecution}
        <p class="notice">
          실행 결과: {podmanPruneExecution.executed ? "성공" : `실패(${podmanPruneExecution.status_code})`} ·
          전체 디스크 가용 공간 변화(다른 작업의 영향 포함) {podmanPruneExecution.observed_available_gain_bytes === null
            ? "미확인"
            : fmtBytes(podmanPruneExecution.observed_available_gain_bytes)}
        </p>
        <p class="notice">이 작업에 귀속되는 정리량은 완료 영수증 집계에서 별도로 확인하세요.</p>
      {/if}
      {#if podmanPlan.system_df}
        <p>연결 없는 volume 후보 {fmtBytes(podmanPlan.system_df.local_volumes.reclaimable_bytes)}</p>
      {/if}
      {#if podmanPlan.assessment.recommended_actions.length > 0}
        <ul class="errors">
          {#each podmanPlan.assessment.recommended_actions as action (action.kind)}
            <li>{action.kind}: {action.rationale}</li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>

<OrphanCleanup />

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .list { list-style: none; padding: 0; max-height: 30vh; overflow-y: auto; }
  .list li { display: flex; justify-content: space-between; gap: 1rem; padding: 2px 0; }
  .size { color: #666; font-variant-numeric: tabular-nums; margin-left: 0.5rem; }
  .path { color: #999; font-size: 0.8rem; overflow-wrap: anywhere; text-align: right; }
  .disabled { color: #aaa; }
  .notice { color: #555; font-size: 0.9rem; }
  .error, .errors { color: #b00; }
  .errors { font-size: 0.85rem; }
  .podman-evidence { margin-top: 0.75rem; padding: 0.75rem; border: 1px solid #b7c6d8; border-radius: 4px; background: #f8fafc; }
  .podman-prune { margin-top: 0.75rem; display: grid; gap: 0.5rem; }
  .podman-prune label { display: grid; gap: 0.25rem; }
  .podman-prune input, .podman-prune textarea { width: 100%; box-sizing: border-box; }
  .badge-safe, .badge-caution, .badge-keep, .badge-unrated {
    display: inline-block; margin-left: 0.4rem; padding: 1px 6px; border-radius: 8px;
    font-size: 0.75rem; color: #fff;
  }
  .badge-safe { background: #2a8f4a; }
  .badge-caution { background: #b8860b; }
  .badge-keep { background: #b03030; }
  .badge-unrated { background: #888; }
</style>
