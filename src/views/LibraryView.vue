<script setup lang="ts">
import { computed } from "vue";
import { BookOpen, Library, Pencil, Plus, Search, Trash2, TriangleAlert } from "@lucide/vue";
import { useCollections } from "../composables/useCollections";
import UiSelect from "../components/ui/UiSelect.vue";
import UiSkeleton from "../components/ui/UiSkeleton.vue";
import AddBooksMenu from "../components/library/AddBooksMenu.vue";
import BookCard from "../components/library/BookCard.vue";
import ContinueCard from "../components/library/ContinueCard.vue";
import WeekCard from "../components/library/WeekCard.vue";
import { useLibraryStore, type LibraryFilter } from "../stores/library";
import { usePlayerStore } from "../stores/player";
import { useReaderStore } from "../stores/reader";
import { useNavigationStore } from "../stores/navigation";
import type { LibrarySort } from "../api/types";

const library = useLibraryStore(), reader = useReaderStore(), player = usePlayerStore(), navigation = useNavigationStore();
const filters: { value: LibraryFilter; label: string }[] = [
  { value: "all", label: "All" }, { value: "progress", label: "In progress" }, { value: "offline", label: "Ready offline" }, { value: "preparing", label: "Preparing" },
];
/** The playing book, else the most recently opened book that is started and not finished. */
const continued = computed(() => {
  const present = library.books.filter(b => b.status.kind === "present");
  const playing = present.find(b => b.id === player.state.bookId);
  if (playing) return playing;
  return present.filter(b => (b.progress?.fraction ?? 0) > 0 && (b.progress?.fraction ?? 0) < 1).sort((a, b) => (b.lastOpenedAt ?? 0) - (a.lastOpenedAt ?? 0))[0] ?? null;
});
const shelves = useCollections();
const plural = (n: number) => `${n} ${n === 1 ? "book" : "books"}`;
const importErrors = computed(() => library.imports.filter(r => r.error));
const SORTS: { value: LibrarySort; label: string }[] = [{ value: "recent", label: "Recent" }, { value: "title", label: "Title" }, { value: "author", label: "Author" }, { value: "progress", label: "Progress" }];
</script>

