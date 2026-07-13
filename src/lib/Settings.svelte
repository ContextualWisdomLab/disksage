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
  온라인 모드(미분류 확장자 웹 조회 — 확장자 토큰만 전송, 기본 꺼짐)
</label>
