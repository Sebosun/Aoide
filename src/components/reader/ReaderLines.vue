<script setup lang="ts">
import { computed, onBeforeUpdate, ref, watch } from "vue";
import type { SegmentView } from "../../api/types";
import CoverArt from "../CoverArt.vue";
import ChapterMenu from "./ChapterMenu.vue";
import ReaderTools from "./ReaderTools.vue";
import PlayerTransport from "../player/PlayerTransport.vue";
import { usePlayerStore } from "../../stores/player";
import { useLibraryStore } from "../../stores/library";
import { useReaderStore } from "../../stores/reader";
import { useNavigationStore } from "../../stores/navigation";
import { prepareChapter } from "../../preparation";

/** Lines mode, for listening from across the room: the spoken sentence large, filling word by
 *  word, with the sentences around it at reading size. */
const emit = defineEmits<{ tools: [] }>();
const reader = useReaderStore(), player = usePlayerStore(), library = useLibraryStore(), navigation = useNavigationStore();
const coverUrl = computed(() => library.books.find(b => b.id === reader.book?.id)?.coverUrl ?? null);
const playingHere = computed(() => Boolean(reader.book) && player.state.bookId === reader.book?.id);
const onPage = computed(() => playingHere.value && player.state.chapter === reader.currentChapter);
const spoken = computed<SegmentView[]>(() => reader.chapter?.blocks.flatMap(b => b.segments).filter(s => s.speakable) ?? []);
const index = computed(() => {
  const at = onPage.value ? spoken.value.findIndex(s => s.id === player.state.segmentId) : -1;
  return Math.max(0, at);
});
const now = computed(() => spoken.value[index.value]);
/** The spoken sentence with two on either side. One keyed list, so a sentence keeps its
 *  element as the playhead moves and glides to its new place (TransitionGroup). */
const shown = computed(() => spoken.value.slice(Math.max(0, index.value - 2), index.value + 3));
/** 1 while moving forward (lines rise), -1 after stepping back (lines sink). */
const direction = ref(1);
watch(index, (to, from) => { direction.value = to >= from ? 1 : -1; }, { flush: "sync" });
// A leaving line is taken out of the flow so the others can move; it keeps the box it had.
const stage = ref<HTMLElement>(), boxes = new WeakMap<Element, { top: number; left: number; width: number }>();
onBeforeUpdate(() => {
  for (const line of stage.value?.querySelectorAll<HTMLElement>(".reader-lines__line") ?? []) boxes.set(line, { top: line.offsetTop, left: line.offsetLeft, width: line.offsetWidth });
});
function pin(element: Element) {
  const box = boxes.get(element), line = element as HTMLElement;
  if (box) Object.assign(line.style, { top: `${box.top}px`, left: `${box.left}px`, width: `${box.width}px` });
}
function wordState(i: number) {
  if (!onPage.value || player.state.wordIndex === null) return onPage.value ? "ahead" : "plain";
  return i < player.state.wordIndex ? "done" : i === player.state.wordIndex ? "current" : "ahead";
}
const play = (segment: SegmentView) => { if (reader.book) void player.play(reader.book.id, reader.currentChapter, segment.id); };
</script>

<template>
  <section v-if="reader.book" class="reader-lines">
    <aside class="reader-lines__side" aria-label="About this book">
      <button type="button" class="reader-lines__cover-link" :aria-label="`Book page for ${reader.book.title}`" title="Book page: details, contents and your listening" @click="navigation.openBook(reader.book.id)"><CoverArt class="reader-lines__cover" :title="reader.book.title" :url="coverUrl" /></button>
      <div class="reader-lines__book"><h2 class="reader-lines__title"><button type="button" class="reader-lines__title-link" tabindex="-1" aria-hidden="true" @click="navigation.openBook(reader.book.id)">{{ reader.book.title }}</button></h2><span class="reader-lines__author">{{ reader.book.authors.join(", ") || "Unknown author" }}</span></div>
      <ChapterMenu variant="chip" prepare :chapters="reader.book.chapters" :current="reader.currentChapter" :playing="playingHere ? player.state.chapter : null" @pick="reader.startChapter($event)" @prepare="prepareChapter(reader.book.id, $event)" />
    </aside>
    <ReaderTools class="reader-lines__tools" />
    <div ref="stage" class="reader-lines__stage" :style="{ '--lines-shift': direction }" aria-label="Spoken sentence" aria-live="off">
      <p v-if="reader.error" class="reader-lines__error" role="alert">{{ reader.error }}</p>
      <TransitionGroup name="reader-lines__line" @before-leave="pin">
        <p v-for="segment in shown" :key="segment.id" class="reader-lines__line" :class="{ 'reader-lines__line--now': segment.id === now?.id }" :data-segment-id="segment.id" role="button" tabindex="0" @click="play(segment)" @keydown.enter.prevent="play(segment)"><template v-if="segment.id === now?.id"><template v-for="(word, i) in segment.words" :key="i"><span class="reader-lines__word" :class="`reader-lines__word--${wordState(i)}`">{{ word }}</span>{{ i < segment.words.length - 1 ? " " : "" }}</template></template><template v-else>{{ segment.words.join(" ") }}</template></p>
      </TransitionGroup>
    </div>
    <PlayerTransport class="reader-lines__transport" @tools="emit('tools')" />
  </section>
