<script setup lang="ts">
import { nextTick, onUnmounted, ref, watch } from "vue";
import EngineQueue from "./EngineQueue.vue";
import { usePopover } from "../../composables/usePopover";
import { usePlayerStore } from "../../stores/player";

/** The engine's state in the top bar. Clicking it shows what the engine is working on.
 *  Escape, a click outside or opening something from the panel closes it. */
const player = usePlayerStore();
const pill = ref<HTMLElement>(), panel = ref<HTMLElement>(), open = ref(false);
const { style } = usePopover(pill, panel, open, { align: "end", maxHeight: 520 });

function close(refocus = false) {
  open.value = false;
  if (refocus) pill.value?.focus();
}
function outside(event: PointerEvent) {
  const target = event.target as Node;
  if (!panel.value?.contains(target) && !pill.value?.contains(target)) close();
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") { event.stopPropagation(); close(true); }
}
watch(open, async value => {
  document.removeEventListener("pointerdown", outside, true);
  if (!value) return;
  document.addEventListener("pointerdown", outside, true);
  await nextTick();
  panel.value?.focus();
});
onUnmounted(() => document.removeEventListener("pointerdown", outside, true));
</script>

<template>
  <button ref="pill" type="button" class="engine-pill" :class="`engine-pill--${player.engine.state}`" aria-haspopup="dialog" :aria-expanded="open" :title="player.engine.detail ?? player.engine.profileName" @click="open = !open">
    <i class="engine-pill__dot" aria-hidden="true" />
    <span class="engine-pill__label" role="status">{{ player.engine.profileName }} <span class="engine-pill__state">{{ player.engine.state }}</span></span>
  </button>
  <Teleport to="body">
    <div v-if="open" ref="panel" class="engine-pill__panel" role="dialog" aria-label="Speech queue" tabindex="-1" :style="style" @keydown="onKeydown">
      <EngineQueue @close="close()" />
    </div>
  </Teleport>
</template>

<style scoped>
.engine-pill { display: inline-flex; align-items: center; gap: 7px; height: 32px; padding: 0 12px; border-radius: 999px; background: var(--glass); font-size: 12px; font-weight: 600; white-space: nowrap; min-width: 0; }
.engine-pill:hover, .engine-pill[aria-expanded="true"] { background: var(--wash); }
.engine-pill__label { overflow: hidden; text-overflow: ellipsis; }
.engine-pill__state { color: var(--dim); }
.engine-pill__dot { width: 7px; height: 7px; border-radius: 50%; background: var(--faint); flex-shrink: 0; }
.engine-pill--ready .engine-pill__dot { background: var(--ok); }
.engine-pill--busy .engine-pill__dot { background: var(--accent); }
.engine-pill--loading .engine-pill__dot { background: var(--warn); }
.engine-pill--error .engine-pill__dot { background: var(--accent); }
.engine-pill--error .engine-pill__state { color: var(--accent); }
.engine-pill__panel { position: fixed; z-index: 40; width: 340px; max-width: calc(100vw - 16px); overflow-y: auto; overscroll-behavior: contain; padding: 14px 16px; border-radius: 16px; background: var(--page); color: var(--ink); font-family: var(--font-ui); box-shadow: 0 18px 40px -16px var(--shadow), 0 0 0 1px var(--line); transform-origin: top right; animation: engine-pill-in var(--dur-1) var(--ease); }
.engine-pill__panel:focus { outline: none; }
@keyframes engine-pill-in { from { opacity: 0; transform: scale(.97); } }
@media (prefers-reduced-motion: no-preference) {
  .engine-pill--busy .engine-pill__dot, .engine-pill--loading .engine-pill__dot { animation: engine-pill-pulse 1.4s ease-in-out infinite; }
}
@keyframes engine-pill-pulse { 50% { opacity: .35; } }
@media (max-width: 760px) { .engine-pill { padding: 0 11px; } .engine-pill__label { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); } }
</style>
