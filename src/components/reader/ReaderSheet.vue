<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { ArrowRight, LocateFixed } from "@lucide/vue";
import ChapterText from "./ChapterText.vue";
import BookMargin from "./BookMargin.vue";
import PlayerDock from "../player/PlayerDock.vue";
import ArticleHeader from "../article/ArticleHeader.vue";
import NeedsYouNotice from "../article/NeedsYouNotice.vue";
import { useFollow } from "../../composables/useFollow";
import { useOpenArticle } from "../../composables/useOpenArticle";
import { useArticlesStore } from "../../stores/articles";
import { useReadingPlace } from "../../composables/useReadingPlace";
import { useMediaQuery } from "../../composables/useMediaQuery";
import { usePlayerControls } from "../../composables/usePlayerControls";
import { formatDuration, readingMs, speechMs, wordsLeft } from "../../estimate";
import type { FetchReason, Shape } from "../../api/types";
import { useLibraryStore } from "../../stores/library";
import { useReaderStore } from "../../stores/reader";
import { useNavigationStore } from "../../stores/navigation";
import { prepareChapter } from "../../preparation";

/** Page mode: a full-width sheet with the text in a 640px column, the book in the left
 *  margin and footnotes and tools in the right margin. Listening (`readAlong`) docks the
 *  transport underneath and follows the playhead; reading silently (`read`) shows the progress
 *  through the book there instead, and the text is only read. */
const props = defineProps<{ shape: Shape }>();
const emit = defineEmits<{ tools: [tab?: "details" | "voice"] }>();
const reader = useReaderStore(),
  library = useLibraryStore(),
  navigation = useNavigationStore();
const { player, timeLeftMs } = usePlayerControls();
const wide = useMediaQuery("(min-width: 1000px)");
const scroller = ref<HTMLElement>();
const { article, openOriginal } = useOpenArticle();
const articles = useArticlesStore();
/** Keeps the open article, opened from a feed, in the library. */
const keep = () => {
  if (reader.book) articles.keep(reader.book.id).catch((e) => (reader.error = String(e)));
};
/** Why the article needs the user, kept through a fetch again until one gets all of it. */
const needs = ref<FetchReason | null>(null);
watch(
  () => article.value?.stage,
  (stage) => {
    if (!stage || stage.kind === "ready") needs.value = null;
    else if (stage.kind === "needsYou") needs.value = stage.reason;
  },
  { immediate: true },
);
const refetch = (source: "http" | "archive") =>
  article.value && articles.refetch(article.value.id, source).catch((e) => (reader.error = String(e)));

const along = computed(() => props.shape === "readAlong");
const playingHere = computed(() => along.value && Boolean(reader.book) && player.state.bookId === reader.book?.id);
const onPage = computed(() => playingHere.value && player.state.chapter === reader.currentChapter);
const { following, follow, backToPlayhead, restart, listeners } = useFollow(
  scroller,
  () => playingHere.value && Boolean(player.state.segmentId),
);
const place = useReadingPlace(scroller, () => props.shape === "read");
function onScroll() {
  listeners.onScroll();
  place.schedule();
}
// Another chapter of the same book (next, previous, or picked): start at its top.
watch(
  () => [reader.book?.id, reader.currentChapter] as const,
  ([id, chapter], [oldId, oldChapter]) => {
    if (id && id === oldId && chapter !== oldChapter) restart();
  },
  { flush: "post" },
);
watch(
  () => [player.state.segmentId, player.state.status, reader.chapter],
  async () => {
    if (onPage.value && player.state.status === "playing") {
      await nextTick();
      follow();
    }
  },
  { immediate: true, flush: "post" },
);

const coverUrl = computed(() => library.books.find((b) => b.id === reader.book?.id)?.coverUrl ?? null);
const chapterLeft = computed(() => {
  if (onPage.value && timeLeftMs.value !== null) return `${formatDuration(timeLeftMs.value)} left`;
  if (!reader.chapter) return "";
  const words = wordsLeft(reader.chapter, null, null);
  return along.value
    ? `${formatDuration(speechMs(words, player.state.speed))} left`
    : `${formatDuration(readingMs(words, library.readingWpm))} to read`;
});
/** Through the whole book, as the core last reported it. */
const fraction = computed(() => {
  const summary = library.books.find((b) => b.id === reader.book?.id);
  return Math.min(1, Math.max(0, summary?.progress?.fraction ?? reader.book?.progress?.fraction ?? 0));
});
const bookLeft = computed(() => {
  const words = (reader.book?.wordCount ?? 0) * (1 - fraction.value);
  return `${formatDuration(readingMs(words, library.readingWpm))} left to read`;
});
async function play(segmentId: string) {
  if (along.value && reader.book) await player.play(reader.book.id, reader.currentChapter, segmentId);
}
const jump = (index: number) => reader.startChapter(index);
/** The chapter after this one, offered at the end of the text while reading silently. */
const next = computed(() => {
  const chapters = reader.book?.chapters ?? [];
  return along.value ? null : (chapters[reader.currentChapter + 1] ?? null);
});
</script>

