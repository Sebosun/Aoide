<script setup lang="ts">
import { computed } from "vue";
import { Bookmark, Pause, Play, SkipBack, SkipForward, SlidersHorizontal, Square } from "@lucide/vue";
import PlayerScrubber from "./PlayerScrubber.vue";
import SpeedPill from "./SpeedPill.vue";
import SleepPill from "./SleepPill.vue";
import { usePlayerControls } from "../../composables/usePlayerControls";

/** The full two-row transport of Lines mode: scrubber, then speed, sleep and status;
 *  previous, a 66px play button and next; bookmark, book tools and stop. */
const emit = defineEmits<{ tools: [] }>();
const controls = usePlayerControls();
const { player, active, timeLeft, ahead, percent, toggle, seek, step, bookmark, status, error } = controls;
const canPlay = computed(() => Boolean(controls.bookId.value));
</script>

<template>
  <section class="transport" aria-label="Player">
    <PlayerScrubber :played="player.state.chapterFraction" :ahead="ahead" :start="`${percent}%`" :end="timeLeft" :disabled="!player.state.bookId" @seek="seek" @step="step" />
    <div class="transport__row">
      <div class="transport__side">
        <SpeedPill />
        <SleepPill />
        <p class="transport__status" aria-live="polite"><span v-if="error" class="transport__error" role="alert">{{ error }}</span><span>{{ status }}</span></p>
      </div>
      <div class="transport__main">
        <button type="button" class="transport__icon" aria-label="Previous chapter" title="Previous chapter" :disabled="!player.state.bookId" @click="player.skipChapter(-1)"><SkipBack :size="22" /></button>
        <button type="button" class="transport__play" :aria-label="active ? 'Pause' : 'Play'" :disabled="!canPlay" @click="toggle"><Pause v-if="active" :size="24" fill="currentColor" /><Play v-else :size="24" fill="currentColor" /></button>
        <button type="button" class="transport__icon" aria-label="Next chapter" title="Next chapter" :disabled="!player.state.bookId" @click="player.skipChapter(1)"><SkipForward :size="22" /></button>
      </div>
      <div class="transport__side transport__side--end">
        <button type="button" class="transport__round" aria-label="Add bookmark" title="Bookmark this sentence" :disabled="!canPlay" @click="bookmark"><Bookmark :size="16" /></button>
        <button type="button" class="transport__round" aria-label="Book tools" aria-haspopup="dialog" title="Bookmarks, search, voice and details" @click="emit('tools')"><SlidersHorizontal :size="16" /></button>
        <button type="button" class="transport__round" aria-label="Stop" title="Stop" :disabled="!player.state.bookId" @click="player.stop"><Square :size="15" /></button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.transport { display: grid; gap: 14px; align-content: center; min-height: 156px; padding: 14px 0; border-top: 1px solid var(--line); }
.transport__row { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; gap: 16px; }
.transport__side { display: flex; align-items: center; gap: 8px; min-width: 0; }
.transport__side--end { justify-content: flex-end; }
.transport__status { min-width: 0; font-size: 12px; font-weight: 600; color: var(--dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.transport__error { margin-right: 8px; color: var(--accent); }
.transport__main { display: flex; align-items: center; gap: 22px; }
.transport__icon { display: grid; place-items: center; width: 40px; height: 40px; border-radius: 50%; }
.transport__play { display: grid; place-items: center; width: 66px; height: 66px; border-radius: 50%; background: var(--accent); color: var(--on-accent); box-shadow: 0 12px 30px -10px var(--accent); }
.transport__round { display: grid; place-items: center; width: 36px; height: 36px; border-radius: 50%; background: var(--glass); }
@media (max-width: 760px) {
  .transport__row { grid-template-columns: 1fr; justify-items: center; }
  .transport__side, .transport__side--end { justify-content: center; flex-wrap: wrap; }
  .transport__main { order: -1; }
}
</style>
