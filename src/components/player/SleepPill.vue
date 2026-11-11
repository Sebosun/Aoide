<script setup lang="ts">
import { computed } from "vue";
import { Moon } from "@lucide/vue";
import UiSelect from "../ui/UiSelect.vue";
import { SLEEP_MINUTES, usePlayerControls } from "../../composables/usePlayerControls";
const { sleepChoice, setSleep, sleepLabel, player } = usePlayerControls();
const options = computed(() => [
  { value: "off", label: "Off" },
  ...SLEEP_MINUTES.map(m => ({ value: String(m), label: `${m} min` })),
  { value: "chapter", label: "End of chapter" },
  ...(sleepChoice.value === "" ? [{ value: "", label: sleepLabel.value, disabled: true }] : []),
]);
</script>

<template>
  <UiSelect class="sleep-pill" :class="{ 'sleep-pill--on': player.state.sleep }" variant="pill" label="Sleep timer" title="Sleep timer" :options="options" :model-value="sleepChoice" @update:model-value="setSleep">
    <template #value><Moon :size="15" aria-hidden="true" /><span>{{ sleepLabel || "Sleep" }}</span></template>
  </UiSelect>
</template>

<style scoped>
.sleep-pill { gap: 6px; color: var(--dim); }
.sleep-pill--on { color: var(--ink); }
.sleep-pill--on svg { color: var(--accent); }
</style>
