<script lang="ts">
  import * as api from "./api";
  import * as devArtifactApi from "./devArtifactApi";
  import { fmtBytes } from "./fmt";
  import { verdictBadge } from "./verdictBadge";
  import {
    executeRuntimeStorageMutation,
    runtimeStorageRecoverySucceeded,
  } from "./runtimeStorageMaintenanceFlow";
  import { confirm, open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import GitWorktreeCleanup from "./GitWorktreeCleanup.svelte";
  import BrewCleanup from "./BrewCleanup.svelte";
  import OrphanCleanup from "./OrphanCleanup.svelte";
  import ContainerOrphanCleanup from "./ContainerOrphanCleanup.svelte";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let caches: api.CacheCandidate[] = $state([]);
  let artifacts: devArtifactApi.DevArtifact[] = $state([]);
  let devArtifactRoot = $state("");
  let selected: Set<string> = $state(new Set());
  let results: api.CleanResult[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");
  let cacheRetryMessage = $state("");
  let devArtifactApproval: devArtifactApi.DevArtifactApproval | null = $state(null);
  let devArtifactConfirmationPhrase = $state("");
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
      "cargo-target-cache": "Cargo 공용 빌드 캐시",
      ".venv": "Python 환경 파일",
      ".venv314": "Python 3.14 환경 파일",
      ".mypy_cache": "Python 형식 검사 캐시",
      ".pytest_cache": "Python 테스트 캐시",
      ".ruff_cache": "Python 코드 검사 캐시",
      ".tox": "Python 호환성 테스트 환경",
      ".nox": "Python 자동화 테스트 환경",
      dist: "배포용 빌드 파일",
      build: "빌드 파일",
      ".next": "Next.js 빌드 파일",
      "dist-electron": "Electron 빌드 파일",
      ".codegraph": "코드 분석 자료",
      ".obsolete": "VS Code 폐기 확장",
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

  function invalidateDevArtifactApproval() {
    devArtifactApproval = null;
    devArtifactConfirmationPhrase = "";
  }

  onMount(() => {
    const timer = window.setInterval(() => {
      if (
        devArtifactApproval !== null
        && !devArtifactApi.isDevArtifactApprovalCurrent(devArtifactApproval, Date.now())
      ) {
        invalidateDevArtifactApproval();
      }
    }, 1000);
    return () => window.clearInterval(timer);
  });

  async function load() {
    loadError = "";
    invalidateDevArtifactApproval();
    try {
      caches = await api.listCacheCandidates();
      artifacts = devArtifactRoot ? await devArtifactApi.listDevArtifacts(devArtifactRoot) : [];
      loadVerdicts(artifacts.map((a) => a.path));
    } catch {
      loadError = "정리 대상을 불러오지 못했습니다. 저장 공간과 폴더 접근 권한을 확인한 뒤 다시 시도하세요.";
    }
  }

  async function chooseDevArtifactRoot() {
    if (busy) return;
    loadError = "";
    let chosenRoot: string | string[] | null;
    try {
      chosenRoot = await open({
        directory: true,
        multiple: false,
        title: "정리할 개발 폴더 선택",
      });
    } catch {
      loadError = "개발 폴더 선택 창을 열지 못했습니다. 폴더 접근 권한을 확인한 뒤 다시 시도하세요.";
      return;
    }
    if (typeof chosenRoot !== "string" || chosenRoot.length === 0) return;

    busy = true;
    devArtifactRoot = chosenRoot;
    selected = new Set();
    invalidateDevArtifactApproval();
    try {
      artifacts = await devArtifactApi.listDevArtifacts(devArtifactRoot);
      loadVerdicts(artifacts.map((a) => a.path));
    } catch {
      artifacts = [];
      loadError = "선택한 개발 폴더를 확인하지 못했습니다. 접근 권한과 폴더 상태를 확인하세요.";
    } finally {
      busy = false;
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

  function toggleArtifact(path: string) {
    selected = toggle(selected, path);
    invalidateDevArtifactApproval();
  }

  function selectedDevArtifacts(): devArtifactApi.DevArtifact[] {
    return artifacts.filter(
      (artifact) => selected.has(artifact.path) && artifact.scan_complete && artifact.skipped === 0,
    );
  }

  let totalSelected = $derived(
    artifacts
      .filter((a) => selected.has(a.path) && a.scan_complete && a.skipped === 0)
      .reduce((sum, artifact) => sum + artifact.allocated_bytes, 0),
  );

  let selectionCount = $derived(
    artifacts.filter((a) => selected.has(a.path) && a.scan_complete && a.skipped === 0).length,
  );

  async function reviewDevArtifactSelection() {
    const selectedArtifacts = selectedDevArtifacts();
    if (busy || selectedArtifacts.length === 0 || !devArtifactRoot) return;
    busy = true;
    loadError = "";
    invalidateDevArtifactApproval();
    try {
      devArtifactApproval = await devArtifactApi.reviewDevArtifacts(devArtifactRoot, selectedArtifacts);
    } catch {
      loadError = "선택 항목이 바뀌었거나 현재 상태를 다시 확인할 수 없습니다. 목록을 새로고침한 뒤 다시 검토하세요.";
    } finally {
      busy = false;
    }
  }

  function devArtifactExecutionReady(): boolean {
    return !busy
      && devArtifactApi.isDevArtifactApprovalCurrent(devArtifactApproval, Date.now())
      && devArtifactConfirmationPhrase.trim() === devArtifactApproval.exact_phrase
      && selectionCount > 0;
  }

  async function executeClean() {
    const selectedArtifacts = selectedDevArtifacts();
    const approval = devArtifactApproval;
    if (
      selectedArtifacts.length === 0
      || !devArtifactRoot
      || !devArtifactApi.isDevArtifactApprovalCurrent(approval, Date.now())
      || devArtifactConfirmationPhrase.trim() !== approval.exact_phrase
    ) return;

    const summary = selectedArtifacts.map(
      (artifact) => `${artifact.path} (로컬 ${fmtBytes(artifact.allocated_bytes)}, ${artifact.files}개)`,
    );
    const okay = await confirm(
      `다음 ${summary.length}개 항목을 휴지통으로 보냅니다 (현재 로컬 사용량 ${fmtBytes(totalSelected)}):\n\n` +
        summary.slice(0, 15).join("\n") +
        (summary.length > 15 ? `\n… 외 ${summary.length - 15}개` : "") +
        "\n\n입력한 승인 문구와 선택 지문을 백엔드에서 다시 검증합니다. 휴지통에서 복원할 수 있으며, 휴지통을 비우기 전에는 물리 공간이 회수되지 않습니다.",
      { title: "DiskSage 개발 파일 정리", kind: "warning" },
    );
    if (!okay) return;
    if (!devArtifactApi.isDevArtifactApprovalCurrent(approval, Date.now())) {
      invalidateDevArtifactApproval();
      loadError = "승인 시간이 만료되었습니다. 선택 항목은 유지했습니다. 다시 검토해 승인 문구를 생성하세요.";
      return;
    }

    busy = true;
    loadError = "";
    try {
      const cleanResults = await devArtifactApi.cleanDevArtifactsBound(
        devArtifactRoot,
        0,
        selectedArtifacts,
        approval,
        devArtifactConfirmationPhrase.trim(),
      );
      results = cleanResults;
      const approvalFailure = cleanResults.some(
        (result) => !result.ok && (
          result.error.includes("development-artifact-approval-")
          || result.error.includes("development-artifact-confirmation-")
          || result.error.includes("development-artifact-selection-")
        ),
      );
      if (approvalFailure) {
        invalidateDevArtifactApproval();
        loadError = "승인 증거가 더 이상 유효하지 않습니다. 선택 항목은 유지했습니다. 다시 검토해 승인 문구를 생성하세요.";
        return;
      }
      selected = new Set();
      invalidateDevArtifactApproval();
      await load();
    } catch {
      invalidateDevArtifactApproval();
      loadError = "개발 파일을 정리하지 못했습니다. 목록을 새로고침하고 현재 상태를 다시 검토하세요.";
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
    알려진 캐시 루트의 직계 항목만 파일 정보·크기·수정 시각을 다시 확인한 뒤 휴지통으로 보냅니다. 캐시 루트 자체는 보존됩니다.
  </p>
  <button onclick={cleanRegenerableCaches} disabled={busy}>
    {busy ? "재생성 캐시 확인 중…" : "관측된 재생성 캐시 자동 정리"}
  </button>
  <p class="notice" role="status">
    npm·pnpm·Adobe·Edge·uv·Trivy·AppMap·Superset·Playwright 캐시만 대상으로 하며, 사용 중이거나 확인이 바뀐 항목은 자동으로 건너뜁니다. 정리 범위를 확인하세요.
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

  <h3>개발 빌드 파일 {devArtifactRoot ? `(${devArtifactRoot})` : "(개발 폴더를 선택하세요)"}</h3>
  <p class="notice" role="status">
    전체 디스크 스캔 위치를 재사용하지 않습니다. 정리할 개발 작업공간을 직접 선택하면, 그 안에서 다시 만들 수 있다고 확인된 빌드 파일과 캐시만 표시합니다.
  </p>
  <button onclick={chooseDevArtifactRoot} disabled={busy}>
    {busy ? "개발 폴더 확인 중…" : "개발 폴더 선택"}
  </button>
  {#if devArtifactRoot && artifacts.length === 0 && !busy && !loadError}
    <p class="notice" role="status">선택한 폴더에서 정리 가능한 개발 빌드 파일을 찾지 못했습니다.</p>
  {/if}
  <ul class="list">
    {#each artifacts as a (a.path)}
      <li>
        <label class:disabled={!a.scan_complete || a.skipped > 0}>
          <input
            type="checkbox"
            disabled={busy || !a.scan_complete || a.skipped > 0}
            checked={selected.has(a.path)}
            onchange={() => toggleArtifact(a.path)}
          />
          {artifactKindLabel(a.kind)} <em>({a.project}, {a.age_days}일)</em>
          <span class="size">
            {!a.scan_complete
              ? `${fmtBytes(a.bytes)} · 파일 정보 확인 미완료`
              : a.skipped > 0
                ? `${fmtBytes(a.bytes)} · 읽기 오류 ${a.skipped}`
                : `로컬 ${fmtBytes(a.allocated_bytes)} · 논리 ${fmtBytes(a.bytes)}`}
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
    <button onclick={reviewDevArtifactSelection} disabled={busy || selectionCount === 0}>
      {busy ? "선택 항목 재검증 중…" : `선택 ${selectionCount}개 재검증 및 승인 문구 생성`}
    </button>
    {#if devArtifactApproval}
      <div class="typed-approval" aria-live="polite">
        <p class="notice" role="status">
          아래 문구는 현재 선택과 파일 상태에만 5분 동안 유효합니다. 선택을 바꾸거나 새로고침하면 다시 검토해야 합니다.
        </p>
        <code>{devArtifactApproval.exact_phrase}</code>
        <label>
          정확한 승인 문구 입력
          <input
            bind:value={devArtifactConfirmationPhrase}
            autocomplete="off"
            spellcheck="false"
            disabled={busy}
          />
        </label>
      </div>
    {/if}
    <button onclick={executeClean} disabled={!devArtifactExecutionReady()}>
      {busy ? "정리 중…" : `검토된 선택 항목 휴지통으로 (로컬 ${fmtBytes(totalSelected)})`}
    </button>
  </div>

  {#if results.length > 0}
    <p role="status">
      {results.filter((r) => r.ok).length}/{results.length}개 휴지통으로 이동 완료 —
      휴지통에서 복원할 수 있습니다.
    </p>
    {#if failedResults.length > 0}
      <ul class="errors">
        {#each failedResults as r (r.path)}
          <li title={r.path}>⚠ {r.path} — {r.error || "정리하지 못했습니다. 상태를 확인한 뒤 다시 시도하세요."}</li>
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
              : "연결 복구가 완료되지 않았습니다. 실행 중인 작업과 연결 상태를 확인하세요."}
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
  .actions { display: grid; gap: 0.6rem; }
  .typed-approval { display: grid; gap: 0.5rem; max-width: 100%; }
  .typed-approval code { overflow-wrap: anywhere; }
  .typed-approval label { display: grid; gap: 0.25rem; }
  .typed-approval input { width: 100%; box-sizing: border-box; }
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