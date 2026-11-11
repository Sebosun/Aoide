import { onMounted, ref } from "vue";
import { api } from "../api";
import type { AppSettings } from "../api/types";

/** App settings for one settings tab. `save` merges the tab's change into the latest saved
 *  settings, so tabs never overwrite each other's fields. */
export function useAppSettings() {
  const settings = ref<AppSettings | null>(null), error = ref(""), message = ref("");
  onMounted(async () => { try { settings.value = await api.appSettings(); } catch (e) { error.value = String(e); } });
  async function save(change: Partial<AppSettings>, done = "Saved.") {
    error.value = ""; message.value = "";
    try {
      const next = { ...(await api.appSettings()), ...change };
      await api.saveAppSettings(next);
      settings.value = next;
      message.value = done;
      return true;
    } catch (e) { error.value = String(e); return false; }
  }
  return { settings, error, message, save };
}
