<script setup lang="ts">
import UiButton from "@/components/ui/UiButton.vue";
import { computed, onMounted, ref } from "vue";
import { BookmarkPlus, Trash2 } from "@lucide/vue";
import { api } from "@/api";
import type { Bookmark } from "@/api/types";
import { formatClock, partsOf } from "@/estimate";
import { usePlayerStore } from "@/stores/player";
import { useReaderStore } from "@/stores/reader";
const props = defineProps<{ bookId: string }>();
const player = usePlayerStore(),
  reader = useReaderStore();
const marks = ref<Bookmark[]>([]),
  note = ref(""),
  error = ref(""),
  editing = ref<number | null>(null);
async function load() {
  try {
    marks.value = await api.bookmarks(props.bookId);
  } catch (e) {
    error.value = String(e);
  }
}
onMounted(load);
async function add() {
  error.value = "";
  try {
    await api.addBookmark(props.bookId, note.value.trim());
    note.value = "";
    await load();
  } catch (e) {
    error.value = String(e);
  }
}
async function saveNote(mark: Bookmark, event: Event) {
  const next = (event.target as HTMLInputElement).value.trim();
  editing.value = null;
  if (next === mark.note) return;
  try {
    await api.updateBookmarkNote(mark.id, next);
    mark.note = next;
  } catch (e) {
    error.value = String(e);
  }
}
async function remove(mark: Bookmark) {
  try {
    await api.deleteBookmark(mark.id);
    await load();
  } catch (e) {
    error.value = String(e);
  }
}
const chapterTitle = (index: number) =>
  reader.book?.chapters.find((c) => c.index === index)?.title ?? `Chapter ${index + 1}`;
/** An audiobook on its own: its bookmarks are times, in its parts. */
const audio = computed(() => reader.shape === "listen");
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
const when = (seconds: number) => new Date(seconds * 1000).toLocaleDateString();
</script>

<template>
  <form class="book-bookmarks__add" @submit.prevent="add">
    <input v-model="note" type="text" aria-label="Bookmark note" placeholder="Optional note" />
    <UiButton size="sm" type="submit" class="book-bookmarks__button"
      ><BookmarkPlus :size="15" /> Bookmark here</UiButton
    >
  </form>
  <p class="book-bookmarks__hint">
    {{ audio ? "Marks the moment playing" : "Marks the sentence being read" }}, or where you left off.
  </p>
  <p v-if="error" class="book-bookmarks__error" role="alert">{{ error }}</p>
  <p v-if="!marks.length" class="book-bookmarks__empty">No bookmarks yet.</p>
  <ul class="book-bookmarks__list">
    <li v-for="mark in marks" :key="mark.id" class="book-bookmarks__item">
      <button type="button" class="book-bookmarks__play" :title="'Play from here'" @click="jump(mark)">
        <small>{{ where(mark) }} · {{ when(mark.createdAt) }}</small>
        <span>{{ mark.excerpt || "Start of chapter" }}</span>
      </button>
      <div class="book-bookmarks__row">
        <input
          v-if="editing === mark.id"
          :value="mark.note"
          type="text"
          aria-label="Edit bookmark note"
          placeholder="Add a note"
          @change="saveNote(mark, $event)"
          @keydown.esc="editing = null"
        />
        <button
          v-else
          type="button"
          class="book-bookmarks__note"
          :class="{ 'book-bookmarks__note--empty': !mark.note }"
          title="Edit note"
          @click="editing = mark.id"
        >
          {{ mark.note || "Add a note" }}
        </button>
        <button type="button" class="book-bookmarks__delete" aria-label="Delete bookmark" @click="remove(mark)">
          <Trash2 :size="14" />
        </button>
      </div>
    </li>
  </ul>
</template>

<style scoped>
.book-bookmarks__add {
  display: flex;
  gap: var(--space-4);
}
.book-bookmarks__add input {
  flex: 1;
}
.book-bookmarks__hint {
  font-size: 11px;
  color: var(--dim);
  margin: var(--space-4) 0 var(--space-8);
}
.book-bookmarks__list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: var(--space-6);
}
.book-bookmarks__item {
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: var(--space-3);
}
.book-bookmarks__play {
  display: grid;
  gap: var(--space-2);
  text-align: left;
  width: 100%;
  border-radius: var(--radius-sm);
  font: 14px/1.5 var(--font-read);
}
.book-bookmarks__play:hover {
  background: var(--wash);
}
.book-bookmarks__play small {
  font: 11px var(--font-ui);
  color: var(--dim);
}
.book-bookmarks__row {
  display: flex;
  gap: var(--space-3);
  align-items: center;
  padding: 0 var(--space-2) var(--space-2);
}
.book-bookmarks__row input {
  flex: 1;
  font-size: 12px;
  padding: var(--space-3) var(--space-4);
}
.book-bookmarks__note {
  flex: 1;
  text-align: left;
  font-size: 12px;
  color: var(--dim);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-sm);
}
.book-bookmarks__note:hover {
  background: var(--wash);
}
.book-bookmarks__note--empty {
  color: var(--dim);
  font-style: italic;
}
.book-bookmarks__delete {
  color: var(--dim);
}
.book-bookmarks__empty {
  font-size: 12px;
  color: var(--dim);
}
.book-bookmarks__error {
  font-size: 12px;
  color: var(--accent);
}
</style>
