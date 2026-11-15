<script setup lang="ts">
import { computed } from "vue";
import { BookOpen, Search, Trash2, TriangleAlert } from "@lucide/vue";
import UiSelect from "../components/ui/UiSelect.vue";
import UiSkeleton from "../components/ui/UiSkeleton.vue";
import BookCard from "../components/library/BookCard.vue";
import { useLibraryStore, type BuiltInFilter } from "../stores/library";
import { usePlayerStore } from "../stores/player";
import { useReaderStore } from "../stores/reader";
import { useNavigationStore } from "../stores/navigation";
import type { BookSummary, ItemKind, LibrarySort } from "../api/types";

const library = useLibraryStore(),
  reader = useReaderStore(),
  player = usePlayerStore(),
  navigation = useNavigationStore();
const FILTERS: { value: BuiltInFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "unread", label: "Unread" },
  { value: "progress", label: "In progress" },
  { value: "finished", label: "Finished" },
  { value: "listen", label: "Can listen" },
  { value: "preparing", label: "Preparing" },
];
/** Preparing is offered only while something is being prepared (or it is the filter). */
const filters = computed(() =>
  FILTERS.filter((f) => f.value !== "preparing" || library.counts.preparing > 0 || library.filter === "preparing"),
);
const KINDS: { value: ItemKind; label: string }[] = [
  { value: "book", label: "Books" },
  { value: "article", label: "Articles" },
  { value: "audiobook", label: "Audiobooks" },
];
/** The kind switch: All and each kind the library holds, once it holds two. */
const kinds = computed(() =>
  library.kinds.length < 2
    ? []
    : [
        { value: "all" as const, label: "All", count: library.counts.all },
        ...KINDS.filter((k) => library.kinds.includes(k.value)).map((k) => ({
          ...k,
          count: library.kindCounts[k.value],
        })),
      ],
);
const plural = (n: number) => `${n} ${n === 1 ? "book" : "books"}`;
const importErrors = computed(() => library.imports.filter((r) => r.error));
/** An unstarted book opens on its page. A started one opens in the reader, and if another book
 *  is playing, playback moves to this one where it was left. */
async function openCard(book: BookSummary) {
  if (!book.progress) {
    navigation.openItem(book.id);
    return;
  }
  const status = player.state.status;
  const switching = (status === "playing" || status === "buffering") && player.state.bookId !== book.id;
  await reader.open(book.id);
  if (switching && reader.book?.id === book.id) await player.play(book.id, null, null);
}
const SORTS: { value: LibrarySort; label: string }[] = [
  { value: "recent", label: "Recent" },
  { value: "title", label: "Title" },
  { value: "author", label: "Author" },
  { value: "progress", label: "Progress" },
];
</script>

<template>
  <section class="library" :aria-busy="!library.ready">
    <p v-if="!library.ready" class="visually-hidden" role="status" aria-label="Loading your library">
      Loading your library…
    </p>
    <p v-if="library.error || reader.error" class="library__alert" role="alert">{{ library.error || reader.error }}</p>
    <ul v-if="importErrors.length" class="library__alert" role="alert">
      <li v-for="result in importErrors" :key="result.path">{{ result.path }}: {{ result.error }}</li>
    </ul>
    <section v-if="library.failed.length" class="library__failed" aria-label="Books that could not be imported">
      <p class="library__failed-title">
        <TriangleAlert :size="15" /> {{ plural(library.failed.length) }} could not be imported
      </p>
      <ul class="library__failed-list">
        <li v-for="book in library.failed" :key="book.id" class="library__failed-item">
          <b class="library__failed-name">{{ book.title }}</b>
          <span class="library__failed-reason">{{ book.status.kind === "error" ? book.status.message : "" }}</span>
          <button
            type="button"
            class="library__failed-remove"
            :aria-label="`Remove ${book.title}`"
            title="Remove from the library (the file is left as it is)"
            @click="library.remove(book.id)"
          >
            <Trash2 :size="14" />
          </button>
        </li>
      </ul>
    </section>

    <div class="library__head">
      <h1 class="library__title">{{ library.collection?.name ?? "Library" }}</h1>
      <span class="library__spacer" />
      <template v-if="library.books.length">
        <label class="library__search"
          ><Search :size="16" /><input
            v-model="library.query"
            type="search"
            aria-label="Search library"
            placeholder="Title or author"
        /></label>
        <UiSelect
          class="library__sort"
          variant="pill"
          label="Sort library"
          :options="SORTS"
          :model-value="library.sort"
          @update:model-value="library.setSort"
        />
      </template>
    </div>
    <div class="library__bar">
      <div v-if="kinds.length" class="library__kinds" role="group" aria-label="Kind">
        <button
          v-for="k in kinds"
          :key="k.value"
          type="button"
          class="library__kind"
          :class="{ 'library__kind--on': library.kind === k.value }"
          :aria-pressed="library.kind === k.value"
          @click="library.kind = k.value"
        >
          {{ k.label }}<span class="library__count">{{ k.count }}</span>
        </button>
      </div>
      <div
        v-if="library.books.length && !library.collection"
        class="library__chips"
        role="group"
        aria-label="Filter books"
      >
        <button
          v-for="f in filters"
          :key="f.value"
          type="button"
          class="library__chip"
          :class="{ 'library__chip--on': library.filter === f.value }"
          :aria-label="f.label"
          :aria-pressed="library.filter === f.value"
          @click="library.filter = f.value"
        >
          {{ f.label }}<span v-if="f.value === 'all'" class="library__count">{{ library.counts.all }}</span>
        </button>
      </div>
    </div>
    <ul v-if="!library.ready" class="library__grid" aria-hidden="true">
      <li v-for="n in 12" :key="n" class="library__ghost">
        <UiSkeleton class="library__ghost-cover" /><UiSkeleton class="library__ghost-line" /><UiSkeleton
          class="library__ghost-line library__ghost-line--short"
        />
      </li>
    </ul>
    <div v-else-if="!library.books.length" class="library__empty">
      <BookOpen :size="36" :stroke-width="1.4" />
      <b>Your library is empty</b>
      <span>Drop EPUB files on this window, or use Add in the sidebar.</span>
    </div>
    <p v-else-if="library.query.trim() && !library.visible.length" class="library__note">
      No books match “{{ library.query }}”.
    </p>
    <p v-else-if="library.books.length && !library.visible.length" class="library__note">No books here yet.</p>
    <ul class="library__grid">
      <BookCard
        v-for="book in library.visible"
        :key="book.id"
        :book="book"
        :busy="reader.loading"
        @open="openCard(book)"
        @details="navigation.openItem(book.id)"
        @remove="library.remove(book.id)"
      />
    </ul>
  </section>
