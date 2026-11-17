<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onMounted, ref } from "vue";
import { Bookmark as BookmarkIcon, BookmarkPlus, ClipboardCopy, FileDown, Highlighter, Trash2 } from "@lucide/vue";
import { api } from "@/api";
import type { Bookmark, HighlightView } from "@/api/types";
import { formatClock, partsOf } from "@/estimate";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
import { useUiStore } from "@/stores/ui";

/** Book tools' Notes: the book's highlights and bookmarks in reading order, a filter for each,
 *  Copy as Markdown and Export…. A bookmark plays from its place; a highlight from its sentence.
 *  An audiobook on its own has bookmarks only. */
const props = defineProps<{ bookId: string }>();
const player = usePlayerStore(),
  reader = useReaderStore(),
  ui = useUiStore();
const marks = ref<Bookmark[]>([]),
  note = ref(""),
  error = ref(""),
  editing = ref<string | null>(null),
  showHighlights = ref(true),
  showBookmarks = ref(true);
async function load() {
  try {
    marks.value = await api.bookmarks({ bookId: props.bookId });
  } catch (e) {
    error.value = String(e);
  }
}
onMounted(load);
async function add() {
  error.value = "";
  try {
    await api.addBookmark({ bookId: props.bookId, note: note.value.trim() });
    note.value = "";
    await load();
  } catch (e) {
    error.value = String(e);
  }
}
/** An audiobook on its own: its bookmarks are times, in its parts, and it has no highlights. */
const audio = computed(() => reader.shape === "listen");
const highlights = computed(() => (reader.book?.id === props.bookId ? reader.highlights : []));

type Entry =
  { kind: "bookmark"; key: string; mark: Bookmark } | { kind: "highlight"; key: string; mark: HighlightView };
/** Bookmarks and highlights in reading order (a bookmark before a highlight starting at it). */
const entries = computed<Entry[]>(() => {
  const all: (Entry & { at: [number, number, number] })[] = [
    ...(showBookmarks.value ? marks.value : []).map((mark, i) => ({
      kind: "bookmark" as const,
      key: `b${mark.id}`,
      mark,
      at: [mark.chapter, mark.positionMs ?? mark.charOffset, i] as [number, number, number],
    })),
    ...(showHighlights.value ? highlights.value : []).map((mark) => ({
      kind: "highlight" as const,
      key: `h${mark.id}`,
      mark,
      at: [mark.chapter, mark.start, Number.MAX_SAFE_INTEGER] as [number, number, number],
    })),
  ];
  return all.sort((a, b) => a.at[0] - b.at[0] || a.at[1] - b.at[1] || a.at[2] - b.at[2]);
});

async function saveNote(entry: Entry, event: Event) {
  const next = (event.target as HTMLInputElement).value.trim();
  editing.value = null;
  if (next === entry.mark.note) return;
  try {
    if (entry.kind === "highlight") await reader.noteHighlight(entry.mark.id, next);
    else {
      await api.updateBookmarkNote({ id: entry.mark.id, note: next });
      entry.mark.note = next;
    }
  } catch (e) {
    error.value = String(e);
  }
}
async function remove(entry: Entry) {
  try {
    if (entry.kind === "highlight") await reader.removeHighlight(entry.mark.id);
    else {
      await api.deleteBookmark({ id: entry.mark.id });
      await load();
    }
  } catch (e) {
    error.value = String(e);
  }
}
const chapterTitle = (index: number) =>
  reader.book?.chapters.find((c) => c.index === index)?.title || `Chapter ${index + 1}`;
