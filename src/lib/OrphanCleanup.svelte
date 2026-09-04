<script lang="ts">
  import { confirm } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { cleanAndRefreshOrphanPlan } from "./orphanCleanupFlow";

  let plan: api.OrphanPlan | null = $state(null);
  let selected: Set<string> = $state(new Set());
  let confirmationPhrase = $state("");
  let rationale = $state("");
  let busy = $state(false);
  let error = $state("");
  let result: api.OrphanCleanupResult | null = $state(null);

  function orphanReasonLabel(reason: string): string {
    if (reason.includes("active") || reason.includes("use")) return "사용 중일 수 있어 보류됨";
    if (reason.includes("incomplete") || reason.includes("scan")) return "확인이 끝나지 않아 보류됨";
    if (reason.includes("support")) return "앱 데이터 폴더라 보류됨";
    return "추가 확인이 필요함";
  }

  let selectedCandidates: api.OrphanCandidate[] = $derived.by(() =>
    plan?.scan_complete
      ? plan.candidates.filter(
          (candidate) => selected.has(candidate.candidate_id) && candidate.auto_trash_eligible,
        )
      : [],
  );
  let selectedBytes = $derived.by(() =>
    selectedCandidates.reduce((total, candidate) => total + candidate.bytes, 0),
  );

  async function inspect() {
    if (busy) return;
    busy = true;
    error = "";
    result = null;
    selected = new Set();
    try {
      plan = await api.planOrphanCleanup();
      confirmationPhrase = "";
      rationale = "";
    } catch {
      plan = null;
      error = "정리 후보를 조사하지 못했습니다. 시스템 설정에서 DiskSage의 파일 접근 권한을 확인한 뒤 다시 시도하세요.";
    } finally {
      busy = false;
    }
  }

  function toggle(candidateId: string) {
    const next = new Set(selected);
    next.has(candidateId) ? next.delete(candidateId) : next.add(candidateId);
    selected = next;
  }

  async function clean() {
    if (
      !plan ||
      !plan.scan_complete ||
      busy ||
      selectedCandidates.length === 0 ||
      confirmationPhrase !== plan.exact_approval_phrase
    ) return;
    busy = true;
    const okay = await confirm(
      `${selectedCandidates.length}개의 완전 확인된 미사용 캐시(${fmtBytes(selectedBytes)})만 휴지통으로 보냅니다. 앱이 사용하는 폴더와 확인이 끝나지 않았거나 사용 중인 후보는 포함되지 않습니다.`,
      { title: "DiskSage 관계 기반 고아 정리", kind: "warning" },
    );
    if (!okay) {
      busy = false;
      return;
    }
    error = "";
    try {
      const outcome = await cleanAndRefreshOrphanPlan(
        () => api.cleanOrphanCandidates(
          plan!.plan_fingerprint,
          selectedCandidates.map((candidate) => ({
            candidate_id: candidate.candidate_id,
            metadata_fingerprint: candidate.metadata_fingerprint,
            bytes: candidate.bytes,
            files: candidate.files,
            skipped: candidate.skipped,
            scan_complete: candidate.scan_complete,
            object_id: candidate.object_id,
          })),
          confirmationPhrase,
          rationale.trim(),
        ),
        () => api.planOrphanCleanup(),
      );
      result = outcome.result;
      plan = outcome.plan;
      selected = new Set();
      confirmationPhrase = "";
      rationale = "";
      if (outcome.refresh_failed) {
        error = "휴지통 이동 요청은 처리됐지만 새 후보 목록을 불러오지 못했습니다. 이동 결과를 확인한 뒤 고아 관계 조사를 다시 실행하세요.";
      }
    } catch {
      error = "선택 캐시를 휴지통으로 이동하지 못했습니다. 후보를 다시 조사한 뒤 재시도하세요.";
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h3>관계 기반 macOS 고아 후보</h3>
  <p class="notice">
    앱이 사용하는 폴더와 파일의 기본 정보를 비교해 사용하지 않는 캐시 후보를 찾습니다.
    파일 내용은 읽지 않으며, 앱이 사용하는 폴더·확인이 끝나지 않은 후보·사용 중 후보는 자동 정리하지 않습니다.
    목록을 확인한 뒤 이동할 항목을 선택하세요.
  </p>
  <button onclick={inspect} disabled={busy}>{busy ? "고아 관계 조사 중…" : "고아 관계 조사"}</button>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if plan}
    <p class="muted" role="status">
      후보 {plan.candidate_count}개 · {fmtBytes(plan.candidate_bytes)} ·
      {plan.scan_complete ? "전체 확인 완료" : "확인 불완전 — 정리 차단"}
    </p>
    <ul class="list">
      {#each plan.candidates as candidate (candidate.candidate_id)}
        <li>
          <label class:disabled={!plan.scan_complete || !candidate.auto_trash_eligible}>
            <input
              type="checkbox"
              disabled={busy || !plan.scan_complete || !candidate.auto_trash_eligible}
              checked={selected.has(candidate.candidate_id)}
              onchange={() => toggle(candidate.candidate_id)}
            />
            캐시 파일 그룹 · {fmtBytes(candidate.bytes)}
          </label>
          <span class="muted">
            {candidate.auto_trash_eligible ? "전체 확인·미사용 캐시" : candidate.review_reasons.map(orphanReasonLabel).join(", ")}
          </span>
        </li>
      {/each}
    </ul>
    {#if selectedCandidates.length > 0}
      <p class="muted">승인 문구를 그대로 입력해야 실행됩니다: <code>{plan.exact_approval_phrase}</code></p>
      <label>승인 문구 <input bind:value={confirmationPhrase} autocomplete="off" /></label>
      <label>검토 사유 <input bind:value={rationale} maxlength="1000" /></label>
      <button
        onclick={clean}
        disabled={busy || !plan.scan_complete || confirmationPhrase !== plan.exact_approval_phrase || rationale.trim().length === 0}
      >
        {busy ? "휴지통 이동 중…" : "선택 캐시를 휴지통으로"}
      </button>
    {/if}
  {/if}
  {#if result}
    <p class="muted" role="status">{result.moved_count}/{result.requested_count}개를 휴지통으로 이동했습니다. 휴지통을 비우기 전에는 복원할 수 있습니다.</p>
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  .notice { color: var(--ds-text-muted); font-size: 0.9rem; }
  .error { color: #b00; }
  .muted { color: var(--ds-text-muted); font-size: 0.85rem; }
  .list { list-style: none; padding: 0; max-height: 30vh; overflow-y: auto; }
  .list li { display: grid; gap: 0.25rem; padding: 0.25rem 0; }
  .disabled { color: #aaa; }
</style>
