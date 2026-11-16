<script setup lang="ts">
import UiCard from "@/components/ui/UiCard.vue";
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onMounted, ref, watch } from "vue";
import {
  AudioLines,
  BookOpen,
  CircleCheck,
  ExternalLink,
  FolderPlus,
  Headphones,
  Library,
  Mic,
  Plus,
  RefreshCw,
  RotateCcw,
  Settings2,
  TriangleAlert,
} from "@lucide/vue";
import { api } from "@/api";
import type { BookDetails, BookSettings, BookView, DayListening } from "@/api/types";
import CoverArt from "@/components/CoverArt.vue";
import UiMenu from "@/components/ui/UiMenu.vue";
import type { MenuEntry } from "@/components/ui/types";
import UiSkeleton from "@/components/ui/UiSkeleton.vue";
import MetadataLookup from "@/components/book/MetadataLookup.vue";
import BookListening from "@/components/book/BookListening.vue";
import BookNarration from "@/components/book/BookNarration.vue";
import BookContents from "@/components/book/BookContents.vue";
import ExtractionNotes from "@/components/book/ExtractionNotes.vue";
import { useCollections } from "@/composables/useCollections";
import { useRecordingsStore } from "@/stores/recordings";
import { useArticlesStore } from "@/stores/articles";
import { fetching, NEEDS, siteOf } from "@/articles";
import { formatBytes } from "@/bytes";
import { formatClock, formatDuration, readingMs, speechMs } from "@/estimate";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
import { ACTION_ICONS, ACTION_LABELS, describePreparation, preparationStatus, runPrepareAction } from "@/preparation";

/** An item's page: what it is, its text, what it plays with, its contents and its details
 *  online. Nothing plays until the person asks: Read opens the text, Listen plays it. */
const props = defineProps<{ bookId: string }>();
const library = useLibraryStore(),
  navigation = useNavigationStore(),
  player = usePlayerStore(),
  reader = useReaderStore(),
  recordings = useRecordingsStore(),
  articles = useArticlesStore(),
  shelves = useCollections();
const overview = ref<BookView | null>(null),
  details = ref<BookDetails | null>(null),
  settings = ref<BookSettings | null>(null),
  listening = ref<DayListening[]>([]),
  error = ref("");
const expanded = ref(false);
/** An article with no copy yet: it has no text, and its fetch says why. */
const unfetched = ref(false);
const loading = computed(() => !overview.value && !error.value && !unfetched.value);
async function load() {
  try {
    const [view, info, days, book] = await Promise.all([
      api.bookOverview({ bookId: props.bookId }),
      api.bookDetails({ bookId: props.bookId }),
      api.bookListening({ bookId: props.bookId }).catch(() => []),
      api.bookSettings({ bookId: props.bookId }).catch(() => null),
    ]);
    overview.value = view;
    details.value = info;
    listening.value = days;
    settings.value = book;
    unfetched.value = false;
  } catch (e) {
    const article = await articles.load(props.bookId).then(
      () => true,
      () => false,
    );
    if (article) unfetched.value = true;
    else error.value = String(e);
  }
}
onMounted(load);

const summary = computed(() => library.books.find((b) => b.id === props.bookId) ?? null);
const title = computed(() => summary.value?.title ?? overview.value?.title ?? "");
/** An article without a byline names no one. */
const authors = computed(
  () =>
    (summary.value?.authors ?? overview.value?.authors ?? []).join(", ") ||
    (summary.value?.kind === "article" ? "" : "Unknown author"),
);
const progress = computed(() => summary.value?.progress ?? overview.value?.progress ?? null);
/** What the item is, as the core says: from its view, else (while it has none, such as an
 *  article not fetched yet) from its summary. */