</template>

<style scoped>
.library {
  display: grid;
  gap: 26px;
  align-content: start;
  max-width: 1400px;
  margin: 0 auto;
  padding: 8px 36px 0;
}
.library__alert {
  margin: 0;
  padding: 10px 14px;
  border-radius: 12px;
  background: var(--page);
  color: var(--accent);
  font-weight: 600;
  list-style: none;
  overflow-wrap: anywhere;
}
.library__failed {
  display: grid;
  gap: 6px;
  padding: 12px 14px;
  border-radius: 14px;
  background: var(--glass);
  font-size: 13px;
}
.library__failed-title {
  display: flex;
  align-items: center;
  gap: 7px;
  margin: 0;
  font-weight: 700;
  color: var(--warn);
}
.library__failed-list {
  display: grid;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.library__failed-item {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}
.library__failed-name {
  flex: none;
  max-width: 40%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--ink);
}
.library__failed-reason {
  flex: 1;
  min-width: 0;
  color: var(--dim);
  overflow-wrap: anywhere;
}
.library__failed-remove {
  flex: none;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 999px;
  color: var(--dim);
}
.library__failed-remove:hover {
  background: var(--wash);
  color: var(--ink);
}
.library__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
}
.library__title {
  margin: 0 10px 0 0;
  font-size: 24px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.library__bar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  margin-top: -8px;
}
.library__bar:empty {
  display: none;
}
.library__kinds {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding: 4px;
  border-radius: 24px;
  background: var(--wash);
}
.library__kind {
  height: 40px;
  padding: 0 16px;
  border-radius: 20px;
  background: transparent;
  font-weight: 600;
  color: var(--dim);
}
.library__kind--on {
  background: var(--ink);
  color: var(--ground);
}
.library__chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.library__chip {
  height: 40px;
  padding: 0 14px;
  border-radius: 20px;
  background: var(--page);
  font-weight: 500;
  color: var(--dim);
}
.library__chip:hover:not(.library__chip--on) {
  color: var(--ink);
}
.library__chip--on {
  background: var(--ink);
  color: var(--ground);
  font-weight: 600;
}
.library__count {
  margin-left: 6px;
  font-variant-numeric: tabular-nums;
  opacity: 0.7;
}
.library__spacer {
  flex: 1;
}
.library__search {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 44px;
  padding: 0 14px;
  border-radius: 22px;
  background: var(--page);
  color: var(--dim);
}
.library__search input {
  width: 180px;
  padding: 0;
  border: 0;
  background: none;
  color: var(--ink);
}
.library__search input:focus {
  outline: none;
}
.library__search:focus-within {
  outline: 2px solid var(--accent);
  outline-offset: 3px;
}
.library__sort {
  height: 44px;
  padding: 0 16px;
  font-weight: 600;
  background: var(--page);
}
.library__note {
  color: var(--dim);
}
.library__empty {
  display: grid;
  justify-items: center;
  gap: 10px;
  padding: 48px 20px;
  border-radius: 22px;
  background: var(--page);
  color: var(--dim);
  text-align: center;
}
.library__empty b {
  color: var(--ink);
  font-size: 18px;
}
.library__ghost {
  display: grid;
  gap: 8px;
  align-content: start;
}
.library__ghost-cover {
  aspect-ratio: 2 / 3;
  border-radius: 12px;
}
.library__ghost-line {
  height: 12px;
  width: 80%;
  border-radius: 6px;
}
.library__ghost-line--short {
  width: 50%;
}
.library__grid {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: 24px 18px;
  margin: 0;
  padding: 0;
  list-style: none;
}
@media (max-width: 1180px) {
  .library__grid {
    grid-template-columns: repeat(5, minmax(0, 1fr));
  }
}
@media (max-width: 999px) {
  .library__grid {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }
}
@media (max-width: 760px) {
  .library {
    padding: 8px 16px 0;
  }
  .library__grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
  .library__search input {
    width: 110px;
  }
}
@media (max-width: 480px) {
  .library__grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
