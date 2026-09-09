<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import {
    manualPhotoSelectionCompatible,
    quarantineApprovalReady,
    selectionsForGroups,
    syncPhotoCandidatePaths,
  } from "./photoReviewState";
  import { open } from "@tauri-apps/plugin-dialog";

  let { scannedRoot, duplicateGroups }: { scannedRoot: string; duplicateGroups: api.DupeGroup[] } = $props();
  let audit: api.PhotoDuplicateAudit | null = $state(null);
  let plan: api.PhotoQuarantinePlan | null = $state(null);
  let receipt: api.PhotoQuarantineReceipt | null = $state(null);
  let keepers: Record<string, string> = $state({});
  let approval = $state("");
  let rationale = $state("");
  let busy = $state(false);
  let status = $state("");
  let error = $state("");
  let selectedPaths: string[] = $state([]);
  let manualSelectionRoot: string | null = $state(null);

  const samePaths = (left: string[], right: string[]) =>
    left.length === right.length && left.every((path, index) => path === right[index]);

  function clearReviewState() {
    audit = null;
    plan = null;
    receipt = null;
    keepers = {};
    approval = "";
    rationale = "";
  }

  $effect(() => {
    if (manualSelectionRoot !== null && manualSelectionRoot !== scannedRoot) {
      manualSelectionRoot = null;
    }
    const source = manualSelectionRoot === null ? "scan" : "manual";
    const nextPaths = syncPhotoCandidatePaths(duplicateGroups, selectedPaths, source);
    if (!samePaths(nextPaths, selectedPaths)) {
      selectedPaths = nextPaths;
      clearReviewState();
      status = "";
      error = "";
    }
  });

  const relative = (path: string) => {
    const normalizedPath = path.replaceAll("\\", "/");
    const normalizedRoot = scannedRoot.replaceAll("\\", "/").replace(/\/$/, "");
    return normalizedPath.startsWith(`${normalizedRoot}/`)
      ? normalizedPath.slice(normalizedRoot.length + 1)
      : normalizedPath;
  };
  const allSelected = () => audit ? selectionsForGroups(audit.exact_groups, keepers) !== null : false;

  async function review() {
    busy = true; error = ""; plan = null; receipt = null;
    try {
      audit = await api.auditExactPhotoDuplicates(selectedPaths);
      keepers = Object.fromEntries(audit.exact_groups.flatMap((group) =>
        group.keeper_path ? [[group.content_digest, group.keeper_path]] : [],
      ));
      status = audit.exact_groups.length
        ? `${audit.exact_groups.length}개 그룹을 확인했습니다. 남길 사진을 확인하세요.`
        : "정확히 같은 사진이 없습니다. 다른 사진은 자동으로 묶지 않습니다.";
    } catch {
      error = "사진을 확인하지 못했습니다. 파일이 로컬에 내려받아져 있는지 확인한 뒤 다시 시도하세요.";
    } finally { busy = false; }
  }

  async function choosePhotos() {
    const chosen = await open({ multiple: true, directory: false, filters: [{ name: "사진", extensions: ["png"] }] });
    if (!chosen) return;
    const paths = Array.isArray(chosen) ? chosen : [chosen];
    if (!manualPhotoSelectionCompatible(paths)) {
      error = "한 번에 같은 디스크나 네트워크 공유에 있는 PNG만 선택하세요. 다른 위치는 따로 검토하세요.";
      return;
    }
    manualSelectionRoot = scannedRoot;
    selectedPaths = paths;
    clearReviewState();
    error = "";
    status = `${selectedPaths.length}개 사진을 선택했습니다. 화질 검토를 시작하세요.`;
  }

  async function makePlan() {
    if (!audit || !allSelected()) return;
    busy = true; error = "";
    try {
      const selections = selectionsForGroups(audit.exact_groups, keepers);
      if (!selections) return;
      plan = await api.planExactPhotoDuplicateQuarantine(audit, selections);
      approval = "";
      status = `${plan.candidate_file_count}개 사진을 휴지통으로 보낼 준비가 됐습니다. 승인 문구를 직접 입력하세요.`;
    } catch {
      error = "사진 상태가 변경됐습니다. 다시 검토한 뒤 새 계획을 만드세요.";
    } finally { busy = false; }
  }

  async function execute() {
    if (!audit || !plan || !quarantineApprovalReady(plan, approval, rationale)) return;
    busy = true; error = "";
    try {
      receipt = await api.executeExactPhotoDuplicateQuarantine(
        audit, plan, approval, rationale.trim(), Date.now(),
      );
      status = `${receipt.moved_file_count}개 사진을 휴지통으로 옮겼습니다. 복원하려면 휴지통에서 되돌리세요.`;
    } catch {
      error = "사진이 이동되지 않았습니다. 변경된 파일이나 사용 중인 앱을 확인하고 다시 검토하세요.";
    } finally { busy = false; }
  }
</script>

