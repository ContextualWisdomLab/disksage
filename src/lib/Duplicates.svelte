<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";
  import { blocksDeletion } from "./dupeGuard";
  import { verdictBadge } from "./verdictBadge";
  import { confirm } from "@tauri-apps/plugin-dialog";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let groups: api.DupeGroup[] = $state([]);
  let busy = $state(false);
  let confirming = $state(false);
  let loadError = $state("");
  // 각 그룹에서 삭제 대상으로 선택된 경로 (보존할 하나를 제외한 나머지)
  let toDelete: Set<string> = $state(new Set());
  let results: api.CleanResult[] = $state([]);
  let verdicts: Record<string, api.Verdict> = $state({});
  let scanGeneration = $state(0);
  let observedRoot: string | null = null;

  $effect(() => {
    const root = scannedRoot;
    if (root === observedRoot) return;
    observedRoot = root;
    ++scanGeneration;
    busy = false;
    confirming = false;
    groups = [];
    toDelete = new Set();
    verdicts = {};
    results = [];
    loadError = "";
  });

  async function loadVerdicts(paths: string[], generation: number) {
    try {
      const fvs = await api.fileVerdicts(paths);
      if (generation !== scanGeneration) return;
      verdicts = Object.fromEntries(fvs.map((f) => [f.path, f.verdict]));
    } catch {
      /* advisory only — ignore */
    }
  }

  async function scan() {
    const root = scannedRoot;
    if (!root) return;
    const generation = ++scanGeneration;
    busy = true;
    loadError = "";
    groups = [];
    toDelete = new Set();
    verdicts = {};
    results = [];
    try {
      const nextGroups = await api.findDuplicateFiles(root);
      if (generation !== scanGeneration || root !== scannedRoot) return;
      groups = nextGroups;
      // 기본 선택: 각 그룹의 첫 파일을 보존, 나머지를 삭제 후보로
      const next = new Set<string>();
      for (const g of groups) {
        for (const p of g.paths.slice(1)) next.add(p);
      }
      toDelete = next;
      loadVerdicts(groups.flatMap((g) => g.paths), generation);
    } catch {
      if (generation === scanGeneration && root === scannedRoot) {
        loadError = "중복 파일 검색에 실패했습니다. 스캔 대상 폴더의 접근 권한을 확인하고 스캔을 다시 실행한 뒤 중복 찾기를 다시 누르세요.";
      }
    } finally {
      if (generation === scanGeneration && root === scannedRoot) busy = false;
    }
  }

  function toggle(path: string) {
    const next = new Set(toDelete);
    next.has(path) ? next.delete(path) : next.add(path);
    toDelete = next;
    loadError = "";
  }

  let reclaimable = $derived(
    groups.reduce(
      (sum, g) => sum + g.size * g.paths.filter((p) => toDelete.has(p)).length,
      0,
    ),
  );

  async function deleteSelected() {
    if (busy || confirming) return;
    const paths = [...toDelete];
    if (paths.length === 0) return;
    loadError = "";
    // 안전: 그룹 전체가 삭제 선택되면 최소 1개는 보존하도록 막는다
    if (blocksDeletion(groups, toDelete)) {
      loadError = "중복 그룹 전체가 삭제 대상으로 선택됐습니다. 각 그룹에서 최소 1개는 보존하도록 선택을 해제한 뒤 다시 시도하세요.";
      return;
    }
    const root = scannedRoot;
    const generation = scanGeneration;
    confirming = true;
    try {
      const okay = await confirm(
        `${paths.length}개 중복 파일을 휴지통으로 보냅니다 (논리 크기 ${fmtBytes(reclaimable)}, 실제 회수량 미검증).\n` +
          `각 그룹의 사본 1개는 보존됩니다. 휴지통을 비우기 전에는 물리 공간이 회수되지 않으며, APFS 공유 블록 때문에 실제 회수량은 더 작을 수 있습니다.`,
        { title: "DiskSage", kind: "warning" },
      );
      if (!okay || generation !== scanGeneration || root !== scannedRoot) return;
      busy = true;
      confirming = false;
      try {
        const r = await api.cleanPaths(paths);
        if (root !== scannedRoot) return;
        await scan();
        results = r;
      } catch {
        loadError = "선택한 중복 파일을 휴지통으로 보내지 못했습니다. 파일이 열려 있는지와 휴지통 접근 권한을 확인한 뒤 중복 찾기부터 다시 실행하세요.";
      } finally {
        busy = false;
      }
    } catch {
      loadError = "휴지통 이동 확인 창을 열지 못했습니다. 다른 확인 창을 닫은 뒤 다시 시도하세요.";
    } finally {
      confirming = false;
    }
  }
