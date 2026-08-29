<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { photosApprovalReady, photosSelections } from "./photosLibraryState";

  let authorization = $state("checking");
  let inventory: api.PhotosDuplicateInventory | null = $state(null);
  let plan: api.PhotosDeletionPlan | null = $state(null);
  let receipt: api.PhotosDeletionReceipt | null = $state(null);
  let keepers: Record<string, string> = $state({});
  let approval = $state("");
  let rationale = $state("");
  let status = $state("");
  let error = $state("");
  let busy = $state(false);

  $effect(() => {
    api.photosAuthorizationStatus()
      .then((result) => { authorization = result.authorization; })
      .catch(() => { authorization = "unavailable"; });
  });

  async function connect() {
    busy = true; error = "";
    try {
      authorization = (await api.requestPhotosAuthorization()).authorization;
      status = authorization === "authorized" || authorization === "limited"
        ? "사진 접근이 허용됐습니다. 이제 사본을 확인하세요."
        : "사진 접근이 허용되지 않았습니다. 시스템 설정에서 DiskSage의 사진 접근을 허용하세요.";
    } catch { error = "이 기기에서는 사진 앱을 연결할 수 없습니다."; }
    finally { busy = false; }
  }

  async function inspect() {
    busy = true; error = ""; plan = null; receipt = null;
    try {
      inventory = await api.inspectPhotosDuplicates();
      keepers = {};
      status = inventory.unavailable_count
        ? `${inventory.unavailable_count}개 원본은 이 Mac에 없어 제외했습니다. 로컬 사본만 계속 검토할 수 있습니다.`
        : inventory.exact_groups.length
          ? `${inventory.exact_groups.length}개 정확한 사본 그룹을 찾았습니다. 그룹마다 남길 사진을 고르세요.`
          : "내용이 정확히 같은 사진을 찾지 못했습니다.";
    } catch { error = "사진을 확인하지 못했습니다. 접근 권한을 확인한 뒤 다시 시도하세요."; }
    finally { busy = false; }
  }

  async function makePlan() {
    if (!inventory) return;
    const selections = photosSelections(inventory, keepers);
    if (!selections) return;
    busy = true; error = "";
    try {
      plan = await api.planPhotosDuplicateDeletion(inventory, selections);
      approval = "";
      status = `${plan.delete_identifiers.length}개 사본을 사진 앱의 삭제 확인으로 보낼 준비가 됐습니다.`;
    } catch { error = "사진 상태가 변경됐거나 원본이 부족합니다. 다시 확인하세요."; }
    finally { busy = false; }
  }

  async function execute() {
    if (!inventory || !plan || !photosApprovalReady(plan, approval, rationale)) return;
    busy = true; error = "";
    try {
      receipt = await api.executePhotosDuplicateDeletion(inventory, plan, approval, rationale.trim(), Date.now());
      status = `${receipt.deleted_count}개 사본이 최근 삭제된 항목으로 이동했습니다. 되돌리려면 사진 앱의 최근 삭제된 항목을 여세요.`;
    } catch { error = "사진을 삭제하지 않았습니다. 사진 앱의 확인 결과나 변경된 사진을 확인하고 다시 검토하세요."; }
    finally { busy = false; }
  }
</script>

