<script lang="ts">
  import type { EntryView } from "./api";
  import { fmtBytes } from "./fmt";

  let { files }: { files: EntryView[] } = $props();
</script>

<section>
  <h2 id="top-files-heading">대용량 파일 Top {files.length}</h2>
  {#if files.length === 0}
    <p class="empty" role="status">표시할 대용량 파일이 없습니다. 다른 폴더를 스캔하거나 스캔 범위를 넓히세요.</p>
  {:else}
    <div class="table-scroll" role="region" tabindex="0" aria-labelledby="top-files-heading">
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
  .table-scroll { max-height: 40vh; overflow-y: auto; }
  .table-scroll:focus-visible { outline: 2px solid currentColor; outline-offset: 2px; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; position: sticky; top: 0; background: #fff; }
  td { padding: 2px 8px 2px 0; }
  .size { white-space: nowrap; font-variant-numeric: tabular-nums; }
  .path { overflow-wrap: anywhere; color: #444; }
  .empty { margin: 0; color: #555; }
</style>
