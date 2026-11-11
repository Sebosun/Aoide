<script setup lang="ts">
import { computed } from "vue";
import { Volume1, Volume2, VolumeX } from "@lucide/vue";
import { usePlayerStore } from "../../stores/player";

/** A mute button and a volume slider, in percent. */
const player = usePlayerStore();
const percent = computed(() => Math.round(player.state.volume * 100));
const muted = computed(() => percent.value === 0);
const icon = computed(() => (muted.value ? VolumeX : percent.value < 50 ? Volume1 : Volume2));
const input = (event: Event) => player.setVolume(Number((event.target as HTMLInputElement).value) / 100);
</script>

<template>
  <div class="volume">
    <button type="button" class="volume__mute" :aria-label="muted ? 'Unmute' : 'Mute'" :title="muted ? 'Unmute' : 'Mute'" @click="player.toggleMute()"><component :is="icon" :size="16" /></button>
    <input class="volume__range" type="range" min="0" max="100" step="1" aria-label="Volume" :title="`Volume ${percent}%`" :value="percent" @input="input" />
  </div>
</template>

<style scoped>
.volume { display: flex; align-items: center; gap: 4px; flex: none; height: 34px; padding: 0 12px 0 3px; border-radius: 999px; background: var(--glass); color: var(--dim); }
.volume__mute { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 50%; color: inherit; }
.volume__mute:hover { color: var(--ink); }
.volume__range { width: 72px; height: 4px; margin: 0; accent-color: var(--accent); cursor: pointer; }
</style>
