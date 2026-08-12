<script lang="ts">
  import { getSettings, setSettings } from "./api";

  let online = $state(false);
  let busy = $state(true);
  let error = $state("");

  $effect(() => {
    error = "";
    getSettings()
      .then((settings) => {
        online = settings.online_mode;
      })
      .catch(() => {
        error = "설정을 불러오지 못했습니다.";
      })
      .finally(() => {
        busy = false;
      });
  });

  async function toggle() {
    busy = true;
    error = "";
    try {
      const settings = await setSettings(!online);
      online = settings.online_mode;
    } catch {
      error = "설정을 저장하지 못했습니다.";
    } finally {
      busy = false;
    }
  }
</script>

<label class="setting">
  <input type="checkbox" checked={online} disabled={busy} onchange={toggle} />
  온라인 모드(미분류 확장자 웹 조회 — 확장자 토큰만 전송, 기본 꺼짐)
</label>
{#if error}
  <p class="error" role="alert">{error}</p>
{/if}

<style>
  .error { margin: 0.35rem 0 0; color: #b00; }
</style>
