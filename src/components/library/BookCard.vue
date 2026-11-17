<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { ITEM_DRAG } from "@/drag";
import { computed, ref } from "vue";
import {
  Archive,
  ArchiveRestore,
  BookDown,
  CircleCheck,
  Download,
  Ellipsis,
  Eraser,
  FolderOpen,
  FolderPlus,
  Headphones,
  Info,
  ListEnd,
  ListStart,
  ListX,
  RotateCcw,
  Tag,
  Trash2,
  TriangleAlert,
} from "@lucide/vue";
import { NEEDS, fetching } from "@/articles";
import { RECORDING_SHORT_LABELS } from "@/recording";
import { useRecordingsStore } from "@/stores/recordings";
import { useNavigationStore } from "@/stores/navigation";
import { useLibraryStore } from "@/stores/library";
import { useQueueStore } from "@/stores/queue";
import { api } from "@/api";
import type { BookSummary } from "@/api/types";
import CoverArt from "@/components/CoverArt.vue";
import ArticleTile from "@/components/article/ArticleTile.vue";
import UiMenu from "@/components/ui/UiMenu.vue";
import TagsDialog from "@/components/library/TagsDialog.vue";
import type { MenuEntry } from "@/components/ui/types";
import { useUiStore } from "@/stores/ui";
import { formatBytes } from "@/bytes";
import { timeLeft } from "@/estimate";
import { ACTION_ICONS, ACTION_LABELS, describePreparation, preparationStatus, runPrepareAction } from "@/preparation";

/** `seriesPlace`: show its place in its series on the cover (the library sorted by series). */
const props = defineProps<{ book: BookSummary; busy?: boolean; seriesPlace?: boolean }>();
const emit = defineEmits<{ open: []; details: []; remove: [] }>();
const unavailable = computed(() => props.book.status.kind !== "present");
const percent = computed(() => Math.round((props.book.progress?.fraction ?? 0) * 100));
/** What the book has, then how far along it is: "Text + voice · 38%", "Text · unread". While it
 *  is being prepared, or its recording syncs, that progress is shown instead. */
const HAS = { none: "Text", speech: "Text + voice", recording: "Text + recording" } as const;
/** An audiobook on its own: square cover, no text to prepare, export or pair audio with. */
const audiobook = computed(() => props.book.kind === "audiobook");
/** An article: a text tile in place of a cover, and its fetch on the card until it is in. */
const article = computed(() => props.book.article);
const loading = computed(() => !!article.value && fetching(article.value.stage));
const status = computed(() => {
  const { prepare, progress, recording, audio } = props.book;
  const stage = article.value?.stage;
  if (stage && fetching(stage)) return "Fetching…";
  if (stage?.kind === "failed") return "Could not be fetched";
  if (stage?.kind === "needsYou") return `Needs you · ${NEEDS[stage.reason]}`;
  if (audiobook.value)
    return `Audio only · ${props.book.readState === "finished" ? "finished" : progress ? `${percent.value}%` : "unplayed"}`;
  const along = props.book.readState === "finished" ? "finished" : progress ? `${percent.value}%` : "unread";
  if (prepare && prepare.state !== "done") return describePreparation(preparationStatus(prepare));
  if (recording && recording !== "ready")
    return progress?.fraction
      ? `${percent.value}% · ${RECORDING_SHORT_LABELS[recording]}`
      : RECORDING_SHORT_LABELS[recording].replace(/^./, (c) => c.toUpperCase());
  const left = progress && props.book.readState === "inProgress" ? timeLeft(props.book, library.readingWpm) : "";
  const has = article.value ? (audio === "none" ? "Article" : "Article + voice") : HAS[audio];
  return [has, along, left].filter(Boolean).join(" · ");
});
/** The status, and whether it is archived (under Archive, or found by a search among the rest). */
const line = computed(() => (props.book.archived ? `Archived · ${status.value}` : status.value));
const ui = useUiStore(),
  library = useLibraryStore(),
  navigation = useNavigationStore();
