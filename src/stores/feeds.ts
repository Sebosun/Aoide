import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "@/api";
import type { EntryView, FeedView, QueuePlace } from "@/api/types";
import { useArticlesStore } from "@/stores/articles";
import { useLibraryStore } from "@/stores/library";
import { useReaderStore } from "@/stores/reader";
import { useQueueStore } from "@/stores/queue";

/** What the Feeds screen lists: every feed's entries, the saved ones, or one feed's. */
export type FeedsSource = { kind: "all" } | { kind: "saved" } | { kind: "feed"; id: number };
/** Entries asked for at a time. */
export const ENTRY_PAGE = 50;

/** Subscriptions and the listed entries, kept current from "feeds://changed". */
export const useFeedsStore = defineStore("feeds", () => {
  const feeds = ref<FeedView[]>([]);
  const entries = ref<EntryView[]>([]);
  const source = ref<FeedsSource>({ kind: "all" });
  /** The entry being opened while its page is fetched. */
  const opening = ref<number | null>(null);
  /** Older entries may follow the listed ones. */
  const more = ref(false);
  const loaded = ref(false);
  const error = ref("");
  let startup: Promise<void> | undefined, unlisten: (() => void) | undefined;
  /** Counts the times the list was replaced: an answer for an older list is dropped. */
  let generation = 0;
  /** The older page being loaded. */
  let loading: Promise<void> | undefined;

  const unread = computed(() => feeds.value.reduce((sum, feed) => sum + feed.unread, 0));
  /** When the feeds last answered (the newest of them). */
  const checkedAt = computed(() =>
    feeds.value.reduce<number | null>((at, feed) => Math.max(at ?? 0, feed.lastOkAt ?? 0) || at, null),
  );

  const ask = (before: number | null, limit: number) => {
    const from = source.value;
    return api.feedEntries({
      feedId: from.kind === "feed" ? from.id : null,
      filter: from.kind === "saved" ? "saved" : "all",
      before,
      limit,
    });
  };
  /** Lists the source's entries again, as many as were shown (one page at least). */
  async function refresh() {
    try {
      const shown = Math.max(ENTRY_PAGE, entries.value.length);
      const asked = ++generation;
      const [views, list] = await Promise.all([api.feeds(), ask(null, shown)]);
      if (asked !== generation) return;
      feeds.value = views;
      entries.value = list;
      more.value = list.length === shown;
      loaded.value = true;
    } catch (e) {
      error.value = String(e);
    }
  }
  function start() {
    return (startup ??= (async () => {
      unlisten = await api.onFeedsChanged(() => void refresh());
      await refresh();
    })());
  }
  async function choose(next: FeedsSource) {
    source.value = next;
    entries.value = [];
    generation++;
    await start();
    await refresh();
  }
  /** The next page of older entries (once, however often it is asked for meanwhile). */
  function loadMore() {
    return (loading ??= (async () => {
      const last = entries.value.at(-1);
      if (!more.value || !last) return;
      const asked = generation;
      try {
        const page = await ask(last.id, ENTRY_PAGE);
        if (asked !== generation) return;
        entries.value = [...entries.value, ...page];
        more.value = page.length === ENTRY_PAGE;
      } catch (e) {
        error.value = String(e);
      }
    })().finally(() => (loading = undefined)));
  }
  /** Reads an entry (made into an item when needed); it is read after. An entry the feed
   *  gave only a summary of opens once its page is fetched. */
  async function open(entry: EntryView) {
    error.value = "";
    opening.value = entry.id;
    try {
      const id = await api.openFeedEntry({ entryId: entry.id });
      // The reader shows it as an article (with Save to library) from the start.
      await useArticlesStore().whenFetched(id);
      const reader = useReaderStore();
      await reader.open(id);
      if (reader.error) throw new Error(reader.error);
    } catch (e) {
      error.value = String(e).replace(/^Error: /, "");
    } finally {
      opening.value = null;
    }
  }
  /** Saves an entry to the library, or takes it out again (back to the feed). */
  async function toggleSaved(entry: EntryView) {
    try {
      if (entry.inLibrary && entry.bookId) await api.removeBook({ bookId: entry.bookId });
      else await api.saveFeedEntry({ entryId: entry.id });
      await Promise.all([refresh(), useLibraryStore().refresh()]);
    } catch (e) {
      error.value = String(e);
    }
  }
  const run = async (task: () => Promise<void>) => {
    try {
      await task();
    } catch (e) {
      error.value = String(e);
    }
  };
  /** Lines an entry up to listen to, next or last; it stays unread until it has played. */
  const queue = (entry: EntryView, place: QueuePlace) =>
    run(async () => {
      await useQueueStore().addEntry(entry.id, place);
    });
  const checkNow = () => run(() => api.checkFeeds());
  /** Marks the listed feed's entries read (every feed's, unless one is chosen). */
  const markAllRead = () =>
    run(() => api.markAllRead({ feedId: source.value.kind === "feed" ? source.value.id : null }));
  const unsubscribe = (id: number) =>
    run(async () => {
      await api.unsubscribe({ feedId: id });
      if (source.value.kind === "feed" && source.value.id === id) await choose({ kind: "all" });
    });
  /** Chooses the collection feed `id`'s entries go on when saved (`null`: none). */
  const setCollection = (id: number, collection: number | null) =>
    run(() => api.setFeedCollection({ feedId: id, collectionId: collection }));
  /** Imports a subscription list the user picks; null when they pick none. */
  async function importList() {
    const path = await api.chooseOpml();
    if (!path) return null;
    const result = await api.importOpml({ path });
    await refresh();
    return result;
  }
  /** Exports the subscriptions where the user picks; how many, or null when they pick nowhere. */
  async function exportList() {
    const path = await api.chooseOpmlTarget();
    return path ? api.exportOpml({ path }) : null;
  }
  function dispose() {
    unlisten?.();
    unlisten = undefined;
    startup = undefined;
  }
  return {
    feeds,
    entries,
    source,
    opening,
    more,
    loaded,
    error,
    unread,
    checkedAt,
    start,
    refresh,
    choose,
    loadMore,
    open,
    toggleSaved,
    queue,
    checkNow,
    markAllRead,
    unsubscribe,
    setCollection,
    importList,
    exportList,
    dispose,
  };
});
