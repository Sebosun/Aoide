import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "../api";
import type { BookSummary, Collection, ImportResult, LibrarySort } from "../api/types";

export type BuiltInFilter = "all" | "progress" | "offline" | "preparing";
/** A built-in filter, or one collection. */
export type LibraryFilter = BuiltInFilter | `collection:${number}`;
const filters: Record<BuiltInFilter, (book: BookSummary) => boolean> = {
  all: () => true,
  progress: book => (book.progress?.fraction ?? 0) > 0 && (book.progress?.fraction ?? 0) < 1,
  offline: book => book.prepare?.state === "done",
  preparing: book => Boolean(book.prepare && book.prepare.state !== "done"),
};

export const useLibraryStore = defineStore("library", () => {
  const books = ref<BookSummary[]>([]), loading = ref(false), folder = ref(""), error = ref("");
  /** False until the first load has finished (or failed): screens show placeholders until then. */
  const ready = ref(false);
  const imports = ref<ImportResult[]>([]);
  const query = ref(""), sort = ref<LibrarySort>("recent"), filter = ref<LibraryFilter>("all");
  const collections = ref<Collection[]>([]);
  /** Books whose file could not be read: listed apart, never among the books to open. */
  const failed = computed(() => books.value.filter(b => b.status.kind === "error"));
  const usable = computed(() => books.value.filter(b => b.status.kind !== "error"));
  const counts = computed(() => Object.fromEntries(Object.entries(filters).map(([key, test]) => [key, usable.value.filter(test).length])) as Record<BuiltInFilter, number>);
  /** The collection the library is filtered by, if any. */
  const collection = computed(() => filter.value.startsWith("collection:") ? collections.value.find(c => `collection:${c.id}` === filter.value) ?? null : null);
  const matchesFilter = (book: BookSummary) => {
    const value = filter.value;
    return value.startsWith("collection:") ? book.collections.includes(Number(value.slice(11))) : filters[value as BuiltInFilter](book);
  };
  const visible = computed(() => {
    const q = query.value.trim().toLowerCase();
    const shown = usable.value.filter(matchesFilter);
    const matches = q ? shown.filter(b => b.title.toLowerCase().includes(q) || b.authors.some(a => a.toLowerCase().includes(q))) : shown;
    const text = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base", numeric: true });
    const byTitle = (a: BookSummary, b: BookSummary) => text(a.title.replace(/^(the|a|an)\s+/i, ""), b.title.replace(/^(the|a|an)\s+/i, ""));
    const order: Record<LibrarySort, (a: BookSummary, b: BookSummary) => number> = {
      recent: (a, b) => Math.max(b.lastOpenedAt ?? 0, b.addedAt) - Math.max(a.lastOpenedAt ?? 0, a.addedAt) || byTitle(a, b),
      title: byTitle,
      author: (a, b) => text(a.authors[0] ?? "\uffff", b.authors[0] ?? "\uffff") || byTitle(a, b),
      progress: (a, b) => (b.progress?.fraction ?? -1) - (a.progress?.fraction ?? -1) || byTitle(a, b),
    };
    return [...matches].sort(order[sort.value]);
  });
  async function setSort(next: LibrarySort) {
    sort.value = next;
    try { const settings = await api.appSettings(); await api.saveAppSettings({ ...settings, librarySort: next }); } catch (e) { error.value = String(e); }
  }
  let startup: Promise<void> | undefined;
  let unlisten: (() => void) | undefined;
  async function refresh() {
    loading.value = true;
    try {
      const [list, shelves] = await Promise.all([api.listBooks(), api.collections()]);
      books.value = list; collections.value = shelves;
      if (filter.value.startsWith("collection:") && !collection.value) filter.value = "all";
    } catch (e) { error.value = String(e); } finally { loading.value = false; }
  }
  /** Collection changes throw (for example a duplicate name) so the caller can say why. */
  async function createCollection(name: string) { const id = await api.createCollection(name); await refresh(); return id; }
  async function renameCollection(id: number, name: string) { await api.renameCollection(id, name); await refresh(); }
  async function deleteCollection(id: number) { await api.deleteCollection(id); await refresh(); }
  async function setBookCollections(bookId: string, ids: number[]) { await api.setBookCollections(bookId, ids); await refresh(); }
  async function perform(action: () => Promise<unknown>) { error.value = ""; try { await action(); await refresh(); } catch (e) { error.value = String(e); } }
  function start() { return startup ??= (async () => { try { unlisten = await api.onLibraryChanged(() => { void refresh(); }); folder.value = (await api.appInfo()).libraryFolder; sort.value = (await api.appSettings()).librarySort ?? "recent"; await refresh(); } catch (e) { error.value = String(e); } finally { ready.value = true; } })(); }
  async function importFiles(paths: string[]) { await perform(async () => { imports.value = await api.importFiles(paths); }); }
  function dispose() { unlisten?.(); startup = undefined; }
  return { books, failed, ready, visible, query, sort, setSort, filter, counts, collections, collection, createCollection, renameCollection, deleteCollection, setBookCollections, loading, folder, error, imports, start, refresh, importFiles, rescan: () => perform(() => api.rescanLibrary()), remove: (id: string) => perform(() => api.removeBook(id)), dispose };
});