/** Runs a card action; failures become an error toast. */
async function act(action: () => Promise<unknown>) {
  try {
    await action();
  } catch (e) {
    ui.toast(String(e), "error");
  }
}
const preparation = computed(() => preparationStatus(props.book.prepare));
async function exportBook() {
  ui.toast(`Exporting ${props.book.title}… this can take a while`);
  const path = await api.exportBook({ bookId: props.book.id });
  ui.toast(`Exported to ${path}`);
}
async function exportReadAlong() {
  ui.toast(`Exporting ${props.book.title} with its recording… this can take a minute`);
  const path = await api.exportReadAlong({ bookId: props.book.id });
  ui.toast(`Exported to ${path}`);
}
const recordings = useRecordingsStore();
async function addAudiobook() {
  const paths = await api.chooseAudio({ folder: false });
  if (paths?.length) recordings.openPairing(paths, props.book.id);
}
async function deleteAudio() {
  const ok = await ui.confirm({
    title: `Delete the generated audio of ${props.book.title}?`,
    body: `Frees ${formatBytes(props.book.audioBytes ?? 0)}. Aoide makes the audio again when you play the book.`,
    confirmLabel: "Delete audio",
    danger: true,
  });
  if (ok) {
    await api.regenerateBook({ bookId: props.book.id });
    ui.toast("Generated audio deleted");
  }
}
async function remove() {
  const ok = await ui.confirm({
    title: `Remove ${props.book.title} from the library?`,
    body: audiobook.value
      ? "Its audio files stay where they are. Aoide forgets the audiobook and its bookmarks."
      : article.value
        ? "Aoide forgets the article and deletes its saved page, generated audio and bookmarks."
        : "The book's file stays where it is. Aoide forgets the book and deletes its generated audio and bookmarks.",
    confirmLabel: "Remove",
    danger: true,
  });
  if (ok) emit("remove");
}
/** Lining the item up to listen to next, or taking it off the queue. */
const queue = useQueueStore();
function queueActions(book: BookSummary): MenuEntry[] {
  if (book.queued)
    return [{ id: "unqueue", label: "Remove from queue", icon: ListX, action: () => act(() => queue.remove(book.id)) }];
  return [
    { id: "next", label: "Listen next", icon: ListStart, action: () => act(() => queue.add(book.id, "next")) },
    { id: "queue", label: "Add to queue", icon: ListEnd, action: () => act(() => queue.add(book.id, "last")) },
  ];
}
/** What only a book with text offers: preparing its speech, pairing audio, exports. */
function textActions(book: BookSummary): MenuEntry[] {
  const list: MenuEntry[] = preparation.value.actions.map((action) => ({
    id: action,
    label: ACTION_LABELS[action],
    icon: ACTION_ICONS[action],
    action: () => runPrepareAction(book.id, action),
  }));
  // Only a book (an EPUB) is paired with an audiobook.
  if (book.kind === "book")
    list.push({
      id: "audiobook",
      label: book.recording ? "Replace audiobook…" : "Add audiobook…",
      icon: Headphones,
      action: () => act(addAudiobook),
    });
  if (book.recording === "ready")
    list.push({
      id: "export-read-along",
      label: "Export read-along EPUB",
      icon: BookDown,
      action: () => act(exportReadAlong),
    });
  else list.push({ id: "export", label: "Export as M4B", icon: Download, action: () => act(exportBook) });
  return list;
}
/** The item's tags are being edited, in a dialog of their own. */
const editingTags = ref(false);
const items = computed<MenuEntry[]>(() => {
  const book = props.book,
    list: MenuEntry[] = [];
  if (!unavailable.value) {
    list.push({ id: "details", label: "Details", icon: Info, action: () => emit("details") });
    list.push(...queueActions(book));
    if (book.readState !== "finished")
      list.push({
        id: "finished",
        label: "Mark as finished",
        icon: CircleCheck,
        action: () => act(() => library.markRead(book.id, "finished")),
      });
    if (book.readState !== "unread")
      list.push({
        id: "unread",
        label: "Mark as unread",
        icon: RotateCcw,
        action: () => act(() => library.markRead(book.id, "unread")),
      });
    list.push(
      book.archived
        ? {
            id: "unarchive",
            label: "Take out of the archive",
            icon: ArchiveRestore,
            action: () => act(() => library.archive(book.id, false)),
          }
        : { id: "archive", label: "Archive", icon: Archive, action: () => act(() => library.archive(book.id, true)) },
    );
    list.push({
      id: "folder",
      label: "Show in folder",
      icon: FolderOpen,
      action: () => act(() => api.showInFolder({ bookId: book.id })),
    });
    if (!audiobook.value) list.push(...textActions(book));
    if (book.audioBytes)
      list.push({ id: "audio", label: "Delete generated audio", icon: Eraser, action: () => act(deleteAudio) });
    list.push({ id: "tags", label: "Tags…", icon: Tag, action: () => (editingTags.value = true) });
    list.push(
      {
        id: "collections",
        label: "Add to collection…",
        icon: FolderPlus,
        action: () => navigation.pickCollections(book.id),
      },
      { kind: "separator" },
    );
  }
  list.push({
    id: "remove",
    label: "Remove from library",
    ariaLabel: `Remove ${book.title} from the library`,
    icon: Trash2,
    danger: true,
    action: remove,
  });
  return list;
});
/** An article without a picture shows its text (site, headline, first lines) as its tile; every
 *  other item shows its picture (cover, drawn art, the article's image) with its title and
 *  author laid over its foot. */
