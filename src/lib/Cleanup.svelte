<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { verdictBadge } from "./verdictBadge";
  import { confirm } from "@tauri-apps/plugin-dialog";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let caches: api.CacheCandidate[] = $state([]);
  let artifacts: api.DevArtifact[] = $state([]);
  let selected: Set<string> = $state(new Set());
  let selectedRules: Set<string> = $state(new Set());
  let results: api.CleanResult[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");
  let podmanPlan: api.PodmanReclaimPlan | null = $state(null);
  let podmanBusy = $state(false);
  let podmanError = $state("");
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

  function toggle(set: Set<string>, key: string) {
    const next = new Set(set);
    next.has(key) ? next.delete(key) : next.add(key);
    return next;
  }

  let totalSelected = $derived(
    caches.filter((c) => selectedRules.has(c.id)).reduce((s, c) => s + c.bytes, 0) +
      artifacts.filter((a) => selected.has(a.path)).reduce((s, a) => s + a.bytes, 0),
  );

  let selectionCount = $derived(
    caches.filter((c) => selectedRules.has(c.id) && c.exists).length +
      artifacts.filter((a) => selected.has(a.path)).length,
  );

  async function executeClean() {
    // 검토·확인 (스펙 §7-6): 명시적 승인 없이는 아무것도 실행되지 않는다
    const ruleDirs = caches.filter((c) => selectedRules.has(c.id) && c.exists);
    const artifactPaths = artifacts.filter((a) => selected.has(a.path)).map((a) => a.path);
    const summary = [
      ...ruleDirs.map((c) => `${c.label} (${fmtBytes(c.bytes)}) — 내용물 비우기`),
      ...artifactPaths,
    ];
    if (summary.length === 0) return;
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
      const paths: string[] = [...artifactPaths];
      for (const c of ruleDirs) {
        paths.push(...(await api.expandCleanTargets(c.path)));
      }
      results = await api.cleanPaths(paths);
      selected = new Set();
      selectedRules = new Set();
      await load();
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  async function loadPodmanEvidence() {
    podmanBusy = true;
    podmanError = "";
    try {
      podmanPlan = await api.podmanReclaimPlan();
    } catch (e) {
      podmanPlan = null;
      podmanError = String(e);
    } finally {
      podmanBusy = false;
    }
  }

  function podmanActionLabel(kind: api.PodmanRecommendedActionKind): string {
    switch (kind) {
      case "restore_guest_headroom": return "게스트 최소 여유 확보 검토";
      case "investigate_api": return "Podman API 상태 조사";
      case "review_guest_trim": return "게스트 TRIM 전후 관측 검토";
      case "review_stopped_containers": return "중지 컨테이너 검토";
    }
  }

  let failedResults = $derived(results.filter((r) => !r.ok));
</script>

<section>
  <h2>정리 <button onclick={load} disabled={busy}>새로고침</button></h2>
  {#if loadError}<p class="error">{loadError}</p>{/if}

  <h3>캐시</h3>
  <ul class="list">
    {#each caches as c (c.id)}
      <li>
        <label class:disabled={!c.exists}>
          <input
            type="checkbox"
            disabled={!c.exists || busy}
            checked={selectedRules.has(c.id)}
            onchange={() => (selectedRules = toggle(selectedRules, c.id))}
          />
          {c.label}
          <span class="size">{c.exists ? fmtBytes(c.bytes) : "없음"}</span>
        </label>
        <span class="path" title={c.path}>{c.path}</span>
      </li>
    {/each}
  </ul>

  <div class="podman-panel">
    <h3>Podman VM 물리 할당 증거</h3>
    <p class="note">
      읽기 전용 진단입니다. 이미지 prune, 컨테이너·볼륨 삭제, 머신 중지, TRIM은 실행하지 않습니다.
    </p>
    <button onclick={loadPodmanEvidence} disabled={podmanBusy || busy}>
      {podmanBusy ? "Podman 확인 중…" : "Podman 증거 확인"}
    </button>
    {#if podmanError}<p class="error">{podmanError}</p>{/if}
    {#if podmanPlan}
      <p>
        증거 {podmanPlan.evidence_complete ? "완전" : "부분"} · {podmanPlan.elapsed_ms / 1000}s ·
        호스트 물리 회수량 {podmanPlan.assessment.physically_reclaimable_bytes === null
          ? "미검증"
          : fmtBytes(podmanPlan.assessment.physically_reclaimable_bytes)}
      </p>
      {#if podmanPlan.machine}
        <p>
          {podmanPlan.machine.name}: {podmanPlan.machine.state}
          {#if podmanPlan.machine.configured_disk_bytes !== null}
            · 설정 {fmtBytes(podmanPlan.machine.configured_disk_bytes)}
          {/if}
        </p>
      {/if}
      {#if podmanPlan.raw_image}
        <p title={podmanPlan.raw_image.path}>
          raw 논리 {fmtBytes(podmanPlan.raw_image.logical_bytes)} · 할당
          {podmanPlan.raw_image.allocated_bytes === null ? "관측 불가" : fmtBytes(podmanPlan.raw_image.allocated_bytes)}
        </p>
      {/if}
      {#if podmanPlan.guest_filesystem}
        <p>
          guest 사용 {fmtBytes(podmanPlan.guest_filesystem.used_bytes)} · 여유
          {fmtBytes(podmanPlan.guest_filesystem.available_bytes)}
        </p>
      {/if}
      {#if podmanPlan.store}
        <p>
          이미지 {podmanPlan.store.images}개 · 컨테이너 {podmanPlan.store.containers_total}개
          (실행 {podmanPlan.store.containers_running}, 중지 {podmanPlan.store.containers_stopped})
        </p>
      {/if}
      {#if podmanPlan.assessment.recommended_actions.length > 0}
        <ul class="evidence-list">
          {#each podmanPlan.assessment.recommended_actions as action (action.kind)}
            <li>
              {podmanActionLabel(action.kind)}
              {action.requires_human_approval ? " — 사람 승인 필요" : " — 읽기 전용 조사"}
              <span class="path">{action.rationale}</span>
            </li>
          {/each}
        </ul>
      {/if}
      {#if podmanPlan.issues.length > 0}
        <ul class="errors">
          {#each podmanPlan.issues as issue (issue)}<li>⚠ {issue}</li>{/each}
        </ul>
      {/if}
    {/if}
  </div>

  <h3>오래된 개발 아티팩트 {scannedRoot ? `(${scannedRoot}, 30일+)` : "(먼저 스캔하세요)"}</h3>
  <ul class="list">
    {#each artifacts as a (a.path)}
      <li>
        <label>
          <input
            type="checkbox"
            disabled={busy}
            checked={selected.has(a.path)}
            onchange={() => (selected = toggle(selected, a.path))}
          />
          {a.kind} <em>({a.project}, {a.age_days}일)</em>
          <span class="size">{fmtBytes(a.bytes)}</span>
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
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .list { list-style: none; padding: 0; max-height: 30vh; overflow-y: auto; }
  .list li { display: flex; justify-content: space-between; gap: 1rem; padding: 2px 0; }
  .size { color: #666; font-variant-numeric: tabular-nums; margin-left: 0.5rem; }
  .path { color: #999; font-size: 0.8rem; overflow-wrap: anywhere; text-align: right; }
  .disabled { color: #aaa; }
  .podman-panel { margin: 1rem 0; padding: 0.8rem; border: 1px solid #ddd; border-radius: 8px; }
  .note { color: #666; font-size: 0.9rem; }
  .evidence-list { padding-left: 1.25rem; }
  .evidence-list li { margin: 0.25rem 0; }
  .evidence-list .path { display: block; text-align: left; }
  .error, .errors { color: #b00; }
  .errors { font-size: 0.85rem; }
  .badge-safe, .badge-caution, .badge-keep, .badge-unrated {
    display: inline-block; margin-left: 0.4rem; padding: 1px 6px; border-radius: 8px;
    font-size: 0.75rem; color: #fff;
  }
  .badge-safe { background: #2a8f4a; }
  .badge-caution { background: #b8860b; }
  .badge-keep { background: #b03030; }
  .badge-unrated { background: #888; }
</style>
