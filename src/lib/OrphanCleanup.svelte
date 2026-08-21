<script lang="ts">
  import { confirm } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let plan: api.OrphanPlan | null = $state(null);
  let selected: Set<string> = $state(new Set());
  let confirmationPhrase = $state("");
  let rationale = $state("");
  let busy = $state(false);
  let error = $state("");
  let result: api.OrphanCleanupResult | null = $state(null);

  let selectedCandidates: api.OrphanCandidate[] = $derived.by(() =>
    plan
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
    } catch (e) {
      plan = null;
      error = String(e);
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
    if (!plan || busy || selectedCandidates.length === 0 || confirmationPhrase !== plan.exact_approval_phrase) return;
    const okay = await confirm(
      `${selectedCandidates.length}개의 완전 스캔된 미사용 캐시(${fmtBytes(selectedBytes)})만 휴지통으로 보냅니다. Application Support와 불완전·사용 중 후보는 포함되지 않습니다.`,
      { title: "DiskSage 관계 기반 고아 정리", kind: "warning" },
    );
    if (!okay) return;
    busy = true;
    error = "";
    try {
      result = await api.cleanOrphanCandidates(
        plan.plan_fingerprint,
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
        rationale,
      );
      plan = await api.planOrphanCleanup();
      selected = new Set();
      confirmationPhrase = "";
      rationale = "";
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h3>관계 기반 macOS 고아 후보</h3>
  <p class="notice">
    설치된 앱 bundle ID와 Library 메타데이터만 비교합니다. 공개 증거에는 로컬 경로가 없고,
    Application Support·불완전 스캔·사용 중 후보는 자동 정리하지 않습니다.
  </p>
  <button onclick={inspect} disabled={busy}>{busy ? "고아 관계 조사 중…" : "고아 관계 조사"}</button>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if plan}
    <p class="muted" role="status">
      후보 {plan.candidate_count}개 · {fmtBytes(plan.candidate_bytes)} ·
      {plan.scan_complete ? "전체 증거 완료" : "증거 불완전 — 정리 차단"}
    </p>
    <ul class="list">
      {#each plan.candidates as candidate (candidate.candidate_id)}
        <li>
          <label class:disabled={!candidate.auto_trash_eligible}>
            <input
              type="checkbox"
              disabled={busy || !candidate.auto_trash_eligible}
              checked={selected.has(candidate.candidate_id)}
              onchange={() => toggle(candidate.candidate_id)}
            />
            {candidate.kind} · {candidate.bundle_id ?? "식별 불가"} · {fmtBytes(candidate.bytes)}
          </label>
          <span class="muted">
            {candidate.auto_trash_eligible ? "완전 스캔·미사용 캐시" : candidate.review_reasons.join(", ")}
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
        disabled={busy || confirmationPhrase !== plan.exact_approval_phrase || rationale.trim().length === 0}
      >
        {busy ? "휴지통 이동 중…" : "선택 캐시를 휴지통으로"}
      </button>
    {/if}
  {/if}
  {#if result}
    <p class="muted" role="status">{result.moved_count}/{result.requested_count}개를 휴지통으로 이동했습니다. 휴지통을 비우기 전에는 복원할 수 있습니다.</p>
  {/if}
</section>
