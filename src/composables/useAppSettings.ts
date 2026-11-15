import { onMounted, ref } from "vue";
import { api } from "../api";
import type { AppSettings, AppSettingsPatch } from "../api/types";

/** App settings for one settings tab. `save` sends the tab's change, which the core merges into
 *  the saved settings, so tabs never overwrite each other's fields. */
export function useAppSettings() {
  const settings = ref<AppSettings | null>(null), error = ref(""), message = ref("");
  onMounted(async () => { try { settings.value = await api.appSettings(); } catch (e) { error.value = String(e); } });
  async function save(change: AppSettingsPatch, done = "Saved.") {
    error.value = ""; message.value = "";
    try {
      settings.value = await api.updateAppSettings(change);
      message.value = done;
      return true;
    } catch (e) { error.value = String(e); return false; }
  }
  return { settings, error, message, save };
}