<section class="photos-library" aria-labelledby="photos-library-title">
  <div class="heading">
    <div><p class="eyebrow">Apple Photos</p><h3 id="photos-library-title">사진 앱의 정확한 사본 정리</h3></div>
    {#if authorization !== "authorized" && authorization !== "limited"}
      <button class="secondary" onclick={connect} disabled={busy}>사진 앱 연결</button>
    {:else}
      <button class="secondary" onclick={inspect} disabled={busy}>{busy ? "확인 중…" : "사진 사본 확인"}</button>
    {/if}
  </div>
  <p class="guidance">사진 앱이 관리하는 원본만 안전하게 확인합니다. 이 Mac에 없는 원본은 다운로드하거나 삭제하지 않습니다.</p>
  <div class="status" role="status" aria-live="polite">{status}</div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if inventory?.unavailable_count}
    <p class="blocker" role="status">이 Mac에 없는 원본은 다운로드하거나 삭제하지 않습니다. 해당 사진도 비교하려면 사진 앱에서 원본을 먼저 다운로드하세요.</p>
  {/if}
  {#if inventory?.inventory_truncated}
    <p class="blocker" role="alert">검토 범위가 너무 큽니다. 사진 앱에서 검토할 사진을 줄인 뒤 다시 확인하세요.</p>
  {/if}
  {#each inventory?.exact_groups ?? [] as group, index (group.content_sha256)}
    <fieldset>
      <legend>사본 그룹 {index + 1} · {group.members.length}개</legend>
      <p class="evidence">내용 해시가 정확히 같습니다. 해상도·픽셀 수·파일 크기는 각각 표시하며 합산 점수는 사용하지 않습니다.</p>
      {#each group.members as member (member.local_identifier)}
        <label class:selected={keepers[group.content_sha256] === member.local_identifier}>
          <input type="radio" name={`photos-${group.content_sha256}`} checked={keepers[group.content_sha256] === member.local_identifier}
            onchange={() => { keepers = { ...keepers, [group.content_sha256]: member.local_identifier }; plan = null; }} />
          <span><strong>{member.original_filename || "사진"}</strong><small>{member.width_pixels}×{member.height_pixels} · {fmtBytes(member.encoded_bytes ?? 0)}</small></span>
        </label>
      {/each}
    </fieldset>
  {/each}
  {#if inventory?.exact_groups.length}
    <button onclick={makePlan} disabled={busy || !inventory.evidence_complete || !photosSelections(inventory, keepers)}>남길 사진 확인 후 계획 만들기</button>
  {/if}
  {#if plan}
    <div class="approval-panel">
      <p><strong>{plan.delete_identifiers.length}개</strong> · 논리 {fmtBytes(plan.logical_candidate_bytes)} · 사진 앱의 확인 전에는 삭제되지 않습니다.</p>
      <code>{plan.exact_approval_phrase}</code>
      <label for="photos-approval">승인 문구</label><input id="photos-approval" bind:value={approval} autocomplete="off" spellcheck="false" />
      <label for="photos-rationale">정리 이유</label><textarea id="photos-rationale" bind:value={rationale} rows="3"></textarea>
      <button onclick={execute} disabled={busy || !photosApprovalReady(plan, approval, rationale)}>사진 앱에서 삭제 확인</button>
    </div>
  {/if}
  {#if receipt}<p class="receipt">완료 기록 {receipt.receipt_id.slice(0, 12)} · 사진 앱의 최근 삭제된 항목에서 복원할 수 있습니다.</p>{/if}
</section>

<style>
  .photos-library{--ink:#17221d;--paper:#f5f0e4;--accent:#176b4b;margin-top:1rem;padding:1rem;border:1px solid #b8ad98;border-radius:10px;color:var(--ink);background:linear-gradient(135deg,var(--paper),#fffdf7)}
  .heading{display:flex;justify-content:space-between;gap:1rem;align-items:center}.eyebrow{margin:0;color:var(--accent);font-size:.75rem;font-weight:700;letter-spacing:.12em;text-transform:uppercase}h3{margin:.1rem 0;font-family:Georgia,serif}.guidance,.evidence,small{color:#514b40}.status:empty{display:none}
  fieldset{margin:1rem 0;border:1px solid #c9bea9;border-radius:8px}legend{font-weight:700}fieldset label{display:flex;gap:.75rem;align-items:center;min-height:44px;padding:.35rem .5rem;border-radius:6px}fieldset label.selected{background:#dcecdf}label span{display:grid;gap:.15rem;overflow-wrap:anywhere}
  button,input,textarea{min-height:44px;font:inherit}button{padding:.55rem .85rem;border:0;border-radius:6px;color:white;background:var(--accent);font-weight:700}button.secondary{color:var(--accent);background:#e0eadf}button:disabled{opacity:.5;cursor:not-allowed}button:focus-visible,input:focus-visible,textarea:focus-visible{outline:3px solid #d26a18;outline-offset:2px}
  .approval-panel{display:grid;gap:.5rem;margin-top:1rem;padding:1rem;border-left:5px solid var(--accent);background:white}code{display:block;padding:.5rem;overflow-wrap:anywhere;background:#ece7db}input:not([type="radio"]),textarea{box-sizing:border-box;width:100%;border:1px solid #6d665a;border-radius:5px;padding:.5rem}.blocker,.error{color:#8a1f17;font-weight:650}.receipt{padding:.75rem;background:#dcecdf;border-left:5px solid var(--accent)}
  @media(max-width:640px){.heading{align-items:stretch;flex-direction:column}}@media(prefers-reduced-motion:reduce){*{scroll-behavior:auto!important}}
</style>
