<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from "vue";
import EngineQueue from "./EngineQueue.vue";
import { usePopover } from "../../composables/usePopover";
import { useLibraryStore } from "../../stores/library";
import { usePlayerStore } from "../../stores/player";
import { useRecordingsStore } from "../../stores/recordings";

/** The speech engine in the sidebar, only while it matters: books are being narrated ahead,
 *  recordings are syncing, or the engine has failed. Clicking it shows what the engine is
 *  working on; Escape, a click outside or opening something from the panel closes it. */
const player = usePlayerStore(),
  library = useLibraryStore(),
  recordings = useRecordingsStore();
const card = ref<HTMLElement>(),
  panel = ref<HTMLElement>(),
  open = ref(false);
const { style } = usePopover(card, panel, open, { align: "start", maxHeight: 520 });

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
const queue = computed(() =>
  library.books
    .filter((b) => b.prepare && b.prepare.state !== "done")
    .sort((a, b) => (a.prepare!.position ?? Infinity) - (b.prepare!.position ?? Infinity)),
);
const failed = computed(() => player.engine.state === "error");
const shown = computed(() => failed.value || queue.value.length > 0 || recordings.syncing.length > 0);
const title = computed(() => {
  if (failed.value) return `${player.engine.profileName} has a problem`;
  if (queue.value.length) return `Narrating ${plural(queue.value.length, "book")}`;
  return `Syncing ${plural(recordings.syncing.length, "recording")}`;
});
/** The book being narrated now: the first in the queue. */
const current = computed(() => {
  const book = queue.value[0];
  if (!book?.prepare) return null;
  const { done, total } = book.prepare;
  return { title: book.title, fraction: total ? done / total : 0, count: total ? `${done} / ${total}` : "" };
});
const line = computed(() => {
  if (failed.value) return player.engine.detail ?? "Check the speech engine in Settings";
  if (current.value) return [current.value.title, current.value.count].filter(Boolean).join(" · ");
  const first = recordings.syncing[0];
  return first ? (library.books.find((b) => b.id === first.bookId)?.title ?? "") : "";
});

function close(refocus = false) {
  open.value = false;
  if (refocus) card.value?.focus();
}
watch(shown, (value) => {
  if (!value) close();
});
function outside(event: PointerEvent) {
  const target = event.target as Node;
  if (!panel.value?.contains(target) && !card.value?.contains(target)) close();
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.stopPropagation();
    close(true);
  }
}
watch(open, async (value) => {
  document.removeEventListener("pointerdown", outside, true);
  if (!value) return;
  document.addEventListener("pointerdown", outside, true);
  await nextTick();
  panel.value?.focus();
});
onUnmounted(() => document.removeEventListener("pointerdown", outside, true));
</script>

<template>
  <button
    v-if="shown"
    ref="card"
    type="button"
    class="engine-card"
    :class="{ 'engine-card--error': failed }"
    aria-haspopup="dialog"
    :aria-expanded="open"
    :title="player.engine.detail ?? player.engine.profileName"
    @click="open = !open"
  >
    <span class="engine-card__title" role="status">{{ title }}</span>
    <span v-if="current && !failed" class="engine-card__track" aria-hidden="true"
      ><i :style="{ width: `${current.fraction * 100}%` }"
    /></span>
    <span class="engine-card__line" :role="failed ? 'alert' : undefined">{{ line }}</span>
  </button>
  <Teleport to="body">
    <div
      v-if="open"
      ref="panel"
      class="engine-card__panel"
      role="dialog"
      aria-label="Speech queue"
      tabindex="-1"
      :style="style"
      @keydown="onKeydown"
    >
      <EngineQueue @close="close()" />
    </div>
  </Teleport>
</template>

<style scoped>
.engine-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 14px;
  border-radius: 16px;
  background: var(--page);
  text-align: left;
}
.engine-card:hover,
.engine-card[aria-expanded="true"] {
  box-shadow: 0 0 0 1px var(--line);
}
.engine-card__title {
  font-weight: 700;
  font-size: 13px;
}
.engine-card--error .engine-card__title {
  color: var(--accent);
}
.engine-card__track {
  height: 3px;
  border-radius: 2px;
  background: var(--wash);
}
.engine-card__track i {
  display: block;
  height: 100%;
  border-radius: 2px;
  background: var(--accent);
}
.engine-card__line {
  font-size: 12px;
  color: var(--dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}
.engine-card__panel {
  position: fixed;
  z-index: 40;
  width: 340px;
  max-width: calc(100vw - 16px);
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 14px 16px;
  border-radius: 16px;
  background: var(--page);
  color: var(--ink);
  font-family: var(--font-ui);
  box-shadow:
    0 18px 40px -16px var(--shadow),
    0 0 0 1px var(--line);
  animation: engine-card-in var(--dur-1) var(--ease);
}
.engine-card__panel:focus {
  outline: none;
}
@keyframes engine-card-in {
  from {
    opacity: 0;
    transform: scale(0.97);
  }
}
</style>
