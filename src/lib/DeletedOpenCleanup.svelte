<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let plan: api.DeletedOpenActionPlan | null = $state(null);
  let busy = $state(false);
  let error = $state("");
  const visibleActionLimit = 25;

  async function inspect() {
    if (busy) return;
    busy = true;
    error = "";
    try {
      plan = await api.inspectDeletedOpenFiles();
    } catch {
      plan = null;
      error = "확인을 마치지 못했습니다. 잠시 후 다시 확인하세요.";
    } finally {
      busy = false;
    }
  }
</script>

<section aria-labelledby="deleted-open-title">
  <h3 id="deleted-open-title">앱을 닫으면 확보되는 공간</h3>
  <p>이미 삭제된 파일을 앱이 계속 사용 중인지 확인합니다. 앱이나 파일을 강제로 닫지 않습니다.</p>
  <button onclick={inspect} disabled={busy}>
    {busy ? "확인 중…" : plan ? "다시 확인" : "공간을 붙잡고 있는 앱 확인"}
  </button>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if plan}
    <div class="result" aria-live="polite">
      {#if !plan.evidence_complete}
        <p>확인을 마치지 못했습니다. 앱을 그대로 둔 채 잠시 후 다시 확인하세요.</p>
      {:else if plan.actions.length === 0}
        <p>지금 닫아야 할 앱이 없습니다.</p>
      {:else}
        <p>
          표시된 앱을 정상적으로 종료하면 논리 크기 {fmtBytes(plan.observed_logical_bytes)}를
          더 이상 붙잡지 않게 할 수 있습니다. 실제 여유 공간은 종료 후 다시 측정합니다.
        </p>
        <ul>
          {#each plan.actions.slice(0, visibleActionLimit) as action}
            <li>
              <strong>{action.application}</strong>
              <span>{fmtBytes(action.observed_logical_bytes)} · 실행 중 {action.holder_count}개</span>
              <span>{action.application}을 모두 정상 종료한 뒤 다시 확인하세요.</span>
            </li>
          {/each}
        </ul>
        {#if plan.actions.length > visibleActionLimit}
          <p>
            우선 큰 항목 {visibleActionLimit}개를 정상 종료하고 다시 확인하세요. 나머지
            {(plan.actions.length - visibleActionLimit).toLocaleString()}개 앱은 다음 확인에서 이어서 안내합니다.
          </p>
        {/if}
      {/if}
      <details>
        <summary>확인 기록</summary>
        <p>확인 시각 {new Date(plan.receipt.observed_at_ms).toLocaleString()}</p>
        <p>기록 번호 {plan.receipt.receipt_id}</p>
        <p>실제 확보량은 아직 측정하지 않았습니다.</p>
      </details>
    </div>
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  button { min-height: 44px; }
  .result { margin-top: 0.75rem; }
  ul { display: grid; gap: 0.75rem; padding-left: 1.25rem; }
  li { display: grid; gap: 0.2rem; }
  li span { color: #555; }
  .error { color: #b00020; }
  details { margin-top: 0.75rem; color: #555; overflow-wrap: anywhere; }
</style>
