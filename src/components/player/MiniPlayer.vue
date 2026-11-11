<script setup lang="ts">
import { computed } from "vue";
import { Pause, Play, SkipBack, SkipForward } from "@lucide/vue";
import CoverArt from "../CoverArt.vue";
import { usePlayerControls } from "../../composables/usePlayerControls";
import { useNavigationStore } from "../../stores/navigation";
import { useReaderStore } from "../../stores/reader";

/** The player as a floating pill on screens other than Now Playing; it slides up when
 *  something starts playing and away on the way into Now Playing. */
const controls = usePlayerControls();
const { player, active, book, chapterTitle, toggle, sleepLabel, error } = controls;
const reader = useReaderStore(), navigation = useNavigationStore();
const subtitle = computed(() => {
  const parts = [book.value?.title ?? ""];
  if (player.state.status === "buffering") parts.push("Buffering…");
  parts.push(`${player.state.speed}×`);
  if (sleepLabel.value) parts.push(`sleep ${sleepLabel.value}`);
  return parts.filter(Boolean).join(" · ");
});
async function openNowPlaying() {
  const id = controls.bookId.value;
  if (!id) return;
  if (reader.book?.id === id) navigation.view = "reader";
  else await reader.open(id);
}
</script>

<template>
  <Transition name="mini-player">
  <section v-if="book && navigation.view !== 'reader'" class="mini-player" aria-label="Now playing">
    <button type="button" class="mini-player__open" aria-label="Open Now Playing" title="Open Now Playing" @click="openNowPlaying">
      <CoverArt class="mini-player__art" :title="book.title" :url="book.coverUrl" :label="false" shape="square" />
      <span class="mini-player__text">
        <b class="mini-player__title">{{ chapterTitle || book.title }}</b>
        <small class="mini-player__subtitle"><template v-if="error">{{ book.title }} · <span class="mini-player__error" role="alert">{{ error }}</span></template><template v-else>{{ subtitle }}</template></small>
      </span>
    </button>
    <span class="mini-player__controls">
      <button type="button" class="mini-player__icon" aria-label="Previous chapter" title="Previous chapter" :disabled="!player.state.bookId" @click="player.skipChapter(-1)"><SkipBack :size="18" /></button>
      <button type="button" class="mini-player__play" :aria-label="active ? 'Pause' : 'Play'" @click="toggle"><Pause v-if="active" :size="18" fill="currentColor" /><Play v-else :size="18" fill="currentColor" /></button>
      <button type="button" class="mini-player__icon" aria-label="Next chapter" title="Next chapter" :disabled="!player.state.bookId" @click="player.skipChapter(1)"><SkipForward :size="18" /></button>
    </span>
    <span class="mini-player__edge" aria-hidden="true"><i :style="{ width: `${player.state.chapterFraction * 100}%` }" /></span>
  </section>
  </Transition>
</template>

<style scoped>
.mini-player { position: fixed; z-index: 5; left: 50%; bottom: 20px; transform: translateX(-50%); width: min(620px, calc(100% - 32px)); height: 66px; display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 14px; align-items: center; padding: 0 10px 0 8px; border-radius: 999px; background: var(--page); box-shadow: 0 18px 40px -16px var(--shadow), 0 0 0 1px var(--line); overflow: hidden; }
.mini-player-enter-active, .mini-player-leave-active { transition: translate var(--dur-2) var(--ease), opacity var(--dur-2) var(--ease); }
.mini-player-enter-from, .mini-player-leave-to { translate: 0 calc(100% + 24px); opacity: 0; }
.mini-player__open { display: grid; grid-template-columns: 50px minmax(0, 1fr); gap: 14px; align-items: center; min-width: 0; text-align: left; border-radius: 999px; }
.mini-player__art { width: 50px; border-radius: 50%; padding: 0; box-shadow: none; }
.mini-player__text { display: grid; min-width: 0; }
.mini-player__title { font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.mini-player__subtitle { color: var(--dim); font-weight: 500; font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-variant-numeric: tabular-nums; }
.mini-player__error { color: var(--accent); font-weight: 600; }
.mini-player__controls { display: flex; align-items: center; gap: 8px; }
.mini-player__icon { display: grid; place-items: center; width: 34px; height: 34px; border-radius: 50%; }
.mini-player__play { display: grid; place-items: center; width: 44px; height: 44px; border-radius: 50%; background: var(--accent); color: var(--on-accent); }
.mini-player__edge { position: absolute; left: 0; right: 0; bottom: 0; height: 3px; background: var(--line); }
.mini-player__edge i { display: block; height: 100%; background: var(--accent); }
@media (max-width: 480px) { .mini-player__icon { display: none; } }
</style>