const kind = computed(() => overview.value?.kind ?? summary.value?.kind ?? null);
/** An audiobook on its own: no text, so Listen is its only way in and its parts its contents. */
const audiobook = computed(() => kind.value === "audiobook");
/** An article: from the web, in one chapter; its fetch decides what there is to read. */
const article = computed(() => articles.byId[props.bookId] ?? null);
// An article on a feed is not in the library's list and has no view until fetched; its article
// view says what it is.
const isArticle = computed(() => kind.value === "article" || Boolean(article.value));
watch(
  isArticle,
  (yes) => {
    if (yes) articles.load(props.bookId).catch((e) => (error.value = String(e)));
  },
  { immediate: true },
);
// A new copy was stored: the page shows its text.
watch(
  () => article.value?.fetchedAt,
  (now, before) => {
    if (before !== undefined && now !== before) void load();
  },
);
const busy = computed(() => Boolean(article.value && fetching(article.value.stage)));
const articleLead = computed(() => {
  const view = article.value;
  if (view && fetching(view.stage)) return "Fetching…";
  if (overview.value) return `About ${formatDuration(readingMs(words.value, library.readingWpm))} to read`;
  return view?.stage.kind === "failed" ? "Could not be fetched" : "Not fetched yet";
});
const articleFacts = computed(() => {
  const view = article.value;
  if (!view) return [];
  const list = [siteOf(view)];
  if (overview.value) {
    if (view.capturedWords) list.push(`${view.capturedWords.toLocaleString()} words`);
    if (view.fetchedAt)
      list.push(
        `Fetched ${new Date(view.fetchedAt).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" })}`,
      );
    if (view.via === "archive") list.push("from an archived copy");
    if (view.via === "browser") list.push("rendered in the browser");
  }
  return list;
});
async function openOriginal() {
  await articles.openOriginal(props.bookId).catch((e) => (error.value = String(e)));
}
async function refetch() {
  await articles.refetch(props.bookId, "http").catch((e) => (error.value = String(e)));
}
/** Its recording's files, as the sync state lists them. */
const recordingFiles = computed(() => recordings.byBook[props.bookId]?.files ?? []);
const audiobookLength = computed(
  () => summary.value?.durationMs ?? overview.value?.chapters.reduce((sum, c) => sum + (c.durationMs ?? 0), 0) ?? 0,
);
const audiobookFacts = computed(() => {
  const parts = overview.value?.chapters.length ?? 0,
    files = recordingFiles.value.length;
  return [`${parts} ${parts === 1 ? "part" : "parts"}`, files ? `${files} ${files === 1 ? "file" : "files"}` : null]
    .filter(Boolean)
    .join(" · ");
});
const percent = computed(() => Math.round((progress.value?.fraction ?? 0) * 100));
const finished = computed(() => summary.value?.readState === "finished");
const state = computed(() =>
  finished.value ? "finished" : progress.value ? `${percent.value}%` : audiobook.value ? "unplayed" : "unread",
);
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
const longDescription = computed(() => (details.value?.description?.length ?? 0) > 420);