</script>

<section>
  <h2>
    중복 파일 {scannedRoot ? "" : "(먼저 스캔하세요)"}
    <button onclick={scan} disabled={busy || confirming || !scannedRoot}>{busy ? "찾는 중…" : "중복 찾기"}</button>
  </h2>
  {#if loadError}<p class="error" role="alert">{loadError}</p>{/if}

  {#if groups.length === 0 && !busy}
    <p class="muted">중복을 찾으려면 스캔 후 "중복 찾기"를 누르세요.</p>
  {/if}

  {#each groups as g (g.hash)}
    <div class="group">
      <div class="ghead">
        {g.paths.length}개 사본 · 각 {fmtBytes(g.size)} · 중복 논리 크기 {fmtBytes(g.size * (g.paths.length - 1))}
      </div>
      <ul>
        {#each g.paths as p (p)}
          <li>
            <label>
              <input
                type="checkbox"
                disabled={busy || confirming}
                checked={toDelete.has(p)}
                onchange={() => toggle(p)}
              />
              <span class="path" title={p}>{p}</span>
              {#if verdicts[p]}
                {@const b = verdictBadge(verdicts[p])}
                <span class={b.cls} title={b.title}>{b.label}</span>
              {/if}
              {#if !toDelete.has(p)}<em class="keep">보존</em>{/if}
            </label>
          </li>
        {/each}
      </ul>
    </div>
  {/each}

  {#if groups.length > 0}
    <div class="actions">
      <button onclick={deleteSelected} disabled={busy || confirming || toDelete.size === 0}>
        {confirming ? "휴지통 이동 확인 대기 중…" : `선택 중복 휴지통으로 (논리 ${fmtBytes(reclaimable)})`}
      </button>
    </div>
  {/if}

  {#if results.length > 0}
    <p>{results.filter((r) => r.ok).length}/{results.length}개 휴지통으로 이동했습니다. 복원이 필요하면 휴지통에서 되돌리세요.</p>
    {#if results.some((r) => !r.ok)}
      <ul class="errors">
        {#each results.filter((r) => !r.ok) as r (r.path)}
          <li title={r.path}>⚠ {r.path} — 파일이 사용 중인지와 접근 권한을 확인한 뒤 중복 찾기부터 다시 실행하세요.</li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .group { border: 1px solid #eee; border-radius: 4px; margin: 0.5rem 0; padding: 0.5rem; }
  .ghead { font-size: 0.85rem; color: #555; margin-bottom: 0.25rem; }
  .group ul { list-style: none; padding: 0; margin: 0; }
  .group li { padding: 1px 0; }
  .path { overflow-wrap: anywhere; }
  .keep { color: #080; margin-left: 0.5rem; font-size: 0.8rem; }
  .muted { color: #999; }
  .error { color: #b00; }
  .errors { color: #b00; font-size: 0.85rem; list-style: none; padding: 0; }
  .badge-safe, .badge-caution, .badge-keep, .badge-unrated {
    display: inline-block; margin-left: 0.4rem; padding: 1px 6px; border-radius: 8px;
    font-size: 0.75rem; color: #fff;
  }
  .badge-safe { background: #2a8f4a; }
  .badge-caution { background: #b8860b; }
  .badge-keep { background: #b03030; }
  .badge-unrated { background: #888; }
</style>
