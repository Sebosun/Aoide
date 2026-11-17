<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { ArrowRight, LocateFixed } from "@lucide/vue";
import ChapterText from "@/components/reader/ChapterText.vue";
import ReaderSelection from "@/components/reader/ReaderSelection.vue";
import HighlightCard from "@/components/reader/HighlightCard.vue";
import BookMargin from "@/components/reader/BookMargin.vue";
import PlayerDock from "@/components/player/PlayerDock.vue";
import SleepPill from "@/components/player/SleepPill.vue";
import ArticleHeader from "@/components/article/ArticleHeader.vue";
import NeedsYouNotice from "@/components/article/NeedsYouNotice.vue";
import { useFollow } from "@/composables/useFollow";
import { useOpenArticle } from "@/composables/useOpenArticle";
import { useArticlesStore } from "@/stores/articles";
import { useReadingPlace } from "@/composables/useReadingPlace";
import { useMediaQuery } from "@/composables/useMediaQuery";
import { usePlayerControls } from "@/composables/usePlayerControls";
import { formatDuration, readingMs, speechMs, wordsLeft } from "@/estimate";
import type { FetchReason, Shape } from "@/api/types";
import { useLibraryStore } from "@/stores/library";
import { useReaderStore } from "@/stores/reader";
import { useNavigationStore } from "@/stores/navigation";
import { useSignInStore } from "@/stores/signin";
import { api } from "@/api";
import { prepareChapter } from "@/preparation";

/** Page mode: a full-width sheet with the text in a column as wide as the reader's text width, the book in the left
 *  margin and footnotes and tools in the right margin. Listening (`readAlong`) docks the
 *  transport underneath and follows the playhead; reading silently (`read`) shows the progress
 *  through the book there instead, and the text is only read. While another book plays, the
 *  sleep timer's menu (the T key) opens over that foot: its button lies unseen at its right end,
 *  out of the way of the pointer and of Tab, as in the mini player. */
const props = defineProps<{ shape: Shape }>();
const emit = defineEmits<{ tools: [tab?: "details"] }>();
const reader = useReaderStore(),
  library = useLibraryStore(),
  navigation = useNavigationStore();
const { player } = usePlayerControls();
const wide = useMediaQuery("(min-width: 1000px)");
const scroller = ref<HTMLElement>();
const text = ref<InstanceType<typeof ChapterText>>();
const textRoot = computed(() => text.value?.$el as HTMLElement | undefined);
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
/** Whether the browser for articles is on: the notice offers a sign-in window, else turning
 *  it on. Read again when Settings closes. */
const browserOn = ref(false);
const loadBrowser = () =>
  api
    .appSettings()
    .then((s) => (browserOn.value = s.browser.use !== "off"))
    .catch(() => {});
