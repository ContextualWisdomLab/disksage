<script lang="ts">
  import { getSettings, setSettings } from "./api";
  import { persistOnlineToggle } from "./settingsPersistenceFlow";

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
        online = false;
        error = "설정을 불러오지 못했습니다. 안전을 위해 오프라인 모드를 유지합니다. DiskSage 데이터 폴더의 권한을 확인한 뒤 앱을 다시 열어 보세요.";
      })
      .finally(() => {
        busy = false;
      });
  });

  async function toggle(event: Event) {
    const checkbox = event.currentTarget as HTMLInputElement;
    const persistedOnline = online;
    busy = true;
    error = "";
    try {
      online = await persistOnlineToggle(persistedOnline, checkbox, setSettings);
    } catch {
      error = "설정을 저장하지 못했습니다. 이전 온라인 모드 설정을 유지합니다. DiskSage 데이터 폴더의 권한과 여유 공간을 확인한 뒤 다시 시도하세요.";
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
