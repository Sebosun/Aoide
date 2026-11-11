<script setup lang="ts">
import { computed, nextTick, onMounted, ref, type Component } from "vue";
import { AudioLines, HardDrive, Highlighter, Info, Library, Palette, X } from "@lucide/vue";
import UiDialog from "../ui/UiDialog.vue";
import AppearanceSettings from "./AppearanceSettings.vue";
import EngineSettings from "./EngineSettings.vue";
import LibrarySettings from "./LibrarySettings.vue";
import WordTimingSettings from "./WordTimingSettings.vue";
import StorageSettings from "./StorageSettings.vue";
import AboutSettings from "./AboutSettings.vue";
import { useNavigationStore, type SettingsTab } from "../../stores/navigation";
import { usePreferencesStore } from "../../stores/preferences";

/** Settings as a dialog over the current screen: a vertical list of tabs on the left, one
 *  section per tab on the right. Tabs keep their unsaved edits while the dialog is open. */
const navigation = useNavigationStore(), preferences = usePreferencesStore();
const tabs: { id: SettingsTab; label: string; icon: Component; panel: Component }[] = [
  { id: "appearance", label: "Appearance", icon: Palette, panel: AppearanceSettings },
  { id: "engine", label: "Speech engine", icon: AudioLines, panel: EngineSettings },
  { id: "library", label: "Library", icon: Library, panel: LibrarySettings },
  { id: "highlighting", label: "Word highlighting", icon: Highlighter, panel: WordTimingSettings },
  { id: "storage", label: "Storage", icon: HardDrive, panel: StorageSettings },
  { id: "about", label: "About", icon: Info, panel: AboutSettings },
];
const list = ref<HTMLElement>(), body = ref<HTMLElement>();
const current = computed(() => tabs.find(t => t.id === navigation.settingsTab) ?? tabs[0]);
const problems = computed(() => [
  ...preferences.problems,
  ...preferences.themes.flatMap(theme => theme.problems.map(problem => `themes/${theme.name}.toml: ${problem}`)),
]);
onMounted(() => { void preferences.start(); });

async function select(id: SettingsTab, focus = false) {
  navigation.settingsTab = id;
  body.value?.scrollTo?.({ top: 0 });
  if (!focus) return;
  await nextTick();
  list.value?.querySelector<HTMLElement>('[aria-selected="true"]')?.focus();
}
function onKey(event: KeyboardEvent) {
  const at = tabs.findIndex(t => t.id === current.value.id);
  const to = { ArrowDown: at + 1, ArrowRight: at + 1, ArrowUp: at - 1, ArrowLeft: at - 1, Home: 0, End: tabs.length - 1 }[event.key];
  if (to === undefined) return;
  event.preventDefault();
  void select(tabs[(to + tabs.length) % tabs.length].id, true);
}
</script>

<template>
  <UiDialog label="Settings" initial-focus='[role="tab"][aria-selected="true"]' return-focus='header [aria-label="Settings"]' @close="navigation.closeSettings()">
    <div class="settings">
      <nav class="settings__nav">
        <h1 class="settings__title">Settings</h1>
        <div ref="list" class="settings__tabs" role="tablist" aria-label="Settings sections" aria-orientation="vertical">
          <button v-for="tab in tabs" :id="`settings-tab-${tab.id}`" :key="tab.id" type="button" role="tab" class="settings__tab" :class="{ 'settings__tab--on': tab.id === current.id }" :aria-selected="tab.id === current.id" :aria-controls="`settings-panel-${tab.id}`" :tabindex="tab.id === current.id ? 0 : -1" @click="select(tab.id)" @keydown="onKey"><component :is="tab.icon" :size="16" aria-hidden="true" /><span>{{ tab.label }}</span></button>
        </div>
      </nav>
      <div ref="body" class="settings__body">
        <button type="button" class="settings__close" aria-label="Close settings" @click="navigation.closeSettings()"><X :size="17" /></button>
        <ul v-if="problems.length" class="settings__problems" role="alert">
          <li v-for="problem in problems" :key="problem">{{ problem }}</li>
        </ul>
        <div :id="`settings-panel-${current.id}`" class="settings__panel" role="tabpanel" :aria-labelledby="`settings-tab-${current.id}`">
          <KeepAlive><component :is="current.panel" :key="current.id" /></KeepAlive>
        </div>
      </div>
    </div>
  </UiDialog>
</template>

<style scoped>
.settings { width: min(940px, calc(100vw - 32px)); height: min(720px, calc(100vh - 48px)); display: grid; grid-template-columns: 220px minmax(0, 1fr); }
.settings__nav { display: grid; align-content: start; gap: 14px; padding: 22px 14px; border-right: 1px solid var(--line); background: color-mix(in srgb, var(--ground) 45%, var(--page)); border-radius: 20px 0 0 20px; }
.settings__title { padding: 0 10px; font-size: 20px; font-weight: 800; letter-spacing: -.02em; }
.settings__tabs { display: grid; gap: 2px; }
.settings__tab { display: flex; align-items: center; gap: 10px; padding: 9px 12px; border-radius: 10px; text-align: left; font-weight: 600; font-size: 13.5px; color: var(--dim); }
.settings__tab:hover { background: var(--wash); color: var(--ink); }
.settings__tab--on, .settings__tab--on:hover { background: var(--ink); color: var(--ground); }
.settings__body { position: relative; overflow-y: auto; overscroll-behavior: contain; padding: 26px 30px 32px; scroll-behavior: smooth; }
.settings__close { position: sticky; top: 0; float: right; z-index: 1; display: grid; place-items: center; width: 34px; height: 34px; margin: -6px -10px 0 12px; border-radius: 50%; background: var(--ground); }
.settings__problems { margin: 0 0 18px; padding: 10px 14px 10px 30px; border-radius: 12px; background: var(--wash); font-size: 12.5px; font-weight: 600; color: var(--accent); overflow-wrap: anywhere; }
@media (max-width: 720px) {
  .settings { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); height: calc(100vh - 48px); }
  .settings__nav { padding: 16px 14px 10px; border-right: 0; border-bottom: 1px solid var(--line); border-radius: 20px 20px 0 0; }
  .settings__tabs { grid-auto-flow: column; overflow-x: auto; padding: 4px; margin: -4px; }
  .settings__tab { white-space: nowrap; }
  .settings__body { padding: 18px 16px 24px; }
}
</style>
