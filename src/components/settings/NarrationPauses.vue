<script setup lang="ts">
import { ref, watch } from "vue";
import { LoaderCircle, Pilcrow, Square, Volume2 } from "@lucide/vue";
import { api } from "@/api";
import type { PauseSettings } from "@/api/types";
import UiButton from "@/components/ui/UiButton.vue";
import SettingsSection from "@/components/settings/SettingsSection.vue";
import SettingsNote from "@/components/settings/SettingsNote.vue";
import SettingsActions from "@/components/settings/SettingsActions.vue";
import { useAppSettings } from "@/composables/useAppSettings";
import { useSample } from "@/composables/useVoicePreview";

/** The silences narration leaves. Hear it speaks a few short paragraphs with the values on
 *  screen, saved or not. */
const { settings, error, message, save } = useAppSettings();
const { state: hearing, error: hearError, play, stop } = useSample();
const pauses = ref<PauseSettings>({ paragraphMs: 250, breakMs: 1200, headingMs: 700 });
watch(
  settings,
  (value) => {
    if (value) pauses.value = { ...value.pauses };
  },
  { once: true },
);
const SLIDERS: { key: keyof PauseSettings; label: string; name: string }[] = [
  { key: "paragraphMs", label: "Between paragraphs", name: "Pause between paragraphs" },
  { key: "breakMs", label: "At a scene break", name: "Pause at a scene break" },
  { key: "headingMs", label: "After a heading", name: "Pause after a heading" },
];
const shown = (ms: number) =>
  ms < 1000 ? `${ms} ms` : `${(ms / 1000).toLocaleString("en", { maximumFractionDigits: 2 })} s`;
function set(key: keyof PauseSettings, event: Event) {
  pauses.value = { ...pauses.value, [key]: Number((event.target as HTMLInputElement).value) };
}
function hear() {
  const asked = { ...pauses.value };
  void play(JSON.stringify(asked), () => api.previewPauses({ pauses: asked }));
}
</script>

<template>
  <form v-if="settings" class="pauses" @submit.prevent="save({ pauses: { ...pauses } })">
    <SettingsSection title="Pauses" :icon="Pilcrow">
      <SettingsNote
        >The silence narration leaves after each paragraph, at a scene break (<code>* * *</code> or a rule), and after a
        heading or a chapter's end. Changing them makes no audio again.</SettingsNote
      >
      <label v-for="slider in SLIDERS" :key="slider.key" class="pauses__row">
        <span class="pauses__label">{{ slider.label }}</span>
        <input
          class="pauses__range"
          type="range"
          min="0"
          max="3000"
          step="50"
          :aria-label="slider.name"
          :value="pauses[slider.key]"
          @input="set(slider.key, $event)"
        />
        <output class="pauses__value">{{ shown(pauses[slider.key]) }}</output>
      </label>
      <div class="pauses__hear">
        <UiButton v-if="hearing === 'idle'" size="sm" aria-label="Hear the pauses" @click="hear">
          <Volume2 :size="15" /> Hear it
        </UiButton>
        <UiButton v-else variant="primary" size="sm" aria-label="Stop hearing the pauses" @click="stop">
          <LoaderCircle v-if="hearing === 'loading'" :size="15" class="pauses__spin" /><Square
            v-else
            :size="13"
            fill="currentColor"
          />
          {{ hearing === "loading" ? "Preparing…" : "Stop" }}
        </UiButton>
        <span v-if="hearError" class="pauses__error" role="alert">Could not say it: {{ hearError }}</span>
        <span v-else class="pauses__hint">Three short paragraphs in your voice, with these pauses.</span>
      </div>
    </SettingsSection>
    <SettingsActions label="Save pauses" :message="message" :error="error" />
  </form>
</template>

<style scoped>
.pauses {
  display: grid;
  gap: var(--space-8);
}
.pauses__row {
  display: grid;
  grid-template-columns: minmax(110px, 160px) minmax(0, 1fr) 4.5em;
  align-items: center;
  gap: var(--space-6);
  font-size: 13px;
}
.pauses__label {
  color: var(--dim);
}
.pauses__range {
  width: 100%;
  accent-color: var(--accent);
}
.pauses__value {
  font-variant-numeric: tabular-nums;
  color: var(--dim);
  text-align: right;
}
.pauses__hear {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-4) var(--space-6);
}
.pauses__spin {
  animation: pauses-spin 1s linear infinite;
}
@keyframes pauses-spin {
  to {
    transform: rotate(1turn);
  }
}
.pauses__hint {
  font-size: 12px;
  color: var(--dim);
}
.pauses__error {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--accent);
  overflow-wrap: anywhere;
}
@media (max-width: 480px) {
  .pauses__row {
    grid-template-columns: 1fr auto;
  }
  .pauses__range {
    grid-column: 1 / -1;
    grid-row: 2;
  }
}
</style>
