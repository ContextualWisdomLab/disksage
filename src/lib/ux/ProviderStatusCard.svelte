<script lang="ts">
  export type ProviderStatusState = "clear" | "checking" | "provider-sync-incomplete" | "materialization-stalled";

  type Props = {
    provider: string;
    state: ProviderStatusState;
    details: string;
    observedAt?: string;
    blockedFor?: string;
    canCancel?: boolean;
    cancelLabel?: string;
    onCancel?: () => void;
    statusId: string;
    headingLevel?: "h1" | "h2";
  };

  let {
    provider,
    state,
    details,
    observedAt = "",
    blockedFor = "",
    canCancel = false,
    cancelLabel = "복사 취소 요청",
    onCancel,
    statusId,
    headingLevel = "h2",
  }: Props = $props();

  const stateLabel: Record<ProviderStatusState, string> = {
    clear: "새 복사 허용 가능",
    checking: "상태 확인 중",
    "provider-sync-incomplete": "공급자 동기화 증거 불완전",
    "materialization-stalled": "파일 materialization 정체",
  };

  const stateTone: Record<ProviderStatusState, string> = {
    clear: "success",
    checking: "checking",
    "provider-sync-incomplete": "warning",
    "materialization-stalled": "danger",
  };
</script>

<section
  class="status-card {stateTone[state]}"
  data-state={state}
  aria-labelledby={`${statusId}-title`}
  aria-describedby={`${statusId}-details`}
>
  <div class="status-heading">
    <svelte:element this={headingLevel} id={`${statusId}-title`}>{provider} 전역 동기화</svelte:element>
    <span class="state" role="status" aria-live="polite">{stateLabel[state]}</span>
  </div>
  <p id={`${statusId}-details`}>{details}</p>
  {#if observedAt || blockedFor}
    <p class="metadata">
      {#if observedAt}마지막 관찰 {observedAt}{/if}
      {#if observedAt && blockedFor} · {/if}
      {#if blockedFor}동일 차단 {blockedFor}{/if}
    </p>
  {/if}
  {#if canCancel}
    <button
      class="ds-control"
      type="button"
      onclick={onCancel}
      disabled={state === "checking" || !onCancel}
      aria-disabled={state === "checking" || !onCancel}
    >
      {cancelLabel}
    </button>
  {/if}
</section>

<style>
  .status-card {
    display: grid;
    gap: var(--ds-space-2);
    margin-block: var(--ds-space-4);
    padding: var(--ds-space-4);
    border: 1px solid var(--ds-border);
    border-inline-start: 0.35rem solid var(--ds-border);
    border-radius: var(--ds-radius-md);
    background: var(--ds-surface-muted);
  }

  .status-card.success { border-inline-start-color: var(--ds-success-text); }
  .status-card.checking { border-inline-start-color: var(--ds-action); }
  .status-card.warning { border-inline-start-color: var(--ds-warning-text); }
  .status-card.danger { border-inline-start-color: var(--ds-danger-text); }

  .status-heading { display: flex; gap: var(--ds-space-3); align-items: baseline; justify-content: space-between; flex-wrap: wrap; }
  h1, h2 { margin: 0; font-size: 1.1rem; }
  p { margin: 0; }
  .state { font-weight: 700; }
  .metadata { color: var(--ds-text-muted); font-size: 0.9rem; }

  @media (max-width: 40rem) {
    .status-heading { align-items: flex-start; flex-direction: column; }
  }
</style>
