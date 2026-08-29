<script lang="ts">
  import { confirm } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();
  let clones: string[] = $state([]);
  let selected = $state("");
  let reference = $state("origin/main");
  let plan: api.GitCloneReclaimPlan | null = $state(null);
  let phrase = $state("");
  let rationale = $state("");
  let busy = $state(false);
  let message = $state("");

  async function discover() {
    if (!scannedRoot || busy) return;
    busy = true;
    message = "";
    plan = null;
    try {
      const report = await api.inventoryStandaloneGitClones([scannedRoot]);
      clones = report.clone_roots;
      selected = clones[0] ?? "";
      message = report.evidence_complete
        ? clones.length ? "정리할 수 있는 저장소인지 하나씩 확인하세요." : "이 위치에서 별도 저장소를 찾지 못했습니다."
        : "일부 폴더를 확인하지 못했습니다. 접근 권한을 확인한 뒤 다시 시도하세요.";
    } catch {
      message = "저장소를 확인하지 못했습니다. 접근 권한을 확인한 뒤 다시 시도하세요.";
    } finally {
      busy = false;
    }
  }

  async function inspect() {
    if (!selected || !reference.trim() || busy) return;
    busy = true;
    message = "";
    try {
      plan = await api.planStaleGitClone(selected, [reference.trim()], true);
      phrase = "";
      rationale = "";
      message = plan.customer_next_action;
    } catch {
      plan = null;
      message = "원격 저장소 상태를 확인하지 못했습니다. 네트워크와 GitHub 로그인을 확인한 뒤 다시 시도하세요.";
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!plan?.eligible_after_human_approval || phrase !== plan.exact_approval_phrase || !rationale.trim() || busy) return;
    if (!await confirm(`${fmtBytes(plan.size.allocated_bytes)}의 로컬 복사본을 휴지통으로 보냅니다. 계속하시겠습니까?`, { title: "저장소 복사본 정리", kind: "warning" })) return;
    busy = true;
    try {
      await api.removeStaleGitClone(selected, [reference.trim()], true, null, plan.plan_fingerprint, phrase, rationale.trim());
      plan = null;
      clones = clones.filter((path) => path !== selected);
      selected = clones[0] ?? "";
      message = "휴지통으로 이동했습니다. 필요하면 휴지통에서 복원할 수 있습니다.";
    } catch {
      message = "상태가 달라져 이동하지 않았습니다. 다시 확인한 뒤 승인하세요.";
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h3>완료된 작업의 저장소 복사본</h3>
  <p>GitHub에서 작업 완료와 원격 보존을 다시 확인한 복사본만 휴지통으로 보냅니다.</p>
  <button onclick={discover} disabled={!scannedRoot || busy}>{busy ? "확인 중…" : "저장소 찾기"}</button>
  {#if clones.length}
    <label>저장소 <select bind:value={selected}>{#each clones as clone}<option value={clone}>{clone}</option>{/each}</select></label>
    <label>보존 기준 <input bind:value={reference} /></label>
    <button onclick={inspect} disabled={busy || !selected || !reference.trim()}>정리 가능 여부 확인</button>
  {/if}
  {#if plan}
    <p>{fmtBytes(plan.size.allocated_bytes)} · {plan.customer_next_action}</p>
    {#if plan.eligible_after_human_approval}
      <p>승인 문구: <code>{plan.exact_approval_phrase}</code></p>
      <label>승인 문구 <input bind:value={phrase} autocomplete="off" /></label>
      <label>검토 사유 <input bind:value={rationale} maxlength="1000" /></label>
      <button onclick={remove} disabled={busy || phrase !== plan.exact_approval_phrase || !rationale.trim()}>휴지통으로 이동</button>
    {/if}
  {/if}
  {#if message}<p role="status">{message}</p>{/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; display: grid; gap: .6rem; }
  label { display: grid; gap: .25rem; }
  select, input, button { min-height: 44px; }
  code { overflow-wrap: anywhere; }
</style>
