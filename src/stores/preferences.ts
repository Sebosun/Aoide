import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "../api";
import type { Appearance, Preferences, Theme } from "../api/types";
import { applyAppearance } from "../theme";

/** Appearance settings from ~/.config/aoide, applied to the document as they change. */
export const usePreferencesStore = defineStore("preferences", () => {
  const appearance = ref<Appearance>({ mode: "auto", theme: "default", textSize: 2, readerMode: "page", zoom: 1 });
  const themes = ref<Theme[]>([]), problems = ref<string[]>([]), configDir = ref(""), error = ref("");
  function receive(next: Preferences) {
    appearance.value = next.appearance; themes.value = next.themes; problems.value = next.problems; configDir.value = next.configDir;
    applyAppearance(next.appearance, next.themes);
  }
  let startup: Promise<void> | undefined;
  let unlisten: (() => void) | undefined;
  function start() {
    return startup ??= (async () => {
      try { unlisten = await api.onConfigChanged(receive); receive(await api.preferences()); }
      catch (e) { error.value = String(e); }
    })();
  }
  async function update(change: Partial<Appearance>) {
    const next = { ...appearance.value, ...change };
    appearance.value = next; error.value = "";
    applyAppearance(next, themes.value);
    try { await api.saveAppearance(next); } catch (e) { error.value = String(e); }
  }
  function dispose() { unlisten?.(); startup = undefined; }
  return { appearance, themes, problems, configDir, error, start, update, dispose };
});
