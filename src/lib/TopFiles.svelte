<script lang="ts">
  import type { EntryView } from "./api";
  import { fmtBytes } from "./fmt";

  let { files }: { files: EntryView[] } = $props();
</script>

<section>
  <h2 id="top-files-heading">가장 큰 파일 {files.length}개</h2>
  {#if files.length === 0}
    <p class="empty" role="status">표시할 대용량 파일이 없습니다. 다른 폴더를 선택해 다시 스캔하세요.</p>
  {:else}
    <a class="table-focus" href="#top-files-table">파일 표 탐색 시작</a>
    <div id="top-files-table" class="table-scroll" role="region" tabindex="-1" aria-labelledby="top-files-heading">
      <table aria-labelledby="top-files-heading">
        <thead><tr><th scope="col">크기</th><th scope="col">경로</th></tr></thead>
        <tbody>
          {#each files as f}
            <tr>
              <td class="size">{fmtBytes(f.size)}</td>
              <td class="path" title={f.path}>{f.path}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<style>
  .table-scroll { max-height: 40vh; max-width: 100%; overflow: auto; }
  .table-scroll:focus-visible { outline: 2px solid currentColor; outline-offset: 2px; }
  .table-focus { display: inline-block; margin-block-end: 0.35rem; }
  table { width: 100%; table-layout: fixed; border-collapse: collapse; font-size: 0.85rem; }
  th:first-child { width: 5.5rem; }
  th { text-align: left; position: sticky; top: 0; background: Canvas; color: CanvasText; }
  td { padding: 2px 8px 2px 0; }
  .size { white-space: nowrap; font-variant-numeric: tabular-nums; }
  .path { overflow-wrap: anywhere; word-break: break-all; }
  .empty { margin: 0; }
</style>
