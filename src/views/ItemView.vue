<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  AudioLines,
  BookOpen,
  Check,
  CircleCheck,
  FolderPlus,
  Headphones,
  Library,
  Mic,
  RotateCcw,
  Settings2,
  Sparkles,
  TriangleAlert,
} from "@lucide/vue";
import { api } from "../api";
import type { BookDetails, BookSettings, BookView, ChapterAudioView, DayListening } from "../api/types";
import CoverArt from "../components/CoverArt.vue";
import UiMenu from "../components/ui/UiMenu.vue";
import type { MenuEntry } from "../components/ui/types";
import UiSkeleton from "../components/ui/UiSkeleton.vue";
import MetadataLookup from "../components/book/MetadataLookup.vue";
import BookListening from "../components/book/BookListening.vue";
import BookNarration from "../components/book/BookNarration.vue";
import { useCollections } from "../composables/useCollections";
import { useRecordingsStore } from "../stores/recordings";
import { formatBytes } from "../bytes";
import { formatDuration, readingMs, speechMs } from "../estimate";
import { useLibraryStore } from "../stores/library";
import { useNavigationStore } from "../stores/navigation";
import { usePlayerStore } from "../stores/player";
import { useReaderStore } from "../stores/reader";
import {
  ACTION_ICONS,
  ACTION_LABELS,
  describePreparation,
  prepareChapter,
  preparationStatus,
  runPrepareAction,
} from "../preparation";

/** An item's page: what it is, its text, what it plays with, its contents and its details
 *  online. Nothing plays until the person asks: Read opens the text, Listen plays it. */
const props = defineProps<{ bookId: string }>();
const library = useLibraryStore(),
  navigation = useNavigationStore(),
  player = usePlayerStore(),
  reader = useReaderStore(),
  recordings = useRecordingsStore(),
  shelves = useCollections();
const overview = ref<BookView | null>(null),
  details = ref<BookDetails | null>(null),
  settings = ref<BookSettings | null>(null),
  listening = ref<DayListening[]>([]),
  error = ref("");
const expanded = ref(false),
  notesOpen = ref(false);
const loading = computed(() => !overview.value && !error.value);
onMounted(async () => {
  try {
    const [view, info, days, book] = await Promise.all([
      api.bookOverview(props.bookId),
      api.bookDetails(props.bookId),
      api.bookListening(props.bookId).catch(() => []),
      api.bookSettings(props.bookId).catch(() => null),
    ]);
    overview.value = view;
    details.value = info;
    listening.value = days;
    settings.value = book;
  } catch (e) {
    error.value = String(e);
  }
});

