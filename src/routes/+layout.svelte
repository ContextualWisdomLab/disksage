<script lang="ts">
  import type { Snippet } from "svelte";
  import "$lib/ui/design-tokens.css";

  let { children }: { children: Snippet } = $props();
</script>

<a class="ds-skip-link" href="#main-content">본문으로 건너뛰기</a>
{@render children()}