/** The item is listened to: Listen is the main action. */
const listened = computed(() => (summary.value?.mode ?? overview.value?.mode) === "listen");
/** Read opens the text where it was left, to be read silently (pausing it if it plays). */
async function read() {
  try {
    if (listened.value) await api.setMode({ bookId: props.bookId, mode: "read" });
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
async function pair() {
  try {
    const paths = await api.chooseAudio({ folder: false });
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
      <CoverArt
        class="item-page__cover"
        :title="title"
        :url="summary?.coverUrl"
        :shape="audiobook ? 'square' : 'portrait'"
      />
      <div class="item-page__intro">
        <span class="item-page__kind"
          >{{ audiobook ? "Audiobook" : isArticle ? "Article" : "Book" }} · {{ state }}</span
        >
        <template v-if="title"
          ><h1 class="item-page__title">{{ title }}</h1>
          <p v-if="authors" class="item-page__authors">{{ authors }}</p></template
        >
        <UiSkeleton v-else-if="loading" class="item-page__ghost-title" />
        <div class="item-page__actions">
          <UiButton
            v-if="!audiobook && !unfetched"
            size="lg"
            :variant="listened ? 'raised' : 'accent'"
            aria-label="Read"
            @click="read"
          >
            <BookOpen :size="18" aria-hidden="true" /> Read
          </UiButton>
          <UiButton
            v-if="!unfetched"
            size="lg"
            :variant="listened || audiobook ? 'accent' : 'raised'"
            aria-label="Listen"
            @click="listen"
          >
            <Headphones :size="18" aria-hidden="true" /> Listen
          </UiButton>
          <UiMenu v-if="summary" v-slot="{ toggle, trigger }" label="Collections" :items="collectionItems" :width="260">
            <UiButton variant="raised" size="lg" aria-label="Add to collection" v-bind="trigger" @click="toggle">
              <Library :size="17" aria-hidden="true" /> Add to collection
            </UiButton>
          </UiMenu>
          <template v-if="isArticle">
            <UiButton variant="raised" size="lg" aria-label="Open original" @click="openOriginal">
              <ExternalLink :size="17" aria-hidden="true" /> Open original
            </UiButton>
            <UiButton variant="raised" size="lg" aria-label="Fetch again" :disabled="busy" @click="refetch">
              <RefreshCw :size="17" aria-hidden="true" /> Fetch again
            </UiButton>
          </template>
          <UiButton variant="raised" size="lg" v-if="summary" @click="markRead">
            <component :is="finished ? RotateCcw : CircleCheck" :size="17" aria-hidden="true" />
            {{ finished ? "Mark as unread" : "Mark as finished" }}
          </UiButton>
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

    <div v-if="overview || unfetched" class="item-page__cards">
      <UiCard v-if="isArticle" class="item-page__card" aria-labelledby="item-text">
        <div class="item-page__card-head">
          <h2 id="item-text" class="item-page__heading">Text</h2>
          <span class="item-page__badge" :class="{ 'item-page__badge--ok': overview }"
            ><i v-if="overview" aria-hidden="true" />Web page</span
          >
        </div>
        <b class="item-page__lead">{{ articleLead }}</b>
        <span class="item-page__facts">{{ articleFacts.join(" · ") }}</span>
        <p v-if="article?.stage.kind === 'needsYou'" class="item-page__problem" role="status">
          <TriangleAlert :size="15" aria-hidden="true" /> Needs you · {{ NEEDS[article.stage.reason] }}
        </p>
        <p v-if="article?.error && !busy" class="item-page__problem" role="alert">
          <TriangleAlert :size="15" aria-hidden="true" /> {{ article.error }}
        </p>
        <ExtractionNotes :warnings="overview?.warnings ?? []" :excluded="overview?.excluded ?? []" />
      </UiCard>
      <UiCard v-else-if="audiobook" class="item-page__card" aria-labelledby="item-text">
        <div class="item-page__card-head">
          <h2 id="item-text" class="item-page__heading">Text</h2>
          <span class="item-page__badge">None</span>
        </div>
        <b class="item-page__lead">No text</b>
        <span class="item-page__facts"
          >Add the EPUB of this book to read it, or to see the words highlighted as they are spoken. Your place in the
          recording is kept.</span
        >
        <div class="item-page__row">
          <UiButton size="sm" @click="recordings.addText(bookId)">
            <Plus :size="15" aria-hidden="true" /> Add the EPUB…
          </UiButton>
        </div>
      </UiCard>
      <UiCard v-else class="item-page__card" aria-labelledby="item-text">
        <div class="item-page__card-head">
          <h2 id="item-text" class="item-page__heading">Text</h2>
          <span class="item-page__badge item-page__badge--ok"><i aria-hidden="true" />EPUB</span>
        </div>
        <b class="item-page__lead">{{
          words ? `About ${formatDuration(readingMs(words, library.readingWpm))} to read` : "No readable words"
        }}</b>
        <span class="item-page__facts">{{ textFacts.join(" · ") }}</span>
        <ExtractionNotes :warnings="overview?.warnings ?? []" :excluded="overview?.excluded ?? []" />
      </UiCard>

      <UiCard v-if="overview" class="item-page__card" aria-labelledby="item-audio" :data-audio="audio">
        <div class="item-page__card-head">
          <h2 id="item-audio" class="item-page__heading">Audio</h2>
          <span class="item-page__badge">{{
            audio === "recording" ? "Recording" : audio === "speech" ? "Voice" : "None yet"
          }}</span>
        </div>
        <template v-if="audiobook">
          <b class="item-page__lead">{{ formatDuration(audiobookLength) }}</b>
          <span class="item-page__facts">{{ audiobookFacts }}</span>
          <p v-for="warning in overview?.warnings ?? []" :key="warning" class="item-page__problem" role="alert">
            <TriangleAlert :size="15" aria-hidden="true" /> {{ warning }}
          </p>
          <ul v-if="recordingFiles.length" class="item-page__files" aria-label="Audio files">
            <li v-for="file in recordingFiles" :key="file.path" :title="file.path">
              <span class="item-page__file-name">{{ file.name }}</span><span class="item-page__file-length">{{
                formatClock(file.durationMs)
              }}</span>
            </li>
          </ul>
        </template>
        <template v-else-if="audio === 'none'">
          <b class="item-page__lead">Listen to this {{ isArticle ? "article" : "book" }}</b>
          <button type="button" class="item-page__option" aria-label="Narrate with a voice" @click="listen">
            <Mic :size="20" aria-hidden="true" />
            <span
              ><b>Narrate with a voice</b><small>{{ engineLine }}</small></span
            >
          </button>
          <button
            v-if="!isArticle"
            type="button"
            class="item-page__option"
            aria-label="Pair an audiobook"
            @click="pair"
          >
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
            <UiButton
              size="sm"
              v-for="action in preparation.actions"
              :key="action"
              :aria-label="ACTION_LABELS[action]"
              @click="runPrepareAction(bookId, action)"
            >
              <component :is="ACTION_ICONS[action]" :size="15" aria-hidden="true" /> {{ ACTION_LABELS[action] }}
            </UiButton>
            <UiButton size="sm" @click="navigation.openSettings('engine')">
              <Settings2 :size="15" aria-hidden="true" /> Voice settings
            </UiButton>
          </div>
          <button v-if="!isArticle" type="button" class="item-page__link" aria-label="Pair an audiobook" @click="pair">
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
          :word-count="overview!.wordCount"
          :duration-ms="summary?.durationMs ?? null"
          :fraction="progress?.fraction ?? 0"
          :speed="player.state.speed"
        />
      </UiCard>
    </div>
    <div v-else-if="loading" class="item-page__cards" aria-hidden="true">
      <UiSkeleton class="item-page__ghost-card" /><UiSkeleton class="item-page__ghost-card" />
    </div>

    <UiCard v-if="overview && !isArticle" class="item-page__card" aria-labelledby="item-contents">
      <h2 id="item-contents" class="item-page__heading">Contents</h2>
      <BookContents
        :book-id="bookId"
        :chapters="overview.chapters"
        :current="progress?.chapter ?? null"
        :audiobook="audiobook"
        :recorded="recorded"
      />
    </UiCard>

    <UiCard v-if="overview && !audiobook && !isArticle" class="item-page__card" aria-labelledby="item-online">
      <h2 id="item-online" class="item-page__heading">Details online</h2>
      <MetadataLookup :book-id="bookId" :matched="Boolean(details?.openLibraryKey)" @applied="details = $event" />
    </UiCard>
  </article>
</template>

<style scoped>
.item-page {
  max-width: 1120px;
  padding: var(--space-4) var(--space-16) 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-14);
}
.item-page__error {
  margin: 0;
  color: var(--accent);
  font-weight: 600;
}
.item-page__hero {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-16);
}
/* A fixed width and padding: CoverArt's 10% padding would be taken from the whole row. */
.item-page__hero .item-page__cover {
  flex: none;
  width: 200px;
  padding: var(--space-10);
  align-self: flex-start;
  border-radius: var(--radius-md);
  font-size: 18px;
  box-shadow: 0 18px 36px -10px var(--shadow);
}
.item-page__intro {
  flex: 1 1 360px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
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
  gap: var(--space-5);
  margin-top: var(--space-4);
}
.item-page__about {
  margin-top: var(--space-5);
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
  margin-top: var(--space-3);
  font-weight: 700;
  color: var(--dim);
}
.item-page__subjects {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}
.item-page__subjects li {
  padding: var(--space-2) var(--space-5);
  border-radius: var(--radius-pill);
  background: var(--glass);
  font-size: 12px;
  color: var(--dim);
}
.item-page__cards {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-10);
  align-items: stretch;
}
.item-page__cards > .item-page__card {
  flex: 1 1 320px;
  min-width: 0;
}
.item-page__card {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}
.item-page__problem {
  margin: 0;
  display: flex;
  align-items: center;
  gap: var(--space-4);
  font-size: 13px;
  font-weight: 600;
  color: var(--warn);
}
.item-page__files {
  margin: 0;
  padding: 0;
  list-style: none;
  display: grid;
  gap: var(--space-2);
  font-size: 13px;
  color: var(--dim);
}
.item-page__files li {
  display: flex;
  gap: var(--space-6);
}
.item-page__file-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item-page__file-length {
  font-variant-numeric: tabular-nums;
}
.item-page__card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.item-page__badge {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
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
.item-page__option {
  min-height: 60px;
  display: flex;
  align-items: center;
  gap: var(--space-6);
  padding: var(--space-5) var(--space-7);
  border-radius: var(--radius-md);
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
  gap: var(--space-4);
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
  margin-top: var(--space-4);
}
.item-page__ghost-title {
  height: 40px;
  width: 60%;
  border-radius: var(--radius-md);
}
.item-page__ghost-text {
  display: grid;
  gap: var(--space-4);
  margin-top: var(--space-5);
}
.item-page__ghost-line {
  height: 14px;
  border-radius: var(--radius-sm);
}
.item-page__ghost-line:last-child {
  width: 60%;
}
.item-page__ghost-card {
  flex: 1 1 320px;
  height: 200px;
  border-radius: var(--radius-lg);
}
@media (max-width: 719px) {
  .item-page {
    padding: 0 var(--space-8);
  }
  .item-page__hero {
    gap: var(--space-10);
  }
  .item-page__hero .item-page__cover {
    width: 140px;
    padding: var(--space-7);
  }
  .item-page__title {
    font-size: 28px;
  }
}
</style>