const textTile = computed(() => Boolean(article.value) && (!props.book.coverUrl || loading.value));
const byline = computed(() => props.book.authors.join(", ") || (article.value ? "" : "Unknown author"));
/** A card is dragged onto a collection in the sidebar to put the item on it. */
function dragged(event: DragEvent) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData(ITEM_DRAG, props.book.id);
  event.dataTransfer.effectAllowed = "copy";
}
</script>

<template>
  <li
    class="book-card"
    :class="{ 'book-card--unavailable': unavailable }"
    :draggable="!unavailable"
    @dragstart="dragged"
  >
    <button
      type="button"
      class="book-card__open"
      :aria-label="`Open ${book.title}`"
      :disabled="unavailable || busy"
      @click="emit('open')"
    >
      <ArticleTile
        v-if="textTile"
        class="book-card__tile"
        :class="{ 'book-card__tile--loading': loading }"
        :site="article!.siteName ?? book.title"
        :title="book.title"
        :lead="article!.lead"
        :loading="loading"
      />
      <CoverArt
        v-else
        class="book-card__cover"
        :title="book.title"
        :url="book.coverUrl"
        :label="false"
        :shape="audiobook ? 'square' : 'portrait'"
      />
      <span
        v-if="seriesPlace && book.series?.position != null"
        class="book-card__series"
        :title="`Book ${book.series.position} of ${book.series.name}`"
        >{{ book.series.position }}</span
      >
      <span v-if="book.kind === 'book' && book.recording" class="book-card__audiobook" title="Has an audiobook"
        ><Headphones :size="13" aria-hidden="true"
      /></span>
    </button>
    <h3 v-if="textTile" class="book-card__title visually-hidden">{{ book.title }}</h3>
    <span v-else class="book-card__caption"
      ><span v-if="article" class="book-card__kicker">{{ article.siteName ?? "" }}</span>
      <h3 class="book-card__title" :title="book.title">{{ book.title }}</h3>
      <span v-if="byline" class="book-card__byline">{{ byline }}</span></span
    >
    <span class="book-card__track"><i :style="{ width: `${percent}%` }" /></span>
    <span v-if="book.status.kind === 'missing'" class="book-card__badge book-card__badge--warn">File missing</span>
    <span v-else-if="book.status.kind === 'error'" class="book-card__problem"
      ><span class="book-card__badge book-card__badge--error">Can't open this book</span
      ><span class="book-card__reason" :title="book.status.message">{{ book.status.message }}</span></span
    >
    <span
      v-else
      class="book-card__line book-card__status"
      :class="{ 'book-card__status--warn': article?.stage.kind === 'needsYou' }"
      :title="article?.stage.kind === 'failed' ? (article.error ?? undefined) : undefined"
      ><TriangleAlert
        v-if="article?.stage.kind === 'needsYou'"
        class="book-card__mark"
        :size="12"
        aria-hidden="true"
      /><Archive v-else-if="book.archived" class="book-card__mark" :size="12" aria-hidden="true" /><Headphones
        v-else-if="book.audio !== 'none'"
        class="book-card__mark"
        :size="12"
        aria-hidden="true"
      />{{ line }}</span
    >
    <UiMenu
      v-slot="{ toggle, trigger }"
      class="book-card__more"
      :label="`Actions for ${book.title}`"
      :items="items"
      align="end"
    >
      <UiButton
        variant="glass"
        size="sm"
        icon
        class="book-card__more-button"
        :aria-label="`More for ${book.title}`"
        v-bind="trigger"
        @click="toggle"
      >
        <Ellipsis :size="16" />
      </UiButton>
    </UiMenu>
    <!-- Out of the card: a card is dragged and never selected, and a dialog's field must be. -->
    <Teleport to="body">
      <TagsDialog v-if="editingTags" :book-id="book.id" :title="book.title" @close="editingTags = false" />
    </Teleport>
  </li>
</template>

<style scoped>
/* A card is clicked and dragged, never selected as text. Its three rows (picture, progress,
   status) are rows of the library grid, so a row of cards shares them: the picture row is as
   tall as the row's tallest picture, and the lines below it line up across the row. */
