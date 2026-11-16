<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { onMounted, ref } from "vue";
import { Copy, FileCog, Keyboard } from "@lucide/vue";
import { getVersion } from "@tauri-apps/api/app";
import SettingsSection from "@/components/settings/SettingsSection.vue";
import SettingsField from "@/components/settings/SettingsField.vue";
import SettingsNote from "@/components/settings/SettingsNote.vue";
import { TOKENS } from "@/theme";
import { SHORTCUTS } from "@/shortcuts";
import { usePreferencesStore } from "@/stores/preferences";
import { useUiStore } from "@/stores/ui";

/** Where settings live, how to write a theme file, the keyboard shortcuts, and the version. */
const preferences = usePreferencesStore(),
  ui = useUiStore();
const version = ref("");
onMounted(async () => {
  void preferences.start();
  try {
    version.value = await getVersion();
  } catch {
    version.value = "";
  }
});
async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    ui.toast("Copied");
  } catch (e) {
    ui.toast(`Could not copy: ${String(e)}`, "error");
  }
}
</script>

<template>
  <div class="about-settings">
    <SettingsSection title="Config files" :icon="FileCog">
      <SettingsField label="Config folder" as="div">
        <code class="about-settings__path">{{ preferences.configDir }}</code>
        <UiButton
          size="sm"
          icon
          class="about-settings__icon"
          aria-label="Copy config folder path"
          title="Copy path"
          @click="copy(preferences.configDir)"
        >
          <Copy :size="14" />
        </UiButton>
      </SettingsField>
      <SettingsNote
        ><code>settings.toml</code> holds these settings. You can edit it while Aoide runs; changes apply when you save
        the file.</SettingsNote
      >
      <details class="about-settings__themes">
        <summary>Theme files</summary>
        <p>
          Put a file in <code>{{ preferences.configDir }}/themes/</code> with a <code>[light]</code> and a
          <code>[dark]</code> table of colours, then pick it under Appearance › Colours. A file named like a theme that
          comes with Aoide (<code>nord.toml</code>, <code>gruvbox.toml</code>…) takes its place.
        </p>
        <pre class="about-settings__code"><code># {{ preferences.configDir }}/themes/dusk.toml
title = "Dusk"

[light]
accent = "#C2410C"

[dark]
accent = "#FB923C"
ground = "#101326"</code></pre>
        <p>Colours you can set: {{ TOKENS.join(", ") }}.</p>
      </details>
    </SettingsSection>
    <SettingsSection title="Keyboard shortcuts" :icon="Keyboard">
      <dl class="about-settings__keys" aria-label="Keyboard shortcuts">
        <div v-for="shortcut in SHORTCUTS" :key="shortcut.action" class="about-settings__shortcut">
          <dt>
            <template v-for="(combo, i) in shortcut.keys" :key="i"
              ><span v-if="i" class="about-settings__or">or</span
              ><kbd v-for="key in combo" :key="key">{{ key }}</kbd></template
            >
          </dt>
          <dd>{{ shortcut.action }}</dd>
        </div>
      </dl>
    </SettingsSection>
    <SettingsSection title="Aoide">
      <p class="about-settings__line">
        Local EPUB audiobooks with read-along{{ version ? ` · version ${version}` : "" }}.
      </p>
      <slot />
    </SettingsSection>
  </div>
</template>

<style scoped>
.about-settings {
  display: grid;
  gap: var(--space-11);
}
.about-settings__path {
  padding: var(--space-4) var(--space-6);
  border-radius: var(--radius-md);
  background: var(--ground);
  font-size: 12.5px;
  overflow-wrap: anywhere;
}
.about-settings__themes {
  display: grid;
  gap: var(--space-5);
  font-size: 13px;
  color: var(--dim);
  line-height: 1.55;
}
.about-settings__themes summary {
  cursor: pointer;
  font-weight: 700;
  color: var(--ink);
}
.about-settings__themes code {
  font-size: 12px;
  overflow-wrap: anywhere;
}
.about-settings__code {
  margin: 0;
  padding: var(--space-6) var(--space-7);
  border-radius: var(--radius-md);
  background: var(--ground);
  color: var(--ink);
  font-size: 12px;
  line-height: 1.6;
  overflow-x: auto;
}
.about-settings__keys {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  font-size: 13px;
}
.about-settings__shortcut {
  display: grid;
  grid-template-columns: 160px minmax(0, 1fr);
  align-items: center;
  gap: var(--space-6);
  padding: var(--space-2) 0;
}
.about-settings__shortcut dt {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}
.about-settings__shortcut dd {
  margin: 0;
  color: var(--dim);
  font-weight: 600;
}
.about-settings__shortcut kbd {
  min-width: 26px;
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--ground);
  box-shadow: inset 0 -1px 0 var(--line);
  font: 700 12px/1.4 var(--font-ui);
  text-align: center;
}
.about-settings__or {
  margin: 0 var(--space-1);
  font-size: 12px;
  color: var(--faint);
}
.about-settings__line {
  font-size: 13px;
  color: var(--dim);
}
</style>
