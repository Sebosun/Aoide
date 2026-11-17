import { defineStore } from "pinia";
import { computed, ref, toRaw } from "vue";
import { api } from "@/api";
import { useNavigationStore } from "@/stores/navigation";
import type {
  BookProgress,
  BookSummary,
  Collection,
  ImportResult,
  ItemKind,
  LibrarySort,
  TagFilter,
} from "@/api/types";

/** How a tag filters the library. */
export type TagState = "include" | "exclude" | "off";
export type BuiltInFilter = "all" | "unread" | "progress" | "finished" | "listen" | "preparing" | "archive";
/** A built-in filter, or one collection. */
export type LibraryFilter = BuiltInFilter | `collection:${number}`;
/** Each filter reads one field the core sends; none works out state of its own. */
const filters: Record<BuiltInFilter, (book: BookSummary) => boolean> = {
  all: () => true,
  unread: (book) => book.readState === "unread",
  progress: (book) => book.readState === "inProgress",
  finished: (book) => book.readState === "finished",
  listen: (book) => book.audio !== "none",
  preparing: (book) => Boolean(book.prepare && book.prepare.state !== "done"),
  archive: () => true,
};
/** Archived items are under Archive only: every other filter and collection leaves them out, but
 *  a search finds them. */
const onShelf = (book: BookSummary, filter: LibraryFilter) => (filter === "archive" ? book.archived : !book.archived);

/** `next`, keeping each summary of `before` that did not change: a card whose book is the same
 *  object is not drawn again, so a refresh redraws only what changed. */
function keepUnchanged(next: BookSummary[], before: BookSummary[]) {
  const old = new Map(before.map((book) => [book.id, toRaw(book)]));
  const kept = next.map((book) => {
    const same = old.get(book.id);
    return same && JSON.stringify(same) === JSON.stringify(book) ? same : book;
  });
  // Nothing changed: the list itself is kept, and nothing that reads it runs again.
  return kept.length === before.length && kept.every((book, i) => book === toRaw(before[i])) ? before : kept;
}