</template>

<style scoped>
.reader-lines { position: relative; flex: 1; min-height: 0; display: grid; grid-template-columns: 300px minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) auto; column-gap: 56px; padding-top: 10px; }
.reader-lines__side { grid-column: 1; grid-row: 1; display: grid; gap: 14px; align-content: center; justify-items: start; min-height: 0; }
.reader-lines__cover-link { display: block; width: min(100%, 44vh * 2 / 3); border-radius: 12px; }
.reader-lines__cover { width: 100%; font-size: 20px; }
.reader-lines__title-link { font: inherit; color: inherit; text-align: left; letter-spacing: inherit; }
.reader-lines__title-link:hover { text-decoration: underline; text-underline-offset: 3px; }
.reader-lines__title { font-size: 24px; font-weight: 800; letter-spacing: -.02em; line-height: 1.15; }
.reader-lines__author { color: var(--dim); font-weight: 500; }
.reader-lines__tools { position: absolute; top: 14px; right: 0; z-index: 2; }
.reader-lines__tools :deep(.reader-tools__aa), .reader-lines__tools :deep(.reader-tools__mode) { background: var(--glass); }
.reader-lines__stage { position: relative; grid-column: 2; grid-row: 1; min-height: 0; display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 20px; overflow: hidden; -webkit-mask-image: linear-gradient(transparent, var(--ink) 14%, var(--ink) 86%, transparent); mask-image: linear-gradient(transparent, var(--ink) 14%, var(--ink) 86%, transparent); }
.reader-lines__line { width: min(100%, 660px); font: 400 calc(21px * var(--read-scale)) / 1.45 var(--font-read); color: var(--dim); cursor: pointer; border-radius: 6px; }
.reader-lines__line:hover { color: var(--reading); }
/* The playhead moved: lines that stay glide to their new place, the line that drops out fades
   the way the others travel, and a new one fades in after them. */
.reader-lines__line-move { transition: transform var(--dur-3) var(--ease); }
.reader-lines__line-enter-active { transition: opacity var(--dur-3) var(--ease), transform var(--dur-3) var(--ease); }
.reader-lines__line-leave-active { position: absolute; transition: opacity var(--dur-2) var(--ease), transform var(--dur-3) var(--ease); }
.reader-lines__line-enter-from { opacity: 0; transform: translateY(calc(32px * var(--lines-shift))); }
.reader-lines__line-leave-to { opacity: 0; transform: translateY(calc(-32px * var(--lines-shift))); }
@keyframes reader-lines-now { from { opacity: .45; } }
.reader-lines__line--now { font: 500 calc(35px * var(--read-scale)) / 1.24 var(--font-read); letter-spacing: -.012em; color: var(--ink); cursor: default; animation: reader-lines-now var(--dur-3) var(--ease); }
.reader-lines__line--now:hover { color: var(--ink); }
.reader-lines__word { transition: opacity .2s; }
.reader-lines__word--ahead { opacity: .5; }
.reader-lines__word--current { text-decoration: underline 3px var(--accent); text-underline-offset: 7px; }
.reader-lines__error { color: var(--accent); font-weight: 600; }
.reader-lines__transport { grid-column: 1 / -1; grid-row: 2; }
@media (max-width: 999px) {
  .reader-lines { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr) auto; padding-top: 6px; }
  .reader-lines__side { grid-row: 1; grid-template-columns: 72px minmax(0, 1fr); align-items: center; gap: 4px 14px; padding-right: 180px; }
  .reader-lines__cover-link { grid-row: span 2; width: 100%; }
  .reader-lines__cover { font-size: 0; }
  .reader-lines__title { font-size: 18px; }
  .reader-lines__book { min-width: 0; }
  .reader-lines__tools { top: 6px; }
  .reader-lines__stage { grid-column: 1; grid-row: 2; }
  .reader-lines__transport { grid-row: 3; }
}
@media (max-width: 600px) { .reader-lines__side { padding-right: 0; padding-top: 44px; } .reader-lines__line--now { font-size: calc(28px * var(--read-scale)); } }
</style>
