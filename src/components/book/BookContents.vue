<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { AudioLines, Check, Sparkles } from "@lucide/vue";
import { api } from "@/api";
import type { ChapterAudioView, ChapterSummary } from "@/api/types";
import { formatClock } from "@/estimate";
import { prepareChapter, preparationStatus } from "@/preparation";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";

/** An item's contents: its chapters (or an audiobook's parts), each opening where it starts, with
 *  where the reader is and, for a narrated book, whether each chapter's audio is made, being
 *  made, or can be prepared on its own. */
const props = defineProps<{
  bookId: string;
  chapters: ChapterSummary[];
  /** The chapter the reader is in. */
  current: number | null;
  /** An audiobook on its own: parts play rather than open. */
  audiobook: boolean;
  /** Played from a recording: no audio is made. */
  recorded: boolean;
}>();
const library = useLibraryStore(),
  player = usePlayerStore(),
  reader = useReaderStore();
const prepare = computed(() => library.books.find((b) => b.id === props.bookId)?.prepare ?? null);

/** Each chapter's audio, refreshed while the book is being prepared (at most every 2 s). */
const chapterAudio = ref<ChapterAudioView[]>([]);
async function loadChapterAudio() {
  try {
    chapterAudio.value = await api.chapterAudio({ bookId: props.bookId });
  } catch {
    chapterAudio.value = [];
  }
}
let refreshTimer: ReturnType<typeof setTimeout> | undefined;
function refreshChapterAudio() {
  refreshTimer ??= setTimeout(() => {
    refreshTimer = undefined;
    void loadChapterAudio();
  }, 2000);
}
onMounted(loadChapterAudio);
onUnmounted(() => clearTimeout(refreshTimer));
watch(() => [prepare.value, player.generation.preparing?.done], refreshChapterAudio, { deep: true });

type ChapterState = { kind: "ready" } | { kind: "queued"; label: string } | { kind: "none" } | { kind: "silent" };
function chapterState(index: number): ChapterState {
  const audio = chapterAudio.value[index];
  if (audio && audio.total === 0) return { kind: "silent" };
  if (audio && audio.done === audio.total) return { kind: "ready" };
  const p = prepare.value;
  if (p && p.state !== "done" && (!p.chapters || p.chapters.includes(index))) {
    return {
      kind: "queued",
      label: [preparationStatus(p).label, audio ? `${audio.done} / ${audio.total}` : null].filter(Boolean).join(" · "),
    };
  }
  return { kind: "none" };
}
const states = computed(() => new Map(props.chapters.map((c) => [c.index, chapterState(c.index)])));
async function prepareOne(index: number) {
  await prepareChapter(props.bookId, index);
  await library.refresh();
  await loadChapterAudio();
}
async function open(index: number) {
  await reader.open(props.bookId);
  await reader.startChapter(index);
}
</script>

<template>
  <ol class="book-contents" aria-label="Contents">
    <li v-for="(c, i) in chapters" :key="c.index" class="book-contents__item">
      <button
        type="button"
        class="book-contents__chapter"
        :class="{ 'book-contents__chapter--current': current === c.index }"
        :aria-label="`${audiobook ? 'Play part' : 'Open chapter'} ${i + 1}: ${c.title}`"
        @click="open(c.index)"
      >
        <span class="book-contents__number">{{ i + 1 }}</span><span class="book-contents__title">{{ c.title }}</span
        ><AudioLines v-if="current === c.index" :size="15" class="book-contents__here" aria-hidden="true" />
      </button>
      <span v-if="c.durationMs !== null" class="book-contents__audio">{{ formatClock(c.durationMs) }}</span>
      <template v-if="!recorded">
        <span
          v-if="states.get(c.index)?.kind === 'ready'"
          class="book-contents__audio book-contents__audio--ready"
          title="This chapter's audio is made: it plays at once, even offline"
          ><Check :size="14" aria-hidden="true" /> Ready</span
        >
        <span v-else-if="states.get(c.index)?.kind === 'queued'" class="book-contents__audio">{{
          (states.get(c.index) as { label: string }).label
        }}</span>
        <button
          v-else-if="states.get(c.index)?.kind === 'none'"
          type="button"
          class="book-contents__prepare"
          :aria-label="`Prepare chapter ${i + 1}: ${c.title}`"
          title="Prepare this chapter"
          @click="prepareOne(c.index)"
        >
          <Sparkles :size="14" aria-hidden="true" />
        </button>
      </template>
    </li>
  </ol>
</template>

<style scoped>
.book-contents {
  margin: 0;
  padding: 0;
  list-style: none;
}
.book-contents__item {
  display: flex;
  align-items: center;
  gap: var(--space-5);
  border-bottom: 1px solid var(--wash);
}
.book-contents__item:last-child {
  border-bottom: 0;
}
.book-contents__chapter {
  flex: 1;
  min-width: 0;
  min-height: 44px;
  display: flex;
  align-items: center;
  gap: var(--space-9);
  text-align: left;
}
.book-contents__chapter:hover .book-contents__title {
  color: var(--accent);
}
.book-contents__chapter--current .book-contents__title {
  font-weight: 700;
}
.book-contents__number {
  width: 20px;
  flex: none;
  text-align: right;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}
.book-contents__title {
  flex: 1;
  min-width: 0;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.book-contents__here {
  color: var(--accent);
}
.book-contents__audio {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font-size: 12.5px;
  color: var(--dim);
  white-space: nowrap;
}
.book-contents__audio--ready {
  color: var(--ok);
  font-weight: 600;
}
.book-contents__prepare {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  color: var(--dim);
  opacity: 0;
}
.book-contents__item:hover .book-contents__prepare,
.book-contents__prepare:focus-visible {
  opacity: 1;
}
.book-contents__prepare:hover {
  background: var(--wash);
  color: var(--ink);
}
@media (hover: none) {
  .book-contents__prepare {
    opacity: 1;
  }
}
</style>
