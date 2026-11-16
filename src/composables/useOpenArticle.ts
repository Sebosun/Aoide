import { computed, watch } from "vue";
import { useArticlesStore } from "@/stores/articles";
import { useReaderStore } from "@/stores/reader";

/** The reader's item when it is an article: its source and fetch, loaded when it opens and
 *  kept current from "article://state". What the item is comes with its view from the core. */
export function useOpenArticle() {
  const reader = useReaderStore(),
    articles = useArticlesStore();
  const isArticle = computed(() => reader.book?.kind === "article");
  watch(
    () => (isArticle.value ? reader.book?.id : undefined),
    (id) => {
      if (id) articles.load(id).catch((e) => (reader.error = String(e)));
    },
    { immediate: true },
  );
  const article = computed(() => (isArticle.value && reader.book ? (articles.byId[reader.book.id] ?? null) : null));
  async function openOriginal() {
    if (reader.book) await articles.openOriginal(reader.book.id).catch((e) => (reader.error = String(e)));
  }
  return { isArticle, article, openOriginal };
}
