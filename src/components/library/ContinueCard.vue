<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Play } from "@lucide/vue";
import { api } from "../../api";
import type { BookSummary, BookView } from "../../api/types";
import CoverArt from "../CoverArt.vue";
import ChapterMenu from "../reader/ChapterMenu.vue";
import { formatDuration, speechMs } from "../../estimate";
import { usePlayerStore } from "../../stores/player";
import { useReaderStore } from "../../stores/reader";
import { useNavigationStore } from "../../stores/navigation";
import { describePreparation, preparationStatus } from "../../preparation";

/** "Continue listening": the playing book, else the last opened book in progress. */
const props = defineProps<{ book: BookSummary }>();
const player = usePlayerStore(), reader = useReaderStore(), navigation = useNavigationStore();
const overview = ref<BookView | null>(null);
watch(() => props.book.id, async id => {
  overview.value = null;
  try { const view = await api.bookOverview(id); if (props.book.id === id) overview.value = view; } catch { /* the card still shows the summary */ }
}, { immediate: true });

const playingThis = computed(() => player.state.bookId === props.book.id);
const chapterIndex = computed(() => playingThis.value ? player.state.chapter : props.book.progress?.chapter ?? 0);
const chapterTitle = computed(() => overview.value?.chapters.find(c => c.index === chapterIndex.value)?.title ?? `Chapter ${chapterIndex.value + 1}`);
const fraction = computed(() => props.book.progress?.fraction ?? 0);
const details = computed(() => {
  const parts = [`${Math.round(fraction.value * 100)}%`];
  if (overview.value?.wordCount) parts.push(`about ${formatDuration(speechMs(overview.value.wordCount * (1 - fraction.value), player.state.speed))} left at ${player.state.speed}×`);
  const preparation = describePreparation(preparationStatus(props.book.prepare));
  if (preparation) parts.push(preparation.toLowerCase());
  return parts.join(" · ");
});
const active = computed(() => playingThis.value && (player.state.status === "playing" || player.state.status === "buffering"));
async function resume() {
  if (active.value) return;
  if (playingThis.value && player.state.status === "paused") await player.resume();
  else await player.play(props.book.id, null, null);
}
async function openNowPlaying() {
  if (reader.book?.id === props.book.id) navigation.view = "reader";
  else await reader.open(props.book.id);
}
async function chapter(index: number) {
  if (reader.book?.id !== props.book.id) await reader.open(props.book.id);
  await reader.startChapter(index);
}
</script>

<template>
  <section class="continue" aria-label="Continue listening">
    <CoverArt class="continue__cover" :title="book.title" :url="book.coverUrl" />
    <div class="continue__body">
      <div>
        <span class="continue__eyebrow">{{ active ? "Now playing" : "Continue listening" }}</span>
        <h2 class="continue__title"><button type="button" class="continue__title-link" :aria-label="`Open ${book.title} in Now Playing`" @click="openNowPlaying">{{ book.title }}</button></h2>
        <span class="continue__meta">{{ book.authors.join(", ") || "Unknown author" }} · {{ chapterTitle }}</span>
      </div>
      <div class="continue__progress">
        <span class="continue__track"><i :style="{ width: `${fraction * 100}%` }" /></span>
        <span class="continue__meta continue__meta--small">{{ details }}</span>
      </div>
      <div class="continue__actions">
        <button type="button" class="continue__resume" aria-label="Resume" :disabled="active" @click="resume"><Play :size="17" fill="currentColor" /> {{ active ? "Playing" : "Resume" }}</button>
        <ChapterMenu v-if="overview" variant="pill" :chapters="overview.chapters" :current="chapterIndex" :playing="playingThis ? player.state.chapter : null" @pick="chapter" />
      </div>
    </div>
  </section>
</template>

<style scoped>
.continue { display: grid; grid-template-columns: 176px minmax(0, 1fr); gap: 26px; align-items: center; padding: 22px; border-radius: 22px; background: var(--hero); }
.continue__cover { font-size: 17px; }
.continue__body { display: grid; gap: 14px; min-width: 0; }
.continue__eyebrow { font-size: 12px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--dim); }
.continue__title { margin: 8px 0 4px; font-size: 32px; font-weight: 800; letter-spacing: -.03em; line-height: 1.05; overflow-wrap: anywhere; }
.continue__title-link { text-align: left; border-radius: 6px; letter-spacing: inherit; }
.continue__title-link:hover { text-decoration: underline 2px; text-underline-offset: 5px; }
.continue__meta { color: var(--dim); font-weight: 500; }
.continue__meta--small { font-size: 12.5px; font-variant-numeric: tabular-nums; }
.continue__progress { display: grid; gap: 7px; }
.continue__track { position: relative; height: 5px; border-radius: 3px; background: var(--line); overflow: hidden; }
.continue__track i { position: absolute; inset: 0 auto 0 0; border-radius: 3px; background: var(--accent); }
.continue__actions { display: flex; flex-wrap: wrap; gap: 10px; }
.continue__resume { display: inline-flex; align-items: center; gap: 9px; height: 46px; padding: 0 24px; border-radius: 999px; background: var(--accent); color: var(--on-accent); font-weight: 800; font-size: 15px; }
.continue__resume:disabled { opacity: 1; cursor: default; }
@media (max-width: 640px) { .continue { grid-template-columns: 96px minmax(0, 1fr); gap: 16px; padding: 16px; } .continue__cover { font-size: 10px; } .continue__title { font-size: 24px; } }
</style>
