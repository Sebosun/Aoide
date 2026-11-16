<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { ITEM_DRAG } from "@/drag";
import { computed, ref } from "vue";
import {
  BookDown,
  CircleCheck,
  Download,
  Ellipsis,
  Eraser,
  FolderOpen,
  FolderPlus,
  Headphones,
  Info,
  RotateCcw,
  Tag,
  Trash2,
  TriangleAlert,
} from "@lucide/vue";
import { NEEDS, fetching } from "@/articles";
import { RECORDING_SHORT_LABELS } from "@/recording";
import { useRecordingsStore } from "@/stores/recordings";
import { useCollections } from "@/composables/useCollections";
import { useLibraryStore } from "@/stores/library";
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

const props = defineProps<{ book: BookSummary; busy?: boolean }>();
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
const ui = useUiStore(),
  library = useLibraryStore(),
  shelves = useCollections();
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
        : "The EPUB file stays where it is. Aoide forgets the book and deletes its generated audio and bookmarks.",
    confirmLabel: "Remove",
    danger: true,
  });
  if (ok) emit("remove");
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
    list.push({ kind: "separator" }, { kind: "heading", label: "Collections" });
    for (const c of library.collections)
      list.push({
        id: `collection:${c.id}`,
        label: c.name,
        ariaLabel: c.name,
        checked: book.collections.includes(c.id),
        action: () => act(() => shelves.toggle(book, c.id)),
      });
    list.push(
      {
        id: "new-collection",
        label: "New collection…",
        icon: FolderPlus,
        action: () => act(() => shelves.create(book)),
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
        :class="{ 'book-card__cover--square': audiobook }"
        :title="book.title"
        :url="book.coverUrl"
        :label="false"
        :shape="audiobook ? 'square' : 'portrait'"
      />
      <span v-if="book.kind === 'book' && book.recording" class="book-card__audiobook" title="Has an audiobook"
        ><Headphones :size="13" aria-hidden="true"
      /></span>
    </button>
    <h3 v-if="textTile" class="book-card__title visually-hidden">{{ book.title }}</h3>
    <span v-else class="book-card__caption" :class="{ 'book-card__caption--square': audiobook }"
      ><span v-if="article" class="book-card__kicker">{{ article.siteName ?? "" }}</span>
      <h3 class="book-card__title" :title="book.title">{{ book.title }}</h3>
      <span v-if="byline" class="book-card__byline">{{ byline }}</span></span
    >
    <span class="book-card__track"><i :style="{ width: `${percent}%` }" /></span>
    <span v-if="book.status.kind === 'missing'" class="book-card__badge book-card__badge--warn">File missing</span>
    <template v-else-if="book.status.kind === 'error'"
      ><span class="book-card__badge book-card__badge--error">Can't open this EPUB</span
      ><span class="book-card__reason" :title="book.status.message">{{ book.status.message }}</span></template
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
      /><Headphones v-else-if="book.audio !== 'none'" class="book-card__mark" :size="12" aria-hidden="true" />{{
        status
      }}</span
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
/* A card is clicked and dragged, never selected as text. */
.book-card {
  position: relative;
  -webkit-user-select: none;
  user-select: none;
  display: grid;
  /* Its one column keeps to the library's track, whatever the tile's text. */
  grid-template-columns: minmax(0, 1fr);
  gap: var(--space-3);
  align-content: start;
  min-width: 0;
  font-size: 12.5px;
}
/* A button sizes to its content even as a block: it fills the grid track instead. */
.book-card__open {
  grid-row: 1;
  grid-column: 1;
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
/* A square cover sits at the foot of a portrait frame, so the titles below line up. */
.book-card__open:has(.book-card__cover--square) {
  aspect-ratio: 2 / 3;
  display: flex;
  align-items: flex-end;
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