.book-card {
  position: relative;
  -webkit-user-select: none;
  user-select: none;
  display: grid;
  grid-row: span 3;
  grid-template-rows: subgrid;
  /* Its one column keeps to the library's track, whatever the tile's text. */
  grid-template-columns: minmax(0, 1fr);
  gap: var(--space-3);
  min-width: 0;
  margin-bottom: var(--space-12);
  font-size: 12.5px;
}
.book-card__problem {
  display: grid;
  gap: var(--space-3);
  min-width: 0;
}
/* A button sizes to its content even as a block: it fills the grid track instead. A square cover
   in a row with portrait ones sits at the foot of the row's picture row. */
.book-card__open {
  grid-row: 1;
  grid-column: 1;
  align-self: end;
  position: relative;
  display: block;
  width: 100%;
  border-radius: var(--radius-md);
}
.book-card__audiobook {
  position: absolute;
  top: var(--space-3);
  left: var(--space-3);
  display: grid;
  place-items: center;
  width: var(--space-12);
  height: var(--space-12);
  border-radius: var(--radius-pill);
  background: color-mix(in srgb, var(--ink) 72%, transparent);
  color: var(--page);
  box-shadow: 0 2px 8px -2px var(--shadow);
}
.book-card__series {
  position: absolute;
  top: var(--space-3);
  right: var(--space-3);
  min-width: var(--space-12);
  height: var(--space-12);
  padding: 0 var(--space-3);
  display: grid;
  place-items: center;
  border-radius: var(--radius-pill);
  background: color-mix(in srgb, var(--ink) 72%, transparent);
  color: var(--page);
  font-size: 12px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  box-shadow: 0 2px 8px -2px var(--shadow);
}
.book-card__open:disabled {
  opacity: 1;
  cursor: default;
}
.book-card__tile {
  aspect-ratio: 2 / 3;
  transition: box-shadow var(--dur-2) var(--ease);
}
.book-card__open:not(:disabled):hover .book-card__tile {
  box-shadow:
    0 22px 44px -16px var(--shadow),
    0 0 0 2px var(--line);
}
.book-card__cover {
  font-size: 13px;
  border-radius: var(--radius-md);
  transition: box-shadow var(--dur-2) var(--ease);
}
.book-card__open:not(:disabled):hover .book-card__cover {
  box-shadow:
    0 22px 44px -16px var(--shadow),
    0 0 0 2px var(--line);
}
.book-card--unavailable .book-card__cover {
  filter: grayscale(1);
  opacity: 0.35;
}
/* Over the foot of the picture, in the button's place in the grid; clicks go through to it. It
   fades into the page's colour, so it reads in either theme and on any cover. */
.book-card__caption {
  grid-row: 1;
  grid-column: 1;
  align-self: end;
  z-index: 1;
  display: grid;
  gap: var(--space-1);
  min-width: 0;
  padding: var(--space-16) var(--space-6) var(--space-6);
  border-radius: 0 0 var(--radius-md) var(--radius-md);
  background: linear-gradient(
    transparent,
    color-mix(in srgb, var(--page) 86%, transparent) 42%,
    color-mix(in srgb, var(--page) 96%, transparent)
  );
  pointer-events: none;
}
.book-card--unavailable .book-card__caption {
  opacity: 0.5;
}
.book-card__kicker {
  font-size: 10.5px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.book-card__title {
  margin: 0;
  font-size: 14px;
  font-weight: 700;
  line-height: 1.25;
  color: var(--ink);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.book-card__byline {
  color: var(--dim);
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.book-card__line,
.book-card__reason {
  color: var(--dim);
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.book-card__reason {
  font-size: 11.5px;
}
.book-card__mark {
  vertical-align: -1px;
  margin-right: var(--space-2);
  color: var(--accent);
}
.book-card__track {
  position: relative;
  height: 4px;
  border-radius: var(--radius-xs);
  background: var(--line);
  overflow: hidden;
}
.book-card__track i {
  position: absolute;
  inset: 0 auto 0 0;
  background: var(--accent);
}
.book-card__status--warn {
  color: var(--warn);
  font-weight: 700;
}
.book-card__status--warn .book-card__mark {
  color: inherit;
}
.book-card__badge {
  font-size: 12px;
  font-weight: 700;
}
.book-card__badge--warn {
  color: var(--warn);
}
.book-card__badge--error {
  color: var(--accent);
}
.book-card__more {
  position: absolute;
  top: 8px;
  right: 8px;
}
.book-card__more-button {
  opacity: 0;
}
.book-card:hover .book-card__more-button,
.book-card:focus-within .book-card__more-button,
.book-card__more-button[aria-expanded="true"] {
  opacity: 1;
}
</style>
