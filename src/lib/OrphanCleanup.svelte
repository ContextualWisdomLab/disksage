<script lang="ts">
  import * as api from "./api";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { fmtBytes } from "./fmt";
  import { verdictBadge } from "./verdictBadge";
  import { locatedInRelation } from "./orphanRelation";

  let plan: api.OrphanPlan | null = $state(null);
  let selected: Set<string> = $state(new Set());
  let verdicts: Record<string, api.Verdict> = $state({});
  let busy = $state(false);
  let error = $state("");
  let results: api.CleanResult[] = $state([]);

  async function inspect() {
    busy = true;
    error = "";
    results = [];
    try {
      plan = await api.planOrphanCleanup();
      selected = new Set();
      const report = await api.judgeOrphanCleanup();
      if (report.plan_fingerprint === plan.plan_fingerprint) {
        verdicts = Object.fromEntries(report.judgments.map((judgment) => [judgment.path, judgment.verdict]));
      } else {
        verdicts = {};
      }
    } catch (e) {
      error = String(e);
      plan = null;
    } finally {
      busy = false;
    }
  }

  function toggle(path: string) {
    const next = new Set(selected);
    next.has(path) ? next.delete(path) : next.add(path);
    selected = next;
  }

  let eligible = $derived.by((): api.OrphanCandidate[] =>
    plan?.candidates.filter((candidate) => candidate.auto_trash_eligible) ?? [],
  );
  let chosen = $derived.by((): api.OrphanCandidate[] =>
    eligible.filter((candidate) => selected.has(candidate.path)),
  );
  let chosenBytes = $derived.by((): number =>
    chosen.reduce((total, candidate) => total + candidate.bytes, 0),
  );

  async function clean() {
    if (!plan || chosen.length === 0) return;
    const okay = await confirm(
      `${chosen.length}개 재생성 가능 캐시를 휴지통으로 보냅니다 (총 ${fmtBytes(chosenBytes)}).\n\n` +
        chosen.slice(0, 12).map((candidate) => candidate.path).join("\n") +
        "\n\nApplication Support와 보호된 데이터는 이 작업에 포함되지 않습니다.",
      { title: "DiskSage 고아 후보", kind: "warning" },
    );
    if (!okay) return;
    busy = true;
    error = "";
    try {
      results = await api.cleanOrphanCandidates(
        plan.plan_fingerprint,
        chosen.map(({ path, bytes, files, skipped, scan_complete, fingerprint }) => ({
          path,
          bytes,
          files,
          skipped,
          scan_complete,
          fingerprint,
        })),
      );
      selected = new Set();
      await inspect();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="orphan">
  <h3>Library 관계 기반 고아 후보</h3>
  <p class="hint">
    앱 bundle ID·폴더 위치·bounded 메타데이터를 온톨로지 관계로 비교합니다. 파일 내용은 읽지 않으며,
    LLM 배지는 자문일 뿐 삭제 권한이 아닙니다.
  </p>
  <button onclick={inspect} disabled={busy}>{busy ? "분석 중…" : "고아 후보 분석"}</button>
  {#if error}<p class="error">{error}</p>{/if}
  {#if plan}
    <p class="hint">{plan.candidates.length}개 후보 · {fmtBytes(plan.candidate_bytes)} · 계획 지문 {plan.plan_fingerprint.slice(0, 12)}</p>
    {#if plan.notices.length}<ul class="notices">{#each plan.notices as notice}<li>{notice}</li>{/each}</ul>{/if}
    <ul class="list">
      {#each plan.candidates as candidate (candidate.path)}
        {@const located = locatedInRelation(candidate.relations)}
        <li>
          <label class:disabled={!candidate.auto_trash_eligible}>
            <input
              type="checkbox"
              disabled={busy || !candidate.auto_trash_eligible}
              checked={selected.has(candidate.path)}
              onchange={() => toggle(candidate.path)}
            />
            {candidate.kind} · {candidate.bundle_id ?? "broken link"}
            <span class="size">{fmtBytes(candidate.bytes)}</span>
            {#if verdicts[candidate.path]}
              {@const badge = verdictBadge(verdicts[candidate.path])}
              <span class={badge.cls} title="LLM advisory">{badge.label}</span>
            {/if}
          </label>
          <span class="path" title={candidate.path}>{candidate.path}</span>
          {#if located}<span class="relation">{located.predicate} → {located.object}</span>{/if}
          {#if candidate.review_reasons.length}<small>{candidate.review_reasons.join(" · ")}</small>{/if}
        </li>
      {/each}
    </ul>
    <button onclick={clean} disabled={busy || chosen.length === 0}>
      {busy ? "휴지통 이동 중…" : `선택 캐시 휴지통으로 (${fmtBytes(chosenBytes)})`}
    </button>
  {/if}
  {#if results.length}
    <p>{results.filter((result) => result.ok).length}/{results.length}개 이동 완료</p>
  {/if}
</section>

<style>
  .orphan { margin-top: 1rem; padding-top: 1rem; border-top: 1px solid #ddd; }
  .hint, .relation, small { color: #666; font-size: .85rem; }
  .list { list-style: none; padding: 0; max-height: 30vh; overflow-y: auto; }
  .list li { padding: 4px 0; display: grid; gap: 2px; }
  .size { margin-left: .4rem; font-variant-numeric: tabular-nums; }
  .path { color: #999; font-size: .8rem; overflow-wrap: anywhere; }
  .disabled { color: #aaa; }
  .error { color: #b00; }
  .notices { color: #666; font-size: .8rem; }
  .badge-safe, .badge-caution, .badge-keep, .badge-unrated { margin-left: .3rem; }
</style>