<template>
  <section class="library" :aria-busy="!library.ready">
    <template v-if="!library.ready">
      <p class="visually-hidden" role="status" aria-label="Loading your library">Loading your library…</p>
      <div class="library__top" aria-hidden="true"><UiSkeleton class="library__ghost-hero" /><UiSkeleton class="library__ghost-hero" /></div>
    </template>
    <div v-else-if="library.books.length" class="library__top">
      <ContinueCard v-if="continued" :book="continued" />
      <div v-else class="library__welcome"><b>Pick a book to start listening</b><span>Aoide reads it aloud and follows along in the text.</span></div>
      <WeekCard />
    </div>
    <p v-if="library.error || reader.error" class="library__alert" role="alert">{{ library.error || reader.error }}</p>
    <ul v-if="importErrors.length" class="library__alert" role="alert"><li v-for="result in importErrors" :key="result.path">{{ result.path }}: {{ result.error }}</li></ul>
    <section v-if="library.failed.length" class="library__failed" aria-label="Books that could not be imported">
      <p class="library__failed-title"><TriangleAlert :size="15" /> {{ plural(library.failed.length) }} could not be imported</p>
      <ul class="library__failed-list">
        <li v-for="book in library.failed" :key="book.id" class="library__failed-item">
          <b class="library__failed-name">{{ book.title }}</b>
          <span class="library__failed-reason">{{ book.status.kind === "error" ? book.status.message : "" }}</span>
          <button type="button" class="library__failed-remove" :aria-label="`Remove ${book.title}`" title="Remove from the library (the file is left as it is)" @click="library.remove(book.id)"><Trash2 :size="14" /></button>
        </li>
      </ul>
    </section>

    <div class="library__head">
      <h2 class="library__title">Your books</h2>
      <div v-if="library.books.length" class="library__chips" role="group" aria-label="Filter books">
        <button v-for="f in filters" :key="f.value" type="button" class="library__chip" :class="{ 'library__chip--on': library.filter === f.value }" :aria-label="f.label" :aria-pressed="library.filter === f.value" @click="library.filter = f.value">{{ f.label }}<span v-if="f.value === 'all'" class="library__count">{{ library.counts.all }}</span></button>
        <button v-for="c in library.collections" :key="c.id" type="button" class="library__chip library__chip--shelf" :class="{ 'library__chip--on': library.filter === `collection:${c.id}` }" :aria-label="`${c.name}, ${plural(c.books)}`" :aria-pressed="library.filter === `collection:${c.id}`" @click="library.filter = `collection:${c.id}`">{{ c.name }}<span class="library__count">{{ c.books }}</span></button>
        <button type="button" class="library__chip library__chip--new" aria-label="New collection" title="Make a collection of books" @click="shelves.create()"><Plus :size="13" /> New collection</button>
      </div>
      <span class="library__spacer" />
      <template v-if="library.books.length">
        <label class="library__search"><Search :size="15" /><input v-model="library.query" type="search" aria-label="Search library" placeholder="Title or author" /></label>
        <UiSelect class="library__sort" variant="pill" label="Sort library" :options="SORTS" :model-value="library.sort" @update:model-value="library.setSort" />
      </template>
      <AddBooksMenu />
    </div>
    <div v-if="library.collection" class="library__shelf" role="toolbar" :aria-label="`Collection ${library.collection.name}`">
      <Library :size="16" aria-hidden="true" /><b>{{ library.collection.name }}</b><span>{{ plural(library.collection.books) }}</span>
      <button type="button" class="library__shelf-button" aria-label="Rename collection" @click="shelves.rename(library.collection!)"><Pencil :size="13" /> Rename</button>
      <button type="button" class="library__shelf-button library__shelf-button--danger" aria-label="Delete collection" @click="shelves.remove(library.collection!)"><Trash2 :size="13" /> Delete</button>
    </div>
    <ul v-if="!library.ready" class="library__grid" aria-hidden="true">
      <li v-for="n in 12" :key="n" class="library__ghost"><UiSkeleton class="library__ghost-cover" /><UiSkeleton class="library__ghost-line" /><UiSkeleton class="library__ghost-line library__ghost-line--short" /></li>
    </ul>
    <div v-else-if="!library.books.length" class="library__empty">
      <BookOpen :size="36" :stroke-width="1.4" />
      <b>Your library is empty</b>
      <span>Drop EPUB files on this window, or use Add EPUB to find books on this computer.</span>
    </div>
    <p v-else-if="library.query.trim() && !library.visible.length" class="library__note">No books match “{{ library.query }}”.</p>
    <p v-else-if="library.books.length && !library.visible.length" class="library__note">No books here yet.</p>
    <ul class="library__grid">
      <BookCard v-for="book in library.visible" :key="book.id" :book="book" :busy="reader.loading" @open="book.progress ? reader.open(book.id) : navigation.openBook(book.id)" @details="navigation.openBook(book.id)" @remove="library.remove(book.id)" />
    </ul>
  </section>
</template>