<template>
  <section v-if="reader.book" class="sheet">
    <div
      ref="scroller"
      class="sheet__scroll"
      @scroll.passive="onScroll"
      @scrollend="listeners.onScrollend"
      @wheel.passive="listeners.onWheel"
      @touchmove.passive="listeners.onTouchmove"
      @keydown="listeners.onKeydown"
    >
      <div class="sheet__grid">
        <BookMargin
          class="sheet__margin"
          :book="reader.book"
          :cover-url="coverUrl"
          :chapter="reader.currentChapter"
          :playing-chapter="playingHere ? player.state.chapter : null"
          :time-left="chapterLeft"
          :article="Boolean(article)"
          @chapter="jump"
          @details="emit('tools', 'details')"
          @voice="emit('tools', 'voice')"
          @about="navigation.openItem(reader.book.id)"
          @prepare="prepareChapter(reader.book.id, $event)"
        />
        <p v-if="reader.error" class="sheet__error" role="alert">{{ reader.error }}</p>
        <ChapterText
          v-if="reader.chapter"
          class="sheet__text"
          :class="{ 'sheet__text--margins': wide }"
          :chapter="reader.chapter"
          :current-segment="onPage ? player.state.segmentId : null"
          :current-word="onPage ? player.state.wordIndex : null"
          :margin-notes="wide"
          :playable="along"
          @play="play"
        >
          <template v-if="article" #before>
            <ArticleHeader
              :article="article"
              :title="reader.book.title"
              :authors="reader.book.authors"
              :words="reader.book.wordCount"
              :wpm="library.readingWpm"
              @original="openOriginal"
              @save="keep"
            />
          </template>
          <template v-if="article && needs" #after>
            <NeedsYouNotice
              :article="article"
              :reason="needs"
              @fetch="refetch('http')"
              @archive="refetch('archive')"
              @original="openOriginal"
            />
          </template>
        </ChapterText>
        <button v-if="next && reader.chapter" type="button" class="sheet__next" @click="jump(next.index)">
          Next: {{ next.title }}<ArrowRight :size="16" aria-hidden="true" />
        </button>
      </div>
    </div>
    <button
      v-if="!following && playingHere"
      type="button"
      class="sheet__back"
      aria-label="Back to playhead"
      @click="backToPlayhead"
    >
      <LocateFixed :size="16" /> Back to playhead
    </button>
    <PlayerDock v-if="along" @tools="emit('tools')" />
    <footer v-else class="sheet__foot">
      <div
        class="sheet__progress"
        role="progressbar"
        aria-label="Reading progress"
        :aria-valuenow="Math.round(fraction * 100)"
        aria-valuemin="0"
        aria-valuemax="100"
      >
        <i :style="{ width: `${fraction * 100}%` }" />
      </div>
      <div class="sheet__facts">
        <span>{{ Math.round(fraction * 100) }}%</span><span>{{ bookLeft }}</span>
      </div>
    </footer>
  </section>
</template>

<style scoped>
.sheet {
  position: relative;
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-rows: minmax(0, 1fr) auto;
  border-radius: 22px 22px 0 0;
  background: var(--page);
  box-shadow: 0 -10px 44px -30px var(--shadow);
  overflow: hidden;
}
.sheet__scroll {
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  -webkit-mask-image: linear-gradient(var(--ink) 90%, transparent);
  mask-image: linear-gradient(var(--ink) 90%, transparent);
}
.sheet__grid {
  display: grid;
  grid-template-columns: minmax(150px, 204px) minmax(0, 640px) minmax(150px, 204px);
  column-gap: clamp(24px, 3.4vw, 48px);
  justify-content: center;
  padding: 34px 0 16vh;
}
.sheet__margin {
  grid-column: 1;
  grid-row: 1;
  align-self: start;
  position: sticky;
  top: 0;
  padding-left: 28px;
}
.sheet__error {
  grid-column: 2;
  grid-row: 1;
  color: var(--accent);
  font-weight: 600;
  margin-bottom: 18px;
}
.sheet__text {
  grid-column: 2;
  grid-row: 1;
}
.sheet__text--margins {
  grid-column: 2 / 4;
  grid-template-columns: subgrid;
}
.sheet__text--margins :deep(.chapter-text__row) {
  grid-column: 1 / -1;
  display: grid;
  grid-template-columns: subgrid;
  align-items: start;
}
.sheet__text--margins :deep(.chapter-text__margin) {
  padding-right: 28px;
}
.sheet__error ~ .sheet__text {
  margin-top: 3em;
}
.sheet__next {
  grid-column: 2;
  grid-row: 2;
  justify-self: start;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-top: 28px;
  height: 44px;
  padding: 0 18px;
  border-radius: 22px;
  background: var(--glass);
  font-weight: 600;
}
.sheet__foot {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 0 28px 14px;
}
.sheet__progress {
  height: 3px;
  border-radius: 2px;
  background: var(--wash);
}
.sheet__progress i {
  display: block;
  height: 100%;
  border-radius: 2px;
  background: var(--ink);
}
.sheet__facts {
  display: flex;
  justify-content: space-between;
  font-size: 13px;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}
.sheet__back {
  position: absolute;
  left: 50%;
  bottom: 96px;
  transform: translateX(-50%);
  z-index: 3;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 40px;
  padding: 0 18px;
  border-radius: 999px;
  background: var(--ink);
  color: var(--ground);
  font-weight: 700;
  font-size: 13px;
  box-shadow: 0 12px 30px -12px var(--shadow);
}
@media (max-width: 999px) {
  .sheet__grid {
    grid-template-columns: minmax(0, 640px);
    padding: 18px 20px 16vh;
    row-gap: 22px;
  }
  .sheet__margin {
    grid-column: 1;
    grid-row: 1;
    position: static;
    padding: 6px 150px 0 0;
  }
  .sheet__error {
    grid-column: 1;
    grid-row: 2;
  }
  .sheet__text {
    grid-column: 1;
    grid-row: 3;
  }
  .sheet__next {
    grid-column: 1;
    grid-row: 4;
  }
  .sheet__back {
    bottom: 150px;
  }
}
</style>