const summary = computed(() => library.books.find((b) => b.id === props.bookId) ?? null);
const title = computed(() => summary.value?.title ?? overview.value?.title ?? "");
const authors = computed(
  () => (summary.value?.authors ?? overview.value?.authors ?? []).join(", ") || "Unknown author",
);
const progress = computed(() => summary.value?.progress ?? overview.value?.progress ?? null);
const percent = computed(() => Math.round((progress.value?.fraction ?? 0) * 100));
const finished = computed(() => summary.value?.readState === "finished");
const state = computed(() => (finished.value ? "finished" : progress.value ? `${percent.value}%` : "unread"));
async function markRead() {
  try {
    await library.markRead(props.bookId, finished.value ? "unread" : "finished");
  } catch (e) {
    error.value = String(e);
  }
}
/** The Audio card's body: a recording (syncing or synced), speech, or nothing set up yet. */
const audio = computed<"none" | "speech" | "recording">(() => {
  const book = summary.value;
  if (!book) return "none";
  if (book.recording || recordings.byBook[props.bookId]) return "recording";
  return book.audio === "speech" ? "speech" : "none";
});
const recorded = computed(() => summary.value?.audio === "recording");
const preparation = computed(() => preparationStatus(summary.value?.prepare ?? null));
const prepared = computed(() => {
  const book = summary.value;
  if (!book) return "";
  const status = preparation.value,
    size = formatBytes(book.audioBytes);
  if (status.state === "done") return `${status.label} · ${size}`;
  if (status.state) return describePreparation(status);
  return book.audioBytes ? `${size} made so far` : "Nothing made yet";
});
const engineLine = computed(() => {
  const engine = player.engine;
  if (engine.state === "ready" || engine.state === "busy")
    return `Read aloud by ${engine.profileName} · starts in a few seconds`;
  if (engine.state === "error")
    return `${engine.profileName} has a problem${engine.detail ? `: ${engine.detail}` : ""}`;
  return `${engine.profileName} is ${engine.state}`;
});
const words = computed(() => overview.value?.wordCount ?? 0);
const textFacts = computed(() => {
  const view = overview.value,
    info = details.value;
  if (!view) return [];
  const list = [`${view.chapters.length} ${view.chapters.length === 1 ? "chapter" : "chapters"}`];
  if (words.value) list.push(`${words.value.toLocaleString()} words`);
  if (view.language) list.push(view.language);
  if (info?.publisher) list.push(info.publisher);
  if (info?.published) list.push(info.published);
  if (info?.isbn) list.push(`ISBN ${info.isbn}`);
  return list;
});
/** Each chapter's audio, refreshed while the book is being prepared (at most every 2 s). */
const chapterAudio = ref<ChapterAudioView[]>([]);
async function loadChapterAudio() {
  try {
    chapterAudio.value = await api.chapterAudio(props.bookId);
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
watch(() => [summary.value?.prepare, player.generation.preparing?.done], refreshChapterAudio, { deep: true });
type ChapterState = { kind: "ready" } | { kind: "queued"; label: string } | { kind: "none" } | { kind: "silent" };
function chapterState(index: number): ChapterState {
  const audio = chapterAudio.value[index];
  if (audio && audio.total === 0) return { kind: "silent" };
  if (audio && audio.done === audio.total) return { kind: "ready" };
  const prepare = summary.value?.prepare;
  if (prepare && prepare.state !== "done" && (!prepare.chapters || prepare.chapters.includes(index))) {
    return {
      kind: "queued",
      label: [preparationStatus(prepare).label, audio ? `${audio.done} / ${audio.total}` : null]
        .filter(Boolean)
        .join(" · "),
    };
  }
  return { kind: "none" };
}
async function prepareOne(index: number) {
  await prepareChapter(props.bookId, index);
  await library.refresh();
  await loadChapterAudio();
}
const longDescription = computed(() => (details.value?.description?.length ?? 0) > 420);
const notes = computed(() => (overview.value?.warnings.length ?? 0) + (overview.value?.excluded.length ?? 0));

/** The item is listened to: Listen is the main action. */
const listened = computed(() => (summary.value?.mode ?? overview.value?.mode) === "listen");
/** Read opens the text where it was left, to be read silently (pausing it if it plays). */
async function read() {
  try {
    if (listened.value) await api.setMode(props.bookId, "read");
  } catch (e) {
    error.value = String(e);
    return;
  }
  await reader.open(props.bookId);
}
/** Listen opens the text and plays from the saved place. */
async function listen() {
  await reader.open(props.bookId);
  await player.play(props.bookId, null, null);
}
async function chapter(index: number) {
  await reader.open(props.bookId);
  await reader.startChapter(index);
}
async function pair() {
  try {
    const paths = await api.chooseAudio(false);
    if (paths?.length) recordings.openPairing(paths, props.bookId);
  } catch (e) {
    error.value = String(e);
  }
}
const collectionItems = computed<MenuEntry[]>(() => {
  const book = summary.value;
  if (!book) return [];
  return [
    ...library.collections.map((c) => ({
      id: `collection:${c.id}`,
      label: c.name,
      ariaLabel: c.name,
      checked: book.collections.includes(c.id),
      action: () => shelves.toggle(book, c.id),
    })),
    ...(library.collections.length ? [{ kind: "separator" as const }] : []),
    { id: "new-collection", label: "New collection…", icon: FolderPlus, action: () => shelves.create(book) },
  ];
});
</script>

<template>
  <article class="item-page" :aria-label="`About ${title}`" :aria-busy="loading">
    <p v-if="error" class="item-page__error" role="alert">{{ error }}</p>
    <div class="item-page__hero">
      <CoverArt class="item-page__cover" :title="title" :url="summary?.coverUrl" />
      <div class="item-page__intro">
        <span class="item-page__kind">Book · {{ state }}</span>
        <template v-if="title"
          ><h1 class="item-page__title">{{ title }}</h1>
          <p class="item-page__authors">{{ authors }}</p></template
        >
        <UiSkeleton v-else-if="loading" class="item-page__ghost-title" />
        <div class="item-page__actions">
          <button
            type="button"
            :class="listened ? 'item-page__secondary' : 'item-page__primary'"
            aria-label="Read"
            @click="read"
          >
            <BookOpen :size="18" aria-hidden="true" /> Read
          </button>
          <button
            type="button"
            :class="listened ? 'item-page__primary' : 'item-page__secondary'"
            aria-label="Listen"
            @click="listen"
          >
            <Headphones :size="18" aria-hidden="true" /> Listen
          </button>
          <UiMenu v-if="summary" v-slot="{ toggle, trigger }" label="Collections" :items="collectionItems" :width="260">
            <button
              type="button"
              class="item-page__secondary"
              aria-label="Add to collection"
              v-bind="trigger"
              @click="toggle"
            >
              <Library :size="17" aria-hidden="true" /> Add to collection
            </button>
          </UiMenu>
          <button v-if="summary" type="button" class="item-page__secondary" @click="markRead">
            <component :is="finished ? RotateCcw : CircleCheck" :size="17" aria-hidden="true" />
            {{ finished ? "Mark as unread" : "Mark as finished" }}
          </button>
        </div>
        <template v-if="loading">
          <p class="visually-hidden" role="status" aria-label="Loading book details">Loading book details…</p>
          <div class="item-page__ghost-text" aria-hidden="true">
            <UiSkeleton v-for="n in 4" :key="n" class="item-page__ghost-line" />
          </div>
        </template>
        <div v-if="details?.description" class="item-page__about">
          <p
            class="item-page__description"
            :class="{ 'item-page__description--clamped': longDescription && !expanded }"
          >
            {{ details.description }}
          </p>
          <button
            v-if="longDescription"
            type="button"
            class="item-page__more"
            :aria-expanded="expanded"
            @click="expanded = !expanded"
          >
            {{ expanded ? "Less" : "More" }}
          </button>
        </div>
        <ul v-if="details?.subjects.length" class="item-page__subjects" aria-label="Subjects">
          <li v-for="subject in details.subjects.slice(0, 10)" :key="subject">{{ subject }}</li>
        </ul>
      </div>
    </div>

    <div v-if="overview" class="item-page__cards">
      <section class="item-page__card" aria-labelledby="item-text">
        <div class="item-page__card-head">
          <h2 id="item-text" class="item-page__heading">Text</h2>
          <span class="item-page__badge item-page__badge--ok"><i aria-hidden="true" />EPUB</span>
        </div>
        <b class="item-page__lead">{{
          words ? `About ${formatDuration(readingMs(words, library.readingWpm))} to read` : "No readable words"
        }}</b>
        <span class="item-page__facts">{{ textFacts.join(" · ") }}</span>
        <button
          v-if="notes"
          type="button"
          class="item-page__notes-link"
          :aria-expanded="notesOpen"
          aria-controls="item-notes"
          @click="notesOpen = !notesOpen"
        >
          <TriangleAlert :size="15" aria-hidden="true" /> {{ notes }} extraction
          {{ notes === 1 ? "note" : "notes" }}
        </button>
        <div v-if="notes && notesOpen" id="item-notes" class="item-page__notes">
          <p v-for="section in overview.excluded" :key="section.title">
            <strong>{{ section.title }}</strong> left out: {{ section.reason }}
          </p>
          <p v-for="warning in overview.warnings" :key="warning">{{ warning }}</p>
        </div>
      </section>

      <section class="item-page__card" aria-labelledby="item-audio" :data-audio="audio">
        <div class="item-page__card-head">
          <h2 id="item-audio" class="item-page__heading">Audio</h2>
          <span class="item-page__badge">{{
            audio === "recording" ? "Recording" : audio === "speech" ? "Voice" : "None yet"
          }}</span>
        </div>
        <template v-if="audio === 'none'">
          <b class="item-page__lead">Listen to this book</b>
          <button type="button" class="item-page__option" aria-label="Narrate with a voice" @click="listen">
            <Mic :size="20" aria-hidden="true" />
            <span
              ><b>Narrate with a voice</b><small>{{ engineLine }}</small></span
            >
          </button>
          <button type="button" class="item-page__option" aria-label="Pair an audiobook" @click="pair">
            <AudioLines :size="20" aria-hidden="true" />
            <span
              ><b>Pair an audiobook</b><small>M4B, MP3, FLAC or Ogg · plays with read-along highlighting</small></span
            >
          </button>
        </template>
        <template v-else-if="audio === 'speech'">
          <b class="item-page__lead">{{ settings?.voiceId ?? "The engine's default voice" }}</b>
          <span class="item-page__facts">{{ player.engine.profileName }} · {{ prepared }}</span>
          <small
            v-if="player.engine.state !== 'ready' && player.engine.state !== 'busy'"
            class="item-page__engine"
            role="status"
            >{{ engineLine }}</small
          >
          <div class="item-page__row">
            <button
              v-for="action in preparation.actions"
              :key="action"
              type="button"
              class="item-page__small"
              :aria-label="ACTION_LABELS[action]"
              @click="runPrepareAction(bookId, action)"
            >
              <component :is="ACTION_ICONS[action]" :size="15" aria-hidden="true" /> {{ ACTION_LABELS[action] }}
            </button>
            <button type="button" class="item-page__small" @click="navigation.openSettings('engine')">
              <Settings2 :size="15" aria-hidden="true" /> Voice settings
            </button>
          </div>
          <button type="button" class="item-page__link" aria-label="Pair an audiobook" @click="pair">
            Pair an audiobook instead
          </button>
        </template>
        <template v-else>
          <BookNarration :book-id="bookId" />
        </template>
        <BookListening
          v-if="listening.length"
          class="item-page__listening"
          :days="listening"
          :word-count="overview.wordCount"
          :fraction="progress?.fraction ?? 0"
          :speed="player.state.speed"
        />
      </section>
    </div>
    <div v-else-if="loading" class="item-page__cards" aria-hidden="true">
      <UiSkeleton class="item-page__ghost-card" /><UiSkeleton class="item-page__ghost-card" />
    </div>

    <section v-if="overview" class="item-page__card" aria-labelledby="item-contents">
      <h2 id="item-contents" class="item-page__heading">Contents</h2>
      <ol class="item-page__chapters" aria-label="Contents">
        <li v-for="(c, i) in overview.chapters" :key="c.index">
          <button
            type="button"
            class="item-page__chapter"
            :class="{ 'item-page__chapter--current': progress?.chapter === c.index }"
            :aria-label="`Open chapter ${i + 1}: ${c.title}`"
            @click="chapter(c.index)"
          >
            <span class="item-page__number">{{ i + 1 }}</span><span class="item-page__chapter-title">{{ c.title }}</span
            ><AudioLines v-if="progress?.chapter === c.index" :size="15" class="item-page__here" aria-hidden="true" />
          </button>
          <template v-if="!recorded">
            <span
              v-if="chapterState(c.index).kind === 'ready'"
              class="item-page__chapter-audio item-page__chapter-audio--ready"
              title="This chapter's audio is made: it plays at once, even offline"
              ><Check :size="14" aria-hidden="true" /> Ready</span
            >
            <span v-else-if="chapterState(c.index).kind === 'queued'" class="item-page__chapter-audio">{{
              (chapterState(c.index) as { label: string }).label
            }}</span>
            <button
              v-else-if="chapterState(c.index).kind === 'none'"
              type="button"
              class="item-page__chapter-prepare"
              :aria-label="`Prepare chapter ${i + 1}: ${c.title}`"
              title="Prepare this chapter"
              @click="prepareOne(c.index)"
            >
              <Sparkles :size="14" aria-hidden="true" />
            </button>
          </template>
        </li>
      </ol>
    </section>

    <section v-if="overview" class="item-page__card" aria-labelledby="item-online">
      <h2 id="item-online" class="item-page__heading">Details online</h2>
      <MetadataLookup :book-id="bookId" :matched="Boolean(details?.openLibraryKey)" @applied="details = $event" />
    </section>
  </article>
</template>

<style scoped>
.item-page {
  max-width: 1120px;
  padding: 8px 36px 0;
  display: flex;
  flex-direction: column;
  gap: 28px;
}
.item-page__error {
  margin: 0;
  color: var(--accent);
  font-weight: 600;
}
.item-page__hero {
  display: flex;
  flex-wrap: wrap;
  gap: 32px;
}
/* A fixed width and padding: CoverArt's 10% padding would be taken from the whole row. */
.item-page__hero .item-page__cover {
  flex: none;
  width: 200px;
  padding: 20px;
  align-self: flex-start;
  border-radius: 10px;
  font-size: 18px;
  box-shadow: 0 18px 36px -10px var(--shadow);
}
.item-page__intro {
  flex: 1 1 360px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.item-page__kind,
.item-page__heading {
  margin: 0;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--dim);
}
.item-page__title {
  margin: 0;
  font-size: 36px;
  font-weight: 700;
  letter-spacing: -0.02em;
  line-height: 1.1;
}
.item-page__authors {
  margin: 0;
  font-size: 16px;
  color: var(--dim);
}
.item-page__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 8px;
}
.item-page__primary,
.item-page__secondary {
  height: 48px;
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 0 22px;
  border-radius: 24px;
  font-size: 15px;
  font-weight: 600;
  background: var(--page);
}
.item-page__primary {
  padding: 0 26px;
  background: var(--accent);
  color: var(--on-accent);
  font-weight: 700;
}
.item-page__about {
  margin-top: 10px;
  max-width: 60ch;
}
.item-page__description {
  margin: 0;
  font-family: var(--font-read);
  font-size: 18px;
  line-height: 1.5;
  color: var(--reading);
  white-space: pre-line;
}
.item-page__description--clamped {
  display: -webkit-box;
  -webkit-line-clamp: 6;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.item-page__more {
  margin-top: 6px;
  font-weight: 700;
  color: var(--dim);
}
.item-page__subjects {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.item-page__subjects li {
  padding: 4px 10px;
  border-radius: 999px;
  background: var(--glass);
  font-size: 12px;
  color: var(--dim);
}
.item-page__cards {
  display: flex;
  flex-wrap: wrap;
  gap: 20px;
  align-items: stretch;
}
.item-page__cards > .item-page__card {
  flex: 1 1 320px;
  min-width: 0;
}
.item-page__card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 20px;
  border-radius: 20px;
  background: var(--page);
}
.item-page__card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.item-page__badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 700;
  color: var(--dim);
}
.item-page__badge--ok {
  color: var(--ok);
}
.item-page__badge i {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: currentColor;
}
.item-page__lead {
  font-size: 20px;
  font-weight: 700;
}
.item-page__facts {
  color: var(--dim);
}
.item-page__engine {
  color: var(--warn);
  font-weight: 600;
}
.item-page__notes-link {
  margin-top: auto;
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--warn);
  font-weight: 600;
}
.item-page__notes {
  font-size: 13px;
  color: var(--dim);
}
.item-page__notes p {
  margin: 0 0 6px;
}
.item-page__option {
  min-height: 60px;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: 14px;
  background: var(--ground);
  text-align: left;
}
.item-page__option:hover {
  background: var(--wash);
}
.item-page__option span {
  flex: 1;
  display: grid;
}
.item-page__option small {
  font-size: 12px;
  color: var(--dim);
}
.item-page__row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.item-page__small {
  height: 36px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 0 14px;
  border-radius: 18px;
  background: var(--ground);
  font-weight: 600;
  font-size: 13px;
}
.item-page__small:hover {
  background: var(--wash);
}
.item-page__link {
  align-self: flex-start;
  font-size: 13px;
  font-weight: 600;
  color: var(--dim);
}
.item-page__link:hover {
  color: var(--ink);
}
.item-page__listening {
  margin-top: 8px;
}
.item-page__chapters {
  margin: 0;
  padding: 0;
  list-style: none;
}
.item-page__chapters li {
  display: flex;
  align-items: center;
  gap: 10px;
  border-bottom: 1px solid var(--wash);
}
.item-page__chapters li:last-child {
  border-bottom: 0;
}
.item-page__chapter {
  flex: 1;
  min-width: 0;
  min-height: 44px;
  display: flex;
  align-items: center;
  gap: 18px;
  text-align: left;
}
.item-page__chapter:hover .item-page__chapter-title {
  color: var(--accent);
}
.item-page__chapter--current .item-page__chapter-title {
  font-weight: 700;
}
.item-page__number {
  width: 20px;
  flex: none;
  text-align: right;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}
