<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import {
    containerOrphanInspectErrorMessage,
    containerOrphanPruneErrorMessage,
  } from "./containerOrphanErrorFeedback";
  import { confirm } from "@tauri-apps/plugin-dialog";

  let plans: api.ContainerOrphanPlan[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");
  let pruneBusyKey = $state<string | null>(null);
  let pruneErrors = $state<Record<string, string>>({});
  let phrases = $state<Record<string, string>>({});
  let rationales = $state<Record<string, string>>({});
  let executions = $state<Record<string, api.ContainerOrphanPruneExecution>>({});

  const CATEGORY_LABELS: Record<api.OrphanCategory, string> = {
    container: "정지된 컨테이너",
    image: "연결 없는 이미지",
    volume: "연결 없는 볼륨",
    network: "미사용 사용자 정의 네트워크",
  };
  const CATEGORY_HINTS: Record<api.OrphanCategory, string> = {
    container: "실행 중·일시정지 컨테이너는 절대 대상에 포함되지 않습니다.",
    image: "태그가 붙은 이미지는 삭제되지 않고, 참조하는 컨테이너가 없는 태그 없는 이미지만 대상입니다.",
    volume: "컨테이너가 참조하는 볼륨은 대상에서 제외됩니다.",
    network: "기본 네트워크(bridge·host·none 등)와 컨테이너가 붙어 있는 네트워크는 제외됩니다.",
  };

  function planKey(plan: api.ContainerOrphanPlan): string {
    return plan.runtime.kind;
  }

  function categoryKey(key: string, category: api.OrphanCategory): string {
    return `${key}:${category}`;
  }

  function pruneReady(key: string, phrase: string | null, category: api.OrphanCategory): boolean {
    if (phrase === null || pruneBusyKey !== null) return false;
    return (
      phrases[categoryKey(key, category)]?.trim() === phrase &&
      (rationales[categoryKey(key, category)]?.trim().length ?? 0) > 0
    );
  }

  async function inspect() {
    busy = true;
    loadError = "";
    try {
      plans = await api.inspectContainerOrphans();
      pruneBusyKey = null;
      pruneErrors = {};
      phrases = {};
      rationales = {};
      executions = {};
    } catch (error) {
      plans = [];
      // Backend diagnostics may contain local paths/runtime stderr. Keep customer feedback opaque.
      loadError = containerOrphanInspectErrorMessage(error);
    } finally {
      busy = false;
    }
  }

  async function prune(plan: api.ContainerOrphanPlan, category: api.OrphanCategory) {
    const key = categoryKey(planKey(plan), category);
    const cat = plan.categories.find((item) => item.category === category);
    if (!cat?.approval_phrase) return;
    const phrase = cat.approval_phrase;
    const rationale = rationales[key]?.trim();
    if (!rationale || pruneBusyKey !== null) return;
    const granted = await confirm(
      `${CATEGORY_LABELS[category]}만 삭제합니다.\n\n${CATEGORY_HINTS[category]}\n\n실행 직전 목록을 다시 읽어 승인 문구(지문)를 재검증합니다. 이후 휴지통 없이 되돌릴 수 없습니다.`,
      { title: "DiskSage 컨테이너 정리", kind: "warning" },
    );
    if (!granted) return;
    pruneBusyKey = key;
    try {
      executions[key] = await api.executeContainerOrphanPrune(
        plan.runtime.kind,
        category,
        phrase,
        rationale,
      );
      // 실행 후 계획을 폐기해 만료된 승인 문구로 재실행되지 않게 합니다.
      plans = await api.inspectContainerOrphans();
      pruneErrors = {};
      phrases = {};
      rationales = {};
    } catch (error) {
      pruneErrors[key] = containerOrphanPruneErrorMessage(error);
      delete executions[key];
    } finally {
      pruneBusyKey = null;
    }
  }
</script>

<section aria-labelledby="container-orphan-heading">
  <h3 id="container-orphan-heading">Docker · Podman · Colima 미사용 자원</h3>
  <p class="notice">
    각 런타임의 컨테이너·이미지·볼륨·네트워크 중 아무것도 연결되지 않은 항목만 찾아줍니다.
    실행 중인 서비스와 기본 네트워크는 절대 건드리지 않습니다. 삭제 전 승인 문구와 사유를 요구합니다.
  </p>
  <button onclick={inspect} disabled={busy}>
    {busy ? "확인 중…" : "미사용 자원 확인"}
  </button>
  {#if loadError}<p class="error" role="alert">{loadError}</p>{/if}

  {#each plans as plan (plan.runtime.kind)}
    {@const pkey = planKey(plan)}
    <div class="runtime-panel" aria-live="polite">
      <h4>{plan.runtime.display_name}</h4>
      {#if !plan.runtime.healthy}
        <p class="notice">
          이 런타임에 연결할 수 없습니다. 실행 중인지 확인한 뒤 다시 시도해 주세요.
        </p>
      {:else}
        <ul class="categories">
          {#each plan.categories as cat (cat.category)}
            {@const ckey = categoryKey(pkey, cat.category)}
            <li>
              <span class="cat-label">{CATEGORY_LABELS[cat.category]}</span>
              {#if !cat.evidence_complete}
                <span class="notice">증거 불완전 — 안전을 위해 실행할 수 없습니다.</span>
              {:else if cat.evidence && cat.evidence.candidate_records > 0}
                <span>
                  대상 {cat.evidence.candidate_records}개
                  {#if cat.evidence.candidate_size_sum_bytes !== null}
                    · 약 {fmtBytes(cat.evidence.candidate_size_sum_bytes)}
                  {/if}
                </span>
                {#if cat.approval_phrase}
                  <div class="prune-form">
                    <label>승인 문구
                      <input
                        bind:value={phrases[ckey]}
                        placeholder={cat.approval_phrase}
                        disabled={pruneBusyKey !== null}
                      />
                    </label>
                    <label>정리 사유
                      <textarea
                        bind:value={rationales[ckey]}
                        maxlength="1000"
                        placeholder="예: 더 이상 쓰지 않는 미사용 자원이라 정리함"
                        disabled={pruneBusyKey !== null}
                      ></textarea>
                    </label>
                    <button
                      onclick={() => prune(plan, cat.category)}
                      disabled={!pruneReady(pkey, cat.approval_phrase, cat.category)}
                    >
                      {pruneBusyKey === ckey ? "재검증 후 정리 중…" : "정리"}
                    </button>
                    <p class="hint">{CATEGORY_HINTS[cat.category]}</p>
                  </div>
                {/if}
              {:else}
                <span>정리 대상 없음</span>
              {/if}
              {#if pruneErrors[ckey]}
                <p class="error" role="alert">{pruneErrors[ckey]}</p>
              {/if}
              {#if executions[ckey]}
                <p class="notice">
                  결과: {executions[ckey].executed ? "완료" : `실패(${executions[ckey].status_code})`} ·
                  호스트 여유 공간 변화 {executions[ckey].observed_available_gain_bytes === null ? "관측 불가" : `+${fmtBytes(executions[ckey].observed_available_gain_bytes)}`}
                </p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/each}
</section>

<style>
  section { margin-top: 1.25rem; border-top: 1px dashed #ccc; padding-top: 1rem; }
  .notice { color: #555; font-size: 0.9rem; }
  .error { color: #b00; font-size: 0.85rem; }
  .runtime-panel { margin-top: 0.75rem; padding: 0.75rem; border: 1px solid #b7c6d8; border-radius: 4px; background: #f8fafc; }
  .categories { list-style: none; padding: 0; margin: 0.5rem 0 0; display: grid; gap: 0.6rem; }
  .cat-label { font-weight: 600; margin-right: 0.4rem; }
  .prune-form { margin-top: 0.35rem; display: grid; gap: 0.4rem; }
  .prune-form label { display: grid; gap: 0.2rem; }
  .prune-form input, .prune-form textarea { width: 100%; box-sizing: border-box; }
  .hint { color: #666; font-size: 0.8rem; }
</style>