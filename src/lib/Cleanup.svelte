<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { verdictBadge } from "./verdictBadge";
  import {
    executeRuntimeStorageMutation,
    runtimeStorageRecoverySucceeded,
  } from "./runtimeStorageMaintenanceFlow";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import GitWorktreeCleanup from "./GitWorktreeCleanup.svelte";
  import BrewCleanup from "./BrewCleanup.svelte";
  import OrphanCleanup from "./OrphanCleanup.svelte";
  import ContainerOrphanCleanup from "./ContainerOrphanCleanup.svelte";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let caches: api.CacheCandidate[] = $state([]);
  let artifacts: api.DevArtifact[] = $state([]);
  let selected: Set<string> = $state(new Set());
  let results: api.CleanResult[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");
  let cacheRetryMessage = $state("");
  let runtimeStoragePlans: api.RuntimeStoragePlan[] = $state([]);
  let runtimeStorageBusy = $state(false);
  let runtimeStorageError = $state("");
  let runtimeStoragePhrase = $state<Record<string, string>>({});
  let runtimeStorageRationale = $state<Record<string, string>>({});
  let runtimeStorageExecutions: Record<string, api.RuntimeStorageExecution> = $state({});
  let runtimeStorageRecoveryExecutions: Record<string, api.RuntimeStorageRecoveryExecution> = $state({});
  // ponytail: 배지는 개별 파일/디렉토리 후보(artifacts)에만 표시 — caches는 소수의 고정 규칙 카테고리라 자동 자문 가치가 낮음.
  let verdicts: Record<string, api.Verdict> = $state({});

  function artifactKindLabel(kind: string): string {
    const labels: Record<string, string> = {
      node_modules: "Node.js 의존 파일",
      target: "개발 도구 빌드 산출물",
      ".venv": "Python 환경 파일",
      ".venv314": "Python 3.14 환경 파일",
      ".mypy_cache": "Python 형식 검사 캐시",
      ".pytest_cache": "Python 테스트 캐시",
      ".ruff_cache": "Python 코드 검사 캐시",
      ".tox": "Python 호환성 테스트 환경",
      ".nox": "Python 자동화 테스트 환경",
      dist: "배포용 빌드 파일",
      build: "빌드 파일",
      ".codegraph": "코드 분석 자료",
    };
    return labels[kind] ?? "개발 파일";
  }

  function runtimeStorageLabel(runtime: api.RuntimeStorageKind): string {
    return runtime === "podman-machine" ? "Podman" : "Colima";
  }

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
    } catch {
      loadError = "정리 대상을 불러오지 못했습니다. 저장 공간을 확인한 뒤 다시 시도하세요.";
    }
  }

  async function inspectRuntimeStorage() {
    if (runtimeStorageBusy) return;
    runtimeStorageBusy = true;
    runtimeStorageError = "";
    try {
      runtimeStoragePlans = await api.inspectRuntimeStorage();
      runtimeStoragePhrase = {};
      runtimeStorageRationale = {};
      runtimeStorageExecutions = {};
      runtimeStorageRecoveryExecutions = {};
    } catch {
      runtimeStoragePlans = [];
      runtimeStorageError = "저장 공간 상태를 확인하지 못했습니다. 다시 시도하세요.";
    } finally {
      runtimeStorageBusy = false;
    }
  }

  function runtimeStorageReady(plan: api.RuntimeStoragePlan): boolean {
    return plan.exact_approval_phrase !== null
      && runtimeStoragePhrase[plan.runtime]?.trim() === plan.exact_approval_phrase
      && (runtimeStorageRationale[plan.runtime]?.trim().length ?? 0) > 0
      && !runtimeStorageBusy;
  }

  function runtimeStorageRecoveryReady(plan: api.RuntimeStoragePlan): boolean {
    return plan.recovery_approval_phrase !== null
      && runtimeStoragePhrase[plan.runtime]?.trim() === plan.recovery_approval_phrase
      && (runtimeStorageRationale[plan.runtime]?.trim().length ?? 0) > 0
      && !runtimeStorageBusy;
  }

  function invalidateRuntimeStorageApproval() {
    runtimeStoragePhrase = {};
    runtimeStorageRationale = {};
  }

  async function trimRuntimeStorage(plan: api.RuntimeStoragePlan) {
    if (!runtimeStorageReady(plan) || !plan.exact_approval_phrase) return;
    const okay = await confirm(
      `${runtimeStorageLabel(plan.runtime)}에서 회수 가능한 영역만 정리합니다. 개인 파일과 설정은 변경하지 않습니다.\n\n실행 전에 상태를 다시 확인합니다.`,
      { title: "DiskSage 저장 공간 정리", kind: "warning" },
    );
    if (!okay) return;
    runtimeStorageBusy = true;
    runtimeStorageError = "";
    try {
      const outcome = await executeRuntimeStorageMutation(
        () => api.executeRuntimeStorageTrim(
          plan.runtime,
          runtimeStoragePhrase[plan.runtime].trim(),
          runtimeStorageRationale[plan.runtime].trim(),
        ),
        invalidateRuntimeStorageApproval,
        api.inspectRuntimeStorage,
      );
      runtimeStorageExecutions[plan.runtime] = outcome.execution;
      if (outcome.plans) runtimeStoragePlans = outcome.plans;
      if (outcome.refreshFailed) {
        runtimeStorageError = "저장 공간 정리는 실행했지만 최신 상태를 다시 확인하지 못했습니다. 상태를 새로 확인하세요.";
      }
    } catch {
      runtimeStorageError = "저장 공간 정리를 실행하지 못했습니다. 최신 상태를 확인한 뒤 다시 시도하세요.";
    } finally {
      runtimeStorageBusy = false;
    }
  }

  async function recoverRuntimeStorage(plan: api.RuntimeStoragePlan) {
    if (!runtimeStorageRecoveryReady(plan) || !plan.recovery_approval_phrase) return;
    const okay = await confirm(
      `${runtimeStorageLabel(plan.runtime)} 연결을 정상 종료한 뒤 다시 시작합니다. 실행 중인 작업이 있다면 중단될 수 있습니다.\n\n복구 후 저장 공간 상태를 다시 확인합니다.`,
      { title: "저장 공간 연결 복구", kind: "warning" },
    );
    if (!okay) return;
    runtimeStorageBusy = true;
    runtimeStorageError = "";
    try {
      const outcome = await executeRuntimeStorageMutation(
        () => api.executeRuntimeStorageRecovery(
          plan.runtime,
          runtimeStoragePhrase[plan.runtime].trim(),
          runtimeStorageRationale[plan.runtime].trim(),
        ),
        invalidateRuntimeStorageApproval,
        api.inspectRuntimeStorage,
      );
      runtimeStorageRecoveryExecutions[plan.runtime] = outcome.execution;
      if (outcome.plans) runtimeStoragePlans = outcome.plans;
      if (outcome.refreshFailed) {
        runtimeStorageError = "연결 재시작은 실행했지만 최신 게스트 상태를 다시 확인하지 못했습니다. 상태를 새로 확인하세요.";
      }
    } catch {
      runtimeStorageError = "연결을 복구하지 못했습니다. 실행 중인 작업을 확인한 뒤 다시 시도하세요.";
    } finally {
      runtimeStorageBusy = false;
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
          "캐시 루트는 보존하며, 각 항목의 파일 정보·크기·수정 시각·사용 여부를 다시 확인합니다. 사용 중이거나 확인이 불완전한 항목은 건너뜁니다. 휴지통에서 복원할 수 있습니다.",
        { title: "DiskSage", kind: "warning" },
      );
      if (!okay) return;
      results = await api.cleanCacheContents(candidate.path, targets);
      await load();
    } catch (e) {
      if (typeof e === "string" && e.includes("cache-cleanup-targets-stale")) {
        await load();
        cacheRetryMessage = "캐시 내용이 바뀌어 최신 목록을 불러왔습니다. 다시 휴지통으로를 눌러 검토하세요.";
      } else {
        loadError = "캐시를 정리하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.";
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
    } catch {
      loadError = "재생성 가능한 캐시를 정리하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.";
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
    const summary = selectedArtifacts.map((a) => `${a.path} (${fmtBytes(a.bytes)}, ${a.files}개)`);
    const okay = await confirm(
      `다음 ${summary.length}개 항목을 휴지통으로 보냅니다 (논리 크기 합계 ${fmtBytes(totalSelected)}):\n\n` +
        summary.slice(0, 15).join("\n") +
        (summary.length > 15 ? `\n… 외 ${summary.length - 15}개` : "") +
        "\n\n휴지통에서 언제든 복원할 수 있습니다. 휴지통을 비우기 전에는 저장 공간이 회수되지 않습니다.",
      { title: "DiskSage", kind: "warning" },
    );
    if (!okay) return;

    busy = true;
    try {
      results = await api.cleanDevArtifacts(scannedRoot, 30, selectedArtifacts);
      selected = new Set();
      await load();
    } catch {
      loadError = "개발 파일을 정리하지 못했습니다. 상태를 확인한 뒤 다시 시도하십시오.";
    } finally {
      busy = false;
    }
  }

  let failedResults = $derived(results.filter((r) => !r.ok));
</script>

<section>
  <h2>정리 <button onclick={load} disabled={busy}>새로고침</button></h2>
  {#if loadError}<p class="error" role="alert">작업을 다시 시도하세요. {loadError}</p>{/if}

  <h3>캐시</h3>
  <p class="notice" role="status">
    알려진 캐시 루트의 직계 항목만 파일 정보·크기·수정 시각을 다시 확인한 뒤 휴지통으로 보냅니다. 캐시 루트 자체는 보존됩니다.
  </p>
  <button onclick={cleanRegenerableCaches} disabled={busy}>
    {busy ? "재생성 캐시 확인 중…" : "관측된 재생성 캐시 자동 정리"}
  </button>
  <p class="notice" role="status">
    npm·pnpm·Adobe·Edge·uv·Trivy 캐시만 대상으로 하며, 사용 중이거나 확인이 바뀐 항목은 자동으로 건너뜁니다. 정리 범위를 확인하세요.
  </p>
  {#if cacheRetryMessage}<p class="notice" role="status">안내를 확인하세요. {cacheRetryMessage}</p>{/if}
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

  <h3>오래된 개발 파일 {scannedRoot ? `(${scannedRoot}, 30일+)` : "(먼저 스캔하세요)"}</h3>
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
          {artifactKindLabel(a.kind)} <em>({a.project}, {a.age_days}일)</em>
          <span class="size">
            {!a.scan_complete
              ? `${fmtBytes(a.bytes)} · 파일 정보 확인 미완료`
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
          <li title={r.path}>⚠ {r.path} — 정리하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요.</li>
        {/each}
      </ul>
    {/if}
  {/if}

  <GitWorktreeCleanup {scannedRoot} />
  <BrewCleanup />

  <ContainerOrphanCleanup />

  <h3>Podman·Colima 저장 공간</h3>
  <p class="notice">
    Podman과 Colima가 사용하는 저장 공간 상태를 확인합니다. 정리는 목록과 사유를 검토하고 승인한 경우에만 실행합니다.
    전체 저장 공간을 줄이는 기능은 자동으로 실행하지 않으며, 필요하면 해당 도구의 관리 화면에서 상태를 확인하세요.
  </p>
  <button onclick={inspectRuntimeStorage} disabled={runtimeStorageBusy}>
    {runtimeStorageBusy ? "저장 공간 상태 확인 중…" : "Podman·Colima 저장 공간 확인"}
  </button>
  {#if runtimeStorageError}<p class="error" role="alert">{runtimeStorageError}</p>{/if}
  {#if runtimeStoragePlans.length > 0}
    {#each runtimeStoragePlans as plan (plan.runtime)}
      <div class="podman-evidence" aria-live="polite">
        <strong>{runtimeStorageLabel(plan.runtime)} 저장 공간</strong>
        <p>
          {plan.executable_available ? "저장 공간 정리 가능" : "저장 공간 정리를 사용할 수 없음"} ·
          {plan.guest_running === true ? "실행 중" : plan.guest_running === false ? "중지됨" : "상태 미확인"}
          {#if plan.guest_running === true}
            · {plan.guest_reachable === true ? "연결됨" : plan.guest_reachable === false ? "연결 복구 필요" : "연결 상태 미확인"}
          {/if}
        </p>
        {#if plan.host_compaction_supported}
          <p>정리 후 해당 도구의 관리 화면에서 전체 저장 공간을 확인하세요.</p>
        {:else}
          <p class="notice">전체 저장 공간 줄이기는 자동 실행하지 않습니다. 정리 후 해당 도구의 관리 화면에서 상태를 확인하세요.</p>
        {/if}
        {#if plan.exact_approval_phrase}
          <p class="notice">아래 확인 문구를 그대로 입력하고 정리 사유를 남겨야 실행됩니다.</p>
          <code>{plan.exact_approval_phrase}</code>
          <label>확인 문구
            <input bind:value={runtimeStoragePhrase[plan.runtime]} placeholder="위 확인 문구를 직접 입력하세요" disabled={runtimeStorageBusy} />
          </label>
          <label>정리 사유
            <textarea bind:value={runtimeStorageRationale[plan.runtime]} maxlength="1000" placeholder="예: 저장 공간 상태를 확인하고 정리하기로 결정함" disabled={runtimeStorageBusy}></textarea>
          </label>
          <button onclick={() => trimRuntimeStorage(plan)} disabled={!runtimeStorageReady(plan)}>
            {runtimeStorageBusy ? "저장 공간 정리 중…" : "저장 공간 정리"}
          </button>
        {:else if plan.recovery_approval_phrase}
          <p class="notice">저장 공간을 확인할 수 없습니다. 연결을 복구한 뒤 다시 확인하세요.</p>
          <p class="notice">아래 확인 문구를 그대로 입력하고 복구 사유를 남겨야 실행됩니다.</p>
          <code>{plan.recovery_approval_phrase}</code>
          <label>확인 문구
            <input bind:value={runtimeStoragePhrase[plan.runtime]} placeholder="위 확인 문구를 직접 입력하세요" disabled={runtimeStorageBusy} />
          </label>
          <label>복구 사유
            <textarea bind:value={runtimeStorageRationale[plan.runtime]} maxlength="1000" placeholder="예: 연결 상태를 확인하고 다시 시작하기로 결정함" disabled={runtimeStorageBusy}></textarea>
          </label>
          <button onclick={() => recoverRuntimeStorage(plan)} disabled={!runtimeStorageRecoveryReady(plan)}>
            {runtimeStorageBusy ? "연결 복구 중…" : "연결 복구"}
          </button>
        {/if}
        {#if runtimeStorageExecutions[plan.runtime]}
          {@const execution = runtimeStorageExecutions[plan.runtime]}
          <p class="notice" role="status">
            {execution.executed ? "저장 공간 정리를 완료했습니다." : "저장 공간 정리가 완료되지 않았습니다."}
            상태를 다시 확인하세요.
          </p>
          {#if execution.volume_comparison?.available_change.direction === "increased"}
            <p class="notice">
              확인된 사용 가능 공간 증가: {fmtBytes(execution.volume_comparison.available_change.bytes)}
            </p>
          {/if}
        {/if}
        {#if runtimeStorageRecoveryExecutions[plan.runtime]}
          {@const recoveryExecution = runtimeStorageRecoveryExecutions[plan.runtime]}
          <p class="notice" role="status">
            {runtimeStorageRecoverySucceeded(recoveryExecution)
              ? "연결을 복구했습니다. 저장 공간을 다시 확인하세요."
              : "연결 복구가 완료되지 않았습니다. 실행 중인 작업과 게스트 연결 상태를 확인하세요."}
          </p>
        {/if}
      </div>
    {/each}
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
  .badge-safe, .badge-caution, .badge-keep, .badge-unrated {
    display: inline-block; margin-left: 0.4rem; padding: 1px 6px; border-radius: 8px;
    font-size: 0.75rem; color: #fff;
  }
  .badge-safe { background: #2a8f4a; }
  .badge-caution { background: #b8860b; }
  .badge-keep { background: #b03030; }
  .badge-unrated { background: #888; }
</style>
