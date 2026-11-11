<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { Volume1, Volume2, VolumeX } from "@lucide/vue";
import { usePlayerStore } from "../../stores/player";
import { usePopover } from "../../composables/usePopover";

/** A speaker button that opens a small panel: mute and a volume slider, in percent. */
const player = usePlayerStore();
const percent = computed(() => Math.round(player.state.volume * 100));
const muted = computed(() => percent.value === 0);
const icon = computed(() => (muted.value ? VolumeX : percent.value < 50 ? Volume1 : Volume2));
const input = (event: Event) => player.setVolume(Number((event.target as HTMLInputElement).value) / 100);

const open = ref(false), button = ref<HTMLElement>(), panel = ref<HTMLElement>();
const { style, above } = usePopover(button, panel, open, { align: "start", maxHeight: 60 });
function close(focus = false) { open.value = false; if (focus) button.value?.focus(); }
function outside(event: PointerEvent) {
  const target = event.target as Node;
  if (!button.value?.contains(target) && !panel.value?.contains(target)) close();
}
watch(open, value => {
  document.removeEventListener("pointerdown", outside, true);
  if (value) document.addEventListener("pointerdown", outside, true);
});
onUnmounted(() => document.removeEventListener("pointerdown", outside, true));
</script>

<template>
  <button ref="button" type="button" class="volume" :class="{ 'volume--muted': muted }" aria-label="Volume" aria-haspopup="dialog" :aria-expanded="open" :title="`Volume ${percent}%`" @click="open = !open"><component :is="icon" :size="16" /></button>
  <Teleport to="body">
    <div v-if="open" ref="panel" class="volume__panel" :class="{ 'volume__panel--above': above }" role="dialog" aria-label="Volume" :style="{ ...style, minWidth: undefined }" @keydown.esc.stop="close(true)">
      <button type="button" class="volume__mute" :aria-label="muted ? 'Unmute' : 'Mute'" :title="muted ? 'Unmute' : 'Mute'" @click="player.toggleMute()"><component :is="icon" :size="16" /></button>
      <input class="volume__range" type="range" min="0" max="100" step="1" aria-label="Volume" :value="percent" @input="input" />
      <span class="volume__value">{{ percent }}%</span>
    </div>
  </Teleport>
</template>

<style scoped>
.volume { display: grid; place-items: center; flex: none; width: 34px; height: 34px; border-radius: 50%; background: var(--glass); color: var(--dim); }
.volume:hover, .volume[aria-expanded="true"] { color: var(--ink); }
.volume--muted { color: var(--accent); }
.volume__panel { position: fixed; z-index: 40; display: flex; align-items: center; gap: 8px; padding: 6px 12px 6px 6px; border-radius: 999px; background: var(--page); color: var(--ink); box-shadow: 0 18px 40px -16px var(--shadow), 0 0 0 1px var(--line); font: 700 12px/1 var(--font-ui); font-variant-numeric: tabular-nums; animation: volume-in var(--dur-1) var(--ease); }
@keyframes volume-in { from { opacity: 0; transform: scale(.97); } }
.volume__mute { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 50%; color: var(--dim); }
.volume__mute:hover { background: var(--wash); color: var(--ink); }
.volume__range { width: 120px; margin: 0; accent-color: var(--accent); cursor: pointer; }
.volume__value { min-width: 4ch; text-align: right; color: var(--dim); }
</style>
