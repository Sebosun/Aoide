<script setup lang="ts">
import { computed } from "vue";
import { Moon } from "@lucide/vue";
import UiSelect from "../ui/UiSelect.vue";
import { SLEEP_MINUTES, usePlayerControls } from "../../composables/usePlayerControls";
const { sleepChoice, setSleep, sleepLabel, player } = usePlayerControls();
/** Only a running countdown is spelled out; otherwise the moon (lit while set) says enough. */
const countdown = computed(() => player.state.sleep?.remainingMs != null);
const title = computed(() => (player.state.sleep ? `Sleep timer: ${sleepLabel.value === "End of chapter" ? "end of chapter" : sleepLabel.value}` : "Sleep timer"));
const options = computed(() => [
  { value: "off", label: "Off" },
  ...SLEEP_MINUTES.map(m => ({ value: String(m), label: `${m} min` })),
  { value: "chapter", label: "End of chapter" },
  ...(sleepChoice.value === "" ? [{ value: "", label: sleepLabel.value, disabled: true }] : []),
]);
</script>

<template>
  <UiSelect class="sleep-pill" :class="{ 'sleep-pill--on': player.state.sleep }" variant="pill" label="Sleep timer" :title="title" :options="options" :model-value="sleepChoice" @update:model-value="setSleep">
    <template #value><Moon :size="15" aria-hidden="true" /><span v-if="countdown">{{ sleepLabel }}</span></template>
  </UiSelect>
</template>

<style scoped>
.sleep-pill { gap: 6px; min-width: 34px; justify-content: center; color: var(--dim); }
.sleep-pill:not(:has(span)) { padding: 0; width: 34px; }
.sleep-pill--on { color: var(--ink); }
.sleep-pill--on svg { color: var(--accent); }
</style>
