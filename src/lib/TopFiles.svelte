<script lang="ts">
  import type { EntryView } from "./api";
  import { fmtBytes } from "./fmt";

  let { files }: { files: EntryView[] } = $props();
</script>

<section>
  <h2>대용량 파일 Top {files.length}</h2>
  <table>
    <thead><tr><th>크기</th><th>경로</th></tr></thead>
    <tbody>
      {#each files as f}
        <tr>
          <td class="size">{fmtBytes(f.size)}</td>
          <td class="path" title={f.path}>{f.path}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</section>

<style>
  section { max-height: 40vh; overflow-y: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; position: sticky; top: 0; background: #fff; }
  td { padding: 2px 8px 2px 0; }
  .size { white-space: nowrap; font-variant-numeric: tabular-nums; }
  .path { overflow-wrap: anywhere; color: #444; }
</style>
