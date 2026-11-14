<script setup lang="ts">
import { computed } from "vue";
import { Bookmark, Headphones, Pause, Play, SkipBack, SkipForward, SlidersHorizontal, Square } from "@lucide/vue";
import CoverArt from "../CoverArt.vue";
import PlayerScrubber from "./PlayerScrubber.vue";
import SpeedPill from "./SpeedPill.vue";
import SleepPill from "./SleepPill.vue";
import VolumeControl from "./VolumeControl.vue";
import { usePlayerControls } from "../../composables/usePlayerControls";

/** One-row transport docked under the page (Sheet reader). */
const emit = defineEmits<{ tools: [] }>();
const controls = usePlayerControls();
const { player, active, recording, book, timeLeft, ahead, percent, toggle, seek, step, bookmark, status, detail, error } = controls;
const canPlay = computed(() => Boolean(controls.bookId.value));
</script>

<template>
  <section class="dock" aria-label="Player">
    <div class="dock__main">
      <CoverArt v-if="book" class="dock__thumb" :title="book.title" :url="book.coverUrl" :label="false" shape="square" />
      <button type="button" class="dock__icon" aria-label="Previous chapter" title="Previous chapter" :disabled="!player.state.bookId" @click="player.skipChapter(-1)"><SkipBack :size="20" /></button>
      <button type="button" class="dock__play" :aria-label="active ? 'Pause' : 'Play'" :disabled="!canPlay" @click="toggle"><Pause v-if="active" :size="22" fill="currentColor" /><Play v-else :size="22" fill="currentColor" /></button>
      <button type="button" class="dock__icon" aria-label="Next chapter" title="Next chapter" :disabled="!player.state.bookId" @click="player.skipChapter(1)"><SkipForward :size="20" /></button>
    </div>
    <div class="dock__middle">
      <PlayerScrubber :played="player.state.chapterFraction" :ahead="ahead" :start="`${percent}%`" :end="timeLeft" :detail="detail" :disabled="!player.state.bookId" @seek="seek" @step="step" />
      <p class="dock__status" aria-live="polite"><span v-if="recording" class="dock__source" title="The narrator's recording is playing"><Headphones :size="12" aria-hidden="true" /> Recording</span><span v-if="error" class="dock__error" role="alert">{{ error }}</span><span v-if="status">{{ status }}</span></p>
    </div>
    <div class="dock__end">
      <SpeedPill />
      <SleepPill />
      <VolumeControl />
      <span class="dock__divider" aria-hidden="true" />
      <button type="button" class="dock__round" aria-label="Add bookmark" title="Bookmark this sentence" :disabled="!canPlay" @click="bookmark"><Bookmark :size="16" /></button>
      <button type="button" class="dock__round" aria-label="Book tools" aria-haspopup="dialog" title="Bookmarks, search, narration and details" @click="emit('tools')"><SlidersHorizontal :size="16" /></button>
      <button type="button" class="dock__round" aria-label="Stop" title="Stop" :disabled="!player.state.bookId" @click="player.stop"><Square :size="15" /></button>
    </div>
  </section>
</template>

<style scoped>
.dock { min-height: 78px; display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 30px; padding: 10px 28px; border-top: 1px solid var(--line); }
.dock__main { display: flex; align-items: center; gap: 18px; }
.dock__thumb { display: none; width: 46px; border-radius: 10px; box-shadow: none; }
.dock__icon { display: grid; place-items: center; width: 34px; height: 34px; border-radius: 50%; }
.dock__play { display: grid; place-items: center; width: 54px; height: 54px; border-radius: 50%; background: var(--accent); color: var(--on-accent); box-shadow: 0 12px 30px -12px var(--accent); }
.dock__middle { display: grid; gap: 6px; min-width: 0; }
.dock__status { display: flex; gap: 10px; font-size: 12px; font-weight: 600; color: var(--dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.dock__status span { overflow: hidden; text-overflow: ellipsis; }
.dock__status:empty { display: none; }
.dock__error { color: var(--accent); flex-shrink: 0; max-width: 60%; }
.dock__source { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; color: var(--accent); }
.dock__end { display: flex; align-items: center; gap: 8px; }
.dock__end > * { flex: none; }
.dock__divider { width: 1px; height: 20px; margin: 0 2px; background: var(--line); }
.dock__round { display: grid; place-items: center; width: 34px; height: 34px; border-radius: 50%; background: var(--glass); }
@media (max-width: 999px) { .dock__thumb { display: flex; } }
@media (max-width: 900px) {
  .dock { grid-template-columns: auto minmax(0, 1fr); gap: 10px 18px; padding: 12px 16px; }
  .dock__middle { grid-column: 1 / -1; grid-row: 1; }
  .dock__main { gap: 10px; }
  .dock__end { justify-content: flex-end; gap: 6px; }
}
@media (max-width: 520px) {
  .dock { grid-template-columns: 1fr; }
  .dock__thumb { display: none; }
  .dock__main, .dock__end { justify-content: center; }
}
</style>
