<script setup lang="ts">
import UiSegmented from "@/components/ui/UiSegmented.vue";
import { computed, onMounted, ref } from "vue";
import { Minus, Palette, Plus } from "@lucide/vue";
import ThemePicker from "@/components/settings/ThemePicker.vue";
import type { AppearanceMode } from "@/api/types";
import { TEXT_SCALES } from "@/theme";
import { ZOOM_STEPS, clampZoom, stepZoom, zoomLabel } from "@/zoom";
import SettingsNote from "@/components/settings/SettingsNote.vue";
import { usePreferencesStore } from "@/stores/preferences";
import { useLibraryStore } from "@/stores/library";

const preferences = usePreferencesStore(),
  library = useLibraryStore();
const wpmError = ref("");
/** Reading speed, for the time left to read; the core keeps it within 100–600. */
async function saveWpm(event: Event) {
  const input = event.target as HTMLInputElement;
  wpmError.value = "";
  try {
    await library.setReadingWpm(Number(input.value));
  } catch (e) {
    wpmError.value = String(e);
    input.value = String(library.readingWpm);
  }
}
onMounted(() => {
  void preferences.start();
});
const modes: { value: AppearanceMode; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];
const zoom = computed(() => clampZoom(preferences.appearance.zoom));
</script>

<template>
  <section class="appearance" aria-labelledby="appearance-title">
    <h2 id="appearance-title" class="appearance__title"><Palette :size="18" /> Appearance</h2>
    <div class="appearance__row">
      <span class="appearance__label">Mode</span>
      <UiSegmented
        class="appearance__modes"
        label="Light or dark"
        tone="ground"
        :options="modes"
        :model-value="preferences.appearance.mode"
        @update:model-value="preferences.update({ mode: $event })"
      />
    </div>
    <div class="appearance__row">
      <span class="appearance__label">Colours</span>
      <ThemePicker
        class="appearance__themes"
        :themes="preferences.themes"
        :mode="preferences.appearance.mode"
        :model-value="preferences.appearance.theme"
        @update:model-value="(theme) => preferences.update({ theme })"
      />
    </div>
    <label class="appearance__row">
      <span class="appearance__label">Reading text</span>
      <input
        class="appearance__range"
        type="range"
        min="0"
        :max="TEXT_SCALES.length - 1"
        step="1"
        :value="preferences.appearance.textSize"
        aria-label="Reading text size"
        @input="preferences.update({ textSize: Number(($event.target as HTMLInputElement).value) })"
      />
    </label>
    <label class="appearance__row">
      <span class="appearance__label">Reading speed</span>
      <span class="appearance__wpm"
        ><input
          class="appearance__number"
          type="number"
          min="100"
          max="600"
          step="10"
          :value="library.readingWpm"
          aria-label="Reading speed in words a minute"
          @change="saveWpm"
        />
        words a minute</span
      >
    </label>
    <p v-if="wpmError" class="appearance__problems" role="alert">{{ wpmError }}</p>
    <div class="appearance__row">
      <span class="appearance__label">Interface size</span>
      <div class="appearance__zoom" role="group" aria-label="Interface size">
        <button
          type="button"
          class="appearance__step"
          aria-label="Smaller interface"
          title="Smaller (Ctrl −)"
          :disabled="zoom <= ZOOM_STEPS[0]"
          @click="preferences.update({ zoom: stepZoom(zoom, -1) })"
        >
          <Minus :size="15" />
        </button>
        <button
          type="button"
          class="appearance__percent"
          aria-label="Reset interface size to 100%"
          title="Reset to 100% (Ctrl 0)"
          :disabled="zoom === 1"
          @click="preferences.update({ zoom: 1 })"
        >
          {{ zoomLabel(zoom) }}
        </button>
        <button
          type="button"
          class="appearance__step"
          aria-label="Larger interface"
          title="Larger (Ctrl +)"
          :disabled="zoom >= ZOOM_STEPS[ZOOM_STEPS.length - 1]"
          @click="preferences.update({ zoom: stepZoom(zoom, 1) })"
        >
          <Plus :size="15" />
        </button>
      </div>
    </div>
    <SettingsNote
      >Saved in <code>{{ preferences.configDir }}/settings.toml</code>. Your own colour themes: see About › Theme
      files.</SettingsNote
    >
    <p v-if="preferences.error" class="appearance__problems" role="alert">{{ preferences.error }}</p>
  </section>
</template>

<style scoped>
.appearance {
  display: grid;
  gap: var(--space-8);
}
.appearance__title {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  font-size: 16px;
  font-weight: 800;
  letter-spacing: -0.01em;
}
.appearance__row {
  display: grid;
  grid-template-columns: 160px minmax(0, 1fr);
  align-items: center;
  gap: var(--space-6);
  font-size: 13px;
}
.appearance__label {
  color: var(--dim);
  font-weight: 600;
}
.appearance__modes {
  justify-self: start;
}
.appearance__zoom {
  display: inline-flex;
  justify-self: start;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1);
  border-radius: var(--radius-pill);
  background: var(--ground);
}
.appearance__step {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  color: var(--dim);
}
.appearance__step:hover:not(:disabled) {
  background: var(--wash);
  color: var(--ink);
}
.appearance__percent {
  min-width: 58px;
  height: 30px;
  padding: 0 var(--space-4);
  border-radius: var(--radius-pill);
  font-weight: 700;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
}
.appearance__percent:hover:not(:disabled) {
  background: var(--wash);
}
.appearance__percent:disabled {
  opacity: 1;
  cursor: default;
}
.appearance__themes {
  align-self: start;
}
.appearance__range {
  justify-self: start;
  width: min(260px, 100%);
}
.appearance__wpm {
  justify-self: start;
  display: flex;
  align-items: center;
  gap: var(--space-4);
  color: var(--dim);
}
.appearance__number {
  width: 90px;
}
.appearance__problems {
  margin: 0;
  padding-left: var(--space-9);
  font-size: 12px;
  color: var(--accent);
  overflow-wrap: anywhere;
}
@media (max-width: 560px) {
  .appearance__row {
    grid-template-columns: 1fr;
  }
}
</style>
