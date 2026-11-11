<script setup lang="ts">
import UiSelect from "../ui/UiSelect.vue";
import { SPEEDS } from "../../composables/usePlayerControls";
import { usePlayerStore } from "../../stores/player";
const player = usePlayerStore();
const options = SPEEDS.map(speed => ({ value: speed, label: `${speed}×` }));
</script>

<template>
  <UiSelect class="speed-pill" variant="pill" label="Playback speed" title="Playback speed" :options="options" :model-value="player.state.speed" @update:model-value="player.setSpeed">
    <template #value>{{ player.state.speed }}×</template>
  </UiSelect>
</template>
