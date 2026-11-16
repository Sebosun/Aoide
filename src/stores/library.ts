import { defineStore } from "pinia";
import { computed, ref, toRaw } from "vue";
import { api } from "@/api";
import { useNavigationStore } from "@/stores/navigation";
import type { BookProgress, BookSummary, Collection, ImportResult, ItemKind, LibrarySort } from "@/api/types";

export type BuiltInFilter = "all" | "unread" | "progress" | "finished" | "listen" | "preparing";
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
};

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
  /** Reading speed for time estimates, in words a minute (an app setting). */
  const readingWpm = ref(230);
  /** Books whose file could not be read: listed apart, never among the books to open. */
  const failed = computed(() => books.value.filter((b) => b.status.kind === "error"));
  const usable = computed(() => books.value.filter((b) => b.status.kind !== "error"));
  const counts = computed(
    () =>
      Object.fromEntries(
        Object.entries(filters).map(([key, test]) => [key, usable.value.filter(test).length]),
      ) as Record<BuiltInFilter, number>,
  );
  /** How many items of each kind there are. */
  const kindCounts = computed(() => {
    const c: Record<ItemKind, number> = { book: 0, audiobook: 0, article: 0 };
    for (const book of usable.value) c[book.kind]++;
    return c;
  });
  /** The kind switch is offered once the library holds two kinds. */
  const kinds = computed(() => (Object.keys(kindCounts.value) as ItemKind[]).filter((k) => kindCounts.value[k] > 0));
  /** The collection the library is filtered by, if any. */
  const collection = computed(() =>
    filter.value.startsWith("collection:")
      ? (collections.value.find((c) => `collection:${c.id}` === filter.value) ?? null)
      : null,
  );
  const matchesFilter = (book: BookSummary) => {
    const value = filter.value;
    return value.startsWith("collection:")
      ? book.collections.includes(Number(value.slice(11)))
      : filters[value as BuiltInFilter](book);
  };
  const visible = computed(() => {
    const q = query.value.trim().toLowerCase();
    const shown = usable.value.filter(
      (b) => matchesFilter(b) && (kind.value === "all" || kinds.value.length < 2 || b.kind === kind.value),
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
      await api.updateAppSettings({ librarySort: next });
    } catch (e) {
      error.value = String(e);
    }
  }
  /** Marks a book finished, or unread (which also forgets where the user was in it). */
  async function markRead(id: string, state: "finished" | "unread") {
    await (state === "finished" ? api.markFinished(id) : api.markUnread(id));
    await refresh();
  }
  async function setReadingWpm(wpm: number) {
    readingWpm.value = (await api.updateAppSettings({ readingWpm: wpm })).readingWpm;
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
    const id = await api.createCollection(name);
    await refresh();
    return id;
  }
  async function renameCollection(id: number, name: string) {
    await api.renameCollection(id, name);
    await refresh();
  }
  async function deleteCollection(id: number) {
    await api.deleteCollection(id);
    await refresh();
  }
  async function setBookCollections(bookId: string, ids: number[]) {
    await api.setBookCollections(bookId, ids);
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
        readingWpm.value = settings.readingWpm ?? 230;
        await refresh();
      } catch (e) {
        error.value = String(e);
      } finally {
        ready.value = true;
      }
    })());
  }
  async function importFiles(paths: string[]) {
    await perform(async () => {
      imports.value = await api.importFiles(paths);
    });
    await lookForAudiobooks(imports.value.flatMap((r) => (r.bookId ? [r.bookId] : [])));
  }
  /** After books were added: the core looks for their audiobooks (if the user wants that), and
   *  when some wait for the user, Find audiobooks opens on them. */
  async function lookForAudiobooks(ids: string[]) {
    if (!ids.length) return;
    try {
      if ((await api.lookForAudiobooks(ids)) > 0) useNavigationStore().view = "audiobooks";
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
    setProgress,
    createCollection,
    renameCollection,
    deleteCollection,
    setBookCollections,
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
    remove: (id: string) => perform(() => api.removeBook(id)),
    dispose,
  };
});
