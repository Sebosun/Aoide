import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { api } from "@/api";
import type { BookView, ChapterView, ReadingMode } from "@/api/types";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { usePlayerStore } from "@/stores/player";

export const useReaderStore = defineStore("reader", () => {
  const book = ref<BookView | null>(null),
    cache = ref<Record<string, ChapterView>>({}),
    currentChapter = ref(0),
    error = ref(""),
    loading = ref(false);
  let openRequest = 0,
    chapterRequest = 0;
  const chapter = computed(() => (book.value ? cache.value[`${book.value.id}:${currentChapter.value}`] : undefined));
  /** Where the user is reading in the open book: the saved place, then the first sentence
   *  at the top of the screen while reading silently. */
  const place = ref<{ chapter: number; segmentId: string } | null>(null);
  /** What the reader screen is for the open book, as the core says. */
  const shape = computed(() => book.value?.shape ?? "read");
  // Playing the book from elsewhere (the player, a media key) makes the core listen to it; the
  // library brings the new mode.
  const library = useLibraryStore();
  watch(
    () => library.books.find((b) => b.id === book.value?.id),
    (summary) => {
      if (summary && book.value && (summary.shape !== book.value.shape || summary.mode !== book.value.mode))
        book.value = { ...book.value, mode: summary.mode, shape: summary.shape };
    },
  );
  /** Listens to the open book from where the user is reading. */
  async function listen() {
    const id = book.value?.id;
    if (!id) return;
    await setMode("listen");
    await usePlayerStore().play(id, place.value?.chapter ?? null, place.value?.segmentId ?? null);
  }
  /** Reads the open book silently or listens to it; listening does not start playback. */
  async function setMode(mode: ReadingMode) {
    const id = book.value?.id;
    if (!id) return;
    const view = await api.setMode({ bookId: id, mode });
    if (book.value?.id === id) book.value = view;
  }
  /** A chapter's text, fetched once; does not change which chapter is shown. */
  async function chapterText(id: string, index: number) {
    const key = `${id}:${index}`;
    return (cache.value[key] ??= await api.chapter({ bookId: id, chapter: index }));
  }
  async function loadChapter(index: number) {
    const id = book.value?.id;
    if (!id) return;
    const request = ++chapterRequest;
    try {
      await chapterText(id, index);
      if (book.value?.id === id && request === chapterRequest) currentChapter.value = index;
    } catch (e) {
      error.value = String(e);
    }
  }
  async function open(id: string) {
    const request = ++openRequest;
    loading.value = true;
    error.value = "";
    try {
      const result = await api.openBook({ bookId: id });
      if (request !== openRequest) return;
      book.value = result;
      const saved = result.progress;
      place.value = saved?.segmentId ? { chapter: saved.chapter, segmentId: saved.segmentId } : null;
      // An audiobook on its own has no text to show: its parts are played.
      if (result.shape === "listen") currentChapter.value = saved?.chapter ?? 0;
      else await loadChapter(saved?.chapter ?? 0);
      useNavigationStore().view = "reader";
    } catch (e) {
      error.value = String(e);
    } finally {
      if (request === openRequest) loading.value = false;
    }
  }
  /** Shows a chapter of the open book; while listening, plays it from its first spoken
   *  sentence. */
  async function startChapter(index: number) {
    if (shape.value === "listen") {
      if (book.value) await usePlayerStore().play(book.value.id, index, null);
      return;
    }
    await loadChapter(index);
    if (shape.value === "read") return;
    const first = chapter.value?.blocks.flatMap((b) => b.segments).find((s) => s.speakable);
    if (first && book.value && currentChapter.value === index)
      await usePlayerStore().play(book.value.id, index, first.id);
  }
  /** Fetches a book's chapters again (its narration changed what they mark). */
  async function refreshBook(id: string) {
    cache.value = Object.fromEntries(Object.entries(cache.value).filter(([key]) => !key.startsWith(`${id}:`)));
    if (book.value?.id === id && shape.value !== "listen") await loadChapter(currentChapter.value);
  }
  return {
    book,
    cache,
    currentChapter,
    chapter,
    shape,
    place,
    setMode,
    listen,
    error,
    loading,
    open,
    chapterText,
    loadChapter,
    startChapter,
    refreshBook,
  };
});