.item-page__chapter-title {
  flex: 1;
  min-width: 0;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item-page__here {
  color: var(--accent);
}
.item-page__chapter-audio {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12.5px;
  color: var(--dim);
  white-space: nowrap;
}
.item-page__chapter-audio--ready {
  color: var(--ok);
  font-weight: 600;
}
.item-page__chapter-prepare {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  color: var(--dim);
  opacity: 0;
}
.item-page__chapters li:hover .item-page__chapter-prepare,
.item-page__chapter-prepare:focus-visible {
  opacity: 1;
}
.item-page__chapter-prepare:hover {
  background: var(--wash);
  color: var(--ink);
}
@media (hover: none) {
  .item-page__chapter-prepare {
    opacity: 1;
  }
}
.item-page__ghost-title {
  height: 40px;
  width: 60%;
  border-radius: 10px;
}
.item-page__ghost-text {
  display: grid;
  gap: 8px;
  margin-top: 10px;
}
.item-page__ghost-line {
  height: 14px;
  border-radius: 7px;
}
.item-page__ghost-line:last-child {
  width: 60%;
}
.item-page__ghost-card {
  flex: 1 1 320px;
  height: 200px;
  border-radius: 20px;
}
@media (max-width: 719px) {
  .item-page {
    padding: 0 16px;
  }
  .item-page__hero {
    gap: 20px;
  }
  .item-page__hero .item-page__cover {
    width: 140px;
    padding: 14px;
  }
  .item-page__title {
    font-size: 28px;
  }
}
</style>
