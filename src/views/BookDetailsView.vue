<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { ArrowLeft, AudioLines, Play, Sparkles } from "@lucide/vue";
import { api } from "../api";
import type { BookDetails, BookView, DayListening } from "../api/types";
import CoverArt from "../components/CoverArt.vue";
import UiSkeleton from "../components/ui/UiSkeleton.vue";
import MetadataLookup from "../components/book/MetadataLookup.vue";
import BookListening from "../components/book/BookListening.vue";
import { formatBytes } from "../bytes";
import { formatDuration, speechMs } from "../estimate";
import { useLibraryStore } from "../stores/library";
import { useNavigationStore } from "../stores/navigation";
import { usePlayerStore } from "../stores/player";
import { useReaderStore } from "../stores/reader";
import { ACTION_LABELS, describePreparation, preparationStatus, runPrepareAction } from "../preparation";

/** A book's page before (or between) listening: cover, description, facts, contents and
 *  extraction notes, with Start listening. Nothing plays until the person asks. */
const props = defineProps<{ bookId: string }>();
const library = useLibraryStore(), navigation = useNavigationStore(), player = usePlayerStore(), reader = useReaderStore();
const overview = ref<BookView | null>(null), details = ref<BookDetails | null>(null), listening = ref<DayListening[]>([]), error = ref("");
const expanded = ref(false);
const loading = computed(() => !overview.value && !error.value);
onMounted(async () => {
  try {
    const [view, info, days] = await Promise.all([api.bookOverview(props.bookId), api.bookDetails(props.bookId), api.bookListening(props.bookId).catch(() => [])]);
    overview.value = view; details.value = info; listening.value = days;
  } catch (e) { error.value = String(e); }
});

const summary = computed(() => library.books.find(b => b.id === props.bookId) ?? null);
const title = computed(() => summary.value?.title ?? overview.value?.title ?? "");
const authors = computed(() => (summary.value?.authors ?? overview.value?.authors ?? []).join(", ") || "Unknown author");
const progress = computed(() => summary.value?.progress ?? overview.value?.progress ?? null);
const percent = computed(() => Math.round((progress.value?.fraction ?? 0) * 100));
const preparation = computed(() => preparationStatus(summary.value?.prepare ?? null));
const audio = computed(() => {
  const book = summary.value;
  if (!book) return "";
  const status = preparation.value, size = formatBytes(book.audioBytes);
  if (status.state === "done") return `${status.label} · ${size}`;
  if (status.state) return describePreparation(status);
  return book.audioBytes ? `${size} generated` : "Not prepared yet";
});
const facts = computed(() => {
  const view = overview.value, info = details.value;
  if (!view) return [];
  const list: [string, string][] = [["Chapters", String(view.chapters.length)]];
  if (view.wordCount) {
    list.push(["Words", view.wordCount.toLocaleString()]);
    list.push(["Listening time", `${formatDuration(speechMs(view.wordCount, player.state.speed))} at ${player.state.speed}×`]);
  }
  if (view.language) list.push(["Language", view.language]);
  if (info?.publisher) list.push(["Publisher", info.publisher]);
  if (info?.published) list.push(["Published", info.published]);
  if (info?.isbn) list.push(["ISBN", info.isbn]);
  if (audio.value) list.push(["Audio", audio.value]);
  return list;
});
const longDescription = computed(() => (details.value?.description?.length ?? 0) > 420);
const notes = computed(() => (overview.value?.warnings.length ?? 0) + (overview.value?.excluded.length ?? 0));

async function start() {
  await reader.open(props.bookId);
  await player.play(props.bookId, null, null);
}
async function chapter(index: number) {
  await reader.open(props.bookId);
  await reader.startChapter(index);
}
</script>