const parts = computed(() => partsOf(reader.book?.chapters ?? []));
/** Where a bookmark is: "Part 7 · 18:42" (into the part) for a time, else its chapter. */
function where(mark: Bookmark) {
  if (mark.positionMs === null) return chapterTitle(mark.chapter);
  const start = parts.value[mark.chapter]?.start ?? 0;
  return `Part ${mark.chapter + 1} · ${formatClock(mark.positionMs - start)}`;
}
/** Plays from the bookmark: its sentence, or its time. */
async function jump(mark: Bookmark) {
  if (mark.positionMs !== null) {
    await player.seek(props.bookId, mark.positionMs);
    if (player.state.bookId !== props.bookId || player.state.status !== "playing")
      await player.play(props.bookId, null, null);
  } else await player.play(props.bookId, mark.chapter, mark.segmentId);
}
/** Plays from the sentence a highlight starts in. */
async function jumpTo(mark: HighlightView) {
  try {
    const chapter = await reader.chapterText(props.bookId, mark.chapter);
    const segment = chapter.blocks
      .flatMap((b) => b.segments)
      .find((s) => s.starts.length && s.starts[s.starts.length - 1] >= mark.start && s.starts[0] <= mark.end);
    await player.play(props.bookId, mark.chapter, segment?.id ?? null);
  } catch (e) {
    error.value = String(e);
  }
}
const when = (seconds: number) => new Date(seconds * 1000).toLocaleDateString();

async function copyMarkdown() {
  try {
    await navigator.clipboard.writeText(await api.notesMarkdown({ bookId: props.bookId }));
    ui.toast("Copied as Markdown", "info", "copy");
  } catch (e) {
    ui.toast(`Could not copy: ${e}`, "error");
  }
}
/** A file name from the book's title, as the system's save dialog offers it. */
const fileName = () => `${(reader.book?.title ?? "Notes").replace(/[/\\:*?"<>|]+/g, " ").trim() || "Notes"}.md`;
async function exportNotes() {
  try {
    const path = await api.chooseNotesTarget({ fileName: fileName() });
    if (!path) return;
    await api.exportNotes({ bookId: props.bookId, path });
    ui.toast("Notes exported");
  } catch (e) {
    ui.toast(String(e), "error");
  }
}
</script>

<template>
  <form class="book-notes__add" @submit.prevent="add">
    <input v-model="note" type="text" aria-label="Bookmark note" placeholder="Optional note" />
    <UiButton size="sm" type="submit" class="book-notes__button"><BookmarkPlus :size="15" /> Bookmark here</UiButton>
  </form>
  <p class="book-notes__hint">
    {{ audio ? "Marks the moment playing" : "Marks the sentence being read" }}, or where you left off.
    <template v-if="!audio">Select text in the book to highlight it.</template>
  </p>
  <div class="book-notes__bar">
    <div v-if="!audio" class="book-notes__filters" role="group" aria-label="Show">
      <button
        type="button"
        class="book-notes__filter"
        aria-label="Show highlights"
        :aria-pressed="showHighlights"
        @click="showHighlights = !showHighlights"
      >
        <Highlighter :size="14" aria-hidden="true" />Highlights {{ highlights.length }}
      </button>
      <button
        type="button"
        class="book-notes__filter"
        aria-label="Show bookmarks"
        :aria-pressed="showBookmarks"
        @click="showBookmarks = !showBookmarks"
      >
        <BookmarkIcon :size="14" aria-hidden="true" />Bookmarks {{ marks.length }}
      </button>
    </div>
    <div class="book-notes__actions">
      <UiButton size="sm" variant="soft" class="book-notes__copy" @click="copyMarkdown"
        ><ClipboardCopy :size="14" /> Copy as Markdown</UiButton
      >
      <UiButton size="sm" variant="soft" class="book-notes__export" @click="exportNotes"
        ><FileDown :size="14" /> Export…</UiButton
      >
    </div>
  </div>
  <p v-if="error" class="book-notes__error" role="alert">{{ error }}</p>
  <p v-if="!entries.length" class="book-notes__empty">
    {{ marks.length || highlights.length ? "Nothing shown: turn on a filter." : "No notes yet." }}
  </p>
  <ul class="book-notes__list">
    <li
      v-for="entry in entries"
      :key="entry.key"
      class="book-notes__item"
      :class="`book-notes__item--${entry.kind}`"
      :data-kind="entry.kind"
    >
      <button
        v-if="entry.kind === 'bookmark'"
        type="button"
        class="book-notes__play"
        title="Play from here"
        @click="jump(entry.mark)"
      >
        <small
          ><BookmarkIcon :size="11" aria-hidden="true" /> {{ where(entry.mark) }} ·
          {{ when(entry.mark.createdAt) }}</small
        >
        <span>{{ entry.mark.excerpt || "Start of chapter" }}</span>
      </button>
      <button
        v-else
        type="button"
        class="book-notes__play"
        :title="entry.mark.detached ? 'Its text is no longer in the chapter' : 'Play from here'"
        :disabled="entry.mark.detached"
        @click="jumpTo(entry.mark)"
      >
        <small
          ><Highlighter :size="11" aria-hidden="true" /> {{ entry.mark.chapterTitle || chapterTitle(entry.mark.chapter)
          }}<template v-if="entry.mark.detached"> · no longer in the text</template></small
        >
        <span class="book-notes__quote">{{ entry.mark.text }}</span>
      </button>
      <div class="book-notes__row">
        <input
          v-if="editing === entry.key"
          :value="entry.mark.note"
          type="text"
          :aria-label="`Edit ${entry.kind} note`"
          placeholder="Add a note"
          @change="saveNote(entry, $event)"
          @keydown.esc="editing = null"
        />
        <button
          v-else
          type="button"
          class="book-notes__note"
          :class="{ 'book-notes__note--empty': !entry.mark.note }"
          title="Edit note"
          @click="editing = entry.key"
        >
          {{ entry.mark.note || "Add a note" }}
        </button>
        <button type="button" class="book-notes__delete" :aria-label="`Delete ${entry.kind}`" @click="remove(entry)">
          <Trash2 :size="14" />
        </button>
      </div>
    </li>
  </ul>
