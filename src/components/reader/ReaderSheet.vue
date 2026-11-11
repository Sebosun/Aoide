<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { LocateFixed } from "@lucide/vue";
import ChapterText from "./ChapterText.vue";
import BookMargin from "./BookMargin.vue";
import ReaderTools from "./ReaderTools.vue";
import PlayerDock from "../player/PlayerDock.vue";
import { useFollow } from "../../composables/useFollow";
import { useMediaQuery } from "../../composables/useMediaQuery";
import { usePlayerControls } from "../../composables/usePlayerControls";
import { formatDuration, speechMs, wordsLeft } from "../../estimate";
import { useLibraryStore } from "../../stores/library";
import { useReaderStore } from "../../stores/reader";

/** Page mode: a full-width sheet with the text in a 640px column, the book in the left
 *  margin, footnotes and tools in the right margin, and the transport docked underneath. */
const emit = defineEmits<{ tools: [tab?: "details"] }>();
const reader = useReaderStore(), library = useLibraryStore();
const { player, timeLeftMs } = usePlayerControls();
const wide = useMediaQuery("(min-width: 1000px)");
const scroller = ref<HTMLElement>();

const playingHere = computed(() => Boolean(reader.book) && player.state.bookId === reader.book?.id);
const onPage = computed(() => playingHere.value && player.state.chapter === reader.currentChapter);
const { following, follow, backToPlayhead, listeners } = useFollow(scroller, () => playingHere.value && Boolean(player.state.segmentId));
watch(() => [player.state.segmentId, player.state.status, reader.chapter], async () => {
  if (onPage.value && player.state.status === "playing") { await nextTick(); follow(); }
}, { immediate: true, flush: "post" });

const coverUrl = computed(() => library.books.find(b => b.id === reader.book?.id)?.coverUrl ?? null);
const chapterLeft = computed(() => {
  if (onPage.value && timeLeftMs.value !== null) return `${formatDuration(timeLeftMs.value)} left`;
  return reader.chapter ? `${formatDuration(speechMs(wordsLeft(reader.chapter, null, null), player.state.speed))} left` : "";
});
async function play(segmentId: string) { if (reader.book) await player.play(reader.book.id, reader.currentChapter, segmentId); }
const jump = (index: number) => reader.startChapter(index);
</script>

<template>
  <section v-if="reader.book" class="sheet">
    <div ref="scroller" class="sheet__scroll" @scroll.passive="listeners.onScroll" @scrollend="listeners.onScrollend" @wheel.passive="listeners.onWheel" @touchmove.passive="listeners.onTouchmove" @keydown="listeners.onKeydown">
      <div class="sheet__grid">
        <BookMargin class="sheet__margin" :book="reader.book" :cover-url="coverUrl" :chapter="reader.currentChapter" :playing-chapter="playingHere ? player.state.chapter : null" :time-left="chapterLeft" @chapter="jump" @details="emit('tools', 'details')" />
        <ReaderTools class="sheet__tools" />
        <p v-if="reader.error" class="sheet__error" role="alert">{{ reader.error }}</p>
        <ChapterText v-if="reader.chapter" class="sheet__text" :class="{ 'sheet__text--margins': wide }" :chapter="reader.chapter" :current-segment="onPage ? player.state.segmentId : null" :current-word="onPage ? player.state.wordIndex : null" :margin-notes="wide" @play="play" />
      </div>
    </div>
    <button v-if="!following && playingHere" type="button" class="sheet__back" aria-label="Back to playhead" @click="backToPlayhead"><LocateFixed :size="16" /> Back to playhead</button>
    <PlayerDock @tools="emit('tools')" />
  </section>
</template>

<style scoped>
.sheet { position: relative; flex: 1; min-height: 0; display: grid; grid-template-rows: minmax(0, 1fr) auto; border-radius: 22px 22px 0 0; background: var(--page); box-shadow: 0 -10px 44px -30px var(--shadow); overflow: hidden; }
.sheet__scroll { min-height: 0; overflow-y: auto; overscroll-behavior: contain; scrollbar-gutter: stable; -webkit-mask-image: linear-gradient(var(--ink) 90%, transparent); mask-image: linear-gradient(var(--ink) 90%, transparent); }
.sheet__grid { display: grid; grid-template-columns: minmax(150px, 204px) minmax(0, 640px) minmax(150px, 204px); column-gap: clamp(24px, 3.4vw, 48px); justify-content: center; padding: 34px 0 16vh; }
.sheet__margin { grid-column: 1; grid-row: 1; align-self: start; position: sticky; top: 0; padding-left: 28px; }
.sheet__tools { grid-column: 3; grid-row: 1; z-index: 2; align-self: start; justify-self: end; position: sticky; top: 0; padding: 0 20px 8px 8px; background: var(--page); border-radius: 0 0 0 14px; }
.sheet__error { grid-column: 2; grid-row: 1; color: var(--accent); font-weight: 600; margin-bottom: 18px; }
.sheet__text { grid-column: 2; grid-row: 1; }
.sheet__text--margins { grid-column: 2 / 4; grid-template-columns: subgrid; }
.sheet__text--margins :deep(.chapter-text__row) { grid-column: 1 / -1; display: grid; grid-template-columns: subgrid; align-items: start; }
.sheet__text--margins :deep(.chapter-text__margin) { padding-right: 28px; }
.sheet__error ~ .sheet__text { margin-top: 3em; }
.sheet__back { position: absolute; left: 50%; bottom: 96px; transform: translateX(-50%); z-index: 3; display: inline-flex; align-items: center; gap: 8px; height: 40px; padding: 0 18px; border-radius: 999px; background: var(--ink); color: var(--ground); font-weight: 700; font-size: 13px; box-shadow: 0 12px 30px -12px var(--shadow); }
@media (max-width: 999px) {
  .sheet__grid { grid-template-columns: minmax(0, 640px); padding: 18px 20px 16vh; row-gap: 22px; }
  .sheet__margin { grid-column: 1; grid-row: 1; position: static; padding: 6px 150px 0 0; }
  .sheet__tools { grid-column: 1; grid-row: 1; position: static; padding: 0; }
  .sheet__error { grid-column: 1; grid-row: 2; }
  .sheet__text { grid-column: 1; grid-row: 3; }
  .sheet__back { bottom: 150px; }
}
</style>