<template>
  <article class="book-page" :aria-label="`About ${title}`" :aria-busy="loading">
    <button type="button" class="book-page__back" aria-label="Back to library" @click="navigation.view = 'library'"><ArrowLeft :size="16" /> Library</button>
    <p v-if="error" class="book-page__error" role="alert">{{ error }}</p>
    <div class="book-page__hero">
      <CoverArt class="book-page__cover" :title="title" :url="summary?.coverUrl" />
      <div class="book-page__intro">
        <template v-if="title"><h1 class="book-page__title">{{ title }}</h1><p class="book-page__authors">{{ authors }}</p></template>
        <UiSkeleton v-else-if="loading" class="book-page__ghost-title" />
        <div class="book-page__actions">
          <button v-if="progress" type="button" class="book-page__start" aria-label="Resume listening" @click="start"><Play :size="18" fill="currentColor" /> Resume</button>
          <button v-else type="button" class="book-page__start" aria-label="Start listening" @click="start"><Play :size="18" fill="currentColor" /> Start listening</button>
          <button v-if="summary && preparation.actions.includes('prepare')" type="button" class="book-page__secondary" :aria-label="ACTION_LABELS.prepare" @click="runPrepareAction(bookId, 'prepare')"><Sparkles :size="16" /> {{ ACTION_LABELS.prepare }}</button>
          <span v-if="progress" class="book-page__progress"><span class="book-page__track"><i :style="{ width: `${percent}%` }" /></span>{{ percent }}% listened</span>
        </div>
        <template v-if="loading">
          <p class="visually-hidden" role="status" aria-label="Loading book details">Loading book details…</p>
          <div class="book-page__facts" aria-hidden="true"><UiSkeleton v-for="n in 4" :key="n" class="book-page__ghost-fact" /></div>
          <div class="book-page__ghost-text" aria-hidden="true"><UiSkeleton v-for="n in 4" :key="n" class="book-page__ghost-line" /></div>
        </template>
        <dl v-if="facts.length" class="book-page__facts">
          <div v-for="[label, value] in facts" :key="label" class="book-page__fact"><dt>{{ label }}</dt><dd>{{ value }}</dd></div>
        </dl>
        <div v-if="details?.description" class="book-page__about">
          <p class="book-page__description" :class="{ 'book-page__description--clamped': longDescription && !expanded }">{{ details.description }}</p>
          <button v-if="longDescription" type="button" class="book-page__more" :aria-expanded="expanded" @click="expanded = !expanded">{{ expanded ? "Less" : "More" }}</button>
        </div>
        <ul v-if="details?.subjects.length" class="book-page__subjects" aria-label="Subjects">
          <li v-for="subject in details.subjects.slice(0, 10)" :key="subject">{{ subject }}</li>
        </ul>
      </div>
    </div>

    <div v-if="loading" class="book-page__lower" aria-hidden="true"><UiSkeleton class="book-page__ghost-card" /><UiSkeleton class="book-page__ghost-card book-page__ghost-card--side" /></div>
    <div v-if="overview" class="book-page__lower">
      <section class="book-page__card">
        <h2 class="book-page__heading">Contents</h2>
        <ol class="book-page__chapters" aria-label="Contents">
          <li v-for="(c, i) in overview.chapters" :key="c.index">
            <button type="button" class="book-page__chapter" :class="{ 'book-page__chapter--current': progress?.chapter === c.index }" :aria-label="`Play chapter ${i + 1}: ${c.title}`" @click="chapter(c.index)">
              <span class="book-page__number">{{ i + 1 }}</span><span class="book-page__chapter-title">{{ c.title }}</span><AudioLines v-if="progress?.chapter === c.index" :size="15" class="book-page__here" aria-hidden="true" />
            </button>
          </li>
        </ol>
      </section>
      <section class="book-page__card book-page__side">
        <BookListening v-if="listening.length" class="book-page__listening" :days="listening" :word-count="overview.wordCount" :fraction="progress?.fraction ?? 0" :speed="player.state.speed" />
        <h2 class="book-page__heading">Details online</h2>
        <MetadataLookup :book-id="bookId" :matched="Boolean(details?.openLibraryKey)" @applied="details = $event" />
        <details v-if="notes" class="book-page__notes">
          <summary>Extraction notes · {{ notes }}</summary>
          <p v-for="section in overview.excluded" :key="section.title"><strong>{{ section.title }}</strong> left out: {{ section.reason }}</p>
          <p v-for="warning in overview.warnings" :key="warning">{{ warning }}</p>
        </details>
      </section>
    </div>
  </article>
</template>

