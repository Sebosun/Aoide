import { defineStore } from "pinia";
import { computed, markRaw, ref, watch } from "vue";
import { api } from "@/api";
import type { BookView, ChapterView, HighlightView, ReadingMode } from "@/api/types";
import { useLibraryStore } from "@/stores/library";
import { useNavigationStore } from "@/stores/navigation";
import { usePlayerStore } from "@/stores/player";
import { beginShowing, cancelShowing } from "@/frames";

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
  /** The open book's highlights in reading order. They change while a chapter is open, so they
   *  travel beside the chapters (which are replaced whole), not in them. */
  const highlights = ref<HighlightView[]>([]);
  /** Words a search of the library's text found, marked in the open book until another opens:
   *  characters `start..end` of a chapter. */
  const found = ref<{ bookId: string; chapter: number; start: number; end: number; segmentId: string } | null>(null);
  /** The found words as a highlight the page draws (id -1: no card opens on it). */
  const shownHighlights = computed<HighlightView[]>(() => {
    const f = found.value;
    if (!f || f.bookId !== book.value?.id) return highlights.value;
    const mark: HighlightView = {
      id: -1,
      chapter: f.chapter,
      chapterTitle: "",
      start: f.start,
      end: f.end,
      text: "",
      note: "",
      detached: false,
    };
    return [...highlights.value, mark];
  });
  /** The open book's highlights, fetched again. */
  async function loadHighlights() {
    const id = book.value?.id;
    if (!id) return;
    const list = await api.highlights({ bookId: id });
    if (book.value?.id === id) highlights.value = list;
  }
  /** Book `id`'s highlights, when it is the open book. */
  function showHighlights(id: string, list: HighlightView[]) {
    if (book.value?.id === id) highlights.value = list;
  }
  /** Highlights characters `start..end` of a chapter of the open book. */
  async function addHighlight(chapter: number, start: number, end: number) {
    const id = book.value?.id;
    if (!id) return;
    const list = await api.addHighlight({ bookId: id, chapter, start, end });
    if (book.value?.id === id) highlights.value = list;
  }
  /** Writes a highlight's note (the open book's). */
  async function noteHighlight(id: number, note: string) {
    const list = await api.updateHighlightNote({ id, note });
    highlights.value = list;
  }
  async function removeHighlight(id: number) {
    const list = await api.deleteHighlight({ id });
    highlights.value = list;
  }
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
  /** A chapter's text, fetched once; does not change which chapter is shown. It is kept as
   *  plain data (`markRaw`): a chapter is only replaced whole, and wrapping its thousands of
   *  sentences for reactivity would cost every open. */
  async function chapterText(id: string, index: number) {
    const key = `${id}:${index}`;
    return (cache.value[key] ??= markRaw(await api.chapter({ bookId: id, chapter: index })));
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
  /** Opens book `id` at a passage a search found, at its sentence with the words marked,
   *  without playing. */
  async function openAt(id: string, hit: { chapter: number; segmentId: string; found: number[] }) {
    if (book.value?.id !== id || useNavigationStore().view !== "reader") await open(id);
    if (book.value?.id !== id) return;
    place.value = { chapter: hit.chapter, segmentId: hit.segmentId };
    await loadChapter(hit.chapter);
    found.value = {
      bookId: id,
      chapter: hit.chapter,
      start: hit.found[0],
      end: hit.found[1],
      segmentId: hit.segmentId,
    };
    useNavigationStore().view = "reader";
  }
  async function open(id: string) {
    const request = ++openRequest;
    found.value = null;
    loading.value = true;
    error.value = "";
    beginShowing("open");
    try {
      const result = await api.openBook({ bookId: id });
      if (request !== openRequest) return;
      book.value = result;
      highlights.value = [];
      const saved = result.progress;
      place.value = saved?.segmentId ? { chapter: saved.chapter, segmentId: saved.segmentId } : null;
      // An audiobook on its own has no text to show: its parts are played.
      if (result.shape === "listen") currentChapter.value = saved?.chapter ?? 0;
      else await Promise.all([loadChapter(saved?.chapter ?? 0), loadHighlights()]);
      useNavigationStore().view = "reader";
    } catch (e) {
      error.value = String(e);
      // A newer open may be under way: its timing is not this one's to drop.
      if (request === openRequest) cancelShowing();
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
  /** Fetches a book's chapters again (its narration changed what they mark, or an article was
   *  fetched again), and its highlights, which the core moves with their text. */
  async function refreshBook(id: string) {
    cache.value = Object.fromEntries(Object.entries(cache.value).filter(([key]) => !key.startsWith(`${id}:`)));
    if (book.value?.id === id && shape.value !== "listen")
      await Promise.all([
        loadChapter(currentChapter.value),
        loadHighlights().catch((e) => void (error.value = String(e))),
      ]);
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
    openAt,
    found,
    shownHighlights,
    chapterText,
    loadChapter,
    startChapter,
    refreshBook,
    highlights,
    loadHighlights,
    showHighlights,
    addHighlight,
    noteHighlight,
    removeHighlight,
  };
});