</template>

<style scoped>
.book-notes__add {
  display: flex;
  gap: var(--space-4);
}
.book-notes__add input {
  flex: 1;
}
.book-notes__hint {
  font-size: 11px;
  color: var(--dim);
  margin: var(--space-4) 0 var(--space-6);
}
.book-notes__bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}
.book-notes__filters,
.book-notes__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}
.book-notes__filter {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-5);
  border-radius: var(--radius-pill);
  border: 1px solid var(--line);
  color: var(--dim);
  font-size: 12px;
  font-weight: 600;
}
.book-notes__filter[aria-pressed="true"] {
  background: var(--ink);
  border-color: var(--ink);
  color: var(--ground);
}
.book-notes__list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: var(--space-6);
}
.book-notes__item {
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: var(--space-3);
}
.book-notes__play {
  display: grid;
  gap: var(--space-2);
  text-align: left;
  width: 100%;
  border-radius: var(--radius-sm);
  font: 14px/1.5 var(--font-read);
}
.book-notes__play:not(:disabled):hover {
  background: var(--wash);
}
.book-notes__play small {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font: 11px var(--font-ui);
  color: var(--dim);
}
.book-notes__quote {
  padding-left: var(--space-4);
  border-left: 3px solid var(--mark);
  display: -webkit-box;
  -webkit-line-clamp: 4;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.book-notes__item--highlight .book-notes__play:disabled .book-notes__quote {
  color: var(--dim);
  border-left-style: dashed;
}
.book-notes__row {
  display: flex;
  gap: var(--space-3);
  align-items: center;
  padding: 0 var(--space-2) var(--space-2);
}
.book-notes__row input {
  flex: 1;
  font-size: 12px;
  padding: var(--space-3) var(--space-4);
}
.book-notes__note {
  flex: 1;
  text-align: left;
  font-size: 12px;
  color: var(--dim);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-sm);
  white-space: pre-line;
}
.book-notes__note:hover {
  background: var(--wash);
}
.book-notes__note--empty {
  font-style: italic;
}
.book-notes__delete {
  color: var(--dim);
}
.book-notes__empty {
  font-size: 12px;
  color: var(--dim);
}
.book-notes__error {
  font-size: 12px;
  color: var(--accent);
}
</style>