<style scoped>
.book-page { max-width: 1120px; margin: 0 auto; padding: 8px 28px 40px; display: grid; gap: 28px; }
.book-page__back { justify-self: start; display: inline-flex; align-items: center; gap: 6px; height: 34px; padding: 0 14px 0 10px; border-radius: 999px; background: var(--glass); font-weight: 700; font-size: 13px; }
.book-page__error { color: var(--accent); font-weight: 600; }
.book-page__hero { display: grid; grid-template-columns: 260px minmax(0, 1fr); gap: 40px; align-items: start; }
.book-page__cover { font-size: 22px; border-radius: 16px; }
.book-page__intro { display: grid; gap: 14px; min-width: 0; align-content: start; padding-top: 6px; }
.book-page__title { font-size: 40px; font-weight: 800; letter-spacing: -.035em; line-height: 1.04; overflow-wrap: anywhere; }
.book-page__authors { margin-top: -6px; font-size: 16px; font-weight: 600; color: var(--dim); }
.book-page__actions { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; margin-top: 6px; }
.book-page__start { display: inline-flex; align-items: center; gap: 9px; height: 48px; padding: 0 26px; border-radius: 999px; background: var(--accent); color: var(--on-accent); font-weight: 800; font-size: 15px; box-shadow: 0 14px 30px -14px var(--accent); }
.book-page__secondary { display: inline-flex; align-items: center; gap: 8px; height: 48px; padding: 0 20px; border-radius: 999px; background: var(--glass); font-weight: 700; font-size: 14px; }
.book-page__progress { display: inline-flex; align-items: center; gap: 10px; margin-left: 6px; color: var(--dim); font-weight: 600; font-size: 13px; font-variant-numeric: tabular-nums; }
.book-page__track { position: relative; width: 120px; height: 5px; border-radius: 3px; background: var(--line); overflow: hidden; }
.book-page__track i { position: absolute; inset: 0 auto 0 0; background: var(--accent); }
.book-page__facts { display: grid; grid-template-columns: repeat(auto-fill, minmax(168px, 1fr)); gap: 8px; margin: 8px 0 0; }
.book-page__fact { display: grid; gap: 4px; align-content: start; padding: 12px 14px; border-radius: 14px; background: var(--glass); min-width: 0; }
.book-page__fact dt { font-size: 11px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--dim); }
.book-page__fact dd { margin: 0; font-weight: 700; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.book-page__about { display: grid; gap: 4px; justify-items: start; }
.book-page__description { font: 400 17px/1.6 var(--font-read); color: var(--reading); white-space: pre-line; max-width: 68ch; }
.book-page__description--clamped { display: -webkit-box; -webkit-line-clamp: 6; -webkit-box-orient: vertical; overflow: hidden; }
.book-page__more { font-weight: 700; font-size: 13px; color: var(--accent); }
.book-page__subjects { display: flex; flex-wrap: wrap; gap: 6px; padding: 0; margin: 0; list-style: none; }
.book-page__subjects li { padding: 4px 10px; border-radius: 999px; background: var(--wash); color: var(--dim); font-size: 12px; font-weight: 600; }
.book-page__lower { display: grid; grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr); gap: 20px; align-items: start; }
.book-page__listening { padding-bottom: 16px; margin-bottom: 4px; border-bottom: 1px solid var(--line); }
.book-page__card { display: grid; gap: 12px; padding: 20px; border-radius: 20px; background: var(--page); }
.book-page__heading { font-size: 12px; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--dim); }
.book-page__chapters { display: grid; gap: 2px; padding: 0; margin: 0; list-style: none; max-height: 420px; overflow-y: auto; }
.book-page__chapter { display: grid; grid-template-columns: 28px minmax(0, 1fr) auto; gap: 8px; align-items: center; width: 100%; padding: 9px 10px; border-radius: 10px; text-align: left; font-weight: 600; color: var(--reading); font-variant-numeric: tabular-nums; }
.book-page__chapter:hover { background: var(--wash); }
.book-page__chapter--current { background: var(--glass); color: var(--ink); font-weight: 800; }
.book-page__number { color: var(--faint); font-size: 12.5px; }
.book-page__chapter-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.book-page__here { color: var(--accent); }
.book-page__ghost-title { width: min(420px, 80%); height: 42px; border-radius: 12px; }
.book-page__ghost-fact { height: 62px; border-radius: 14px; }
.book-page__ghost-text { display: grid; gap: 10px; max-width: 68ch; margin-top: 4px; }
.book-page__ghost-line { height: 14px; border-radius: 7px; }
.book-page__ghost-line:last-child { width: 60%; }
.book-page__ghost-card { height: 300px; border-radius: 20px; }
.book-page__ghost-card--side { height: 160px; }
.book-page__notes { font-size: 12.5px; color: var(--dim); display: grid; gap: 6px; }
.book-page__notes summary { cursor: pointer; font-weight: 700; }
.book-page__notes p { margin-top: 6px; }
@media (max-width: 860px) {
  .book-page { padding: 8px 16px 32px; }
  .book-page__hero { grid-template-columns: 150px minmax(0, 1fr); gap: 22px; }
  .book-page__title { font-size: 28px; }
  .book-page__lower { grid-template-columns: minmax(0, 1fr); }
}
@media (max-width: 560px) { .book-page__hero { grid-template-columns: minmax(0, 1fr); } .book-page__cover { width: 160px; } }
</style>