export const useLibraryStore = defineStore("library", () => {
  const books = ref<BookSummary[]>([]),
    loading = ref(false),
    folder = ref(""),
    error = ref("");
  /** False until the first load has finished (or failed): screens show placeholders until then. */
  const ready = ref(false);
  const imports = ref<ImportResult[]>([]);
  const query = ref(""),
    sort = ref<LibrarySort>("recent"),
    filter = ref<LibraryFilter>("all");
  /** Which kind of item the library shows; "all" for every kind. */
  const kind = ref<ItemKind | "all">("all");
  const collections = ref<Collection[]>([]);
  /** The tags the library shows only, and those it leaves out (an app setting). */
  const tagFilter = ref<TagFilter>({ include: [], exclude: [] });
  /** Reading speed for time estimates, in words a minute (an app setting). */
  const readingWpm = ref(230);
  /** Books whose file could not be read: listed apart, never among the books to open. */
  const failed = computed(() => books.value.filter((b) => b.status.kind === "error"));
  const usable = computed(() => books.value.filter((b) => b.status.kind !== "error"));
  /** The collection the library is filtered by, if any. */
  const collection = computed(() =>
    filter.value.startsWith("collection:")
      ? (collections.value.find((c) => `collection:${c.id}` === filter.value) ?? null)
      : null,
  );
  const named = (list: string[], tag: string) => list.some((t) => t.toLowerCase() === tag.toLowerCase());
  /** Whether the tag filter lets an item through: none of the tags left out, and one of those
   *  shown only, if any are. */
  const tagged = (book: BookSummary) =>
    !book.tags.some((t) => named(tagFilter.value.exclude, t)) &&
    (!tagFilter.value.include.length || book.tags.some((t) => named(tagFilter.value.include, t)));
  /** The open collection's items, else every item, before the tag filter. */
  const pool = computed(() =>
    collection.value ? usable.value.filter((b) => b.collections.includes(collection.value!.id)) : usable.value,
  );
  /** What the library is showing from: the pool, through the tag filter. Counts are of these, so
   *  a collection's switch counts what it holds. */
  const scope = computed(() => pool.value.filter(tagged));
  /** Each tag on the pool's items with how many have it, the most used first: archived items
   *  only under Archive, as the filter shows them. A tag the filter names stays listed (at no
   *  items, once nothing has it), so it can be cleared. */
  const tagCounts = computed(() => {
    const counts = new Map<string, { name: string; books: number }>();
    for (const book of pool.value.filter((b) => onShelf(b, filter.value)))
      for (const tag of book.tags) {
        const key = tag.toLowerCase();
        const entry = counts.get(key) ?? { name: tag, books: 0 };
        entry.books++;
        counts.set(key, entry);
      }
    for (const tag of [...tagFilter.value.include, ...tagFilter.value.exclude])
      if (!counts.has(tag.toLowerCase())) counts.set(tag.toLowerCase(), { name: tag, books: 0 });
    return [...counts.values()].sort(
      (a, b) => b.books - a.books || a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
    );
  });
  const tagState = (tag: string): TagState =>
    named(tagFilter.value.include, tag) ? "include" : named(tagFilter.value.exclude, tag) ? "exclude" : "off";
  /** Shows only the items with the tag, then leaves them out, then neither; saved. */
  async function cycleTag(tag: string) {
    const state = tagState(tag),
      without = (list: string[]) => list.filter((t) => t.toLowerCase() !== tag.toLowerCase());
    const next: TagFilter = {
      include: state === "off" ? [...without(tagFilter.value.include), tag] : without(tagFilter.value.include),
      exclude: state === "include" ? [...without(tagFilter.value.exclude), tag] : without(tagFilter.value.exclude),
    };
    tagFilter.value = next;
    try {
      await api.updateAppSettings({ patch: { libraryTags: next } });
    } catch (e) {
      error.value = String(e);
    }
  }
  /** Every tag on some item, by name: offered when tagging. */
  const allTags = computed(() =>
    [...new Map(usable.value.flatMap((b) => b.tags).map((t) => [t.toLowerCase(), t])).values()].sort((a, b) =>
      a.localeCompare(b, undefined, { sensitivity: "base" }),
    ),
  );
  /** The user's tags for an item: exactly these. */
  async function setBookTags(bookId: string, tags: string[]) {
    await api.setBookTags({ bookId, tags });
    await refresh();
  }
  const counts = computed(
    () =>
      Object.fromEntries(
        Object.entries(filters).map(([key, test]) => [
          key,
          scope.value.filter((b) => onShelf(b, key as BuiltInFilter) && test(b)).length,
        ]),
      ) as Record<BuiltInFilter, number>,
  );
  /** Whether an item is of `kind`. Audiobooks are those on their own and books that have one
   *  (a paired recording, however far its sync is), so a book can be both. */
  const isKind = (book: BookSummary, kind: ItemKind) =>
    kind === "audiobook" ? book.kind === "audiobook" || book.recording !== null : book.kind === kind;
  /** How many items of each kind there are, in the archive or out of it as the filter is. */
  const kindCounts = computed(() => {
    const c: Record<ItemKind, number> = { book: 0, audiobook: 0, article: 0 };
    for (const book of scope.value.filter((b) => onShelf(b, filter.value)))
      for (const kind of Object.keys(c) as ItemKind[]) if (isKind(book, kind)) c[kind]++;
    return c;
  });
  /** The kind switch is offered once what is shown holds two kinds. */
  const kinds = computed(() => (Object.keys(kindCounts.value) as ItemKind[]).filter((k) => kindCounts.value[k] > 0));
  /** Whether an item is in the filter; a search (`searching`) finds archived items too. */
  const matchesFilter = (book: BookSummary, searching: boolean) => {
    const value = filter.value;
    return (
      (searching || onShelf(book, value)) &&
      (value.startsWith("collection:")
        ? book.collections.includes(Number(value.slice(11)))
        : filters[value as BuiltInFilter](book))
    );
  };
  const visible = computed(() => {
    const q = query.value.trim().toLowerCase();
    const shown = usable.value.filter(
      (b) =>
        matchesFilter(b, Boolean(q)) &&
        tagged(b) &&
        (kind.value === "all" || kinds.value.length < 2 || isKind(b, kind.value as ItemKind)),
    );
    const matches = q
      ? shown.filter((b) => b.title.toLowerCase().includes(q) || b.authors.some((a) => a.toLowerCase().includes(q)))
      : shown;
    const text = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base", numeric: true });
    const byTitle = (a: BookSummary, b: BookSummary) =>
      text(a.title.replace(/^(the|a|an)\s+/i, ""), b.title.replace(/^(the|a|an)\s+/i, ""));
    const order: Record<LibrarySort, (a: BookSummary, b: BookSummary) => number> = {
      recent: (a, b) =>
        Math.max(b.lastOpenedAt ?? 0, b.addedAt) - Math.max(a.lastOpenedAt ?? 0, a.addedAt) || byTitle(a, b),
      title: byTitle,
      author: (a, b) => text(a.authors[0] ?? "\uffff", b.authors[0] ?? "\uffff") || byTitle(a, b),
      progress: (a, b) => (b.progress?.fraction ?? -1) - (a.progress?.fraction ?? -1) || byTitle(a, b),
    };
    return [...matches].sort(order[sort.value]);
  });
  async function setSort(next: LibrarySort) {
    sort.value = next;
    try {
      await api.updateAppSettings({ patch: { librarySort: next } });
    } catch (e) {
      error.value = String(e);
    }
  }
  /** Marks a book finished, or unread (which also forgets where the user was in it). */
  async function markRead(id: string, state: "finished" | "unread") {
    await (state === "finished" ? api.markFinished({ bookId: id }) : api.markUnread({ bookId: id }));
    await refresh();
  }
  /** Archives an item, or takes it out of the archive; nothing of it is deleted. */
  async function archive(id: string, on: boolean) {
    await api.archive({ bookId: id, on });
    await refresh();
  }
  async function setReadingWpm(wpm: number) {
    readingWpm.value = (await api.updateAppSettings({ patch: { readingWpm: wpm } })).readingWpm;
  }
  /** The progress the core reported for a book read silently, until the next refresh. */
  function setProgress(id: string, progress: BookProgress) {
    const book = books.value.find((b) => b.id === id);
    if (book) book.progress = progress;
  }
  let startup: Promise<void> | undefined;
  let unlisten: (() => void) | undefined;
  async function refresh() {
    loading.value = true;
    try {
      const [list, shelves] = await Promise.all([api.listBooks(), api.collections()]);
      books.value = keepUnchanged(list, books.value);
      if (JSON.stringify(shelves) !== JSON.stringify(toRaw(collections.value))) collections.value = shelves;
      if (filter.value.startsWith("collection:") && !collection.value) filter.value = "all";
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }
  /** Collection changes throw (for example a duplicate name) so the caller can say why. */
  async function createCollection(name: string) {
    const id = await api.createCollection({ name });
    await refresh();
    return id;
  }
  async function renameCollection(id: number, name: string) {
    await api.renameCollection({ id, name });
    await refresh();
  }
  async function deleteCollection(id: number) {
    await api.deleteCollection({ id });
    await refresh();
  }
  async function setBookCollections(bookId: string, ids: number[]) {
    await api.setBookCollections({ bookId, ids });
    await refresh();
  }
  /** Puts each of `items` on `collections` too, keeping the collections it is on. */
  async function addToCollections(items: string[], collections: number[]) {
    if (!collections.length) return;
    for (const id of items) {
      const was = books.value.find((b) => b.id === id)?.collections ?? [];
      await api.setBookCollections({ bookId: id, ids: [...new Set([...was, ...collections])] });
    }
    await refresh();
  }
  async function perform(action: () => Promise<unknown>) {
    error.value = "";
    try {
      await action();
      await refresh();
    } catch (e) {
      error.value = String(e);
    }
  }
  function start() {
    return (startup ??= (async () => {
      try {
        unlisten = await api.onLibraryChanged(() => {
          void refresh();
        });
        folder.value = (await api.appInfo()).libraryFolder;
        const settings = await api.appSettings();
        sort.value = settings.librarySort ?? "recent";
        tagFilter.value = settings.libraryTags ?? { include: [], exclude: [] };
        readingWpm.value = settings.readingWpm ?? 230;
        await refresh();
      } catch (e) {
        error.value = String(e);
      } finally {
        ready.value = true;
      }
    })());
  }
  /** Adds the books at `paths`; returns the items this import added (none when it failed). */
  async function importFiles(paths: string[]) {
    let results: ImportResult[] = [];
    await perform(async () => {
      imports.value = results = await api.importFiles({ paths });
    });
    const added = results.flatMap((r) => (r.bookId && !r.error ? [r.bookId] : []));
    await lookForAudiobooks(results.flatMap((r) => (r.bookId ? [r.bookId] : [])));
    return added;
  }
  /** After books were added: the core looks for their audiobooks (if the user wants that), and
   *  when some wait for the user, Find audiobooks opens on them. */
  async function lookForAudiobooks(ids: string[]) {
    if (!ids.length) return;
    try {
      if ((await api.lookForAudiobooks({ ids })) > 0) useNavigationStore().view = "audiobooks";
    } catch (e) {
      error.value = String(e);
    }
  }
  /** Asks for EPUB files with the system's chooser and adds them. */
  async function chooseFiles() {
    error.value = "";
    try {
      const paths = await api.chooseBooks();
      if (paths?.length) await importFiles(paths);
    } catch (e) {
      error.value = String(e);
    }
  }
  function dispose() {
    unlisten?.();
    startup = undefined;
  }
  return {
    books,
    failed,
    ready,
    visible,
    query,
    sort,
    setSort,
    filter,
    counts,
    kind,
    kinds,
    kindCounts,
    collections,
    collection,
    readingWpm,
    setReadingWpm,
    markRead,
    archive,
    setProgress,
    createCollection,
    renameCollection,
    deleteCollection,
    setBookCollections,
    addToCollections,
    tagFilter,
    tagCounts,
    tagState,
    cycleTag,
    allTags,
    setBookTags,
    loading,
    folder,
    error,
    imports,
    start,
    refresh,
    importFiles,
    chooseFiles,
    lookForAudiobooks,
    rescan: () => perform(() => api.rescanLibrary()),
    remove: (id: string) => perform(() => api.removeBook({ bookId: id })),
    dispose,
  };
});