<style scoped>
.library { display: grid; gap: 26px; align-content: start; max-width: 1400px; margin: 0 auto; padding: 8px 28px 0; }
.library__top { display: grid; grid-template-columns: minmax(0, 1fr) 300px; gap: 18px; }
.library__welcome { display: grid; gap: 6px; align-content: center; padding: 22px; border-radius: 22px; background: var(--hero); }
.library__welcome b { font-size: 24px; font-weight: 800; letter-spacing: -.02em; }
.library__welcome span { color: var(--dim); }
.library__alert { margin: 0; padding: 10px 14px; border-radius: 12px; background: var(--page); color: var(--accent); font-weight: 600; list-style: none; overflow-wrap: anywhere; }
.library__failed { display: grid; gap: 6px; padding: 12px 14px; border-radius: 14px; background: var(--glass); font-size: 13px; }
.library__failed-title { display: flex; align-items: center; gap: 7px; margin: 0; font-weight: 700; color: var(--warn); }
.library__failed-list { display: grid; gap: 2px; margin: 0; padding: 0; list-style: none; }
.library__failed-item { display: flex; align-items: baseline; gap: 10px; min-width: 0; }
.library__failed-name { flex: none; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ink); }
.library__failed-reason { flex: 1; min-width: 0; color: var(--dim); overflow-wrap: anywhere; }
.library__failed-remove { flex: none; display: grid; place-items: center; width: 26px; height: 26px; border-radius: 999px; color: var(--dim); }
.library__failed-remove:hover { background: var(--wash); color: var(--ink); }
.library__head { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.library__title { margin-right: 10px; font-size: 20px; font-weight: 800; letter-spacing: -.02em; }
/* Filters (and the collections after them) get their own row under the title and tools. */
.library__chips { order: 3; flex-basis: 100%; display: flex; flex-wrap: wrap; gap: 8px; margin-top: 6px; }
.library__chip { padding: 6px 13px; border-radius: 999px; background: var(--glass); font-size: 12.5px; font-weight: 600; color: var(--dim); }
.library__chip--on { background: var(--ink); color: var(--ground); }
.library__count { margin-left: 5px; font-variant-numeric: tabular-nums; opacity: .7; }
.library__chip--new { display: inline-flex; align-items: center; gap: 5px; background: none; box-shadow: inset 0 0 0 1px var(--line); }
.library__chip--new:hover { color: var(--ink); box-shadow: inset 0 0 0 1px var(--faint); }
.library__shelf { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 10px; margin: 0 0 16px; padding: 10px 14px; border-radius: 14px; background: var(--glass); font-size: 13px; }
.library__shelf b { font-weight: 800; }
.library__shelf span { color: var(--dim); margin-right: auto; }
.library__shelf-button { display: inline-flex; align-items: center; gap: 6px; height: 30px; padding: 0 12px; border-radius: 999px; font-weight: 700; font-size: 12.5px; color: var(--dim); }
.library__shelf-button:hover { background: var(--wash); color: var(--ink); }
.library__shelf-button--danger:hover { color: var(--accent); }
.library__spacer { flex: 1; }
.library__search { display: inline-flex; align-items: center; gap: 6px; height: 36px; padding: 0 12px; border-radius: 999px; background: var(--glass); color: var(--dim); }
.library__search input { width: 150px; padding: 0; border: 0; background: none; font-size: 13px; color: var(--ink); }
.library__search input:focus { outline: none; }
.library__search:focus-within { outline: 2px solid var(--accent); outline-offset: 3px; }
.library__sort { height: 36px; padding: 0 12px 0 14px; font-weight: 600; }
.library__note { color: var(--dim); }
.library__empty { display: grid; justify-items: center; gap: 10px; padding: 48px 20px; border-radius: 22px; background: var(--page); color: var(--dim); text-align: center; }
.library__empty b { color: var(--ink); font-size: 18px; }
.library__ghost-hero { min-height: 220px; border-radius: 22px; }
.library__ghost { display: grid; gap: 8px; align-content: start; }
.library__ghost-cover { aspect-ratio: 2 / 3; border-radius: 12px; }
.library__ghost-line { height: 12px; width: 80%; border-radius: 6px; }
.library__ghost-line--short { width: 50%; }
.library__grid { display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 24px 18px; margin: 0; padding: 0; list-style: none; }
@media (max-width: 1180px) { .library__grid { grid-template-columns: repeat(5, minmax(0, 1fr)); } }
@media (max-width: 999px) { .library__top { grid-template-columns: minmax(0, 1fr) 240px; } .library__grid { grid-template-columns: repeat(4, minmax(0, 1fr)); } }
@media (max-width: 760px) { .library { padding: 8px 16px 0; } .library__top { grid-template-columns: 1fr; } .library__grid { grid-template-columns: repeat(3, minmax(0, 1fr)); } .library__search input { width: 110px; } }
@media (max-width: 480px) { .library__grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