onMounted(loadBrowser);
watch(
  () => navigation.settingsOpen,
  (open) => !open && void loadBrowser(),
);
const signIn = useSignInStore();
const startSignIn = async () => {
  if (article.value && !(await signIn.start(article.value.id))) reader.error = signIn.error;
};
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
  const playing = player.state.chapterLeftMs;
  if (onPage.value && playing !== null) return `${formatDuration(playing)} left`;
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
          @voice="navigation.openNarration()"
          @about="navigation.openItem(reader.book.id)"
          @prepare="prepareChapter(reader.book.id, $event)"
        />
        <p v-if="reader.error" class="sheet__error" role="alert">{{ reader.error }}</p>
        <ChapterText
          v-if="reader.chapter"
          ref="text"
          class="sheet__text"
          :class="{ 'sheet__text--margins': wide }"
          :chapter="reader.chapter"
          :lang="reader.book.language ?? undefined"
          :current-segment="onPage ? player.state.segmentId : null"
          :current-word="onPage ? player.state.wordIndex : null"
          :margin-notes="wide"
          :playable="along"
          :highlights="reader.highlights"
          @play="play"
        >
          <!-- Slots are always given (what they show decides): a slot given only sometimes makes
               the chapter draw again whenever this page does. -->
          <template #before>
            <ArticleHeader
              v-if="article"
              :article="article"
              :title="reader.book.title"
              :authors="reader.book.authors"
              :words="reader.book.wordCount"
              :wpm="library.readingWpm"
              @original="openOriginal"
              @save="keep"
            />
          </template>
          <template #after>
            <NeedsYouNotice
              v-if="article && needs"
              :article="article"
              :reason="needs"
              :browser-on="browserOn"
              @sign-in="startSignIn"
              @turn-on="navigation.openSettings('sites')"
              @fetch="refetch('http')"
              @archive="refetch('archive')"
            />
          </template>
        </ChapterText>
        <button v-if="next && reader.chapter" type="button" class="sheet__next" @click="jump(next.index)">
          Next: {{ next.title }}<ArrowRight :size="16" aria-hidden="true" />
        </button>
      </div>
    </div>
    <div class="sheet__fade" aria-hidden="true" />
    <button
      v-if="!following && playingHere"
      type="button"
      class="sheet__back"
      aria-label="Back to playhead"
      @click="backToPlayhead"
    >
      <LocateFixed :size="16" /> Back to playhead
    </button>
    <ReaderSelection :root="textRoot" :chapter="reader.chapter" />
    <HighlightCard :root="textRoot" />
    <PlayerDock v-if="along" />
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
      <SleepPill v-if="player.state.bookId" class="sheet__sleep" tabindex="-1" />
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
  border-radius: var(--radius-xl) var(--radius-xl) 0 0;
  background: var(--page);
  box-shadow: 0 -10px 44px -30px var(--shadow);
  overflow: hidden;
}
.sheet__scroll {
  grid-row: 1;
  grid-column: 1;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
}
/* The text fades into the page at the bottom. A still layer over the scroller, rather than a
   mask on it, so scrolling does not repaint the text. */
.sheet__fade {
  /* Positioned, so it also covers the scroller's positioned parts (footnote markers, the margin). */
  position: relative;
  grid-row: 1;
  grid-column: 1;
  align-self: end;
  height: 10%;
  background: linear-gradient(transparent, var(--page));
  pointer-events: none;
}
.sheet__grid {
  display: grid;
  grid-template-columns: minmax(150px, 204px) minmax(0, var(--read-width)) minmax(150px, 204px);
  column-gap: clamp(var(--space-12), 3.4vw, var(--space-24));
  justify-content: center;
  padding: var(--space-16) 0 16vh;
}
.sheet__margin {
  grid-column: 1;
  grid-row: 1;
  align-self: start;
  position: sticky;
  top: 0;
  padding-left: var(--space-14);
}
.sheet__error {
  grid-column: 2;
  grid-row: 1;
  color: var(--accent);
  font-weight: 600;
  margin-bottom: var(--space-9);
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
  padding-right: var(--space-14);
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
  gap: var(--space-4);
  margin-top: var(--space-14);
  height: 44px;
  padding: 0 var(--space-9);
  border-radius: var(--radius-xl);
  background: var(--glass);
  font-weight: 600;
}
.sheet__foot {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  padding: 0 var(--space-14) var(--space-7);
}
/* Over the time left, at the foot's right end, so the menu opens above it at a menu's width. */
.sheet__foot .sheet__sleep {
  position: absolute;
  right: var(--space-14);
  bottom: var(--space-7);
  width: 220px;
  opacity: 0;
  pointer-events: none;
}
.sheet__progress {
  height: 3px;
  border-radius: var(--radius-xs);
  background: var(--wash);
}
.sheet__progress i {
  display: block;
  height: 100%;
  border-radius: var(--radius-xs);
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
  gap: var(--space-4);
  height: 40px;
  padding: 0 var(--space-9);
  border-radius: var(--radius-pill);
  background: var(--ink);
  color: var(--ground);
  font-weight: 700;
  font-size: 13px;
  box-shadow: 0 12px 30px -12px var(--shadow);
}
@media (max-width: 999px) {
  .sheet__grid {
    grid-template-columns: minmax(0, var(--read-width));
    padding: var(--space-9) var(--space-10) 16vh;
    row-gap: var(--space-11);
  }
  .sheet__margin {
    grid-column: 1;
    grid-row: 1;
    position: static;
    padding: var(--space-3) 150px 0 0;
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
