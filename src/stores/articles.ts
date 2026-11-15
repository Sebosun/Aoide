import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "../api";
import type { ArticleSource, ArticleView, FeedView } from "../api/types";
import { useFeedsStore } from "./feeds";
import { useLibraryStore } from "./library";
import { useNavigationStore } from "./navigation";
import { useReaderStore } from "./reader";

/** Saved articles' sources and fetches, kept current from "article://state". */
export const useArticlesStore = defineStore("articles", () => {
  const byId = ref<Record<string, ArticleView>>({});
  let startup: Promise<void> | undefined, unlisten: (() => void) | undefined;
  const put = (article: ArticleView) => {
    const before = byId.value[article.id];
    byId.value = { ...byId.value, [article.id]: article };
    // A new copy was stored: the open reader shows its text.
    if (before && before.fetchedAt !== article.fetchedAt) void useReaderStore().refreshBook(article.id);
  };
  function start() {
    return (startup ??= (async () => {
      unlisten = await api.onArticleState(put);
    })());
  }
  async function load(id: string) {
    await start();
    put(await api.article(id));
  }
  /** Saves `link` as an article; the library opens to show it being fetched. Throws the core's
   *  refusal of a link it does not take. */
  async function save(link: string) {
    await start();
    const id = await api.saveArticle(link);
    const navigation = useNavigationStore();
    navigation.closeAdd();
    navigation.view = "library";
    await useLibraryStore().refresh();
    return id;
  }
  /** A link given to Add: a feed is subscribed and Feeds opens on it; anything else is saved
   *  as an article and the library opens to show it being fetched. */
  async function add(link: string) {
    await start();
    const added = await api.addLink(link);
    if (added.kind === "saved") {
      const navigation = useNavigationStore();
      navigation.closeAdd();
      navigation.view = "library";
      await useLibraryStore().refresh();
    } else await showFeed(added.value);
  }
  /** Feeds, on the feed `feed`. */
  async function showFeed(feed: FeedView) {
    const navigation = useNavigationStore();
    navigation.closeAdd();
    navigation.view = "feeds";
    await useFeedsStore().choose({ kind: "feed", id: feed.id });
  }
  /** Keeps an article opened from a feed in the library. */
  async function keep(id: string) {
    await api.saveToLibrary(id);
    await Promise.all([load(id), useLibraryStore().refresh()]);
  }
  async function refetch(id: string, source: ArticleSource) {
    await start();
    await api.refetchArticle(id, source);
  }
  /** Opens the article's page in the system's browser. */
  const openOriginal = (id: string) => api.openOriginal(id);
  function dispose() {
    unlisten?.();
    unlisten = undefined;
    startup = undefined;
  }
  return { byId, start, load, save, add, showFeed, keep, refetch, openOriginal, dispose };
});