<section class="photo-review" aria-labelledby="photo-review-title">
  <div class="heading">
    <div><p class="eyebrow">정확한 사진 사본</p><h3 id="photo-review-title">남길 사진을 먼저 고르세요</h3></div>
    <div class="heading-actions"><button class="secondary" onclick={choosePhotos} disabled={busy}>사진 직접 선택</button><button class="secondary" onclick={review} disabled={busy || selectedPaths.length < 2}>{busy ? "확인 중…" : "사진 화질 검토"}</button></div>
  </div>
  <p class="guidance">픽셀이 정확히 같은 사진만 표시합니다. 비슷해 보이는 사진은 자동 처리하지 않습니다.</p>
  <div class="status" role="status" aria-live="polite">{status}</div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if audit && !audit.evidence_complete}
    <p class="blocker" role="alert">일부 파일 증거가 불완전합니다. 모든 사진을 로컬에 내려받은 뒤 다시 검토하세요.</p>
  {/if}
  {#each audit?.exact_groups ?? [] as group, index (group.content_digest)}
    <fieldset>
      <legend>그룹 {index + 1} · {group.members.length}개</legend>
      <p class="evidence">논리 크기 {fmtBytes(group.members.reduce((sum, member) => sum + member.bytes, 0))} · 실제 회수량은 휴지통 이동 후 측정됩니다.</p>
      {#if group.keeper_blocker}<p class="blocker">화질 근거가 동률입니다. 남길 사진을 직접 선택하세요.</p>{/if}
      {#each group.members as member (member.object_id)}
        <label class:selected={keepers[group.content_digest] === member.path}>
          <input type="radio" name={group.content_digest} value={member.path} checked={keepers[group.content_digest] === member.path} onchange={() => { keepers = { ...keepers, [group.content_digest]: member.path }; plan = null; }} />
          <span><strong>{relative(member.path)}</strong><small>{member.width}×{member.height} · {member.bit_depth}bit · {member.codec} · {fmtBytes(member.bytes)}</small></span>
        </label>
      {/each}
    </fieldset>
  {/each}

  {#if audit?.exact_groups.length}
    <button onclick={makePlan} disabled={busy || !audit.evidence_complete || !allSelected()}>선택 검토 후 계획 만들기</button>
  {/if}
  {#if plan}
    <div class="approval-panel">
      <p><strong>{plan.candidate_file_count}개</strong> · 논리 {fmtBytes(plan.logical_candidate_bytes)} · 영구 삭제 안 함</p>
      <p>아래 문구를 직접 입력하면 휴지통 이동이 활성화됩니다.</p>
      <code>{plan.exact_approval_phrase}</code>
      <label for="photo-approval">승인 문구</label>
      <input id="photo-approval" bind:value={approval} autocomplete="off" spellcheck="false" />
      <label for="photo-rationale">이 사진들을 정리하는 이유</label>
      <textarea id="photo-rationale" bind:value={rationale} rows="3"></textarea>
      <button onclick={execute} disabled={busy || !quarantineApprovalReady(plan, approval, rationale)}>선택한 사본을 휴지통으로</button>
    </div>
  {/if}
  {#if receipt}
    <p class="receipt">성공 {receipt.moved_file_count}개 · 이동 실패 {receipt.failed_file_count}개. 휴지통을 비우기 전에는 되돌릴 수 있습니다.</p>
  {/if}
</section>

<style>
  .photo-review { --ink:#17221d; --paper:#f5f0e4; --accent:#176b4b; margin-top:1rem; padding:1rem; border:1px solid #b8ad98; border-radius:10px; color:var(--ink); background:linear-gradient(135deg,var(--paper),#fffdf7); }
  .heading { display:flex; justify-content:space-between; gap:1rem; align-items:center; } .heading-actions{display:flex;gap:.5rem;flex-wrap:wrap;}
  h3 { margin:.1rem 0; font-family:Georgia,serif; } .eyebrow { margin:0; color:var(--accent); font-size:.75rem; font-weight:700; letter-spacing:.12em; text-transform:uppercase; }
  .guidance,.evidence,small { color:#514b40; } .status:empty { display:none; }
  fieldset { margin:1rem 0; border:1px solid #c9bea9; border-radius:8px; } legend { font-weight:700; }
  fieldset label { display:flex; gap:.75rem; align-items:center; min-height:44px; padding:.35rem .5rem; border-radius:6px; }
  fieldset label.selected { background:#dcecdf; } label span { display:grid; gap:.15rem; overflow-wrap:anywhere; }
  button,input,textarea { min-height:44px; font:inherit; } button { padding:.55rem .85rem; border:0; border-radius:6px; color:white; background:var(--accent); font-weight:700; }
  button.secondary { color:var(--accent); background:#e0eadf; } button:disabled { opacity:.5; cursor:not-allowed; }
  button:focus-visible,input:focus-visible,textarea:focus-visible { outline:3px solid #d26a18; outline-offset:2px; }
  .approval-panel { display:grid; gap:.5rem; margin-top:1rem; padding:1rem; border-left:5px solid var(--accent); background:white; }
  code { display:block; padding:.5rem; overflow-wrap:anywhere; background:#ece7db; }
  input:not([type="radio"]),textarea { box-sizing:border-box; width:100%; border:1px solid #6d665a; border-radius:5px; padding:.5rem; }
  .blocker,.error { color:#8a1f17; font-weight:650; } .receipt { padding:.75rem; background:#dcecdf; border-left:5px solid var(--accent); }
  @media (max-width:640px) { .heading { align-items:stretch; flex-direction:column; } }
  @media (prefers-reduced-motion:reduce) { * { scroll-behavior:auto !important; } }
</style>
