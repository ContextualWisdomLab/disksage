<script lang="ts">
  import { getSettings, setSettings } from "./api";
  let online = $state(false);
  let busy = $state(false);
  $effect(() => { getSettings().then((s) => (online = s.online_mode)).catch(() => {}); });
  async function toggle() {
    busy = true;
    try { const s = await setSettings(!online); online = s.online_mode; } catch {} finally { busy = false; }
  }
</script>
<label class="setting">
  <input type="checkbox" checked={online} disabled={busy} onchange={toggle} />
  온라인 분류 도움 사용 (기본 꺼짐)
</label>
