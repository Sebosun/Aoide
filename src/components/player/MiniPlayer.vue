<script setup lang="ts">
import { computed } from "vue";
import { Pause, Play } from "@lucide/vue";
import CoverArt from "@/components/CoverArt.vue";
import SleepPill from "@/components/player/SleepPill.vue";
import { usePlayerControls } from "@/composables/usePlayerControls";
import { useNavigationStore } from "@/stores/navigation";
import { useReaderStore } from "@/stores/reader";

/** What is playing, as a card at the foot of the sidebar: cover, title, a state line and
 *  play/pause. It shows while something is loaded in the player (the sidebar, and so the card,
 *  is not shown in the reader, which has the full transport); clicking it opens the reader on
 *  that book. The sleep timer's menu (the T key) opens over the card: its button lies unseen
 *  across it, out of the way of the pointer and of Tab. */
const controls = usePlayerControls();
const { player, active, book, chapterTitle, timeLeft, toggle, sleepLabel, error } = controls;
const reader = useReaderStore(),
  navigation = useNavigationStore();
const loaded = computed(() => Boolean(player.state.bookId));
const state = computed(() => {
  const parts = [
    player.state.status === "buffering" ? "Buffering…" : active.value ? "Playing" : "Paused",
    active.value ? chapterTitle.value : timeLeft.value,
  ];
  if (player.state.speed !== 1) parts.push(`${player.state.speed}×`);
  if (sleepLabel.value) parts.push(`sleep ${sleepLabel.value}`);
  return parts.filter(Boolean).join(" · ");
});
async function openReader() {
  const id = controls.bookId.value;
  if (!id) return;
  if (reader.book?.id === id) navigation.view = "reader";
  else await reader.open(id);
}
</script>

<template>
  <section v-if="book && loaded" class="mini-player" aria-label="Now playing">
    <button
      type="button"
      class="mini-player__open"
      :aria-label="`Open ${book.title} in the reader`"
      :title="`Open ${book.title}`"
      @click="openReader"
    >
      <CoverArt class="mini-player__art" :title="book.title" :url="book.coverUrl" :label="false" shape="square" />
      <span class="mini-player__text">
        <b class="mini-player__title">{{ book.title }}</b>
        <small class="mini-player__state"
          ><span v-if="error" class="mini-player__error" role="alert">{{ error }}</span
          ><template v-else>{{ state }}</template></small
        >
      </span>
    </button>
    <button
      type="button"
      class="mini-player__play"
      :aria-label="active ? `Pause ${book.title}` : `Resume ${book.title}`"
      @click="toggle"
    >
      <Pause v-if="active" :size="15" fill="currentColor" /><Play v-else :size="15" fill="currentColor" />
    </button>
    <SleepPill class="mini-player__sleep" tabindex="-1" />
    <span class="mini-player__edge" aria-hidden="true"
      ><i :style="{ width: `${player.state.chapterFraction * 100}%` }"
    /></span>
  </section>
</template>

<style scoped>
.mini-player {
  position: relative;
  display: flex;
  align-items: center;
  gap: var(--space-5);
  padding: var(--space-5);
  border-radius: var(--radius-lg);
  background: var(--page);
  overflow: hidden;
}
.mini-player__open {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: var(--space-5);
  text-align: left;
  border-radius: var(--radius-sm);
}
.mini-player__art {
  width: 40px;
  flex: none;
  border-radius: var(--radius-sm);
  padding: 0;
  box-shadow: none;
}
.mini-player__text {
  display: grid;
  min-width: 0;
}
.mini-player__title {
  font-weight: 700;
  font-size: 13px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.mini-player__state {
  color: var(--dim);
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-variant-numeric: tabular-nums;
}
.mini-player__error {
  color: var(--accent);
  font-weight: 600;
}
.mini-player__play {
  flex: none;
  display: grid;
  place-items: center;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: var(--accent);
  color: var(--on-accent);
}
/* Over the pill's own size. */
.mini-player .mini-player__sleep {
  position: absolute;
  inset: 0;
  width: auto;
  height: auto;
  opacity: 0;
  pointer-events: none;
}
.mini-player__edge {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
}
.mini-player__edge i {
  display: block;
  height: 100%;
  background: var(--accent);
}
</style>
