import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api } from "../api";
import type { EntryView, FeedView } from "../api/types";
import { useLibraryStore } from "./library";
import { useReaderStore } from "./reader";

/** What the Feeds screen lists: every feed's entries, the saved ones, or one feed's. */
export type FeedsSource = { kind: "all" } | { kind: "saved" } | { kind: "feed"; id: number };
/** Entries asked for at a time. */
export const ENTRY_PAGE = 50;

/** Subscriptions and the listed entries, kept current from "feeds://changed". */
export const useFeedsStore = defineStore("feeds", () => {
  const feeds = ref<FeedView[]>([]);
  const entries = ref<EntryView[]>([]);
  const source = ref<FeedsSource>({ kind: "all" });
  /** Older entries may follow the listed ones. */
  const more = ref(false);
  const loaded = ref(false);
  const error = ref("");
  let startup: Promise<void> | undefined, unlisten: (() => void) | undefined;

  const unread = computed(() => feeds.value.reduce((sum, feed) => sum + feed.unread, 0));
  /** When the feeds last answered (the newest of them). */
  const checkedAt = computed(() =>
    feeds.value.reduce<number | null>((at, feed) => Math.max(at ?? 0, feed.lastOkAt ?? 0) || at, null),
  );

  const ask = (before: number | null, limit: number) => {
    const from = source.value;
    return api.feedEntries(
      from.kind === "feed" ? from.id : null,
      from.kind === "saved" ? "saved" : "all",
      before,
      limit,
    );
  };
  /** Lists the source's entries again, as many as were shown (one page at least). */
  async function refresh() {
    try {
      const shown = Math.max(ENTRY_PAGE, entries.value.length);
      const [views, list] = await Promise.all([api.feeds(), ask(null, shown)]);
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
    await refresh();
  }
  /** The next page of older entries. */
  async function loadMore() {
    const last = entries.value.at(-1);
    if (!more.value || !last) return;
    const page = await ask(last.id, ENTRY_PAGE);
    entries.value = [...entries.value, ...page];
    more.value = page.length === ENTRY_PAGE;
  }
  /** Reads an entry (made into an item when needed); it is read after. */
  async function open(entry: EntryView) {
    try {
      const id = await api.openFeedEntry(entry.id);
      await useReaderStore().open(id);
    } catch (e) {
      error.value = String(e);
    }
  }
  /** Saves an entry to the library, or takes it out again (back to the feed). */
  async function toggleSaved(entry: EntryView) {
    try {
      if (entry.inLibrary && entry.bookId) await api.removeBook(entry.bookId);
      else await api.saveFeedEntry(entry.id);
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
  const checkNow = () => run(() => api.checkFeeds());
  /** Marks the listed feed's entries read (every feed's, unless one is chosen). */
  const markAllRead = () => run(() => api.markAllRead(source.value.kind === "feed" ? source.value.id : null));
  const unsubscribe = (id: number) =>
    run(async () => {
      await api.unsubscribe(id);
      if (source.value.kind === "feed" && source.value.id === id) await choose({ kind: "all" });
    });
  function dispose() {
    unlisten?.();
    unlisten = undefined;
    startup = undefined;
  }
  return {
    feeds,
    entries,
    source,
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
    checkNow,
    markAllRead,
    unsubscribe,
    dispose,
  };
});
