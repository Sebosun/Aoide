<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from "vue";
import { SPEEDS } from "@/composables/usePlayerControls";
import { usePopover } from "@/composables/usePopover";
import { usePlayerStore } from "@/stores/player";

/** The speed pill: opens a small panel with the speeds and a switch that keeps the speed for
 *  the book playing alone. A dot on the pill says the book has its own. */
const player = usePlayerStore();
const own = computed(() => player.state.speedScope === "book");
const title = computed(() => (own.value ? "Playback speed, only for this book" : "Playback speed"));

const open = ref(false),
  button = ref<HTMLElement>(),
  panel = ref<HTMLElement>();
const { style, above } = usePopover(button, panel, open, { align: "start", maxHeight: 200 });
function close(focus = false) {
  open.value = false;
  if (focus) button.value?.focus();
}
function pick(speed: number) {
  void player.setSpeed(speed);
  close(true);
}
function outside(event: PointerEvent) {
  const target = event.target as Node;
  if (!button.value?.contains(target) && !panel.value?.contains(target)) close();
}
watch(open, async (value) => {
  document.removeEventListener("pointerdown", outside, true);
  if (!value) return;
  document.addEventListener("pointerdown", outside, true);
  // The panel takes the focus, so Escape closes it and Tab goes on into the speeds.
  await nextTick();
  panel.value?.focus();
});
onUnmounted(() => document.removeEventListener("pointerdown", outside, true));
</script>

<template>
  <button
    ref="button"
    type="button"
    class="speed-pill"
    aria-label="Playback speed"
    aria-haspopup="dialog"
    :aria-expanded="open"
    :title="title"
    @click="open ? close() : (open = true)"
  >
    {{ player.state.speed }}×<span v-if="own" class="speed-pill__dot" aria-hidden="true"></span>
    <!-- Inside the button, so the pill is the component's one root (its parent styles it). -->
    <Teleport to="body">
      <div
        v-if="open"
        ref="panel"
        class="speed-pill__panel"
        :class="{ 'speed-pill__panel--above': above }"
        role="dialog"
        aria-label="Playback speed"
        tabindex="-1"
        :style="{ ...style, minWidth: undefined }"
        @keydown.esc.stop="close(true)"
      >
        <div class="speed-pill__speeds" role="group" aria-label="Speeds">
          <button
            v-for="speed in SPEEDS"
            :key="speed"
            type="button"
            class="speed-pill__speed"
            :class="{ 'speed-pill__speed--on': speed === player.state.speed }"
            :aria-pressed="speed === player.state.speed"
            @click="pick(speed)"
          >
            {{ speed }}×
          </button>
        </div>
        <label class="speed-pill__own"
          ><input
            type="checkbox"
            role="switch"
            aria-label="Only for this book"
            :checked="own"
            :disabled="!player.state.bookId"
            @change="player.setBookSpeed(($event.target as HTMLInputElement).checked)"
          />
          Only for this book</label
        >
      </div>
    </Teleport>
  </button>
</template>

<style scoped>
.speed-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  height: 34px;
  padding: 0 var(--space-6);
  border-radius: var(--radius-pill);
  background: var(--glass);
  font-weight: 700;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.speed-pill__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
}
.speed-pill__panel {
  position: fixed;
  z-index: 40;
  display: grid;
  gap: var(--space-4);
  padding: var(--space-4);
  border-radius: var(--radius-md);
  background: var(--page);
  color: var(--ink);
  box-shadow:
    0 18px 40px -16px var(--shadow),
    0 0 0 1px var(--line);
  font: 600 13px/1.3 var(--font-ui);
  transform-origin: top center;
  animation: speed-in var(--dur-1) var(--ease);
}
.speed-pill__panel:focus-visible {
  outline: none;
}
.speed-pill__panel--above {
  transform-origin: bottom center;
}
@keyframes speed-in {
  from {
    opacity: 0;
    transform: scale(0.97);
  }
}
.speed-pill__speeds {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: var(--space-1);
}
.speed-pill__speed {
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-sm);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  text-align: center;
  color: var(--dim);
}
.speed-pill__speed:hover {
  background: var(--wash);
  color: var(--ink);
}
.speed-pill__speed--on,
.speed-pill__speed--on:hover {
  background: var(--ink);
  color: var(--ground);
}
.speed-pill__own {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-2) var(--space-1);
  border-top: 1px solid var(--line);
  white-space: nowrap;
}
.speed-pill__own:has(input:disabled) {
  color: var(--faint);
}
</style>
